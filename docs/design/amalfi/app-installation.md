# Amalfi app installation

The hand-off spec for the surfaces of the app-installation system (silica-os#3, tracked in silica-desktop#259):
Gatekeeper alerts, the `.sxv` volume window in Mustache, the Security settings, and the small marks in the Launcher,
Dock and Settings. It replaces every `PROVISIONAL(amalfi-gatekeeper)` value in code.

- **Behaviour comes from the reference:** verdicts, choices and flows are in [`docs/spec/trust.md`](../../spec/trust.md)
  and [`docs/spec/volumes.md`](../../spec/volumes.md), and the `.silica-window.toml` format is in silica-os
  `docs/app-bundles.md` § 5. Those files are binding for behaviour.
- **Layout and look are Amalfi:** the 4 pt grid, the type scale, the components and the colour tokens of this folder.

**Kit:** pages **Alerts** (`Gatekeeper / Light|Dark`), **App · Mustache** (`Volume window`, `Sidebar with a volume`),
**App · Settings** (`Privacy & Security › …`, `General › Storage, apps`, `Desktop & Dock › Default apps`),
**App · Faro** (`Launcher, new app`) and **Menu Bar and Dock** (`Dock, missing app`), each in Light and Dark. The masters
named below are on the Symbols page.

**Exports:** `silica-design/kit/symbols/<master name>/<Light|Dark>.json` and `@2x.png`, and `kit/pages/<Page>/<Artboard>.png`.

The example app is "Lemon" by "Marta Rossi", from `lemon.example.org`, on the volume "Lemon 2.1". In the kit, Lemon
wears the Notes icon as a stand-in.

## 1. Gatekeeper alerts

Masters `Alerts/Gatekeeper/<Variant>/Rest/<Light|Dark>`. They are system alerts that the shell draws for
`PromptRequested` and for the Mustache and Dock prompts, centred on the screen. They are opaque, not attached to a
window, and built on the kit's alert (`Alerts/Alert/*`).

### Anatomy (all variants)

| Part | Value |
|---|---|
| Surface | 280 wide, height hugs the content; the alert surface, radius and shadow of `Alerts/Alert/*` |
| Padding | top 24, sides 20, bottom 20; vertical stack, gap 16 |
| Icon | 56 × 56 at x 112, y 24: the app's icon (the volume icon in Eject) |
| Badge (Blocked, Malware) | the Critical alert's badge: a 24 × 20 disc at (152, 50) with the `warning` glyph 16 at (156, 53), over the icon's bottom-right corner |
| Text block | 240 wide at x 20, 16 below the icon; vertical stack, gap 8, bottom padding 4 |
| Title | `Title 3/Primary` (16/22, 600), left-aligned as in the kit's alerts, wraps, no line limit |
| Message | `Callout/Secondary`, wraps |
| Detail (Confirm) | `Callout/Tertiary`, two lines: the origin, then `Developer: <name>` |
| Buttons | 16 below the text block; `Buttons/Push/<Primary|Secondary>/Regular` 28 tall, each `max(72, label + 24)` rounded up to 4, gap 8, centred |
| Button stacking | when the pair is wider than 240 the buttons stack, each 240 wide, the default on top, gap 8 |
| Default button | always the trailing (right) one in a row and the top one in a stack; Return activates it |

### Variants

| Variant | Verdict or trigger | Title | Message / detail | Buttons, left → right (default last) | Escape |
|---|---|---|---|---|---|
| `Confirm` | `confirm` | “Lemon” is an app downloaded from the Internet. Are you sure you want to open it? | detail: `lemon.example.org` / `Developer: Marta Rossi` | Open (Secondary) · **Cancel** (Primary) | Cancel |
| `Blocked` | `block` | “Lemon” Not Opened | Silica cannot verify that “Lemon” is free of malware or has not been modified. | Move to Trash (116, Secondary) · **Done** (Primary) | Done |
| `Malware` | `malware` | “Lemon” will damage your computer. | Its developer’s key was revoked. Move it to the Trash. | Done (Secondary) · **Move to Trash** (116, the destructive red button of the Critical alert) | Done |
| `Disk Image` | an app started from a mounted `.sxv` | “Lemon” is running from a disk image. | Move it to Applications to keep it after the disk image is ejected. | **Move to Applications** (Primary) above Not Now (Secondary), stacked, 240 each | Not Now |
| `Quit and Move` | uninstalling an app that is running | “Lemon” is open. Quit it and move it to the Trash? | — | Quit and Move (116, Secondary) · **Cancel** (Primary) | Cancel |
| `Eject` | after an app is copied from a volume to Applications | Eject “Lemon 2.1” and move the disk image to the Trash? | “Lemon” is now in Applications. | Move to Trash (116, Secondary) · **Keep** (Primary) | Keep |

Button widths in the kit: Open, Cancel, Done, Keep 72; Not Now 80; Move to Trash and Quit and Move 116; Move to
Applications 156. The rule above computes them in code.

- **Origin and developer:** origin is the host of the quarantine origin. When there is no origin, the detail shows only
  the developer line. For a self-signed bundle the developer line reads `Developer: <name> (self-signed)`. For an
  AppImage, which carries no developer, it reads `Developer: unknown`.
- **Blocked reasons:** the same alert covers `unsigned`, `self-signed`, `tampered` and `invalid`. The message does not
  change.
- **Eject icon:** the disk-image volume icon (`silica-icons/files/volume-disk-image`, `icon.png` / `icon-dark.png`).

## 2. Disk-image volume window (Mustache)

### Volume window

Masters `Apps/Mustache/Volume Window/<Default Background|Default Background Selected|Custom Background>/<Light|Dark>`
and `Apps/Mustache/Volume Item/Label Plate/<Light|Dark>`.

| Part | Value |
|---|---|
| Window | a Mustache window with **title bar only**: no toolbar, no sidebar, no status bar. Radius, rim and shadow from `Windows/Window/Document` |
| Title bar | 40 tall: traffic lights (`Controls/Rest` at 20, 12), centred title group = `externaldrive` glyph 16, gap 4, volume name in the title text style (13, 600) |
| Content | `width` × `height` of `.silica-window.toml` under the title bar. The default is 540 × 380 (window 540 × 420) |
| Default background | `Surface/Window`, no separator under the title bar (same surface) |
| Custom background | `.background/<image>` drawn at 1× / 2×, top-left aligned and clipped to the content area. The window's bottom corners clip it; the title bar keeps `Surface/Window` |
| Items | icon of `icon_size` (default **128**) centred on the item's `x, y`; label `Body/Primary` at `text_size` (default **13**), 8 below the icon, box `icon_size + 32` wide, centred, at most 2 lines, then middle truncation |
| Label on a custom background | `Label Plate`: a capsule that hugs the text, padding 6 horizontal, 2 vertical, radius 8; fill `#FFFFFFCC` (Light) or `#1B1C1FCC` (Dark), text `Body/Primary` |
| Selected item | Mustache's icon-view selection: a soft tile behind the icon (`Surface/Sunken`, radius 8, icon + 12 on each side) and the name in an `Accent/Amalfi` capsule (height 22, radius 11, padding 8, `Body/On Accent`). On a custom background the capsule replaces the plate |
| Alias badge | on the Applications link: a 32 circle (`Surface/Window`, shadow 0/1/4 black 20 %) with `arrow-up-right` 20, at the icon's bottom-left (−4, +92 at icon 128). It scales with `icon_size` (¼ of the icon) |
| `view = "list"` | the normal Mustache list view inside the same title-bar-only window |

The background of the kit's Custom Background master is only an example made of shapes (gradient Amalfi → Mare, an
arrow). Silica draws whatever the image contains.

