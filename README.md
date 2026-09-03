# Multi-AI Agents Panel — UI Design

This branch holds the UI design source of truth for
[Multi-AI Agents Panel](https://github.com/SpaceSquare640/Multi-AI_Agents_Panel):
a static HTML/CSS/vanilla-JS mockup of every screen in the desktop app.

It is a standalone branch with **no shared history** with the `Source_Code`
branch. The two are tracked separately on purpose — the mockup is not part of
the Rust/Tauri build, and nothing here is compiled, imported, or shipped.
Designs are settled here first, then ported into the React components on
`Source_Code` by hand.

## Why this branch exists

The application shipped through v1.6.x with an interface that had grown one
screen at a time, ending in eight flat tabs and a terminal-styled theme applied
late in a single pass. Rather than retheme it again, the interface is being
redesigned from nothing: a new information architecture, a token system with
measured contrast, and a mockup that can be exercised in a browser before any
React component is touched. Keeping that work on its own branch means the
design can move at its own pace without interleaving with application commits.

## Structure

```
index.html        Screen index and theme switcher — start here
style-tile.html   Every token rendered, with live contrast measurement
app-icon.png      The application icon; the brand palette is sampled from it

css/
  tokens.css      Three-layer tokens: primitive -> semantic -> component
  base.css        Reset, typography, focus handling, scrollbars, icons
  shell.css       The four-column application frame
  components.css  Buttons, inputs, cards, dialogs, toasts        (planned)
  screens.css     Per-screen styles                              (planned)

js/
  theme.js        system / light / dark, persisted, applied before paint
  shell.js        Panel collapse, resize, rail keyboard navigation
  palette.js      Command palette                                (planned)
  mock.js         Static sample data                             (planned)

screens/
  shell.html      The application frame
  …               One file per screen                            (planned)
```

There is no build step. Open any `.html` file directly in a browser.

## Live preview

Every push to this branch publishes the mockup to GitHub Pages, so the
interface can be looked at without cloning anything:

**https://spacesquare640.github.io/Multi-AI_Agents_Panel/**

The workflow in `.github/workflows/pages.yml` uploads the branch as-is —
there is nothing to compile, and no dependency is installed. It deploys from
this branch only; `Source_Code` is never checked out by it, and it enables
Pages on its first run rather than needing the setting flipped by hand.

What is published is work in progress. Screens are added a batch at a time,
and `index.html` marks which ones exist yet. The pages are mockups of a
desktop application sized for a window, so a phone browser will show a
cramped frame — that is the design, not a fault.

## Design decisions

**Four-column frame.** A rail of top-level destinations grouped as workspace /
capabilities / system, a contextual sidebar that changes with the rail, the
workspace itself, and an inspector for agent settings, guardrail state, tool
calls and live spend. A title bar and a status bar run across the top and
bottom. The previous eight flat tabs could not express which parts of the app
are places you work versus things you configure.

**The command palette is the second navigation axis.** Every destination and
most actions are reachable from it, which is what keeps the rail short without
burying anything.

**Guardrail state is permanent, not transient.** It sits in the status bar. A
rule that can silently stop being enforced is worse than no rule.

**Colour is sampled, not chosen.** The brand palette comes from the application
icon: after dropping transparent, desaturated and near-black pixels, hue
180–210° accounts for 51.9% of the remaining colour, 255–285° for 19.1%, and
30–60° for 14.8%. Cyan therefore leads, violet marks selection and active
state, gold means attention. Neutrals are a Slate ramp.

**Status colour is reserved.** Green means running or succeeded, red means
failed or destructive, gold means needs attention. None of the three is used
for branding, and the brand cyan never signals status — with several agents
running at once, "the accent colour" and "something is happening" have to stay
distinguishable.

**Both themes are designed together.** Only the semantic token layer changes
between them; component tokens are defined once. Every foreground/background
pair was measured rather than estimated, and the style tile recomputes those
ratios live from the resolved values so the documented numbers cannot drift
away from the CSS.

**13px base, dense spacing.** This is a workstation, not a document. The whole
type scale is derived from a single `--font-scale` token so it can be raised
without touching a component.

## Accessibility

The target is WCAG 2.1 AA, carried over from the application's existing
commitment. In practice that means: text contrast at 4.5:1 and meaningful UI
boundaries at 3:1, both verified in light and dark; focus rings that are never
suppressed; icon-only controls that always carry an accessible name; colour
never used as the only carrier of meaning; and `prefers-reduced-motion`
honoured by collapsing the duration tokens rather than removing transitions, so
nothing that waits on `transitionend` can hang.

Standard controls are 32px tall rather than 44px. That is deliberate: this is a
mouse-and-keyboard desktop application with no touch surface, and 44px
everywhere would cost roughly a fifth of the vertical space in a dense list.
Anything that could plausibly be touched uses the larger size.

## Workflow

Design changes land here **before** they are implemented in the app. Edit the
mockup, review it in a browser in both themes, then port the result into the
React components on `Source_Code`.

## License

Part of the Multi-AI Agents Panel project, licensed under the MIT License. See
the `Source_Code` branch for the full license text.
