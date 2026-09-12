/** The shell's icon sprite.
 *
 *  One inline `<symbol>` set rendered once at the top of the tree and
 *  referenced as `<Icon name="chat" />`. Inline because `<use href>`
 *  pointing at a separate document does not work in browsers, and in one
 *  place because a second copy is how two screens end up with two
 *  different versions of the same icon.
 *
 *  House rules carried over from the design source: one family, a 20x20
 *  viewBox, 1.5 stroke, round caps and joins, no fills, never an emoji.
 *  The viewBox belongs on the `<symbol>`, not on the referencing `<svg>` —
 *  a `<g>` carries no coordinate system, so 20x20 paths rendered into a
 *  16px box get cropped rather than scaled.
 *
 *  Only the icons the shell itself uses are here. Screens bring their own
 *  as they are ported. */

const PATHS: Record<string, string> = {
  chat: '<path d="M3 5.5A1.5 1.5 0 0 1 4.5 4h11A1.5 1.5 0 0 1 17 5.5v7a1.5 1.5 0 0 1-1.5 1.5H8l-4 3v-3H4.5A1.5 1.5 0 0 1 3 12.5z"/>',
  notes: '<path d="M5 3h7l3 3v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z"/><path d="M12 3v3h3M7 10h6M7 13h4"/>',
  models: '<path d="M10 3 3 6.5 10 10l7-3.5z"/><path d="M3 10.5 10 14l7-3.5M3 14 10 17.5 17 14"/>',
  skills: '<path d="M12.5 3a4 4 0 0 0-3.2 6.4L3 15.7 4.3 17l6.3-6.3A4 4 0 1 0 12.5 3z"/>',
  game: '<rect x="2.5" y="6.5" width="15" height="8" rx="3"/><path d="M6 10.5h2.5M7.25 9.25v2.5M13 10h.01M15 11.5h.01"/>',
  usage: '<path d="M3 17V9M8 17V4M13 17v-5M18 17V7"/>',
  /* Sliders rather than a gear: a gear's teeth mush into a ring at 16px,
     and the circle-with-radial-ticks version reads as a sun. */
  settings:
    '<path d="M4 6h8M16 6h.5M4 10h2M10 10h6M4 14h9M17 14h-.5"/><circle cx="14" cy="6" r="1.8"/><circle cx="8" cy="10" r="1.8"/><circle cx="15" cy="14" r="1.8"/>',
  help: '<circle cx="10" cy="10" r="7"/><path d="M8 8a2 2 0 1 1 2.7 1.9c-.4.2-.7.6-.7 1.1v.5M10 14h.01"/>',
  "panel-left": '<rect x="3" y="4" width="14" height="12" rx="2"/><path d="M8 4v12"/>',
  "panel-right": '<rect x="3" y="4" width="14" height="12" rx="2"/><path d="M12 4v12"/>',
  local: '<rect x="3" y="4" width="14" height="9" rx="1.5"/><path d="M6 16.5h8"/>',
  /* Brought over by the Manual when it was ported. */
  search: '<circle cx="8.75" cy="8.75" r="5.25"/><path d="M12.6 12.6 17 17"/>',
  /* Brought over by Skills. Copied from the design source's own sprite
     rather than redrawn, so the app and the mockups cannot drift. */
  plus: '<path d="M10 4v12M4 10h12"/>',
  check: '<path d="m4 10.5 4 4 8-9"/>',
  copy: '<rect x="7" y="7" width="10" height="10" rx="2"/><path d="M13 7V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h2"/>',
  info: '<circle cx="10" cy="10" r="7"/><path d="M10 9.5v4M10 6.5h.01"/>',
  alert: '<path d="M10 3.5 2.8 16h14.4z"/><path d="M10 8v3.5M10 14h.01"/>',
};

export type IconName = keyof typeof PATHS;

/** Rendered once, near the root. Hidden from layout and from assistive
 *  technology — it is a definition, not content. */
export function IconSprite() {
  return (
    <svg aria-hidden="true" style={{ display: "none" }}>
      {Object.entries(PATHS).map(([name, d]) => (
        <symbol key={name} id={`i-${name}`} viewBox="0 0 20 20" dangerouslySetInnerHTML={{ __html: d }} />
      ))}
    </svg>
  );
}

/** Always decorative. Every place the shell uses an icon also carries a
 *  real accessible name — visible text, or an aria-label on the control —
 *  so announcing the icon too would just repeat it. */
export function Icon({ name, size }: { name: IconName; size?: "sm" | "lg" }) {
  return (
    <svg className={size ? `icon icon-${size}` : "icon"} aria-hidden="true">
      <use href={`#i-${name}`} />
    </svg>
  );
}
