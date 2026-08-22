//! Orchestrator DAG: task-decomposition Planner/Aggregator pipeline
//! (`Orchestration Design.md`), the piece the module-level doc comment
//! in `orchestrator::mod` explicitly deferred until there was something
//! real to execute — that's this: a caller (today, a Tauri command;
//! later, an Agent-authored plan) submits a graph of subtasks, each
//! assigned to one agent and optionally depending on others' outputs,
//! and this module runs it in dependency order, feeding each node's
//! dependency outputs into its prompt as context before dispatching it
//! through `agent_manager::send_message` — the exact same Guardrails-
//! gated path any other message takes, so a DAG node gets no special
//! privilege over a normal chat turn.
//!
//! Deliberately sequential, not parallel: nodes with no dependency
//! relationship *could* run concurrently, but this pass keeps the
//! execution order = the topological order, one node at a time — the
//! honest scope for "the DAG actually runs, in the right order, with
//! real dependency data flowing between nodes." Parallelizing
//! independent branches is a real future improvement, not something to
//! fake now.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::agent_manager::{self, providers::ChatMessage};
use crate::storage::{Agent, Storage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskNode {
    pub id: String,
    pub agent_id: String,
    pub prompt: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskDag {
    pub nodes: Vec<TaskNode>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DagError {
    /// Error Code Registry E6005 — two nodes share an `id`; the graph
    /// isn't well-formed enough to have an unambiguous execution order.
    DuplicateNodeId(String),
    /// Error Code Registry E6006 — a `depends_on` entry names a node
    /// that isn't in the graph at all (typo, or a node that was removed
    /// without updating its dependents).
    UnknownDependency { node: String, missing: String },
    /// Error Code Registry E6007 — the dependency graph has a cycle, so
    /// no valid execution order exists.
    CycleDetected,
    /// Error Code Registry E6008 — a node names an `agent_id` that
    /// wasn't provided in the `agents` lookup passed to `run` (e.g. the
    /// agent was deleted between plan creation and execution).
    UnknownAgent(String),
    /// Wraps a real dispatch failure from `agent_manager::send_message`
    /// for the node named — same Guardrails/provider error the node's
    /// agent would produce for any other message, just attributed to
    /// which node hit it.
    NodeFailed { node: String, error: agent_manager::ProviderError },
}

impl DagError {
    pub fn error_code(&self) -> &'static str {
        match self {
            DagError::DuplicateNodeId(_) => "E6005",
            DagError::UnknownDependency { .. } => "E6006",
            DagError::CycleDetected => "E6007",
            DagError::UnknownAgent(_) => "E6008",
            DagError::NodeFailed { .. } => "E6009",
        }
    }
}

impl std::fmt::Display for DagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DagError::DuplicateNodeId(id) => write!(f, "{} duplicate task node id \"{id}\"", self.error_code()),
            DagError::UnknownDependency { node, missing } => {
                write!(f, "{} task \"{node}\" depends on unknown task \"{missing}\"", self.error_code())
            }
            DagError::CycleDetected => write!(f, "{} the task graph has a dependency cycle", self.error_code()),
            DagError::UnknownAgent(agent_id) => write!(f, "{} no agent \"{agent_id}\" was provided for this task", self.error_code()),
            DagError::NodeFailed { node, error } => write!(f, "{} task \"{node}\" failed: {error}", self.error_code()),
        }
    }
}

