# Nocturne design system

Nocturne is a quiet, compact interface on the Catppuccin palette — Mocha as dark mode (default), Latte as light mode — Inter at medium weight, soft 8px radii and an accent used as a line and a glow rather than a flood. Rules fade to transparent at their ends — over 48px a side — rather than stopping cleanly; short accent marks stay solid. Contrast comes from the tonal ramps, not from saturation, and photographs blend into the page with their dark values falling away.

## How to use this

- Link the one stylesheet from every page — `<link rel="stylesheet" href="styles.css">` (adjust the relative path) — and take every color, font, spacing, radius and shadow from its variables (`var(--color-*)`, `var(--font-*)`, `var(--space-*)`, `var(--radius-*)`, `var(--shadow-*)`). Never hard-code a hex, a font name or a px value the tokens already carry.
- Build with the classes below rather than inventing parallel ones; the component pages are plain HTML, so view source and copy the markup.
- `templates/` holds starting points a consuming project can copy whole.
- The whole system was derived from `theme.json`. To change the look, edit the tokens at the top of `styles.css` — every page, the thumbnail and this guide read from them — and keep `theme.json` and the written guidance in step so they don't drift from what the CSS actually does.

## Direction

Left-aligned, asymmetric layouts. Flush-left headings; content hugs the left edge with whitespace on the right. Buttons are outlined (1px accent border on transparent), not solid-filled. In decks, section dividers lift to a saturated deep-indigo ground (the `--color-section` tokens — saturation as presence, at slide scale), and the landing template's one full-bleed stat band makes the same presence move at page scale; everywhere else grounds stay desaturated, with soft gradient depth rather than flat fills. Wrap hero and inline images in the `.lighten` class — `mix-blend-mode: lighten` blends them into whatever the page paints behind them: anything darker than the backdrop falls away, so on a dark page a black photo background disappears entirely. Prefer photographs shot on dark or black backgrounds.

## Color

Catppuccin, two flavors: **Mocha** is the default (`:root`), **Latte** applies under `prefers-color-scheme: light` or when `<html data-theme="light">` is set; `data-theme="dark"` pins Mocha regardless of the OS. The raw palette lives in `--ctp-*` tokens (base, mantle, crust, surface0–2, overlay0–2, subtext0–1, text, lavender, mauve, blue, red, green, yellow, peach); every semantic token is defined once against them, so flipping the palette flips every screen.

Roles: `--color-bg` is base, `--color-surface` is surface0, `--color-text` is text, `--color-accent` is lavender (`--color-accent-2` is mauve; treat them as one role). `--color-divider` is text at 16% (Mocha) / 20% (Latte). The neutral ramp follows the ground rather than staying literal: `--color-neutral-900` is always the step nearest the background (crust) and `--color-neutral-100` the farthest (text) — the text-facing steps sit one step farther out than the literal palette order so captions clear AA on surfaces: Mocha 500/400/300 = overlay2/subtext0/subtext1, Latte 500 = subtext1, 400 and 300 = text; 700 is borders and tracks. In Latte `--color-surface` is mantle (surface0 is too dark a panel for small text). The accent ramp is lavender mixed toward text (100–400) or toward base (600–900) in OKLCH; use 200/300 for accent text and hovers, 900 for a dim wash. `--shadow-sm/md/lg` ring with surface1/surface2/overlay0 and carry an ambient shade that lightens in Latte. Use these tokens — never a raw hex.

## Type

Inter for headings over Inter for body text, loaded as `--font-heading` / `--font-body`. Density 0.70× and radius 8px are already baked into the `--space-*` / `--radius-*` scales — use the variables, not raw numbers.

## Icons

