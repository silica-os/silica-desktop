# Folder stacks in the Dock

A folder kept in the Dock, right of the separator. Clicking it shows the folder's files in
one of three views: fan, grid or list. Measured with test folders of 3, 10, 14 and 40 text
files, sorted by name, on the grey wallpaper (D-06, K-06, DOCK-FAN, STK-ANIM).

## Defaults

A folder added to the Dock is sorted by **name**, displayed as a **stack**, and its content is
viewed **automatically**.

## The icon in the Dock

- **Stack:** a pile of at most three items of the folder in the stack's sort order (default
  Name; the Downloads folder: Date Added, newest first), the first in front and the others
  behind, offset upward, not rotated, with no extra shadow or darkening.
  - Each item is drawn on a **50 pt canvas** of the default 64 x 80 tile (never the full 60),
    at x = 7 and y = 17 (front), 15 and 13. Everything scales by k = canvas / 60 and rounds to
    whole points.
  - **User decision (issue #244):** the earlier measurement is considered wrong and will not be
    redone. Each sheet further back is drawn 4 % smaller than the one in front of it (1, 0.96,
    0.9216), centred on the same column and keeping the top offsets above.
  - A folder inside the stack uses the same 50 pt canvas.
- **Empty folder, or Display as Folder:** the folder's own icon on the full 60 pt canvas at
  (2, 10).
- **Open stack, fan or grid:** the tile shows the silica-icons "Stack Close" badge
  (`badges.json` > `dock` > `stack-close`, light and dark) on the folder icon's canvas; the shell
  draws no cross of its own.
- **Open stack, list view:** the folder icon stays, darkened as when pressed.

## Automatic view

| Files | View |
|---|---|
| 3, 10 or 14 | fan |
| 40 | grid |

The threshold lies somewhere between 15 and 40 files. It has not been searched for
(*provisional*: Silica switches to the grid above 15, the most a fan can hold).

## Fan

![Fan geometry](diagrams/dock-stacks-fan.svg)

- **Up to 15 files**, with the first in sort order at the bottom, next to the Dock. A 16th
  slot opens the folder in Mustache:
  - its label is "open in Mustache" when the folder has 15 files or fewer;
  - otherwise it reads "N more in Mustache", with N the files not shown.
- **Items (DOCK-FAN, STK-ANIM):** every icon is **64 pt**, the same size as in the Dock. The
  accessibility boxes grow from 65.8 to 82.3 pt only because the icons are rotated.
- **Rotation:** item k (1 at the bottom) is turned clockwise by **θₖ ≈ 0.4° + 1.23° · k**,
  from 1.6° for the first to 12.7° for the tenth. Its label capsule turns by the same angle,
  around the icon.
- **Placement:**
  - The centres are **64 pt apart vertically**.
  - Each centre is shifted right of the one below by **64 · tan θₖ**, so the column bends
    more and more to the **right**. With 10 files the steps are 2, 4, 5, 7, 8, 9, 11, 13, 13
    and 15 pt.
  - The first centre is about 2 pt right of the Dock icon's centre and about 80 pt above
    it. Its box ends 12 pt above the top of the Dock.
- **Equivalent arc:** the centres lie within 0.6 pt of a circle of radius **≈ 2950 pt**. The
  circle's centre is level with the Dock icon's centre, about 2950 pt to its right. Each
  icon's rotation is the angle of its radius, so the fan can also be laid out by turning
  64 pt steps (about 1.24° each) around that far pivot.
- **Labels:** capsules to the **left** of each icon, made of the same material as the Dock's
  hover label.

  | | Light | Dark |
  |---|---|---|
  | Fill over `#7a7b80` | `#bdbec2` | `#828388` |
  | Border (1 pt) | `#424347` | `#434347` |
  | Text | `#212122` | `#f4f4f5` |
- **Hover:** no visible highlight.
- **Opening (STK-ANIM):**
  - It starts on the **release**, not on the press: the column moves about **35 ms after
    the mouse-up**, however long the button was held.
  - It lasts **205 ms** (201–207 ms in four runs), along **cubic-bezier(0.37, 0.24, 0.60,
    1.00)**.
  - The icons keep their full size and opacity throughout. One progress value p drives the
    whole fan: the vertical pitch is p · 64 pt and every rotation is p · θₖ. The column
    therefore starts as an upright stack over the icon, stretches upwards, and bends into the
    arc as it rises.
  - The labels move with their icons and fade in over the same motion.
- **Closing (STK-ANIM):** the same motion in reverse, about **180–210 ms**, along about
  **cubic-bezier(0.5, 0.3, 0.65, 0.9)**. When it starts depends on what closes it:

  | Trigger | Starts | Duration |
  |---|---|---|
  | Esc | on key-down | 180 ms |
  | Click on the stack's own Dock button | on the **release** | 187 ms |
  | Press outside the stack | on the **press** | 211 ms |
  | Click on a file | on the release | about 200 ms |
- **Clicking a file (STK-ANIM):**
  1. On press the item darkens slightly. Nothing else happens until the release.
  2. On release the fan closes at once, as above.
  3. The app shows the file's window when it is ready, with no relation to the close. A
     running app showed it about **150 ms after the release**, while the fan was about 75 %
     closed. An app that had to launch showed it after about **410 ms**, some 200 ms after
     the fan had gone. The window appeared whole, in a single frame.

## Grid

- **Panel:** a titled panel holding a grid of **128 × 128 pt** cells. The title is the
  folder's name.
- **Columns:** ⌈√(n + 1)⌉ for n files. The extra slot opens the folder in Mustache.

  | Files | Grid | Panel |
  |---|---|---|
  | 3 | 2 columns | 290 × 310 pt |
  | 10 | 4 × 3 | 546 × 438 pt |
  | 14 | 5 × 3 | 674 × 438 pt (content 640 × 384) |
  | 40 | 7 × 6 | 930 × 822 pt (scrolling area 896 × 768) |
- **Position:** centred on the Dock icon, except where it would cross a screen edge. The tip
  points down at the icon.
- **Behaviour:**
  - Hovering a cell shows no highlight.
  - A click outside the panel closes it.
- **Opening (STK-ANIM):**
  - It starts on the **release**, about 10 ms after the mouse-up.
  - It lasts **230 ms** (224–234 ms in three runs), decelerating along
    **cubic-bezier(0.21, 0.22, 0.27, 0.99)**.
  - The panel scales **from 0 to 1** around its **tip**, the bottom edge's centre above the
    Dock icon, uniformly in width and height. The tip stays fixed while the panel grows
    upwards and outwards.
  - Its opacity rises with the same progress as the scale.
  - Files whose thumbnail is not ready yet show a dashed rounded square, which is replaced
    when the thumbnail arrives.
- **Closing (STK-ANIM):** the reverse, scaling down towards the tip while fading, in
  **160 ms**. The curve is close to linear with a mild deceleration: about 60 % done at half
  time. Esc closes it on key-down, a click on its Dock button on the release, and a press
  outside on the press, as for the fan.

| | Light | Dark |
|---|---|---|
| Panel glass | almost white, `#f8f8f8`–`#fcfcfc` | clear dark glass, `#707176`–`#73757a` over `#7a7b80`, with a light hairline `#909197` at the top |
| Title and labels | `#252525` / `#212122` | `#f8f8f8` |
| "Open in Mustache" glyph (circle with an arrow) | dark | `#edeef2` |

## List

- **Contents:** an ordinary menu ([menubar.md](menubar.md#menus-sh-mb-0610)).
  - One row per file, each with a small icon and the file's name.
  - A separator.
  - Options ▸, the stack-settings submenu.
  - "Open in Mustache".
- **Position:** the same as the Dock menus: the frame starts 40 pt left of the icon's centre.
- **Height:** with 40 files the menu is 1055 pt tall and does not scroll.
- **Rows:** 24 pt items. The highlight is the accent blue, `#007aff` in sRGB, with white text.

| | Light | Dark |
|---|---|---|
| Fill | `#c7c8cd` | `#6f7075` |
| Text | `#1e1e1f` | `#e9e9ea` |

## Adding a folder by dragging (MU-40, STK-ANIM)

A folder dragged from Mustache over the Dock, to the right of the separator:

- **Gap:** where the folder would be inserted, the Dock opens a gap one cell wide, 63 pt at
  the default size. The Dock stays centred, so each end moves out by **31.5 pt** (measured
  30.5–32 pt).
- **Timing:** the gap opens in **250 ms** (248–256 ms) along about **cubic-bezier(0.3, 0,
  0.7, 1)**. It starts as soon as the pointer reaches an insertion point, within about 50 ms.
- **Leaving:** if the pointer leaves the Dock upwards, the gap closes the same way in about
  **265 ms**, starting about 30 ms after the pointer leaves. It opens again when the pointer
  comes back.
- **Magnification:** while the pointer is over the Dock, the Dock is also magnified, about
  +10 pt per end with the default magnification. The two effects add up. After the pointer
  leaves upwards, the magnified width stays until the drag ends.
- **Insertion points versus folders:** over the middle of a folder already in the Dock, no
  gap opens and the drop goes **into** that folder. A gap opens only at the edges between
  cells.
- **Drop:** the new tile grows in the gap.

## Not measured yet

- Sorting by date or kind.
- Dragging a file out of a stack.
- Scrolling a full grid with the wheel.
- The list view with subfolders.
- The exact threshold between fan and grid in the automatic view.
- Where the grid scales from when the panel is pushed away from a screen edge.
- The drag gap with magnification off or at other Dock sizes.
