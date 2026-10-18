# Silica 1.0 "Amalfi" design system → UI 1.0.0

Amalfi is Silica's own design system: **behaviour and layout of the reference desktop, our own look and
our own measures**. This folder is the hand-off from the design kit to code. UI 1.0.0 is the release in
which the shell (`shell/`), the compositor's glass (`crates/ossido`), `silica_ui` and every app use it.

| File | What |
|---|---|
| [`tokens.json`](tokens.json) | every colour (light, dark, per-app palettes), type style, spacing, control sizes, radii, shadows, materials, window controls |
| [`materials.md`](materials.md) | Glass/Regular, Glass/Thick, Glass/Clear (lens), opaque surfaces: parameters, render order, **where each one goes** |
| [`motion.md`](motion.md) | spring and fade tokens, rules, the animation of every surface re-derived for Amalfi |
| [`components.md`](components.md) | binding layout metrics, the full component inventory, Faro and Launcher specs |
| [`lock-screen.md`](lock-screen.md) | the lock screen: screen layout rule, masters and states, Glass/Lock, motion (lock, unlock, wake, shake, minute roll), accessibility, provisional values |

## Sources of truth, in order

1. The user's decisions (listed below).
2. These four files.
3. The Sketch UI kit `~/Library/Mobile Documents/com~apple~CloudDocs/Progetti/Silica/Silica 1.0 Amalfi UI Kit.sketch`
   (document id `DEB2F9E8-966A-4E25-A6AF-6BAF189DD87E`, ~52 pages, 3015 symbols) for exact geometry of
   every component and screen.
4. `docs/spec/*.md` for **behaviour and timing** only (they are measurements of the reference; their sizes
   and colours are replaced by Amalfi's).

Icons: system symbols come from the **Silica 1.0 Amalfi Icon Kit** (`…/Silica/Silica 1.0 Amalfi Icon
Kit.sketch`, id `A14E5E90-0F0B-4291-8A35-897B3221F260`; repo copy `silica-icons/design/symbols.sketch`):
972 symbols `<Light|Regular|Semibold> / <name>` at 24 pt, with stroke styles per size
`<Weight>/<12–32>/<Role>`: the stroke scales linearly with the glyph size (Regular 24 → 1.5, 16 → 1.0, 12 → 0.75; Semibold 12 → 1.076), i.e. plain vector scaling of the 24 pt SVG. App, file
and folder icons come from `silica-icons` as before; **the user is redrawing all app icons by hand now:
do not integrate icon changes (the stop on silica-icons v0.7–v0.8 still holds)**.

## User decisions (binding)

- Look and behaviour: same layout and behaviour as the reference desktop, design and measures our own.
  Never copy measured sizes from `docs/spec` into Amalfi components; the only exception is the window
  controls (traffic lights), which keep the measured values of `docs/spec/windows.md` (light glass).
- Vivid palette on white and neutral grey surfaces; **no warm or yellowish greys**. Accent Amalfi
  `#0E77E6` light / `#3D95FF` dark.
- Windows are flat, split into regions by 1 pt separators; only the window has a shadow.
- Title bars and toolbars are built from modular pieces (`Titlebars/Piece/*`), split (2 or 3 sections:
  sidebar · content · inspector) or unified. Toolbar items are glass controls: action segments and 32 pt
  icon buttons.
- One control scale everywhere: 20 / 24 / 28 / 32 (32 = toolbar). Never resize a control inside a bar.
- 4 pt grid, no half points.
- Each app may have its own palette and appearance (light + dark, or a single one), as
  `colors.<light|dark>["App/<App>/<Token>"]` in `tokens.json`.
- Faro is never wider than 640 pt; the Launcher keeps its own 844 × 580 panel. The four Faro buttons appear only after the pointer
  enters the empty bar and disappear while typing, with results and inside a scope (`components.md`).
- Pointers: the classic Adwaita cursor theme, no Amalfi cursors.
- Inter for UI, JetBrains Mono for monospaced text. No reference-vendor fonts or names in code, assets or
  strings.

## How to read the kit (read-only)

Use the Sketch MCP tools on the document id above. **Never write to the kit, never open any document named
"Apple … UI Kit" (licence: forbidden, even to read its structure).**

- `get_layer_tree_summary` on a symbol master gives frames, stacks, text; `get_screenshot` gives the
  pixel target for a golden test.
- `run_code` read-only scripts: `doc.getSymbols()` (names in `components.md`), `layer.style` (fills,
  borders, shadows, `blurs`, `corners.radii`, `corners.smoothing`), `layer.sharedStyle.name`
  (Glass styles and text styles), `layer.frame`.
- Swatches are named `<Light|Dark>/<token>` and text styles `<Light|Dark>/<Style>/<Colour>`; they are
  already exported to `tokens.json`.

## Work plan for UI 1.0.0

1. **Tokens in code.** Generate the token classes in `silica_ui` from `tokens.json` (a small generator
   committed with it, not hand-copied): `AmalfiColors.light/dark` incl. app palettes, `AmalfiType`,
   spacing, radii (smooth corners, smoothing 0.6), shadows, focus halo, control sizes, motion tokens.
2. **Materials.** `GlassRegular`, `GlassThick`, `GlassClear` (lens) in `silica_ui`, and the compositor's
   glass pass parameters in Ossido, exactly as `materials.md`; reduced-transparency fallback.
3. **Every component.** One widget per kit family (Buttons, Fields, Toggles, Segmented, Menus, Popovers,
   Sliders, Steppers, Progress, Scrollbars, Sidebars, Titlebars/Pieces/Toolbars, Windows, Alerts,
   Dialogs, Forms, Group Boxes, Image/Color Wells, Tooltips, Disclosure, Notifications, Control Center,
   Menu Bar, Dock) with **every variant, size and state** in the inventory, light and dark.
   For each one: read the master, implement, add a golden test against the kit screenshot, and a
   gallery page. Keep a checklist (one line per symbol family × variant) in the repo and tick it.
4. **Shell.** Menu bar, Dock, Faro + Launcher, Control Center, notifications, app switcher, Mission
   Control/Panorama, Screenshot UI, window decorations (pieces and window controls), rebuilt
   on the components.
5. **Motion.** Re-derive every animation with `motion.md`; keep measured timings, re-scale amplitudes.
6. **Apps.** Mustache, Mail, Terminal, Calculator, Clock, Settings, Preview, Notes, Text Editor,
   Screenshot, Mission Control: rebuild their UI from the kit's `App · <name>` pages (all screens there),
   with their app palettes.
7. **Release.** `silica_ui` tag, shell `pubspec.yaml` ref, VM check of every surface in light and dark,
   then UI 1.0.0.

Every step follows `AGENTS.md` (clean-room, never crash, quality gates, `ASSETS.md` rows) and is checked
in the VM (`cargo xtask os start`, `docs/vm.md`) before it is pushed.

## Known differences in the kit to fix in code (not in the kit)

- Popovers: some masters lost their glass and look opaque white → use Glass/Thick.
- Status menus: a few use Glass/Regular → use Glass/Thick.
- Lens extends 8 pt above and below (earlier notes said 6).
- Screenshot capture bar is 496 × 48, 8 pt above the Dock (not the reference size).
- Control Center detail panels are 312 wide.
- Some icon glyphs at 8 and 10 pt still carry the 24 pt stroke in the Icon Kit: use the 12 pt stroke
  rule scaled (8 → 0.5, 10 → 0.625).