Use Phosphor icons (https://phosphoricons.com) throughout.

## Interaction states

Interactive states are themed, never browser defaults: give every interactive element a `:hover` tint and a pressed state from the accent ramp (one step past the base — `--color-accent-600` on a light ground, `--color-accent-400` on a dark one, or a `color-mix()` tint for outlined/ghost variants), and style keyboard focus with `:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 2px; }` — never leave the default blue focus ring.

## Components

| Class | What it is | Shown in |
| --- | --- | --- |
| `.btn` with `.btn-primary`, `.btn-secondary`, `.btn-ghost`, `.btn-icon`, `.btn-block` | Actions — the primary is an accent outline, never a fill | components/buttons.html |
| `.tag` with `.tag-accent`, `.tag-accent-2`, `.tag-neutral`, `.tag-outline` | Small labels tinted from the ramps (mono palette: accent-2 reads the same as accent) | components/buttons.html |
| `.field` + `label`, `.input`, `.radio` + `.dot`, `.seg` + `.seg-opt` | Form fields and choices on native elements — no script | components/forms.html |
| `.card` with `.card-kicker`, `.card-title`, `.card-body`, `.card-meta`; `.elev-sm/md/lg` | Surface-filled content cards; elevation utilities | components/cards.html |
| `.nav` + `.nav-brand` | The header bar | components/navigation.html |
| `.table` | Data tables with themed header and row rules | components/table.html |
| `.dialog-backdrop` + `.dialog` (+ `.dialog-title/-body/-actions`) | A modal at the top elevation | components/dialog.html |
| `.hr` | A horizontal rule — present, but this system prefers whitespace; avoid it | — |
| `.lighten` | The image wrapper — every content photograph goes through it | foundations/image.html |
| `.kbd`, `.keys` | A key cap; `.keys` groups a chord or pair (`Shift` `←` `→`). The only way to draw a key | Video Trimmer 4a–4c |
| `.keyhint` + `.keyhint-label` (`data-p="3"` = drops early) | Key(s) + short label; label hides when space runs out, put the full meaning in `title` | Video Trimmer 4a |
| `.keybar` + `.keybar-row` (`data-mode="handle"`), `.keybar-mode`, `.btn.keybar-more` + `.keybar-more-label` | The always-on shortcut layer in the transport bar; a container that degrades by its own width; the `?` button never drops | Video Trimmer 4a, 4b |
| `.keyref` + `-head/-heading/-grid/-col/-group/-title/-note/-row/-act/-scope` (`data-scope="handle"`) | The full shortcut reference popover, grouped Playback · Navigate · Trim · Actions | Video Trimmer 4c |

States are built in: hovers and pressed states come from the accent ramp, keyboard focus is the 2px accent `:focus-visible` ring, `::selection` is an accent tint, and disabled controls drop to 45% opacity. Don't restyle them per page. The accent-to-ground pair is tuned to at least 3:1 — enough for icons, large text and interface chrome, not for body copy — so for paragraph-size text in the accent use a deep ramp step (`--color-accent-300` on this ground) rather than the accent itself.

## Keyboard shortcuts

One home, two layers. **The key bar** (`.keybar`) sits at the right end of the transport bar and shows at most four keys — Space, ←/→, I/O, and `?` for everything else. Enter and Esc ride on the Trim & save / Cancel buttons themselves as `.kbd` caps. **The reference** (`.keyref`) is a popover anchored above the `?` button: press `?` or click *All keys*; `?` or Esc closes. It is the only complete list.

Degradation is by the key bar's own width (container queries, so it doesn't care why space is short): >360px key + label · ≤360px keys only · ≤236px I/O and the *All keys* label drop · ≤132px only `?`. With the 320px sidebar that is roughly: 1280px window still labelled, ~1160 keys only, ~1040 Space + ←/→, 900 `?` only. Nothing vanishes without a path to it.

Handle mode: when a trim handle has keyboard focus, set `data-mode="handle"` on `.keybar-row` and swap its hints to the handle set (←/→, Shift, Alt) behind a `.keybar-mode` label ("In handle" / "Out handle"); a small tip on the handle itself shows its timecode and Home/End. In the reference, rows that act on the focused handle carry `.keyref-scope[data-scope="handle"]` (solid accent dot); playhead scope is the hollow dot. The Navigate group acts on either, and says so once in its note.

Hint text contrast (WCAG AA needs 4.5:1 at these sizes; key caps sit on transparent so they take the ground beneath — bg in the transport and timeline, surface in the sidebar and the reference):

