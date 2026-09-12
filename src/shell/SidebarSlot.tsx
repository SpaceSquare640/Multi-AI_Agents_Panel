import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

/** The DOM nodes the shell's two side regions render into, or null when
 *  the shell has not mounted them. */
const SidebarHost = createContext<HTMLElement | null>(null);
const InspectorHost = createContext<HTMLElement | null>(null);

export const SidebarHostProvider = SidebarHost.Provider;
export const InspectorHostProvider = InspectorHost.Provider;

/** Shared by both regions: mark the host while a ported screen fills it,
 *  and portal the children into it.
 *
 *  The marker exists because of the portal. The content is a child of the
 *  screen in the React tree but a child of `.sidebar` or `.inspector` in
 *  the DOM, and CSS only sees the DOM — so the v2 base rules, which are
 *  scoped to `.screen-v2`, do not reach it. Without the marker a ported
 *  screen's own side panel renders in v1's monospace and uppercase while
 *  its workspace renders correctly.
 *
 *  A flag rather than something inferred: a screen can be half-ported and
 *  portal v1 markup here, which has to keep its v1 styling. The flag goes
 *  away with the last `screen-v2` class. */
function useRegion(host: HTMLElement | null, v2: boolean, children: ReactNode) {
  useEffect(() => {
    if (!host || !v2) return;
    host.dataset.v2 = "true";
    return () => {
      delete host.dataset.v2;
    };
  }, [host, v2]);

  if (!host) return null;
  return createPortal(children, host);
}

/** Renders its children into the shell's sidebar region.
 *
 *  A portal rather than a prop passed up through App: the sidebar's content
 *  is owned by the screen (Chat's session tree needs Chat's state and its
 *  handlers), while its position is owned by the shell. Threading that JSX
 *  up through App would mean lifting a large slice of Chat's state with it,
 *  and every screen ported after this one would have to be lifted too.
 *
 *  A portal keeps the React tree exactly as it is — the content is still a
 *  child of Chat for state, context and event bubbling — and only changes
 *  where it lands in the DOM.
 *
 *  Renders nothing when there is no host, so a screen can use this
 *  unconditionally without knowing whether it is inside the shell. */
export function ShellSidebar({ children, v2 = false }: { children: ReactNode; v2?: boolean }) {
  return useRegion(useContext(SidebarHost), v2, children);
}

/** Renders its children into the shell's inspector — the region the v2
 *  layout reserves for "what is this workspace about", opposite the
 *  sidebar's "which one are you looking at". */
export function ShellInspector({ children, v2 = false }: { children: ReactNode; v2?: boolean }) {
  return useRegion(useContext(InspectorHost), v2, children);
}

/** True while this screen is the visible one.
 *
 *  Every screen stays mounted (see App.tsx), so without this each of them
 *  would portal its own sidebar into the same host at the same time and
 *  the shell would show all of them stacked. The visible screen is the one
 *  whose wrapper is not `hidden`, which is a fact about the DOM rather
 *  than something screens have to be told. */
export function useIsActiveScreen(ref: React.RefObject<HTMLElement | null>) {
  const [active, setActive] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const pane = el.closest("[data-screen-pane]");
    if (!pane) return;

    const read = () => setActive(!(pane as HTMLElement).hidden);
    read();

    // `hidden` is a plain attribute toggle, so there is no event for it —
    // MutationObserver is what makes this react to a tab switch at all.
    const mo = new MutationObserver(read);
    mo.observe(pane, { attributes: true, attributeFilter: ["hidden"] });
    return () => mo.disconnect();
  }, [ref]);

  return active;
}
