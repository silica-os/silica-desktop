# Miscellaneous: shared panel material, alert, context menus, stack, launch bounce, system panels

Measured on macOS 27 (1 pt = 1 px, 2560 × 1440, neutral gray background `#7a7b80`). Saturated
colours are sRGB. Reference: the miscellaneous notes and INV-A4 (SH-HC, SH-NS, SH-AX, SH-AB, SH-LK,
SH-FO) in silica-reference. The Launcher is in [launcher.md](launcher.md); menu and stack tips are
in the tip spec.

## Shared panel material (menus, folder stack, launcher)

| | Light | Dark |
|---|---|---|
| Fill at the centre (on gray) | `#c8c9ce` | `#6e6f74` |
| Outer border (1 pt) | dark, semi-transparent (`#4e4e51`–`#5d5d61`) | nearly black (`#101111`–`#121313`) |
| Inner highlight (1 pt, top and bottom) | `#cbccd1`–`#e1e2e8`, fading into the fill over ~8 pt | `#7c7d81`–`#9a9ba0` |

- The light fill brightens what is behind it; the dark one darkens it slightly.
- A slight horizontal vignette (~15 pt) makes the fill darker near the sides (light `#c3c4c9`).
- The shadow fades out over 30–45 pt (details per object below). Background blur not measured.

## Alert (ref: misc › alert)

![Alert layout](diagrams/misc-alert.svg)

| Property | Light | Dark |
|---|---|---|
| Size (simple alert) | 420 × 132 pt | same |
| Position | centred horizontally, in the upper half of the screen | same |
| Background | `#ffffff`, opaque | `#272727` |
| Border | 1 pt dark line (black at ~25–40 %) | 1 pt `#030303` plus a 1 pt inner `#3d3d3d` |
| Corner radius | ~15 pt (±2) | same |
| Shadow | offset ~10 pt down; ~60 pt below, ~48 right, ~35–40 left, almost none above; up to ~42 % opacity at the edge | denser, similar extent |
| Header | 32 pt tall, then a 1 pt full-width separator (`#e5e5e5`) | separator `#363636` |
| Title | semibold ~13 pt, left-aligned 18 pt from the edge, `#595959` (dimmer than the body) | `#aaabab` |
| Message | ~13–14 pt, 19 pt from the left edge, `#272727` | `#dedede` |
| Buttons | 80 × 24 pt visible, radius ~8, 12 pt apart, right-aligned 20 pt from the right and bottom edges | same |
| Default button | accent fill sRGB `#007aff`, white text; top and bottom edges 1 pt lighter | same accent |
| Other button | `#ececec`, text `#242424` | `#373737`, text `#e0e0e0` |

Not measured: focus and pressed states, appearing animation.

## Desktop context menu (ref: misc › desktop context menu)

| Property | Value |
|---|---|
| Position | left edge at the click x; the first item sits under the pointer |
| Width | ~189 pt (with the outer line) |
| Padding | 6 pt top and bottom |
| Item | 24 pt tall; text ~13 pt starting 17 pt from the edge; light `#272727`, dark `#e9e9ea` |
| Separator | 11 pt tall, 1 pt line inset 17 pt per side; light `#b4b4b9`, dark `#848589` |
| Submenu arrow | "›" ~18 pt from the right edge |
| Corner radius | ~15 pt (±3) |
| Shadow | ~12 pt at the sides, ~22 below, faint above; offset ~5 pt down |

## Dock icon menu (ref: misc › Dock icon menu)

- Same item (24 pt) and separator (11 pt) metrics; text starts ~30 pt from the edge to leave a
  checkmark column; submenu arrows ~25 pt from the right edge.
- Opens above the icon with a tip pointing at it, centred on the icon, ~10 pt above the Dock glass;
  the pressed icon keeps its darkened background while the menu is open.
- Contents (in our words): the app's windows (current one checked), app actions, recent items, an
  "Options" submenu (assign to all desktops / this desktop / a display / none), then show/quit.

## Folder stack, grid (ref: misc › folder stack)

| Property | Value |
|---|---|
| Panel | 802 × 556 pt for a 6 × 4 grid, plus the tip below |
| Corner radius | ~25 pt |
| Tip | centred on the folder icon, ~8 pt deep, ~22 pt base, ~5 pt above the Dock glass |
| Shadow | ~30 pt below, ~32 at the sides |
| Title | the folder name, semibold ~13 pt, centred, 9 pt from the top; light `#1e1e1f` |
| Grid | cells 128 × 128 pt, contiguous; margins left 17, right 16, top 32 (with the title), bottom 11 |
| Icon | ~96 pt nominal (a folder shows 92 × 74), centred, 19 pt from the cell top |
| Label | one line, ~11–12 pt, truncated in the middle with "…", max ~100–110 pt; cap line 109 pt from the cell top; light `#212122`, dark `#f4f4f5` |
| Last cell | an "Open in Mustache" item (arrow in a circle) |

