# Amalfi Status Bar

The status cluster at the trailing end of the top edge, as **one component** for every context. The menu bar nests it,
and the lock screen places it exactly where the menu bar keeps it, so nothing moves when the screen locks or unlocks.

Kit: page **Status Bar** of the Amalfi UI kit (sheets `Status Bar / Light`, `Status Bar / Dark`), masters
`Status Bar/…` on the Symbols page. `Menu Bar/Bar/Closed/Rest/<App>` nests `Status Bar/Bar/Menu Bar/Default/<App>`, and
the `Menu Open` bars nest the closed bar. Exports:
[`silica-design/kit/symbols/Status Bar`](https://github.com/silica-os/silica-design/tree/main/kit/symbols/Status%20Bar)
and `kit/pages/Status Bar/`.

## Anatomy

- The bar is 32 tall. The cluster is a row of items, right-aligned. Its trailing edge is 8 from the screen's right
  edge, which is the menu bar's padding.
- Items are 28 tall at y 2, and they touch (gap 0). The 8 pt padding inside each item gives 16 pt between two glyphs.
- **Hit area:** the whole item, extended to the full 32 pt height of the bar, so a pointer at the screen's top edge
  still hits it.
- **Glyphs:** Silica Symbols Regular 20 (ink about 17 pt). Labels are `Body Emphasis` (Inter 13/18, 500). Both are
  centred on the item's vertical axis.
- **Ink:** a single colour per context (see Variants). Items never draw a background, except the Active capsule and
  the plates.

## Items

| Master | Size | Content |
|---|---|---|
| `Status Bar/Item/Glyph/Rest/<App>` | 36 × 28 | padding 8, glyph 20 at x 8, y 4 (text layer `Glyph Status`, override the glyph name) |
| `Status Bar/Item/Glyph/Active/<App>` | 36 × 28 | Rest plus `Menu Bar/Selection` capsule (white 40 % light, 20 % dark), radius 14, the full item; the glyph keeps its ink |
| `Status Bar/Item/Battery/Rest/<App>` | 72 × 28 | label `84%` right-aligned in a 32 box at x 8, 4 gap, `battery` glyph 20 at x 44 (by level: `battery`, `battery-50`, `battery-10`, `battery-0`; `battery-charging` while charging) |
| `Status Bar/Item/Input Source/Rest/<App>` | 80 × 28 | label (`Italian`) at x 8 in a box rounded up to the 4 pt grid, 4 gap, `keyboard` glyph 20 |
| `Status Bar/Item/Clock/Rest/<App>` | 120 × 28 | `Fri 16 Oct  09:41` (two spaces before the time), centred, padding 8 |

The glyph item carries every one-glyph status item. The glyph names are:

| Item | Glyph |
|---|---|
| Bluetooth | `bluetooth` |
| Wi-Fi | `wifi` (its bars draw from `wifi-0/1/2`) |
| Sound | `volume-2` (`volume-0/1` for lower levels) |
| Search (Faro) | `search` |
| Control Center | `control-center` |
| Silica mark (leading end of the menu bar) | `diamond-fill` |

In code, labels hug their text: the battery label is right-aligned so the glyph does not move as the value changes
(`100%` widens the item to the left). The kit rounds label boxes up to the 4 pt grid. The percentage is
`NumberFormat.percentPattern` of the locale, with tabular figures.

### States

| State | Look |
|---|---|
| Rest | ink only |
| Hover | no change (status items have no hover background on the reference) |
| Active (its menu or panel is open, or pressed) | `Glyph/Active`: the `Menu Bar/Selection` capsule, radius 14, full item |
| Off (e.g. Wi-Fi, Bluetooth, sound muted) | the glyph that represents the state (`wifi-slash`, `bluetooth-slash`, `volume-slash`), never a dimmed ink |

Under Reduce transparency the Active capsule becomes opaque: `Surface/Control Pressed` in light, `Surface/Control`
in dark.

## Contexts and order

| Context | Items, leading to trailing | Width in the kit |
|---|---|---|
| Menu Bar | Bluetooth, Wi-Fi, Sound, Battery (with %), Search, Clock, Control Center | 372 |
| Lock Screen | Input Source, Wi-Fi, Battery (with %) | 188 |

- **Menu bar:** the items shown follow the user's Menu Bar settings. Input Source appears when there are two or more
  input sources. The order above is the default.
- **Lock screen:** what appears follows `docs/spec/lock.md` (status items, L-01). The input source opens the input menu
  (`Glass/Thick`, radius 12), hanging from the item.
- **Shared rule:** an item has the same size and the same x from the right edge in both contexts, whenever the item
  sets on both sides of it are the same.

## Variants

| Master | Ink | Legibility aid |
|---|---|---|
| `Status Bar/Bar/Menu Bar/Default/<App>` | `Text/Primary` of the appearance (dark ink in Light, light ink in Dark) | none: the menu bar adapts its appearance to the wallpaper |
| `Status Bar/Bar/Menu Bar/Increase Contrast/<App>` | `Text/Primary` of the appearance | plate `<App>/Glass/Lock Increase Contrast` |
| `Status Bar/Bar/Menu Bar/Reduce Transparency/<App>` | `Text/Primary` of the appearance | plate `<App>/Glass/Lock Reduce Transparency` |
| `Status Bar/Bar/Lock Screen/Default/<App>` | **on-photo white** in both appearances (`Dark/Text/Primary`, `#F4F5F7`); the Dark items are nested | Light: `shadows.onPhoto` (black 40 %, y 1, blur 6) on each item; Dark: none |
| `Status Bar/Bar/Lock Screen/Increase Contrast/<App>` | `Text/Primary` of the appearance | plate `<App>/Glass/Lock Increase Contrast` |
| `Status Bar/Bar/Lock Screen/Reduce Transparency/<App>` | `Text/Primary` of the appearance | plate `<App>/Glass/Lock Reduce Transparency` |

The **plate** is a capsule behind the whole cluster: 28 tall at y 2, as wide as the items, radius 14 (concentric with
the Active capsule). It sits under the items, with no extra padding (each item already has 8).

| Plate | Light | Dark |
|---|---|---|
| Increase Contrast | `#FFFFFFF2`, 1 pt inside `#1D1E2199` | `#2A2B30F2`, 1 pt inside `#FFFFFF99` |
| Reduce Transparency | `#FFFFFF`, 1 pt inside `#0000001F`, rim light `#FFFFFFB3` | `#5B5C5E` (the Glass/Lock tint over `Surface/Window`), 1 pt inside `#FFFFFF3D`, rim light `#FFFFFF38` |

Both plates are opaque. Under Increase contrast the plate also keeps a background blur of 20, which only shows at the
anti-aliased edge (`tokens.json` › `materials` › `Glass/Lock`).

## Tokens

- `colors.<app>.Text/Primary`, `Menu Bar/Selection`, `Surface/Control`, `Surface/Control Pressed`
- `typography.Body Emphasis`; `iconFont` (Silica Symbols Regular, size 20)
- `shadows.onPhoto`
- `materials.Glass/Lock.increaseContrast`, `materials.Glass/Lock.reduceTransparency` (the plates)
- `controlIcons.menuBar` (20)

## Accessibility

- **Sizes:** never smaller than the reference, comparing ink. Glyph ink is about 17 pt, against 11–14 pt measured.
  Labels are 13 pt, as measured. Items are 28 tall, and the hit area is the full 32 pt bar height.
- **Screen reader:** each item reads its name and value ("Battery, 84 percent", "Wi-Fi, connected to <network>",
  "Input source, Italian"), and Active items read as expanded.
- **Keyboard:** the cluster is part of the menu bar's keyboard navigation (left and right arrows move between items,
  Return or Space opens one). On the lock screen, Tab reaches the input source after the help button.

## Provisional

The plate treatment for Increase contrast and Reduce transparency is an Amalfi decision, not observed on the reference.
So are the Lock Screen Light shadow and the keyboard order.