/// Kahn's algorithm: validates the graph (no duplicate ids, no dangling
/// dependency references, no cycle) and returns node ids in an order
/// where every node comes after everything it depends on. Pure and
/// side-effect-free so the graph shape can be validated — and a bad
/// plan rejected — before any agent is ever actually called.
pub fn topological_order(dag: &TaskDag) -> Result<Vec<String>, DagError> {
    let mut seen = HashSet::new();
    for node in &dag.nodes {
        if !seen.insert(node.id.clone()) {
            return Err(DagError::DuplicateNodeId(node.id.clone()));
        }
    }
    for node in &dag.nodes {
        for dep in &node.depends_on {
            if !seen.contains(dep) {
                return Err(DagError::UnknownDependency { node: node.id.clone(), missing: dep.clone() });
            }
        }
    }

    let mut in_degree: HashMap<&str, usize> = dag.nodes.iter().map(|n| (n.id.as_str(), n.depends_on.len())).collect();
    // dependents[x] = nodes that depend on x, so completing x can unlock them.
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in &dag.nodes {
        for dep in &node.depends_on {
            dependents.entry(dep.as_str()).or_default().push(node.id.as_str());
        }
    }

    let mut ready: Vec<&str> = dag.nodes.iter().filter(|n| n.depends_on.is_empty()).map(|n| n.id.as_str()).collect();
    ready.sort(); // deterministic order among independent nodes, not insertion-order-dependent
    let mut ready: VecDeque<&str> = ready.into();

    let mut order = Vec::with_capacity(dag.nodes.len());
    while let Some(id) = ready.pop_front() {
        order.push(id.to_string());
        if let Some(unlocked) = dependents.get(id) {
            let mut newly_ready = Vec::new();
            for &dependent in unlocked {
                let degree = in_degree.get_mut(dependent).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    newly_ready.push(dependent);
                }
            }
            newly_ready.sort();
            ready.extend(newly_ready);
        }
    }

    if order.len() != dag.nodes.len() {
        return Err(DagError::CycleDetected);
    }
    Ok(order)
}