### Sidebar: Locations and the volume row

Masters `Sidebars/Row/Volume/<Rest|Hover|Selected|Selected Inactive>/<Light|Dark>` (216 × 32). They are used in
`Apps/Mustache/Sidebar Volume/<L|D>` and `Apps/Mustache/Icons Volume/<L|D>`.

- **Section:** an attached volume adds the section **Locations**, after Places and before Tags. Its header is the same
  as the Places header. The section disappears when no volume is attached.
- **Row:** `Sidebars/Row/Plain`, with the glyph `externaldrive` 20 and the volume name. The label is 136 wide and ends 8
  before the eject button.
- **Eject button:** a 24 × 24 hit area at (184, 4), with the `eject` glyph 16 at (4, 4) inside it, in `Text/Secondary`.
  The glyph keeps `Text/Secondary` when the row is Selected, because the selection fill is soft. It is always visible.
  In the row's Hover state, a 24 circle in the row hover fill sits behind the glyph.
- **Eject flow:** clicking it calls `Detach`. A busy volume shows Mustache's "in use" alert. The row and the section go
  away on `Detached`.
- **Selecting the row** shows the volume's contents in the normal window (icon view, toolbar title = volume name). The
  `.silica-window.toml` layout applies only to the volume window opened on attach.

## 3. Launcher: new app dot

