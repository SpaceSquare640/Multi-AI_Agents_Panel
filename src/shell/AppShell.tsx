import type { ReactNode } from "react";
import { Fragment, useCallback, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon, IconSprite, type IconName } from "./Icons";
import StatusBar from "./StatusBar";
import { SidebarHostProvider } from "./SidebarSlot";
import "../styles/shell.css";

/** The destinations in the icon rail, in their three groups.
 *
 *  The grouping — workspace / capabilities / system — is the whole point
 *  of the redesign. The v1 interface was eight parallel tabs, which could
 *  not say which of them are places you work and which are things you
 *  configure. The system group is pushed to the bottom of the rail for
 *  the same reason.
 *
 *  This is eight destinations, not the design's nine. "Machine Learning"
 *  is one v1 screen that contains both Semantic Search and the Game
 *  Agent, and the design splits it into two rail items. Splitting it here
 *  would give two rail entries pointing at one screen, which reads as a
 *  bug. The split happens when those screens are ported, which is the
 *  step that makes two destinations real. */
type TabId =
  | "chat"
  | "notes"
  | "control-center"
  | "skills"
  | "ml"
  | "usage"
  | "settings"
  | "manual";

type RailGroup = {
  /** i18n key for the group's accessible name. */
  labelKey: string;
  items: { id: TabId; labelKey: string; icon: IconName }[];
};

const RAIL: RailGroup[] = [
  {
    labelKey: "shell.group.workspace",
    items: [
      { id: "chat", labelKey: "shell.nav.chat", icon: "chat" },
      { id: "notes", labelKey: "shell.nav.notes", icon: "notes" },
    ],
  },
  {
    labelKey: "shell.group.capabilities",
    items: [
      { id: "control-center", labelKey: "shell.nav.models", icon: "models" },
      { id: "skills", labelKey: "shell.nav.skills", icon: "skills" },
      { id: "ml", labelKey: "shell.nav.ml", icon: "game" },
    ],
  },
  {
    labelKey: "shell.group.system",
    items: [
      { id: "usage", labelKey: "shell.nav.usage", icon: "usage" },
      { id: "settings", labelKey: "shell.nav.settings", icon: "settings" },
      { id: "manual", labelKey: "shell.nav.help", icon: "help" },
    ],
  },
];

export type { TabId };

export default function AppShell({
  active,
  onNavigate,
  children,
}: {
  active: TabId;
  onNavigate: (id: TabId) => void;
  /** Every screen, all mounted at once. See App.tsx for why they are not
   *  swapped. */
  children: ReactNode;
}) {
  const { t } = useTranslation();

  const [sidebarOpen, setSidebarOpen] = useState(true);

  /** The sidebar host node, published to screens through context.
   *
   *  A callback ref rather than useRef: a ref object's `.current` is set
   *  without re-rendering, so the first render would hand screens a null
   *  host and never tell them it had changed. State makes the host arrive
   *  as a render. */
  const [sidebarHost, setSidebarHost] = useState<HTMLElement | null>(null);

  /** Whether any screen currently has content in the sidebar. Read from
   *  the host itself rather than declared by screens, so a screen that
   *  portals nothing collapses the region without having to say so. */
  const [hasSidebarContent, setHasSidebarContent] = useState(false);
  const observerRef = useRef<MutationObserver | null>(null);

  const attachSidebar = useCallback((node: HTMLElement | null) => {
    observerRef.current?.disconnect();
    setSidebarHost(node);
    if (!node) {
      setHasSidebarContent(false);
      return;
    }
    const read = () => setHasSidebarContent(node.childElementCount > 0);
    read();
    const mo = new MutationObserver(read);
    mo.observe(node, { childList: true });
    observerRef.current = mo;
  }, []);

  /* The region only takes up space when a screen has actually filled it.
     An empty 268px panel with a toggle that reveals nothing is worse than
     no panel; the design source's own frame collapses a region it was
     given no content for, for the same reason. */
  const sidebar = hasSidebarContent && sidebarOpen ? "expanded" : "collapsed";

  /** No screen supplies inspector content yet. It arrives with the screens
   *  that have something to inspect. */
  const inspector = "collapsed" as const;

  return (
    <div className="shell" data-screen={active} data-sidebar={sidebar} data-inspector={inspector}>
      <IconSprite />
      <a className="skip-link" href="#workspace">
        {t("shell.skipToContent")}
      </a>

      <header className="titlebar">
        <span className="titlebar-brand">{t("shell.appName")}</span>
        <span style={{ flex: 1 }} />
        {hasSidebarContent && (
          <div className="titlebar-actions">
            <button
              className="titlebar-btn"
              type="button"
              aria-pressed={sidebarOpen}
              aria-label={t("shell.toggleSidebar")}
              title={t("shell.toggleSidebar")}
              onClick={() => setSidebarOpen((v) => !v)}
            >
              <Icon name="panel-left" size="sm" />
            </button>
          </div>
        )}
      </header>

      <nav className="rail" aria-label={t("shell.primaryNav")}>
        {RAIL.map((group, i) => (
          /* A Fragment, not a wrapper element: .rail is a column flex
             container and .rail-spacer works by being a flex child that
             grows. Wrapping each group in a div would make the div the
             flex child, and the spacer inside it would push nothing. */
          <Fragment key={group.labelKey}>
            {i === RAIL.length - 1 && <span className="rail-spacer" />}
            <div className="rail-group" role="group" aria-label={t(group.labelKey)}>
              {group.items.map((item) => {
                const label = t(item.labelKey);
                return (
                  <button
                    key={item.id}
                    type="button"
                    className="rail-item"
                    // aria-current, not aria-selected: these are
                    // destinations, and the rail is navigation rather
                    // than a tablist.
                    aria-current={item.id === active ? "page" : undefined}
                    title={`${label} — ${t(group.labelKey)}`}
                    onClick={() => onNavigate(item.id)}
                  >
                    <Icon name={item.icon} />
                    <span className="sr-only">{label}</span>
                  </button>
                );
              })}
            </div>
          </Fragment>
        ))}
      </nav>

      {/* Always rendered, never conditionally: screens portal into this
          node, so it has to exist before they can decide whether to. The
          grid collapses its track to 0 when it is empty. */}
      <aside className="sidebar" ref={attachSidebar} aria-label={t("shell.contextSidebar")} />

      <main className="workspace" id="workspace" tabIndex={-1}>
        <SidebarHostProvider value={sidebarHost}>{children}</SidebarHostProvider>
      </main>

      <StatusBar />
    </div>
  );
}