/// Runs every node of `dag` in topological order, feeding each node's
/// already-completed dependency outputs into its prompt as context
/// before dispatching it through `agent_manager::send_message`. Returns
/// every node's output keyed by node id — the caller decides what
/// "aggregation" means for its use case (e.g. concatenate, or dispatch
/// one more synthesis call using the last node's agent), since that's
/// specific to what the plan is for, not something this module can
/// honestly generalize.
///
/// `agents` must contain every `agent_id` referenced by `dag` — checked
/// upfront (`DagError::UnknownAgent`) before any agent is called, same
/// as the graph-shape checks in `topological_order`.
pub fn run(storage: &Storage, dag: &TaskDag, agents: &HashMap<String, Agent>) -> Result<HashMap<String, String>, DagError> {
    let order = topological_order(dag)?;
    let nodes_by_id: HashMap<&str, &TaskNode> = dag.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    for node in &dag.nodes {
        if !agents.contains_key(&node.agent_id) {
            return Err(DagError::UnknownAgent(node.agent_id.clone()));
        }
    }

    let mut outputs: HashMap<String, String> = HashMap::new();
    for id in order {
        let node = nodes_by_id[id.as_str()];
        let agent = &agents[&node.agent_id];

        let mut messages = Vec::new();
        if let Some(system_prompt) = &agent.system_prompt {
            messages.push(ChatMessage { role: "system".to_string(), content: system_prompt.clone() });
        }
        for dep in &node.depends_on {
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: format!("[Context from completed task \"{dep}\"]\n{}", outputs[dep]),
            });
        }
        messages.push(ChatMessage { role: "user".to_string(), content: node.prompt.clone() });

        let reply = agent_manager::send_message(storage, agent, &messages)
            .map_err(|error| DagError::NodeFailed { node: node.id.clone(), error })?;
        outputs.insert(node.id.clone(), reply);
    }

    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, agent_id: &str, depends_on: &[&str]) -> TaskNode {
        TaskNode {
            id: id.to_string(),
            agent_id: agent_id.to_string(),
            prompt: format!("do {id}"),
            depends_on: depends_on.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn orders_independent_nodes_deterministically() {
        let dag = TaskDag { nodes: vec![node("b", "agent-1", &[]), node("a", "agent-1", &[])] };
        assert_eq!(topological_order(&dag).unwrap(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn a_dependent_node_always_comes_after_its_dependency() {
        let dag = TaskDag { nodes: vec![node("summarize", "agent-1", &["research"]), node("research", "agent-1", &[])] };
        assert_eq!(topological_order(&dag).unwrap(), vec!["research".to_string(), "summarize".to_string()]);
    }

    #[test]
    fn a_diamond_shaped_graph_resolves_in_a_valid_order() {
        // plan -> {research_a, research_b} -> synthesize
        let dag = TaskDag {
            nodes: vec![
                node("synthesize", "agent-1", &["research_a", "research_b"]),
                node("research_a", "agent-1", &["plan"]),
                node("research_b", "agent-1", &["plan"]),
                node("plan", "agent-1", &[]),
            ],
        };
        let order = topological_order(&dag).unwrap();
        let pos = |id: &str| order.iter().position(|x| x == id).unwrap();
        assert!(pos("plan") < pos("research_a"));
        assert!(pos("plan") < pos("research_b"));
        assert!(pos("research_a") < pos("synthesize"));
        assert!(pos("research_b") < pos("synthesize"));
    }

    #[test]
    fn rejects_a_duplicate_node_id() {
        let dag = TaskDag { nodes: vec![node("a", "agent-1", &[]), node("a", "agent-1", &[])] };
        assert_eq!(topological_order(&dag).unwrap_err(), DagError::DuplicateNodeId("a".to_string()));
    }

    #[test]
    fn rejects_a_dependency_on_a_node_that_does_not_exist() {
        let dag = TaskDag { nodes: vec![node("a", "agent-1", &["nonexistent"])] };
        let err = topological_order(&dag).unwrap_err();
        assert_eq!(err, DagError::UnknownDependency { node: "a".to_string(), missing: "nonexistent".to_string() });
        assert_eq!(err.error_code(), "E6006");
    }

    #[test]
    fn rejects_a_two_node_cycle() {
        let dag = TaskDag { nodes: vec![node("a", "agent-1", &["b"]), node("b", "agent-1", &["a"])] };
        assert_eq!(topological_order(&dag).unwrap_err(), DagError::CycleDetected);
    }

    #[test]
    fn rejects_a_self_dependency() {
        let dag = TaskDag { nodes: vec![node("a", "agent-1", &["a"])] };
        assert_eq!(topological_order(&dag).unwrap_err(), DagError::CycleDetected);
    }

    fn agent(id: &str) -> Agent {
        Agent {
            id: id.to_string(),
            name: id.to_string(),
            role_template: None,
            system_prompt: None,
            provider_kind: "cloud".to_string(),
            provider_name: "unsupported-provider-for-dag-test".to_string(),
            model: "test-model".to_string(),
            pinned_provider_key_id: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn run_rejects_a_node_whose_agent_was_not_provided_before_calling_anything() {
        let storage = Storage::open_in_memory().unwrap();
        let dag = TaskDag { nodes: vec![node("a", "missing-agent", &[])] };
        let agents = HashMap::new();
        let err = run(&storage, &dag, &agents).unwrap_err();
        assert_eq!(err, DagError::UnknownAgent("missing-agent".to_string()));
        assert_eq!(err.error_code(), "E6008");
    }

    #[test]
    fn run_surfaces_a_real_dispatch_failure_attributed_to_the_node_that_hit_it() {
        let storage = Storage::open_in_memory().unwrap();
        let a = agent("agent-1");
        let dag = TaskDag { nodes: vec![node("a", "agent-1", &[])] };
        let mut agents = HashMap::new();
        agents.insert("agent-1".to_string(), a);

        let err = run(&storage, &dag, &agents).unwrap_err();
        match err {
            DagError::NodeFailed { node, .. } => assert_eq!(node, "a"),
            other => panic!("expected NodeFailed, got {other:?}"),
        }
    }

    #[test]
    fn run_validates_graph_shape_before_dispatching_any_node() {
        // A cycle should be caught before `run` ever tries to call
        // `agent_manager::send_message` for any node — proven by using
        // an agent_id that isn't even in the `agents` map, which would
        // otherwise surface as UnknownAgent instead of CycleDetected if
        // validation didn't happen first.
        let storage = Storage::open_in_memory().unwrap();
        let dag = TaskDag { nodes: vec![node("a", "missing-agent", &["b"]), node("b", "missing-agent", &["a"])] };
        let agents = HashMap::new();
        assert_eq!(run(&storage, &dag, &agents).unwrap_err(), DagError::CycleDetected);
    }
}