Masters `Apps/Faro/Launcher Item/<Rest|New>/<Light|Dark>`, 112 × 84 (the Launcher cell).

| Part | Value |
|---|---|
| Icon | 60 × 60, centred in the cell (x 26, y 0) |
| Label row | 112 × 16 at y 68 (8 below the icon), `Callout/Primary` 12 |
| Dot | 8 × 8 circle, `Accent/Amalfi`, vertically centred on the label line |
| Dot gap | 4 between dot and name |
| Group | dot + gap + name centred as a group in the cell; the name truncates (end) first, the dot never |

The dot shows for an app installed and never opened. It goes away at the first launch. Replaces `newDotSize = 6` →
**8** and `newDotGap = 4` → **4**. The colour `Accent/Amalfi` is confirmed.

## 4. Dock: missing app

Master `Dock/Tile/Missing/Rest/<Light|Dark>`, 64 × 80, the frame of every Dock tile.

| Part | Value |
|---|---|
| Tile | a 48 × 48 squircle at (8, 16) in the tile: the visible art box of the Dock's 60 pt app icons, which carry their own padding |
| Shape | radius 12, smooth corners (smoothing 0.6) |
| Fill | Light `#E6E8EC`, Dark `#3A3C42` |
| Border | 1 pt inside, Light `#00000014`, Dark `#FFFFFF1A` |
| Glyph | `question` 28, weight Semibold, `Text/Tertiary`, centred (32 × 32 box at (14, 14) in the squircle) |
| Running dot | never |
| Label on hover | the app's remembered name (`Dock/Label`, width fits the text) |

Clicking it shows nothing and launches nothing. The context menu keeps "Remove from Dock". Replaces `MissingAppIcon`.

## 5. Settings

Every card follows the Settings cards of the kit: `Surface/Sidebar` fill, radius 12, 552 wide in the 952 window, rows
48 tall with 12 side padding, separators inset 12. A section heading is `Headline/Primary`, 32 below the previous card
and 8 above its own card.

### Privacy & Security › Security

Masters `Apps/Settings/Security Card/<Default|Blocked App|Anywhere>/<Light|Dark>` and
`Apps/Settings/Security Menu/<Flathub|Anywhere>/<Light|Dark>`. They go under the heading **Security**, after the
permission card.

