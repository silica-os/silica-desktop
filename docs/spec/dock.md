# Dock

Measured with the Dock at the bottom, the size slider at 0.384 (63 pt cells) and
magnification at 0.429, on the grey wallpaper, in light appearance unless noted. Folder
stacks are in [dock-stacks.md](dock-stacks.md); the tip of the label and of the menus is in
[tip.md](tip.md).

## Geometry at rest (D-00)

![Dock geometry](diagrams/dock-geometry.svg)

- **Cells:** each app cell is 63 × 79 pt, with a 63 pt pitch.
  - The cells start at y 1356 and run to the screen's bottom edge, past the visible
    background.
  - The list of cells measured 926 × 79 pt with 12 apps, one folder and the Trash.
- **Icons:** about **48 pt**, centred in the cell, about 16 pt below the background's top and
  16 pt above its bottom. The exact size depends on the icon's drawing.
- **Icon shadow:**
  - It is short and shifted downwards: it darkens about 3 pt below the icon and 1–2 pt at the
    sides.
  - Below the icon the rows read `#5d5e61`, `#77787e`, `#7d7e85`, `#808188`.
  - Radius and opacity cannot be resolved at 1× (*provisional*).
- **Separator:**
  - Its cell is 28.35 pt wide.
  - In its centre is a 1 pt vertical line, 59 pt tall, 10 pt inside the background at the top
    and at the bottom.
  - Colour: light `#484950`, dark `#a9aaaf`.

## Background (D-00)

- **Size and position:** 928 × 80 pt counting antialiasing; it is solid over the 79 pt height
  of the cells.
  - **4–5 pt from the bottom of the screen.** The bottom edge stays 4 pt from the screen edge
    at every Dock size.
  - Centred horizontally.
- **Corner radius:** **22 pt**, circular. A circle fits better than a superellipse. The
  straight part of each side runs from 17 pt below the top to 17 pt above the bottom.
- **Fixed height:** the background never grows taller when the icons magnify. It widens
  instead (see Magnification).
- **No outer shadow.**

| | Light | Dark |
|---|---|---|
| Fill over `#7a7b80` | `#818289` | `#68696e` |
| Top and bottom edge (1 pt, bright) | `#bdbec8` | `#8e8f95` |
| Gradient from the top and bottom edges, 7 pt inwards | `#8e8f97 #898a91 #86878e #84858c #83848b #82838a #818289` | `#707177 #6d6e73 #6b6c71 #6a6b70 #696a6f #696a6f #68696e` |
| Left and right edge (1 pt, dark) | `#3c3c40` | `#2f3033` |