| Text | Mocha on bg | Mocha on surface | Latte on bg | Latte on surface |
| --- | --- | --- | --- | --- |
| Key cap text `.kbd` (neutral-300) | 9.3:1 | 7.1:1 | 7.1:1 | 6.6:1 |
| Hint label `.keyhint` (neutral-400) | 7.4:1 | 5.6:1 | 7.1:1 | 6.6:1 |
| Reference title / note / scope (neutral-400) | 7.4:1 | 5.6:1 | 7.1:1 | 6.6:1 |
| Handle-mode label `.keybar-mode`, reference rows (text) | 11.3:1 | 8.7:1 | 7.1:1 | 6.6:1 |

Removed — drop from the app:
- The transport-bar chip row (inline-styled Space / , . / I O spans) that was hidden below 1320px. Replaced by `.keybar`.
- The sidebar shortcut hint labels. The sidebar no longer carries hints; Enter/Esc live on its two buttons.
- Inline-styled key caps everywhere (font:500 10.5px mono + divider border). Use `.kbd`.
- Stray bindings from the retired explorations: `,` `.` for frame step, `[` `]` for in/out, `⌘↵` for trim. The set is ←/→, I/O, Enter.

## Product screens (wodeo)

What the shipped app uses — everything else was removed:

- `Video Trimmer.dc.html` — section 3 *Video Trimmer · screens*: 3a Empty · 3b Inspecting · 1a Loaded (portrait) · 3c Loaded (landscape) · 3d Trimming. Section 4 *Shortcuts*: 4a key bar by window width · 4b handle focused · 4c reference open.
- `App Icon.dc.html` — 2a Clip at 256 → 16 px, on dark and light grounds; drawn by `AppIcon.dc.html` (one prop: `size`).
- `Theme Check.dc.html` — Mocha / Latte toggle over both pages.

Retired: Video Trimmer 1b Timecode console and 1c Lens; App Icon 2b Bracket and 2c Lens (and the AppIcon `variant` prop).

## Do

- Keep chroma low outside the accent; lean on the `--color-neutral-*` steps for surfaces, borders and muted text.
- Use the compact spacing scale (density 0.7×) — this system is dense on purpose.
- Outline primary actions and let `:focus-visible` carry the accent.
- Put photographs through the `.lighten` wrapper and prefer subjects shot on dark backgrounds.

## Don't

- Do not flood large areas with the accent or any saturated fill — the exceptions are the deck section-divider ground and the landing template's stat band (both `--color-section`), saturated fields carried as presence (the accent carries its chroma in lines and marks, never as a flood).
- Do not use pure black or pure white — every value comes from the ramps. (Shade is the exception, as in the shadow tokens: ambient darkness mixed from black is a shadow, not a color.)
- Do not stack heavy shadows; on a dark ground elevation is an edge plus ambient darkness.
- Do not bolden headings past their 500 weight — hierarchy here is size and space.

## Files

- `styles.css` — the only stylesheet: the token sheet (`:root` variables, ramps, base type) plus the component layer. Link it from every page.
- `readme.md` — this guide.
- `theme.json` — the parameters these files were derived from (a machine-readable record of the theme).
- `thumbnail.html` — the project cover (brand mark + swatches).
- `foundations/type.html` — the type scale and the heading/body pairing at real sizes.
- `foundations/color.html` — color roles and the 100-900 tonal ramps, with usage notes.
- `foundations/layout.html` — the spacing scale, the grid and how edges are drawn.
- `foundations/icons.html` — the icon set at interface sizes, inline and in buttons.
- `foundations/image.html` — how photographs and figures are treated.
- `components/buttons.html` — buttons, icon buttons and tags in every variant and state.
- `components/forms.html` — text fields, radios and the segmented control on native elements.
- `components/cards.html` — content cards and the elevation steps.
- `components/navigation.html` — the header bar pattern.
- `components/table.html` — a data table with the themed header and row rules.
- `components/dialog.html` — a modal over its backdrop at the top elevation.
- `theme.html` — the theme's parameters rendered as a reference sheet.
- `templates/landing/` — a starter page consuming the system the intended way (`index.html`, its `ds-base.js` loader, and the vendored `image-slot.js` its photograph mounts).
- `assets/photo.jpg` — the reference photograph the imagery page treats.
