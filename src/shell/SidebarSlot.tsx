import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

/** The DOM node the shell's context sidebar renders into, or null when the
 *  shell has not mounted one. */
const SidebarHost = createContext<HTMLElement | null>(null);

export const SidebarHostProvider = SidebarHost.Provider;

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
export function ShellSidebar({ children }: { children: ReactNode }) {
  const host = useContext(SidebarHost);
  if (!host) return null;
  return createPortal(children, host);
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