Not measured: selection, opening animation.

## App launch bounce in the Dock (ref: misc › bounce)

![Launch bounce](diagrams/misc-bounce.svg)

| Property | Value |
|---|---|
| Jumps | 2, back to back, then rest |
| Height | ~17 pt above rest (second ~16–17) |
| Each jump | ~0.7 s (rise ~0.3 s, fall ~0.35–0.4 s) |
| Total | ~1.42 s |
| Shape | parabolic, like a thrown body (equivalent acceleration ~280 pt/s²); no squash, no small rebound |
| Running indicator | 4 pt dot under the icon; fades in linearly over ~0.4 s, starting at the peak of the second jump and finishing on landing |

Sample heights (pt above rest vs. time from the start of the first jump, s): 0.00 → 2, 0.04 → 5,
0.08 → 8, 0.13 → 11, 0.16 → 13, 0.21 → 14, 0.29 → 17, 0.44 → 14, 0.53 → 11, 0.59 → 7, 0.65 → 3,
0.69 → 0.

Not measured: delay from click to the first jump; extra jumps for slow apps; the attention bounce.

## Hot corners settings (ref: SH-HC-02)

- A sheet (~711 × 193 pt) with four pop-up buttons (269 × 24) around a 100 × 69 desktop preview
  with corner marks, and a Done button.
- Choices per corner: Panorama, App Windows, Show Desktop, Notification Center, Launcher, Quick
  Note, start screen saver, prevent screen saver, display sleep, lock screen, or nothing.
- Each choice also has variants that require a modifier (Super, Alt, Ctrl, Shift and
  combinations) to be held for the corner to fire.

## Night Light (ref: SH-NS-01)

- Settings sheet (~528 × 316 pt): schedule pop-up, "on until tomorrow" switch, a colour temperature
  slider from less warm to warmer, help and Done.
- The warm shift is applied after composition (display colour tables), so it never appears in
  screenshots. Silica should do the same (post-composition colour transform).

## Accessibility display options (ref: SH-AX-01, 02, 06, 08)

| Option | Effect |
|---|---|
| Reduce transparency | menu bar becomes opaque (light `#dadada`, dark `#202020`); Dock fill opaque (light `#aeafb3`, dark near black); sidebars and selections become solid |
| Increase contrast | darker Dock fill (light `#78797c`, dark `#48484b`), darker switch off-track (`#9f9f9f`) with a 1 pt darker edge, darker separators and selections; menu bar unchanged |
| Show borders | only switches that are on gain a darker accent outline |

- Switch geometry seen there: track 36 × 16 pt, knob 21 pt white.
- Accessibility shortcuts panel (Super+Alt+F5): a floating ~500 × 1104 pt panel centred on the
  primary display over a full-screen dimming layer (~40 % darker); three grouped lists (vision,
  motor, hearing) of 37 pt rows each with a switch, and buttons for keyboard shortcuts, settings and
  Done.

## About this computer (ref: SH-AB-01)

- Window ~314 × 503 pt, only the close button enabled.
- Product image 194 × 194 near the top; the model name on a 28 pt line, then the model detail.
- A table of right-aligned labels and left-aligned values (chip, memory, startup disk, serial
  number, OS version) with 16 pt rows; labels end ~133 pt from the left, values start at ~141.
- A "More info…" button (112 × 26) that opens Settings › General › About, a regulatory link, and a
  copyright line.

## Lock screen and screen saver settings (ref: SH-LK-01, SH-LK-04)

- Lock screen settings: display-off and password delays, show user name and photo, password
  hints, a message when locked, the user switcher style (list of users or name and password),
  and whether sleep / restart / shut down buttons show.
- Screen saver sheet (~500 × 548 pt): a 160 × 100 preview at the top, a start-after pop-up, the
  automatic or custom choice, then scrolling rows of ~109 × 101 thumbnails grouped by theme with
  "Show all".

## Focus settings (ref: SH-FO-02)

- Modes as 52 pt rows with coloured icons and chevrons (do not disturb, work, reduce
  interruptions, sleep, study), an "Add focus…" row, a "share across devices" switch, and a focus
  status row.

## Not measured

- Menu item hover and accent; alert animation; stack opening; the login window. The lock screen
  itself is in [lock.md](lock.md).