- **Tint:** cool in both appearances, with blue about 8 levels above red.
- **Blur, colour and refraction:** see [glass.md](glass.md#dock-background-glassmd--dock).

## Running-app indicator (D-00, SH-DK-12)

- **Shape and position:** a 4 pt disc, centred under the cell. Its centre is 6 pt above the
  background's bottom edge and about 10 pt below the icon.
- **Colour:** light `#03040b`, dark `#e5e6eb`.
- **Hidden apps:** an app hidden with ⌘H keeps the disc.
- **Dimmed disc:** a grey disc (light `#62636a`, dark `#88898e`) was once seen under an app
  that was only partly running. The cause is unknown (*provisional*).
- **Indicators turned off:** no disc, and the cell geometry does not change.

## Hover label (D-00, D-12)

- **Shape:** a capsule with a tip pointing at the icon. The tip shape is in
  [tip.md](tip.md#shape).
  - The body is 27 pt tall.
  - It is as wide as the text plus about 15–16 pt on each side, for example 55 pt for a
    four-letter name.
- **Position:** centred on the icon. The tip ends about 13–14 pt above the magnified icon's
  top, at any magnification.
- **Text:** about 14 pt (capitals 10 pt), regular weight, centred in the body.
- **Border:** 1 pt `#3c3d41` in both appearances.
- **Shadow:** very weak, about one level.

| | Light | Dark |
|---|---|---|
| Body | `#bdbec3`, top row `#d7d8de`, lower rows `#c1c2c8` | `#7f8086`, top row `#8e8f96` |
| Text | `#1c1c1d` | `#f9f9f9` |

**Timing:**

| Event | Result |
|---|---|
| Pointer enters the Dock | the label appears **with no delay**: first frame at +26 ms, full at +66 ms (a quick fade or scale of 2–3 frames) |
| Pointer moves to the next icon | the label switches **instantly** in one frame, with no cross-fade; it changes at the boundary between the two magnified cells |
| Fast slide over several icons | the label jumps to whichever cell is under the pointer, frame by frame |
| Pointer leaves the Dock | fade of about 250 ms, starting about 40 ms after leaving |
| App launching (bounce) | the label stays and rises with the icon |

## Magnification (D-00)

- **Peak size:** the cell under the pointer reaches **68.0 pt** at the 0.429 setting
  (×1.079), and **132.0 pt** at the maximum setting (×2.095).
- **Cell and icon:** the cell height follows the width, h = w + 16, and the icon scales with
  the cell.
- **Only the pointer's x matters.** Its height within the Dock makes no difference.
- **The icon's bottom edge rises as it grows:** by 1 pt at ×1.079 and by 7 pt at ×2.095.

### Profile

The scale of a cell is s(d) = 1 + (M − 1) · f(d).

- d is the distance from the pointer to the cell's centre **in the resting layout**.
- M is the peak scale.

f is the same at both settings, within 0.001:

| d (pt) | 0 | 16 | 32 | 48 | 64 | 80 | 96 | 112 | 128 | 144 | 160 | 176 | 192 | 208 | 224 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| f(d) | 1.000 | 0.991 | 0.965 | 0.921 | 0.862 | 0.787 | 0.699 | 0.597 | 0.485 | 0.365 | 0.240 | 0.131 | 0.055 | 0.011 | 0.000 |

- **Shape:** the top is flat and the curve falls smoothly to zero at a finite distance,
  between 208 and 224 pt (about 3.5 cells). It is not a Gaussian. Use the table and
  interpolate.
- **Larger Docks:** whether the radius scales with the Dock size was not checked.

![Magnification profile](diagrams/dock-magnification.svg)

### Spreading sideways

- **The point under the pointer stays still.** Cells to its left move left, cells to its
  right move right.
- **Growth per side:** at 0.429 each side grows by up to 9.66 pt.
  - Near an end of the Dock, that side grows less. With the pointer on the first cell, the
    left side does not move and the right side takes all 9.66 pt.
- **The background follows the cells:** it widens by the same amount.
- **At the maximum setting, with the pointer near the centre:** the list spans 1177 pt
  (from 926), and the background grows to match.

### Active zone (D-00, D-01)

| Edge | Behaviour |
|---|---|
| Top, entering | magnification starts with the pointer **5 pt above** the background's top; the threshold is fixed |
| Top, leaving | at the low setting it ends at the same line; at the maximum setting it holds while the pointer stays inside the magnified cell, up to the cell's top |
| Bottom | the pointer cannot leave downwards; magnification holds down to the screen's last row |
| Left and right | the zone is the resting list's span, from about 8 pt before the first cell to about 8 pt after the last; the end cells stay magnified in those 8 pt strips |
| Corners | the zone is rectangular: a corner outside the rounded background already counts |

- **Labels:** none in the 8 pt end strips.
- **No gaps between cells:** the label passes from cell to cell without a gap.
- **Growing near an end at the maximum setting:** the list does not grow past the resting
  left end, so all the growth goes to the other side.
- **Icons above the background:** at the maximum setting the magnified icon sticks out above
  the background, which keeps its resting height.

### Entering and leaving

| Setting | Change in the peak cell | Duration |
|---|---|---|
| 0.429 | 63 → 68 pt | about 55–60 ms |
| maximum | 63 → 132 pt | about 135–160 ms |

- **Duration grows with the amplitude.** Two points are not enough to derive the law.
- **Curve:** entering and leaving share it. It starts gently, runs almost linearly through
  the middle, and arrives gently. There is **no overshoot**.

Progress of the peak cell at the maximum setting:

| t (ms) | 10 | 20 | 30 | 40 | 50 | 60 | 70 | 80 | 90 | 100 | 110 | 120 | 130 | 140 | 150 | 160 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| leaving | 0.000 | 0.021 | 0.075 | 0.122 | 0.211 | 0.295 | 0.387 | 0.483 | 0.579 | 0.673 | 0.760 | 0.838 | 0.903 | 0.959 | 0.991 | 1.000 |
| entering | 0.002 | 0.014 | 0.060 | 0.102 | 0.202 | 0.294 | 0.386 | 0.480 | 0.577 | 0.671 | 0.759 | 0.837 | 0.902 | 0.952 | 0.985 | 0.999 |

### Following the pointer

Inside the zone the peak tracks the pointer directly, with no smoothing or inertia. It lags
by less than one frame at every speed:

| Pointer speed | Median lag of the peak |
|---|---|
| 200 pt/s | 0.7 pt |
| 600 pt/s | 4.1 pt |
| 1500 pt/s | 4.7 pt |

The animation described above applies only when entering and leaving the zone.

## Dock size (SH-DK-size)

The size slider runs from 0 to 1.

| Slider | Cell (pitch × height) | List | Background width |
|---|---|---|---|
| 0 | 18 × 30 | 272 × 30 | 272 |
| 0.125 | 32 × 44 | 474 × 44 | 472 |
| 0.25 | 46 × 58 | 670 × 58 | 668 |
| 0.384 | 63 × 79 | 926 × 79 | 928 |
| 0.5 | 76 × 92 | 1114 × 92 | 1098 |
| 0.75 | 104 × 120 | 1518 × 120 | 1502 |
| 1 | 134 × 150 | 1954 × 150 | 1912 |

The cell is taller than its pitch by 12 pt at the smallest size, and by 16 pt from 0.384
upwards. The extra height is the indicator band.

## Position on the left or right (SH-DK-05, SH-DK-06)

- **Layout:** the Dock turns vertical. Cells are 79 × 63 with the same 63 pt pitch, and the
  separator cell is 79 × 28.35.
- **Background:** 81 × 926 pt, centred vertically, **4 pt** from the screen edge. It has the
  same corners and colours as the bottom Dock.
- **Separator:** a horizontal 1 pt line, 59 pt long, 10 pt inside the background.
- **Running indicator:** on the **screen-edge side** of the cell, 6.5 pt from the outer edge.
- **Which screen:** on the left, the Dock goes to the leftmost display.

## Auto-hide (SH-DK-auto)

- **Hidden:** the Dock sits just below the screen.
- **Reveal:** only the **last row of pixels** reveals it, at any x.
  - Nothing moves for about **185–197 ms**.
  - Then it slides up over about **190 ms**, decelerating.
- **Hide:** when the pointer leaves upwards, it starts at about **+37 ms** and slides down
  over about **175 ms**, accelerating.
- **No fade:** the Dock moves as one piece.
- **After a reveal,** it stays up while the pointer remains on the bottom row, even outside
  the Dock's span.

## Badges (SH-DK-29)

- **Shape:** a red capsule, **23 pt tall**.
  - Its top is 10 pt below the cell's top, about 5 pt above the icon.
  - Its right edge is fixed 3 pt inside the cell's right edge, and it grows to the left.
- **Width by content:**

  | Content | Width (pt) |
  |---|---|
  | 1 or 2 digits (a circle) | 23 |
  | 3 digits | 26 |
  | 4 digits | 33 |
  | a five-letter word | 39 |
- **Colour:** fill **`#f23f34`** with white text, the same in dark. No truncation was seen up
  to 5 characters.

## Download progress (SH-DK-30)

- **Shape:** a thin capsule bar replaces the indicator under a folder that is receiving a
  download. It is **59 × 4 pt**, centred on the cell.
- **Fill:** a 1 pt outline, and a fill growing from the left.
- **Colours:** the indicator's colours, light `#02030a`, dark `#e5e6eb`.
- **No other change:** the folder icon itself does not change.

## Launching an app (D-02)

| Step | Time |
|---|---|
| Press | the icon darkens in the next frame (+7 ms) |
| Bounce starts | about 100 ms after the press (40 ms after the release) |
| Top of the bounce | 17–19 pt high, about 280–400 ms in; it lingers there for about 120 ms |
| Back on the Dock | about 700–800 ms |
| First window | as soon as the app draws it; it can arrive mid-bounce |
| Indicator | appears with the first window |

- **One bounce at a time:** a bounce always finishes landing. Another starts only if the app
  is still launching, so slow apps bounce several times, in whole bounces of about 0.7 s.
- **Shape of a bounce:** the rise and fall are almost symmetric, with no squash.
- **⌘-click on a closed app:** does not open it. It opens a Mustache window on the
  applications folder, with the app selected.
- **⌥-click on a closed app:** opens it and **hides the app that was in front**.
- **Launch animation turned off:** the icon never rises. The Dock widens by one cell
  symmetrically in about 320 ms, and narrows in about 480 ms when the app quits.

## Attention request (SH-DK-attn)

An inactive app asking for attention makes its icon hop in **groups of three decaying hops**,
repeated **every 2.0 s** until the app is activated:

| Hop | Height | Duration |
|---|---|---|
| 1 | 56 pt | about 466 ms (up about 240, down about 226) |
| 2 | 23 pt | about 320 ms |
| 3 | 8 pt | about 214 ms |

After the third hop the icon stays still for about 1 s.

## Clicking a running app (D-03)

| State of the app | Click result |
|---|---|
| In the background, its windows mixed with others | the app activates and **all** its windows come forward, keeping their order; no animation |
| All windows minimised | **one** window comes back, the last one minimised (reverse genie) |
| Hidden | all windows show again and the app activates |
| No windows | up to the app: a document app shows its open panel, the file manager opens a new window |
| Already in front with visible windows | nothing |
| In front, with one visible and one minimised window | nothing; the minimised window stays in the Dock |

## Launch and quit animations (DOCK-ANIM)

When an app that is not kept in the Dock launches, its icon is inserted after the last pinned
app, before the separator; when it quits, it is removed.

**Launch**

- The new cell starts at 10 % of its size (6.3 pt) with its centre fixed, and grows to 63 pt
  in about **350 ms** along cubic-bezier(0.38, 0, 0.64, 1). With magnification it grows to the
  magnified size.
- The neighbours each move exactly half the growth, so the background grows and stays centred.
  The neighbour under the pointer stays magnified while it slides.
- The icon is drawn at the cell's scale, with opacity (scale - 0.2) / 0.8: invisible below
  20 %.
- The 17 pt rise of the bounce (about 18 pt with magnification) is part of the same animation
  and curve. The fall takes about 350 ms, quadratic ease-in, with no settle bounce and no
  squash. One hop is about 705 ms.
- The running dot fades in linearly over about 450 ms (415-480), starting about 255 ms in
  (230-325) and ending at the landing.
- The command to the first change of the Dock takes 49-52 ms (system latency, not reproduced).

**Quit**

- The cell goes from 63 to 0 pt in about **480 ms** (470-490) along cubic-bezier(0.38, 0.05,
  0.61, 1). The icon shrinks in place with opacity = scale squared; the Dock shrinks
  symmetrically.
- The dot fades out over about 370 ms (350-390) from the start.

**Other cases**

- The separator follows the same curves when the running section goes from empty to non-empty
  and back (provisional: not measured on its own).
- Not animated: minimised windows (their own effect), a Dock hidden by auto-hide, and an icon
  dragged out (it fades in place, see Dragging icons).
- Provisional: the reveal of the Dock at login (cannot be captured); at start-up nothing
  animates.

## Context menu (D-04)

The menu opens on a secondary click, a control-click, or a **long press of about 550–600 ms**.

| Element | Items, top to bottom (— = separator) |
|---|---|
| Open app | Options ▸ — Show all windows, Hide, Quit |
| Open app with documents | the app's recent documents — one item per open window — Options ▸ — Show all windows, Hide, Quit |
| Closed pinned app | Options ▸ — Open |
| Options submenu | Keep in Dock or Remove from Dock, Open at login, Show in Mustache — Assign to: all desktops, the desktop on display 1, display 2, none |
| Folder | sort by (name, date added, date modified, date created, kind) — display as (folder, stack) — view content as (fan, grid, list, automatic) — Options ▸ (remove, show in Mustache) — Open the folder |
| Trash | Open — Empty Trash |
| Minimised window | Open the window (one item) |
| Separator | toggle hiding, toggle magnification, position on screen ▸ (left, bottom, right), minimise effect ▸ (genie, scale) — Dock settings… |

- **Rows:** 24 pt items and 11 pt separators. Group titles in folder menus are grey and
  cannot be chosen.
- **⌥ held:**
  - Hide becomes "hide others" and Quit becomes "force quit", in the same rows.
  - On a closed app, Open's alternate is "force quit".
- **Position:** the menu's frame always starts **40 pt left of the icon's centre**, so the
  menu opens towards the right.
  - The visible body starts 13 pt inside that frame, so the tip sits 27 pt from the body's
    left edge.
  - The frame's bottom edge is at the top of the Dock zone, and the tip ends about 6 pt above
    it.
- **The icon while the menu is open:** it stays pressed (darkened) and magnified.
- **Long-press menu:**
  - It appears in one frame, with no fade.
  - Releasing over the icon closes it with a fade of about 150 ms.
  - A menu opened with a secondary click stays open after the release.

## Dragging icons (D-05)

- **Start threshold:** about **5 pt** of movement in any direction. A long press wins if the
  pointer has not moved first.
- **Dragged image:** the icon at its magnified size, opaque, without the pressed darkening,
  held at the grab point. The label disappears.
- **Gap:**
  - The original place closes at once, and a gap opens where the icon would land.
  - When the gap moves by one cell, the neighbour slides over in **about 265 ms** along an
    S-curve. Positions per frame: 0, −1, −3, −7, −11, −16, −23, −29, −36, −43, −55, −60, −66,
    −67 pt.
- **Dropping inside the Dock:** the icon slides into the gap. An open app dropped among the
  pinned apps becomes pinned.
- **Removing:**
  - Dragging above the Dock closes the gap at once.
  - A **"Remove" capsule**, in the label's shape, appears above the icon once the pointer is
    125–135 pt above the grab point, about 90–100 pt above the Dock.
  - Dropping there:
    - an element that is not running **fades in place** in about 250–290 ms, with no puff and
      no scaling, and the Dock closes up;
    - a running app goes back to its place, unpinned, and disappears when it quits.
- **Folder dragged in from Mustache:** to the right of the separator it opens a gap and is
  inserted. The gap's size and timing are in
  [dock-stacks.md](dock-stacks.md#adding-a-folder-by-dragging-mu-40-stk-anim).
  - Hovering it over another folder in the Dock highlights that folder instead.
- **File held over an app:**
  - The app's icon darkens, as when pressed, and its label shows. No gap opens.
  - Dropping opens the file with that app.
  - The dragged image is the file's thumbnail, with its name in a blue capsule below.
- **File held over the Trash:** the Trash darkens and shows its label. Dropping moves the
  file to the Trash.

## Trash (D-07)

- **Menu:** Open — Empty Trash.
- **Empty Trash alert:** "Empty Trash" opens a separate alert window, not a sheet.
  - The window is **292 × 268 pt**, centred horizontally on the screen.
  - It shows a 66 pt Trash icon, a two-line title (250 pt wide) and a secondary text.
  - Two full-width buttons of **262 × 30 pt** are stacked: the destructive action on top,
    Cancel below.
- **Not observed:** the empty Trash icon, and the Trash turning into an eject target.

## Minimised windows and recent apps (D-08)

- **Placement:** minimised windows sit right of the separator, after the folders and before
  the Trash, in the order they were minimised. The newest sits next to the Trash.
- **Cell:** 63 × 79 pt.
  - A window thumbnail of about **57 × 47 pt** is centred in it.
  - The app's icon, about 18 pt, overlaps the thumbnail's bottom-right corner and sticks out
    about 4 pt below it.
- **Growth:** the Dock grows by 63 pt per window, symmetrically, and **the icons do not
  shrink**, at least up to 8 windows.
- **Hidden apps:** a hidden app's minimised windows leave the Dock while it is hidden, and
  come back with it.
- **"Minimise into the app icon":** with this option on there are no thumbnails. The
  windows are listed in the app's menu, and a click on the icon restores only the last one.
- **"Show suggested and recent apps":** a **second separator** appears after the pinned apps.
  - Between the two separators are the open, unpinned apps and recently closed ones, up to 3
    seen.
  - With the option off, open unpinned apps sit left of the only separator.

## Minimise effects (SH-DK-09)

- **Scale:**
  - The window shrinks uniformly towards its new Dock cell, keeping its aspect ratio, with no
    deformation and no fade.
  - The Dock widens by one cell at the same time.
  - It lasts about **186 ms** above the Dock and about 213 ms in total; restoring takes the
    same time.
- **Genie:** about 390 ms to minimise and 450 ms to restore. See [windows.md](windows.md).

## Not measured yet

- The empty Trash icon, and the change from full to empty.
- Dropping a file on an app that does not accept it.
- Clicking an app whose windows are on other spaces.
- Whether the magnification radius scales with the Dock size.
