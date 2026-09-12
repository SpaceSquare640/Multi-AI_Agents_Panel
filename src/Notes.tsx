import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import InstallGuidance from "./InstallGuidance";
import { Icon } from "./shell/Icons";
import { ShellSidebar, useIsActiveScreen } from "./shell/SidebarSlot";
import type { Note } from "./types";
import "./styles/screens/notes.css";

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

/** Notes, rebuilt against the v2 design.
 *
 *  The tree moves to the shell's context sidebar and the editor takes the
 *  workspace, which is the arrangement the design uses and the one the
 *  screen already wanted — it was a two-column layout inside a single
 *  pane before.
 *
 *  Two departures from the design. Its sidebar has a search field and
 *  this app has no note search, so there is nothing to wire it to; and
 *  its note body is rendered prose with headings and lists, while the
 *  content here is plain text edited in place. Showing a formatted
 *  preview of text that has no formatting would be inventing a feature,
 *  so the body stays a textarea. The meta line under the title is real:
 *  `updatedAt` and a word count are both derivable from what is already
 *  loaded. */
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
  const headerRef = useRef<HTMLDivElement>(null);
  const isActiveScreen = useIsActiveScreen(headerRef);

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

  function renderNode(node: NoteNode, depth: number) {
    const hasChildren = node.children.length > 0;
    const isExpanded = expanded.has(node.id);
    return (
      <div key={node.id}>
        <div className="tree-row">
          {hasChildren ? (
            <button
              className="twisty"
              type="button"
              aria-expanded={isExpanded}
              aria-label={t(isExpanded ? "notes.collapse" : "notes.expand", { title: node.title })}
              onClick={() => toggleExpanded(node.id)}
            >
              <Icon name="chevron" size="sm" />
            </button>
          ) : (
            <span className="twisty-spacer" />
          )}
          <button
            className="tree-item"
            type="button"
            style={{ "--depth": depth } as React.CSSProperties}
            aria-current={node.id === selectedId ? "true" : undefined}
            onClick={() => setSelectedId(node.id)}
          >
            <Icon name={hasChildren ? "folder" : "notes"} size="sm" />
            <span className="name">{node.title || t("notes.untitled")}</span>
          </button>
          <button
            className="tree-action"
            type="button"
            aria-label={t("notes.addChildTo", { title: node.title || t("notes.untitled") })}
            onClick={() => void handleCreate(node.id)}
          >
            <Icon name="plus" size="sm" />
          </button>
        </div>
        {hasChildren && isExpanded && node.children.map((child) => renderNode(child, depth + 1))}
      </div>
    );
  }

  const wordCount = editContent.trim() === "" ? 0 : editContent.trim().split(/\s+/).length;

  return (
    <>
      {isActiveScreen && (
        <ShellSidebar v2>
          <div className="sidebar-header">
            <span className="label">{t("notes.title")}</span>
            <button
              className="titlebar-btn"
              type="button"
              aria-label={t("notes.newTopLevel")}
              onClick={() => void handleCreate(null)}
            >
              <Icon name="plus" size="sm" />
            </button>
          </div>

          <div className="sidebar-body">
            {loading && <p className="tree-empty">{t("notes.loading")}</p>}
            {!loading && tree.length === 0 && <p className="tree-empty">{t("notes.none")}</p>}
            <div className="tree" role="tree" aria-label={t("notes.title")}>
              {tree.map((node) => renderNode(node, 0))}
            </div>
          </div>
        </ShellSidebar>
      )}

      <div className="workspace-header" ref={headerRef}>
        <span className="workspace-title">{selected ? editTitle || t("notes.untitled") : t("notes.title")}</span>
        {selected && (
          <div className="workspace-actions">
            <button
              className="btn btn-ghost btn-sm"
              type="button"
              aria-label={t("notes.deleteThis")}
              onClick={() => void handleDelete(selected)}
            >
              <Icon name="trash" size="sm" />
              {t("notes.delete")}
            </button>
          </div>
        )}
      </div>

      <div className="workspace-body">
        {error && (
          <div className="callout" data-kind="danger" role="alert">
            <div className="callout-body">{error}</div>
          </div>
        )}

        {selected ? (
          <div className="note-editor">
            <input
              className="note-title"
              type="text"
              aria-label={t("notes.noteTitle")}
              value={editTitle}
              onChange={(e) => {
                setEditTitle(e.target.value);
                setDirty(true);
              }}
            />
            <div className="note-meta">
              <span>{t("notes.edited", { when: new Date(selected.updatedAt).toLocaleString() })}</span>
              <span>·</span>
              <span className="num">{t("notes.wordCount", { count: wordCount })}</span>
              <span className="note-meta-spacer" />
              <span>{dirty ? t("notes.unsaved") : t("notes.saved")}</span>
              <button
                className="btn btn-primary btn-sm"
                type="button"
                disabled={!dirty || saving}
                onClick={() => void handleSave()}
              >
                {saving ? t("notes.saving") : t("notes.save")}
              </button>
            </div>
            <textarea
              className="textarea note-body-input"
              aria-label={t("notes.noteBody")}
              value={editContent}
              onChange={(e) => {
                setEditContent(e.target.value);
                setDirty(true);
              }}
            />
          </div>
        ) : (
          <div className="pane">
            <p className="pane-intro">{t("notes.selectOrCreate")}</p>
          </div>
        )}

        <div className="pane">
          <section>
            <div className="section-head">
              <h2>{t("notes.cherryTreeHeading")}</h2>
            </div>
            <div className="card card-pad">
              <p className="pane-intro">{t("notes.cherryTreeHint")}</p>
              <InstallGuidance
                command="winget install Giuspen.Cherrytree"
                url="https://www.giuspen.net/cherrytree/#downl"
              />
            </div>
          </section>
        </div>
      </div>
    </>
  );
}
