# Menu bar and menus

The bar at the top of the screen, its app menus and status items, and the menus that open
from it. The gradient behind the bar is in [glass.md](glass.md#menu-bar-gradient-glassmd--menu-bar);
the values below are measured after it. On the grey test wallpaper the bar reads `#6f7075`
at the top.

## Bar (SH-MB-01)

![Menu bar geometry](diagrams/menubar-bar.svg)

- **Height:** 30 pt.
- **No background or border of its own:** the bar is pixel-identical in light and dark.
- **App menu titles:** each title sits in a box about 21–22 pt wider than its text, so there
  are about 10.5 pt on each side.
  - Neighbouring boxes overlap by 1 pt.
  - The first box, the logo, starts 10 pt from the left screen edge.
  - The ink of two adjacent titles is 20–21 pt apart.

| Box (observed order) | x | Width |
|---|---|---|
| logo | 10 | 34 |
| app name (bold) | 43 | 62 |
| title 1 | 104 | 42 |
| title 2 | 145 | 73 |
| title 3 | 217 | 51 |
| title 4 | 267 | 40 |
| title 5 | 306 | 69 |
| title 6 | 374 | 53 |

### Text

- **Colour:** pure white `#ffffff` in both appearances, with no shadow or glow.
- **Size:** capitals are 9.1–9.6 pt tall. With Inter (cap height ratio 0.727) that is
  **13 pt**.
- **Baseline:** y = 19, 11 pt above the bottom of the bar. The capitals are centred about
  0.5 pt above the bar's centre line.
- **Lowercase height:** 6.7–7.0 pt.
- **Weight:** the app name is **bold**. The other titles are regular. Regular and medium
  could not be told apart at 1×.

### Logo

- **Ink:** 14 × 16 pt (x 20–33, y 6–21), centred in its 34 pt box.
- **Colour:** white. Silica draws its own mark here.

### Status area

| Item | Box (x, y, w × h) | Ink size (w × h) |
|---|---|---|
| third-party item | 2185, 3, 47 × 24 | 23 × 18 |
| third-party item | 2230, 2, 42 × 26 | 16 × 16 |
| input source badge | 2270, 3, 46 × 24 | 22 × 16 |
| Wi-Fi | 2323, 4, 22 × 22 | 17 × 12 |
| Faro | 2352, 3, 33 × 24 | 13 × 13 |
| Control Center | 2392, 4, 26 × 22 | 14 × 14 |
| clock | 2434, 4, 106 × 22 | date 61 wide, time 36 wide |

- **Placement:** icons are centred in their boxes within 0.5 pt. The drawings are 4–8 pt
  smaller than the boxes.
- **Spacing:**
  - Icon centres are 42, 42, 41.5, 34 and 36.5 pt apart, from left to right.
  - The boxes are 7 pt apart in the system group, with 16 pt between Control Center and the
    clock.
- **Clock:** fills its box and ends 20 pt from the right screen edge. The date and the time
  are separated by a wider gap of about 8 pt.
- **Colour:** status icons and the clock are **90 % white** (241 over a background of 112).
  App titles and the logo are 100 % white.
- **Clock digits:** 10 pt tall, the same 13 pt size as the titles.
- **Input source badge:** a filled rounded rectangle, 22 × 16 pt, radius about 5. It is
  filled 90 % white, and the letters are cut out so the bar shows through.

### Open title highlight

- **Shape:** while a menu is open its title gets a capsule, 24 pt tall with radius 12,
  centred vertically in the bar.
- **Width:** the box width plus about 7 pt, so about 3.5 pt outside the box on each side.
  - The logo's capsule is 42 pt wide.
  - The other measured capsules are 49.5 and 79.4 pt wide.
- **Fill:**
  - Over the bar's `#6f7075` it reads `#7e7f84`, or `#818287` when the pointer is also on
    the menu (*provisional*: the 3-level difference is unexplained).
  - It is a small material, not a plain overlay: it flattens what lies behind it strongly
    (contrast 175 → 28) and lightens it.
- **Text:** stays white.

## Menus (SH-MB-06…10)

### Geometry

- **Position:**
  - The menu body starts **5 pt left** of the title's box.
  - Its top edge is at **y = 31**, 1 pt below the bar.
- **Size:** the width follows the longest row (label, shortcut and margins). The height is
  the sum of the rows plus the padding.
- **Rows:**
  - Items are **24 pt** tall and separators **11 pt**.
  - A row of colour tags is 49 pt.
  - There are **5 pt** of padding above the first row and below the last.
- **Alternate items:** an item that changes while a modifier is held is the same row,
  showing one variant at a time.
- **Horizontal layout, measured from the body's left edge:**

  | Content | Position |
  |---|---|
  | Text, no icons in the menu | +17 |
  | Icon, when items have icons | +16, 12–14 pt wide |
  | Text, when items have icons | +38 |
  | Checkmark, in a menu with checkmarks and icons | +12…+20 |
  | Icon, in a menu with checkmarks and icons | +29…+40 |
  | Text, in a menu with checkmarks and icons | +49–50 |
  | Last glyph of a shortcut or submenu arrow | 19–20 pt from the right edge |

  A later capture of a menu with checkmarks and 16 pt icons measured the checkmark at +16,
  the icon at about +32 and the text at **+57**.

  In a menu where only some items have icons, items without an icon start their text at
  +16. There is **no shared icon column**: an item with an icon has its text at +39.
- **Text in a row:**
  - The baseline is **17 pt below the row's top**.
  - The capitals are centred in the row within 0.4 pt.

### Border, corners and shadow

- **Hairlines:** the body has a 1 pt **outer** hairline just outside it, and a 1 pt
  **inner** bright hairline just inside.

| | Light | Dark |
|---|---|---|
| Outer hairline over `#7a7b80` | `#5d5d61` (black about 24 %) | `#111112` (black about 86 %) |
| Inner hairline, top | `#f2f2f4` over a fill of `#e9e9ec` | `#86888b` over `#595a5c` |
| Inner hairline, right side | `#cdced2` over `#c3c4c9` | `#818286` over `#6d6e72` |
| Inner hairline, bottom | `#dedfe4` over `#c7c8cd` | `#96979d` over `#626367` |

  The inner hairline is an edge light. It lifts the fill about +23 at the top and bottom in
  light (+36 in dark), and only +6 to +8 at the sides.
- **Corner radius:** **12 pt** on the outer hairline, about 11 on the body (±1).
- **Shadow:** soft, longer below than at the sides. It is offset downwards by about 4 pt
  (*provisional*). Over `#7a7b80`:

| | Light | Dark |
|---|---|---|
| Right side, first pt outside | `#6d6e72` (−11 %) | `#696a6f` (−14 %) |
| Right side, length of the tail | about 16 pt | about 17 pt |
| Bottom, first pt outside | `#67676c` (−16 %) | `#626367` (−20 %) |
| Bottom, length of the tail | about 20 pt | about 20 pt |

### Fill and material

- **Fill over grey `#7a7b80`:** `#c7c8cd` in light, `#6f7075` in dark. The same fill
  measured on a submenu reads `#c8c9ce` / `#707176`.
- **Translucent:** over white content the light fill rises to about `#e5e5e9`–`#f6f6f8`,
  and over dark content the dark fill drops to about `#353536`.
- **Cool tint:** blue is about 5 levels above red.
- **Edge shading:** the fill darkens slightly towards the sides, by about 4 levels over the
  last 12 pt.

**Blur.** Equivalent Gaussian **σ ≈ 9 pt** (±1), w ≈ 0.02–0.04, in both appearances.

- **Long tails:** fitting the edges of wide stripes gives a larger σ (12–16 pt), so the real
  kernel has longer tails than a Gaussian.
- **Weaker at the edges:** within about 25 pt of the side edges the blur looks weaker, or
  the lens stronger.

**Transfer on grey.** It is not linear: the curve is S-shaped, and in dark it peaks at 128
and then drops.

| Input | 16 | 32 | 48 | 64 | 80 | 96 | 112 | 128 | 144 | 160 | 176 | 192 | 208 | 224 | 240 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Light | 150 | 154 | 162 | 170 | 179 | 187 | 195 | 203 | 211 | 218 | 225 | 231 | 237 | 241 | 244 |
| Dark | 57 | 62 | 71 | 80 | 91 | 101 | 109 | 115 | 121 | 125 | 127 | 128 | 128 | 126 | 125 |

- **Plateaus on wide stripes:** black → 156 and white → 243 in light; black → 58 and white
  → 126 in dark.
- **Context effect:** black in the stripes reads 6–8 levels higher than at input 0 on a
  ramp, so the material depends slightly on context (not determined).

Indicative matrix (`out = M · in + b`):

| | M | b | rmse |
|---|---|---|---|
| Light | `[[0.781, −0.200, −0.124], [−0.079, 0.579, −0.034], [−0.166, −0.263, 0.899]]` | `[142, 140, 140]` | 4.1 |
| Dark | `[[0.891, −0.347, −0.113], [−0.150, 0.580, 0.000], [−0.177, −0.391, 1.001]]` | `[44, 47, 45]` | 9.9 |

- **Constrained form:**
  - light: s 1.76, g 0.47, c 141;
  - dark: s 2.36, g 0.44, c 44.
- **Strong saturation boost,** stronger in dark. In dark, a red input comes out redder than
  it went in, while green and blue drop to 11–28.
- **No refraction** was seen at the side edges, beyond 1–2 pt of noise.

### Text

| State | Light | Dark |
|---|---|---|
| Enabled item | black 85 % (`#232323`–`#262626`) | white 85 % (`#e3e2e2`–`#e5e5e6`) |
| Disabled item | black about 26 % (`#b8b9b9`) | white about 26 % (`#716e6f`) |
| Shortcut glyphs | black 26–35 % (`#97989a`–`#a2a2a5`) | white about 30 % (`#86878a`–`#8c8d90`) |
| Highlighted item | white `#ffffff` | white `#ffffff` |

- **Translucent ink:** the text colours are translucent black or white, not fixed greys. The
  alpha stays at 85 % over different backgrounds.
- **Size:** item text is 13 pt (capitals 9.3 pt).
- **Shortcuts:** the same size as the text. Each modifier symbol takes about 9 pt of width;
  the modifiers are right-aligned and the key comes last.
- **Section headers:** 11 pt (capitals 8 pt), black about 26 %, with a slightly heavier
  stroke.

### Item highlight

- **Shape:**
  - The highlight inside a menu is a rounded rectangle, **5 pt in** from both sides of the
    body and as tall as the row (24 pt).
  - Adjacent rows touch, with no gap.
  - The radius is **8 pt** (±1).
- **Colour:** flat **`#007aff`**, the accent colour, the same in light and dark. It has no
  border or shadow.
- **Content on the highlight:**
  - Text and shortcuts turn white.
  - The text starts 12 pt from the highlight's left edge.
  - The last shortcut glyph ends 15 pt from its right edge.

### Separators

- **Line:** 1 pt, in the middle of the 11 pt separator row (5 pt above and 5 below).
- **Extent:** they stop **16 pt** short of each side of the body.
- **Contrast:**
  - light: about 20 levels darker than the fill behind it (black about 10 %);
  - dark: about 20–24 levels lighter (white about 14 %).

### Icons, checkmarks and arrows

- **Item icons:** monochrome, in a box of about 12–14 pt, and the same colour as the item's
  text.
- **App icons** in the recent items submenu are in colour:
  - 14 × 14 pt, 5 pt from the row's top and bottom;
  - 16 pt from the menu's left edge;
  - 9 pt before the text.
- **Checkmark:** about 9 × 9 pt, in the text colour.
- **Submenu arrow:** a chevron 6 × 9 pt, centred vertically. Its right edge is 20 pt from the
  menu's right edge. The stroke is about 1.5 pt.

### Submenus

- **Vertical position:** the submenu's top edge is **5 pt above** its parent row, so its
  first row lines up with the parent row.
- **Horizontal position:** its left edge is 5 pt inside the parent menu's right edge, exactly
  on the right edge of the parent row's highlight.
  - The two menus overlap by 5 pt. The submenu is drawn on top and casts its shadow on the
    parent.
- **Look:** the same radius, hairlines and material as the parent menu.

## Menu behaviour (M-01)

| Action | Result |
|---|---|
| Press on a menu title | the menu **opens on press**, about 118 ms later, in a single frame with no animation. A quick click opens it at about 156 ms |
| Release right after pressing | the menu stays open |
| Press, drag over items, release on one | items highlight under the pointer while dragging; releasing chooses the item |
| Choosing an item | the highlight **blinks once**: off in the next frame (+7 ms), back on at +74 ms, then the menu fades |
| Closing after a choice | fade of about 190 ms, from +114 to about +300 ms after the release |
| Esc | closes with the same 190 ms fade; a single Esc closes everything, even after moving with the arrows |
| Pointer onto another title while a menu is open | the menu switches in one frame, about 80–90 ms after the pointer enters the new title. **User decision (issue #249):** Silica switches at once, as soon as the pointer enters the new title |
| Rest on an item with a submenu | the submenu opens about **295 ms** later, in one frame |
| Moving diagonally towards the submenu across other items | the submenu stays open, as long as the pointer heads towards it |
| Back onto another item of the parent menu | the submenu closes immediately, within about 50 ms |
| ↓ with no item highlighted | highlights the first item; each further ↓ moves down one row |
| Typing a letter | jumps to the first item starting with that letter |
| → / ← | move to the neighbouring menu in the bar; entering the Help menu focuses its search field |

- **The fn key in shortcuts:** shortcuts that need the fn key show its glyph in the modifier
  column.
- **Not observed:** the shortcut that moves keyboard focus to the bar, because the synthetic
  key did not reach it.

## Logo menu (SH-MB-02, SH-MB-04)

- **Geometry:** the same as any menu. It opens at (5, 31), with rows of 24, separators of 11,
  and the first row at y 36. Its width follows the longest row: the logout row, which shows
  the user's full name.
- **Items, in order:**
  1. about this computer
  2. — separator
  3. Settings…
  4. the app store, with a grey capsule showing the number of pending updates
  5. — separator
  6. recent items ▸
  7. — separator
  8. force quit… (⌥⌘⎋)
  9. — separator
  10. sleep
  11. restart…
  12. shut down…
  13. — separator
  14. lock screen (⌃⌘Q)
  15. log out *user*… (⇧⌘Q)

  Only the first three items have icons.
- **With ⌥ held:**
  - "about this computer" becomes the system information item.
  - Restart and shut down lose their ellipsis and act without asking.
  - Log out loses its ellipsis, and its shortcut becomes ⌥⇧⌘Q.
  - Force quit refers to the front app by name.
  - The menu keeps its size.
- **Not measured:** the restart and shut-down confirmation.

## Help menu search (SH-MB-14)

- **Layout:** the Help menu is 360 pt wide. Its first row is 40 pt tall and holds a
  **342 × 26** search field at (+9, +9). The app's own help item follows.
- **Live results:** typing filters as you type, in two sections:
  - **menu items**, shown as their menu path with a small menu glyph;
  - **help topics**, with a question-mark glyph.

  The last row shows all help topics.

## Status menus (M-04)

All of them hang 1 pt below the bar (top edge at y 31) and open in one frame, like app menus.

| Status item | What opens | Geometry |
|---|---|---|
| Wi-Fi | a **Control Center panel**, not a menu: same material as the Control Center detail panels | 308 × 258 pt; right edge 9 pt beyond the icon's right edge |
| Input source | an ordinary menu | 231 × 211 pt; left edge 4 pt left of the item |
| Third-party items | ordinary menus | — |
| Clock | Notification Center ([notifications.md](notifications.md)) | — |

- **Wi-Fi panel, from top to bottom:**
  1. the title with a 54 × 24 switch on the right;
  2. a privacy notice row;
  3. a header and row for personal hotspots;
  4. a header and row for the known network, with its icon in a blue circle when connected
     and a lock;
  5. an expandable "other networks" row;
  6. a separator;
  7. the Wi-Fi settings item.

  Rows are 32 pt tall, with 280 pt of usable width and 14 pt margins.
- **Input source menu, from top to bottom:**
  1. the input sources, each with its label in a small box and a checkmark on the current
     one;
  2. a separator;
  3. show emoji and symbols;
  4. show the keyboard viewer;
  5. a separator;
  6. show the source name;
  7. a separator;
  8. the keyboard settings item.

## Status items and their settings (SH-MB-24…35)

**Settings page.** The menu bar page of Settings contains:

- a pop-up for auto-hiding the bar;
- a switch for a bar background, off by default;
- the number of recent items, 10;
- a switch for suggestions in the controls gallery.

**System items.** A list of system items follows. Each has a checkbox, and some have a "show
when active" pop-up. In order, with their default state:

| Item | Default |
|---|---|
| clock | with its own options |
| Faro | on |
| Wi-Fi | on |
| Bluetooth | off |
| nearby sharing | off |
| Focus | when active |
| screen mirroring | when active |
| display | when active |
| sound | when active |
| now playing | when active |
| fast user switching | off |
| text input | on |
| backup | off |
| timer | when active |
| weather | off |

**Third-party items.** At the end, one switch per app that offers a status item.

**Status items seen in the bar:**

| Item | Size | Shown |
|---|---|---|
| now playing (play glyph in a circle) | 17 × 30 | only while media plays |
| Focus (moon) | 22 × 30, inserted left of the input badge | only while a Focus is on |
| input source badge | 46 × 24 | always |
| Wi-Fi | 22 × 30 | always |
| Faro | 33 × 24 | always |
| Control Center | 26 × 30 | always |
| clock | 108 × 30 | always |

- **Focus menu:** the same panel as the Focus detail in Control Center.
  1. The header shows the title and the active state.
  2. The active Focus row has a filled purple disc. Below it is an inner block of 22 pt rows
     on a tinted band, for "for one hour" and "until this evening".
  3. The other Focus rows follow, then a separator and the Focus settings item.
- **Now playing menu:** a panel of about 308 × 140 pt.
  - On the left, a 112 pt square artwork tile. The app icon is centred in it when there is
    no artwork.
  - At the top right, the app and title with a small waveform glyph.
  - Back, play/pause and forward glyphs.
  - A progress bar with a blue fill and times on both ends: elapsed time, and remaining time
    with a minus sign.
- **Purple-blue dot:** a 4 × 4 pt dot appears at the right end of the bar (core `#312b74`,
  edge `#1f1b44`) **only while** a panel owned by Control Center is open. Its meaning is
  unverified (*provisional*).

## Not measured yet

- **Capsule animation:** how the title capsule appears and disappears.
- **Pressed versus open capsule:** the difference between the two states.
- **Menus near the screen edges:** menus that do not fit, submenus opening to the left, and
  scrolling arrows.
- **Shadow:** above the top edge and on the left side.
- **Material over more wallpapers:** the material over saturated and very bright wallpapers.
