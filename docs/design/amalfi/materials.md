# Amalfi materials: glass, blur, surfaces

The official material spec of Silica 1.0 "Amalfi". Values are in `tokens.json` → `materials`, `shadows`,
`colors.*.Glass/*`. This file says **what each material is, how to render it, and where it goes**. When the
UI kit and this file disagree, this file wins and the kit is fixed later.

## The idea

- Windows are **flat and opaque**: regions (content, inspector, toolbar) separated by 1 pt
  `Line/Separator` lines; the sidebar has no line and meets the content with a tone step (2026-10-17). Only the window as a whole has a shadow (`shadows.3`). No glass inside windows
  except the **controls** that sit in the toolbar (the blur strip over a sidebar is not glass).
- Everything that **floats above** windows or the desktop is glass: Dock, menus, popovers, Faro, Control
  Center, notifications, HUDs, tooltips, sheets.
- Glass is a material, not a colour: it blurs what is behind it, lifts saturation, adds a tint and a soft
  rim. Glass is never stacked on glass: items inside a glass surface are plain fills on it (selection,
  pills, rows), never a second blur.
- Interactive glass behaves as a **lens**: when a glass control is pressed or dragged, a `Glass/Clear`
  capsule grows over it and magnifies what is under it.

## The three materials

All three are **real glass**: blur plus refraction at the edge (the shape bends what is behind it), a slight
chromatic split and a specular highlight on the rim. They differ in how much they blur, bend and tint
(user decision 2026-10-16: Regular and Thick were plain background blur; now they are glass in two forms).

| | Glass/Regular | Glass/Thick | Glass/Clear (lens) |
|---|---|---|---|
| Purpose | light objects close to content | panels you read from | transient lens over a control |
| Blur (σ, pt) | 6 | 10 | 2 |
| Refraction: distortion / depth / chromatic aberration | 0.30 / 0.35 / 0.03 | 0.22 / 0.28 / 0.015 | 0.30 / 0.50 / 0.08 + magnification ≈ 1.1× |
| Specular highlight on the rim | yes | yes | yes |
| Saturation | 1.1 | 1.1 | 1.0 |
| Brightness of the backdrop light / dark | ×1.25 / ×1.10 | ×1.12 / ×1.05 | ×1.04 / ×0.98 |
| Tint light / dark | `#FFFFFF` 60 % / `#2A2B30` 60 % | `#FFFFFF` 58 % / `#2A2B30` 58 % | none |
| Inside rim (1 pt) light / dark | `#000000` 12 % / `#000000` 40 % | same | same |
| Rim light (inner shadow 0, 1, blur 1) | `#FFFFFF` 70 % / `#FFFFFF` 22 % | same | same |
| Drop shadow | `shadows.2` (y 4, blur 12) | `shadows.3` (y 12, blur 32) | none |
| Shape | capsule or smooth-cornered rectangle (smoothing 0.6) | same | capsule only |

Regular bends more (small objects: the edge effect must read); Thick bends less and blurs more (large
panels with text on them must stay calm and readable).

Render order for Regular and Thick: backdrop → blur → refraction at the edge (distortion over a band of about `depth` × the shorter side / 2, chromatic split) → saturation and brightness → tint fill → inside rim → rim light
(top inner edge) → content → drop shadow outside the shape. Blur and tint are clipped to the shape's smooth
corners. In the compositor (Ossido) the blur is the existing glass pass ([`docs/m2-glass.md`](../../m2-glass.md),
[`docs/spec/glass.md`](../../spec/glass.md)): reuse its kernel and only change the parameters to these.

In the Sketch kit, light Regular and Thick glass draw the tint as a separate layer (`<name> tint`, style
`Light/Fill/Glass/Tint` or `Tint Thick`) just above the glass layer: Sketch's glass effect drops a white fill.
In code the tint is part of the material, as in the render order above.

Fallback when blur is unavailable (software rendering, reduced transparency setting): replace the blur with
an opaque fill = tint over `Surface/Window`, keep rim and shadow.

## Where each material goes

