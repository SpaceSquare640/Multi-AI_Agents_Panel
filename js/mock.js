/* ============================================================================
   Mock data
   ----------------------------------------------------------------------------
   Static sample content so the mockup shows a populated interface rather than
   lorem placeholders. Realistic values matter here: a sidebar full of "Item 1,
   Item 2" hides the truncation, alignment and density problems that only turn
   up with real session titles, model identifiers and six-figure token counts.

   Names come from the role templates the application ships. Nothing here is
   fetched, stored or sent anywhere.
   ========================================================================== */

window.Mock = (function () {

  var agents = [
    { id: 'product-lead',    name: 'Product Lead',        provider: 'Anthropic',  model: 'claude-opus-5',            where: 'cloud', state: 'idle' },
    { id: 'lead-architect',  name: 'Lead Architect',      provider: 'Anthropic',  model: 'claude-opus-5',            where: 'cloud', state: 'running' },
    { id: 'uiux-designer',   name: 'UI/UX Designer',      provider: 'OpenRouter', model: 'google/gemini-2.5-pro',    where: 'cloud', state: 'idle' },
    { id: 'fullstack-dev',   name: 'Full-Stack Developer',provider: 'Anthropic',  model: 'claude-sonnet-5',          where: 'cloud', state: 'running' },
    { id: 'qa-engineer',     name: 'QA Engineer',         provider: 'Ollama',     model: 'qwen2.5-coder:14b',        where: 'local', state: 'idle' },
    { id: 'security-tester', name: 'Security Tester',     provider: 'Ollama',     model: 'llama3.1:8b',              where: 'local', state: 'idle' },
    { id: 'release-manager', name: 'Release Manager',     provider: 'OpenRouter', model: 'deepseek/deepseek-v3',     where: 'cloud', state: 'idle' },
    { id: 'doc-writer',      name: 'Documentation Writer',provider: 'Ollama',     model: 'mistral-small:24b',        where: 'local', state: 'idle' }
  ];

  var sessions = [
    { id: 's1', kind: 'group', title: 'Release review',                    members: 4, updated: '2 min ago',  running: true },
    { id: 's2', kind: 'group', title: 'Architecture spike',                members: 3, updated: '1 h ago',    running: false },
    { id: 's3', kind: 'solo',  title: 'Lead Architect — storage layer',    agent: 'lead-architect',  updated: '12 min ago', running: true },
    { id: 's4', kind: 'solo',  title: 'Full-Stack Developer — composer',   agent: 'fullstack-dev',   updated: '40 min ago', running: false },
    { id: 's5', kind: 'solo',  title: 'QA — flaky CI run on Windows',      agent: 'qa-engineer',     updated: 'Yesterday',  running: false },
    { id: 's6', kind: 'solo',  title: 'Security — dependency audit',       agent: 'security-tester', updated: 'Yesterday',  running: false }
  ];

  var skills = [
    { id: 'raffle-winner-picker', name: 'Raffle winner picker', grants: 2, scopes: ['read: session'] },
    { id: 'repo-summarizer',      name: 'Repository summarizer', grants: 3, scopes: ['read: files', 'net: none'] },
    { id: 'csv-cleaner',          name: 'CSV cleaner',           grants: 1, scopes: ['read: files', 'write: files'] },
    { id: 'web-fetch',            name: 'Web fetch',             grants: 0, scopes: ['net: outbound'] }
  ];

  /* Everything the command palette can reach. Kept as one flat list with a
     group label rather than a nested tree: the palette is a search surface,
     and a tree would only matter if it were ever browsed instead of typed
     into. */
  var commands = [
    { group: 'Go to',   name: 'Chat',                   sub: 'Workspace',    icon: 'i-chat',     keys: 'Ctrl 1' },
    { group: 'Go to',   name: 'Notes',                  sub: 'Workspace',    icon: 'i-notes',    keys: 'Ctrl 2' },
    { group: 'Go to',   name: 'Models',                 sub: 'Capabilities', icon: 'i-models',   keys: 'Ctrl 3' },
    { group: 'Go to',   name: 'Skills',                 sub: 'Capabilities', icon: 'i-skills',   keys: 'Ctrl 4' },
    { group: 'Go to',   name: 'Semantic Search',        sub: 'Capabilities', icon: 'i-search',   keys: '' },
    { group: 'Go to',   name: 'Game Agent',             sub: 'Capabilities', icon: 'i-game',     keys: '' },
    { group: 'Go to',   name: 'Usage',                  sub: 'System',       icon: 'i-usage',    keys: '' },
    { group: 'Go to',   name: 'Settings',               sub: 'System',       icon: 'i-settings', keys: 'Ctrl ,' },
    { group: 'Go to',   name: 'Help',                   sub: 'System',       icon: 'i-help',     keys: 'F1' },

    { group: 'Create',  name: 'New solo session',       sub: 'Pick an agent',        icon: 'i-plus', keys: 'Ctrl N' },
    { group: 'Create',  name: 'New group chat',         sub: 'Pick participants',    icon: 'i-plus', keys: 'Ctrl Shift N' },
    { group: 'Create',  name: 'New note',               sub: 'Notes',                icon: 'i-plus', keys: '' },
    { group: 'Create',  name: 'Import a skill',         sub: 'From a folder or zip', icon: 'i-skills', keys: '' },

    { group: 'Session', name: 'Rename this session',    sub: '',                     icon: 'i-notes',    keys: 'F2' },
    { group: 'Session', name: 'End meeting and summarise', sub: 'Group chat',        icon: 'i-chat',     keys: '' },
    { group: 'Session', name: 'Grant folder access',    sub: 'Choose a folder',      icon: 'i-shield',   keys: '' },
    { group: 'Session', name: 'Delete this session',    sub: 'Cannot be undone',     icon: 'i-settings', keys: '', intent: 'danger' },

    { group: 'View',    name: 'Toggle sidebar',         sub: '',                     icon: 'i-panel-left',  keys: 'Ctrl B' },
    { group: 'View',    name: 'Toggle inspector',       sub: '',                     icon: 'i-panel-right', keys: 'Ctrl J' },
    { group: 'View',    name: 'Switch theme',           sub: 'System, light, dark',  icon: 'i-theme',       keys: '' },
    { group: 'View',    name: 'Change language',        sub: '7 available',          icon: 'i-help',        keys: '' },

    { group: 'Guardrails', name: 'Review the guardrail summary', sub: 'Non-negotiable rules', icon: 'i-shield', keys: '' },
    { group: 'Guardrails', name: 'Show blocked messages',        sub: 'This session',         icon: 'i-shield', keys: '' }
  ];

  // Sessions and agents are reachable from the palette too, so a name typed
  // straight in finds the thing rather than only the command that opens it.
  sessions.forEach(function (s) {
    commands.push({ group: 'Sessions', name: s.title, sub: s.kind === 'group' ? 'Group chat' : 'Solo session', icon: 'i-chat', keys: '' });
  });
  agents.forEach(function (a) {
    commands.push({ group: 'Agents', name: a.name, sub: a.provider + ' · ' + a.model, icon: 'i-models', keys: '' });
  });

  return {
    agents: agents,
    sessions: sessions,
    skills: skills,
    commands: commands,
    usage: { todayCost: 1.18, monthCost: 18.4, monthCap: 20, tokensToday: 482100 }
  };
})();
