import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import type { Note } from "./types";
import "./Notes.css";

/** A `Note` plus its direct children — built client-side from the flat
 *  list `list_notes` returns (same approach `SemanticSearch.tsx` already
 *  uses for its index list; no need for a recursive SQL query at this
 *  scale). */
type NoteNode = Note & { children: NoteNode[] };

function buildTree(notes: Note[]): NoteNode[] {
  const byId = new Map<string, NoteNode>(notes.map((n) => [n.id, { ...n, children: [] }]));
  const roots: NoteNode[] = [];
  for (const node of byId.values()) {
    if (node.parentId && byId.has(node.parentId)) {
      byId.get(node.parentId)!.children.push(node);
    } else {
      roots.push(node);
    }
  }
  return roots;
}

export default function Notes() {
  const { t } = useTranslation();
  const [notes, setNotes] = useState<Note[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [editTitle, setEditTitle] = useState("");
  const [editContent, setEditContent] = useState("");
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [installingCherryTree, setInstallingCherryTree] = useState(false);

  const tree = useMemo(() => buildTree(notes), [notes]);
  const selected = notes.find((n) => n.id === selectedId) ?? null;

  async function refresh() {
    setLoading(true);
    try {
      setNotes(await invoke<Note[]>("list_notes"));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (selected) {
      setEditTitle(selected.title);
      setEditContent(selected.content);
      setDirty(false);
    }
  }, [selected?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  function toggleExpanded(id: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function handleCreate(parentId: string | null) {
    setError(null);
    try {
      const note = await invoke<Note>("create_note", { parentId, title: t("notes.untitled") });
      await refresh();
      setSelectedId(note.id);
      if (parentId) setExpanded((prev) => new Set(prev).add(parentId));
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleDelete(note: Note) {
    const confirmed = await ask(t("notes.deleteConfirm", { title: note.title }), {
      title: t("notes.deleteConfirmTitle"),
      kind: "warning",
    });
    if (!confirmed) return;
    setError(null);
    try {
      await invoke("delete_note", { id: note.id });
      if (selectedId === note.id) setSelectedId(null);
      await refresh();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleSave() {
    if (!selected) return;
    setSaving(true);
    setError(null);
    try {
      await invoke<Note>("update_note", { id: selected.id, title: editTitle, content: editContent });
      await refresh();
      setDirty(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  async function handleInstallCherryTree() {
    const confirmed = await ask(t("notes.installCherryTreeConfirm"), {
      title: t("notes.installCherryTreeConfirmTitle"),
      kind: "info",
    });
    if (!confirmed) return;
    setInstallingCherryTree(true);
    setError(null);
    try {
      await invoke("install_cherrytree");
    } catch (err) {
      setError(String(err));
    } finally {
      setInstallingCherryTree(false);
    }
  }

  function renderNode(node: NoteNode, depth: number) {
    const hasChildren = node.children.length > 0;
    const isExpanded = expanded.has(node.id);
    return (
      <div key={node.id}>
        <div
          className={node.id === selectedId ? "notes-tree-item active" : "notes-tree-item"}
          style={{ paddingLeft: `${depth * 1.1 + 0.4}rem` }}
        >
          {hasChildren ? (
            <button className="notes-tree-toggle" onClick={() => toggleExpanded(node.id)}>
              {isExpanded ? "▾" : "▸"}
            </button>
          ) : (
            <span className="notes-tree-toggle-spacer" />
          )}
          <button className="notes-tree-title" onClick={() => setSelectedId(node.id)}>
            {node.title || t("notes.untitled")}
          </button>
          <button
            className="notes-tree-add"
            title={t("notes.addChild")}
            onClick={() => void handleCreate(node.id)}
          >
            +
          </button>
          <button className="notes-tree-delete" title={t("notes.delete")} onClick={() => void handleDelete(node)}>
            ×
          </button>
        </div>
        {hasChildren && isExpanded && node.children.map((child) => renderNode(child, depth + 1))}
      </div>
    );
  }

  return (
    <div className="notes-screen">
      <aside className="notes-toc">
        <div className="notes-toc-head">
          <h1>{t("notes.title")}</h1>
          <button onClick={() => void handleCreate(null)}>{t("notes.newTopLevel")}</button>
        </div>
        {loading && <p className="acc-hint">{t("skills.refreshing")}</p>}
        {!loading && tree.length === 0 && <p className="acc-empty">{t("notes.none")}</p>}
        {tree.map((node) => renderNode(node, 0))}
      </aside>

      <div className="notes-content">
        {error && (
          <div className="acc-error" role="alert">
            {error}
            <button onClick={() => setError(null)} aria-label={t("acc.dismissError")}>×</button>
          </div>
        )}
        {selected ? (
          <>
            <input
              className="notes-title-input"
              type="text"
              value={editTitle}
              onChange={(e) => {
                setEditTitle(e.target.value);
                setDirty(true);
              }}
            />
            <textarea
              className="notes-content-textarea"
              value={editContent}
              onChange={(e) => {
                setEditContent(e.target.value);
                setDirty(true);
              }}
            />
            <div className="notes-save-row">
              <button disabled={!dirty || saving} onClick={() => void handleSave()}>
                {saving ? t("notes.saving") : t("notes.save")}
              </button>
              {!dirty && <span className="acc-hint">{t("notes.saved")}</span>}
            </div>
          </>
        ) : (
          <p className="acc-empty">{t("notes.selectOrCreate")}</p>
        )}

        <section className="acc-section notes-cherrytree-section">
          <h2>{t("notes.cherryTreeHeading")}</h2>
          <p className="acc-hint">{t("notes.cherryTreeHint")}</p>
          <button disabled={installingCherryTree} onClick={() => void handleInstallCherryTree()}>
            {installingCherryTree ? t("notes.cherryTreeInstalling") : t("notes.cherryTreeInstall")}
          </button>
        </section>
      </div>
    </div>
  );
}