| Surface | Material | Radius | Notes |
|---|---|---|---|
| Menu bar | **none** (transparent strip on the wallpaper) | — | items are text/glyphs; an open menu title or active status item gets a translucent white capsule `Menu Bar/Selection` (white 40 % light, 20 % dark), radius 12, full item size; text and glyph stay Primary (user decision: blue was not visible enough on wallpapers) |
| Menus, context menus, submenus | Thick | 12 | highlighted item = `Accent/Amalfi` fill, full item width inside the menu padding, radius 7 |
| Status menus (Wi-Fi, Bluetooth, Battery) | Thick | 12 | the kit uses Regular in a few: use Thick |
| Popovers (all edges, detached) | Thick | 16 | the kit's rebuilt popovers lost the glass and look opaque white: use Thick |
| Tooltips | Thick | 8 | |
| Dock shelf | Regular | 20 | tiles are app icons, no glass on tiles |
| Faro, Launcher | Thick | 28 | round buttons beside the Faro bar: same Thick glass |
| Control Center panel | none (transparent container) | — | each module is a Thick tile (toggle modules are capsules, radius = height / 2); detail views are Thick |
| Notification banners | Regular | 18 | the Notification Centre panel is Thick, radius 24 |
| Alerts | Thick (glass variant) or `Surface/Raised` opaque | 20 | system alerts use glass, app alerts in a window use opaque |
| Sheets attached to a window | Thick | 20 | |
| HUDs (volume, brightness), Screenshot bar | Regular | capsule / 20 | |
| Lock screen field, help button, action buttons | **Glass/Lock**: background blur σ 20, saturation 1.1, white 28 % (dark) / 60 % (light), 1 pt light edge, no refraction or shadow; **Increase Contrast** and **Reduce Transparency** are opaque variants (`tokens.json` › `Glass/Lock`) | capsule / circle | see [`lock-screen.md`](lock-screen.md); banners on the lock screen stay Regular; the avatar is an opaque `Accent/Amalfi` disc |
| Lock screen clock digits | **Glass/Clock**: Glass blur 8 with refraction (distortion 0.5, depth 0.6, chromatic 0.05), saturation 1.3, brightness 1.3, white 76 % fill clipped to the glyphs, 1 pt white 80 % line inside, 1 pt black 32 % rim outside | glyphs | only for `Lock Clock`; IC: solid white + 2 pt black 40 % rim; RT: white 92 %, no blur |
| Status Bar (menu bar and lock screen) | **none**: ink on the wallpaper; under Increase contrast and Reduce transparency the cluster sits on a plate of `Glass/Lock Increase Contrast` / `Glass/Lock Reduce Transparency` | plate capsule, radius 14 | see [`status-bar.md`](status-bar.md) |
| Toolbar controls (icon buttons, action segments, search capsule) | Regular | capsule | the toolbar itself is flat (window surface) |
| Title bar section over a sidebar (split title bars) | **blur strip** (not glass): blur σ 10, tint `Surface/Sidebar` 80 %, no refraction, rim or shadow | — | the sidebar rows scroll under it; see `components.md` › Sidebar in a split title bar |
| Window, sidebar, inspector, content | **opaque** `Surface/Window` / `Surface/Sidebar` | window 18 | separators 1 pt, inset as in the layout rules; none between sidebar and content |
| Fields, push buttons, checkboxes, switches at rest | opaque fills (`Surface/Control`, `Accent/Amalfi`) | per control | |
| Pressed segmented control, dragged slider thumb, dragged switch thumb | Clear (lens) | capsule | see the lens rule |
| Media overlays (video controls, Quick preview controls) | Clear | capsule | |

## Lens rule

- Trigger: pointer down on a glass segment, or drag of a slider/switch thumb.
- Shape: `Glass/Clear` capsule as drawn in the kit: 8 pt wider on each side and 12 pt taller above and
  below the control (a 72 × 20 segment gets an 88 × 44 lens); switch lens = thumb + 12 at Regular;
  radius = height / 2 − 1.
- Content under the lens is magnified ≈ 1.1× and refracted; the label/glyph under it stays readable.
- Motion: grows in with `motion.spring.snappy`, follows the pointer 1:1 while dragging, settles into the
  selected segment on release, fades out in 120 ms.

## Surfaces (opaque)

| Token | Light | Dark | Use |
|---|---|---|---|
| Surface/Window | `#FFFFFF` | `#1B1C1F` | window body, content |
| Surface/Sidebar | `#F5F6F8` | `#232428` | sidebars, source lists |
| Surface/Raised | `#FFFFFF` | `#2A2B30` | cards, fields, alerts in windows |
| Surface/Sunken | `#EEF0F3` | `#141518` | wells, slider tracks, segmented tracks |
| Surface/Control | `#FFFFFF` | `#3A3B40` | secondary buttons, segment thumb |
| Surface/Control Pressed | `#E9ECF0` | `#46474D` | pressed secondary controls |

No warm/yellowish greys anywhere: all neutrals are cool-neutral (user decision).

## Dark mode

Same structure, dark tokens. Glass tint switches to `#2A2B30`, rim dark gets stronger (40 %) and rim light
weaker (22 %); shadows are ~2× denser (`shadows.dark`).

## Implementation in Ossido

The compositor's glass pass draws Glass/Regular and Glass/Thick for every glass region a surface
declares (`crates/ossido-render/src/glass_profile.rs`, details and the kind → material table in
[`docs/m2-glass.md`](../../m2-glass.md)):

- The kit's background blur radius is the σ of a CSS `blur()`: σ 6 pt (Regular), 10 pt (Thick),
  2 pt (Clear). A drop shadow's blur is a CSS `box-shadow` blur: σ = blur / 2.
- Each kind of surface of `silica-glass-v1` maps to one material by the table above; the radius
  and shape come from the region the shell declares.
- Ossido keeps its **sharp share**: before the saturation the blurred backdrop is mixed with a
  little of the sharp one, so the glass blurs a little and keeps part of the backdrop sharp. It
  is one parameter per material of a kind and keeps for now the share each kind had before
  Amalfi (0.28 on the Dock, 0.04 on modules and controls, 0.03 / 0.02 on banners, none on the
  other thick glass). Its value is to be tuned against the kit's PNG exports of each material
  once they are exported; until then it is provisional.
- Regular and Thick refract at their edge with the shader's edge lens (`docs/m2-glass.md` ›
  Refraction); no measured edge or outline; the blur radius never animates (a fading surface
  changes the glass's opacity only).
- Reduced transparency is the `opaque` glass quality (`ui.glass_quality`): the tint over
  `Surface/Window`, rims and shadow kept, no saturation, brightness or refraction.