| Row | Content |
|---|---|
| Allow applications from | `Body/Primary` label; trailing `Buttons/Pop-up/Regular`, which hugs its value (`text + 40`, rounded up to 4): "Flathub and identified developers" 248, "Anywhere" 104 |
| Blocked app (only while one is listed, for one hour) | “Lemon” was blocked to protect Silica. (`Body/Primary`, wraps in 412); trailing **Open Anyway** (`Buttons/Push/Secondary/Regular`, 116). The card grows to 96. With several blocked apps, the most recent shows |
| Anywhere caption | when the policy is `anywhere`, `Callout/Tertiary` 8 under the card at x 12: "Set from the command line. Apps from anywhere open without a check." |

- **Menu:** the pop-up menu, 248 wide, items 24 tall. It has the items "Flathub" and "Flathub and identified developers"
  (checked by default). "Anywhere" is a third item, present and checked only while the policy is `anywhere`; choosing
  one of the others removes it. The menu opens with the checked item over the button (`docs/spec/menus.md`).
  - "Flathub" maps to a policy value Settings1 does not have yet (`flathub`: only Flatpak apps from Flathub and system
    apps open; every bundle and AppImage is `block`).
- **Open Anyway:** it asks for the administrator password with the system authentication alert
  `Alerts/Authentication/Rest/<L|D>` ("Settings needs an administrator’s permission." / "Type the password to allow
  it.", User name and Password fields, Cancel · **Allow**). Then it calls `OpenAnyway`. Artboard
  `Privacy & Security › Open Anyway password`.

### General › Storage: Applications

Masters `Apps/Settings/Storage App Row/<Rest|System>/<Light|Dark>`, 552 × 52, in a card under the heading
**Applications**, after the folder sizes. One row per installed app, sorted by size, largest first.

| Part | Value |
|---|---|
| Icon | the app icon 32 at (12, 10) |
| Name | `Body/Primary` at x 56, y 9 |
| Kind | `Callout/Secondary` at x 56, y 27: "App bundle", "Flatpak · <remote>", "AppImage", "System app" |
| Size | `Body/Secondary`, right-aligned, ending 8 before the button (at the card's padding on System rows) |
| Remove button (Rest) | a 28 × 28 hit area at x 512, vertically centred, with the `trash` glyph 20 in `Text/Secondary`. It moves a bundle or AppImage to the Trash and uninstalls a Flatpak. A running app first asks the `Quit and Move` alert |
| System row | no button |

### Desktop & Dock › Default apps

Masters `Apps/Settings/Default Apps Card/<Light|Dark>`, `Buttons/Pop-up/App/Regular/Rest/<Light|Dark>`,
`Menus/Item/App/<Rest|Checked>/<Light|Dark>` and `Apps/Settings/Default Apps Menu/Web Browser/<Light|Dark>`. They
replace the single "Default web browser" row with the heading **Default apps** and a card of five rows: Web browser,
Mail, Text editor, Image viewer, Terminal.

- **App pop-up:** the Regular pop-up with the app icon 16 at x 12, in a 20 slot, and the label at x 36. Its width is
  `text + 64`, rounded up to 4.
- **App menu item:** a Plain menu item (188 × 24) with the app icon 16 between the check column and the label. The
  menu lists every installed app that declares the type (`x-scheme-handler/http`, `x-scheme-handler/mailto`,
  `text/plain`, `image/*`, the terminal category), sorted by name. The current one is checked.
- In the kit, the browser placeholders are "Silica Web", "Aurora" and "Heron".

## Accessibility

- Alerts: the title is the dialog's accessible name, the message and detail its description. Return activates the
  default button, Escape the button in the Escape column. Tab moves between the buttons.
- Eject button: the accessible name is "Eject <volume>", and it is reachable with Tab in the sidebar.
- Launcher dot: the item reads "<name>, new".
- Missing Dock tile: reads "<name>, not installed".

## Provisional

These are Amalfi decisions, not observed on the reference:

- the Disk Image, Quit and Move and Eject message texts;
- the developer line for self-signed bundles and AppImages;
- the Locations position (after Places) and the label plate on custom backgrounds;
- the "Flathub" policy item, which needs a new Settings1 value;
- the Storage Applications card and the five Default apps rows.
