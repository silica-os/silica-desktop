# Terminal settings window

Terminal › Settings… (⌘,) in Amalfi. Behaviour and content come from the Terminal app
(`silica-os/terminal`: `docs/spec/settings.md`, `lib/src/settings.dart`,
`lib/src/settings_window.dart`); sizes are Amalfi's. Kit page `App · Terminal`, artboards
`Terminal / Settings / <Tab> / <Light|Dark>` and `Terminal / Settings / Profiles / <Sub-tab> / <Light|Dark>`
(18 in all), exported to `silica-design/kit/pages/App · Terminal/`.

## Window

| Item | Amalfi value |
|---|---|
| Size | **800 × 680**, fixed (not resizable). The reference measured 671 × 628; Amalfi widens it so the six profile sub-tabs (6 × 80) and the longest option ("Only if there are processes other than the login shell", 321 pt at Body 13) fit without truncation, and adds 52 for the Settings Tabs bar under the title bar |
| Chrome | window corner 18, smooth 0.6, `Surface/Window`, rim `Line/Border`, shadow `#0000002E` 0/12/32; `Titlebars/Piece/Controls/Active` at 0,0 (96 × 52); no title text, as the other settings windows of the kit |
| Title bar | 52 tall, window controls only |
| Pane tabs | `Settings Tabs/Bar/4 Panes` (see [`components.md`](components.md) › Settings Tabs), 456 × 52 at x 172, y 52 (centred under the title bar): four panes of 112 ("Window Groups", 84 pt at Caption 11, + 24 rounded up to 8): General (`gear`), Profiles (`terminal`), Window Groups (`window-stack`), Encodings (`globe`). The chosen pane swaps to `Settings Tabs/Item/Selected` (control surface, accent glyph) |
| Content padding | 24 (settings) |
| Sections | title `Headline/Secondary` at y 124 (20 below the pane tabs), box below it at y 148: `Surface/Sidebar`, radius 12 (smooth 0.6), full width 752 |
| Form rows | label `Body/Primary`, **right-aligned** in a label cell, 12 to the control. Regular controls (28). Rows of 28 pt controls at a pitch of 40; checkbox and radio rows are 28 slots (control 20 at +4) stacked without gap; 8 between groups; 12 inside the box top and bottom |
| Notes | `Caption/Secondary` under the control they explain, aligned with its label text |


## General

Label cell x 40, width 160; controls at x 212.

| Row | Kit component | Values | App |
|---|---|---|---|
| On launch, open | `Buttons/Pop-up/Regular` 140 | A new window | not a setting yet (spec only) |
| Default profile | Pop-up 140 | the profiles; default Basic | yes |
| Shell for new windows | Pop-up 200 | Default login shell | not implemented (spec only) |
| New windows open in | Pop-up 240 | The home folder · The folder of the current tab; default home | yes |
| New tabs open in | Pop-up 240 | same; default the folder of the current tab | yes |
| Tab switching | `Toggles/Checkbox/With Label` | ⌘1 to ⌘9 select tabs, on | not a setting yet (spec only) |
| Mouse | Checkbox | Report mouse events to programs that ask for them, on | yes |
| Links | Checkbox | Command-click opens links, on | yes |
| Prompts | Checkbox | Mark prompts automatically, on | yes |
| Clipboard | Checkbox × 2 + note | write on, read off; note "Programs ask with the OSC 52 escape sequence…" | yes |

Sections: "Windows and tabs" (box 148–400) and "Programs" (title y 420, box 444–648).

## Profiles

- **Profile list**: box x 24, y 124, 200 × 492 (`Surface/Sidebar`, radius 12), rows 44 (the
  spec's 45 rounded to the grid) inset 4: `Apps/Terminal/Settings/Profile Row/<Rest|Selected>`.
  Row: preview at x 8, y 8 (`Apps/Terminal/Settings/Profile Preview`, 40 × 28, radius 6,
  `Line/Border`, "$ _" in `Mono` 12/16 centred, fill and text overridden with the scheme's background and
  text colour); name at x 60 (`Body/Primary`, selected `Headline/Primary` = weight 600 on the
  `Accent/Amalfi Soft` capsule, radius 8). The default profile reads "Basic (default)".
- Under the list, `Buttons/Push/Secondary/Regular`: Duplicate (88) at x 24 and Remove (80) at
  x 120, y 628. Remove is disabled when only one profile is left.
- **Sub-tabs**: the kit's Segmented Control `Segments/Control/One 6 Text/Regular/Rest`, widened to
  488 × 28 so the six segments are 80 each ("Advanced", the longest, is 63 pt at Body 13), centred over
  the pane at x 268, y 124: Text · Window · Tab · Shell · Keyboard · Advanced. The chosen one swaps its
  item to `Segments/Text Item/Regular/Selected`.
- **Pane**: box x 248, y 168, 528 × 448. Label cell x 264, width 128; controls at x 404.
- The list shows the preset schemes as profiles, with the names of `extraProfiles`: Basic,
  Night Glass, Day Glass, Meadow, Phosphor, Paper, Parchment, Deep Sea, Graphite, Desert, Silver.
  Previews use the scheme colours (Basic follows the appearance: `Surface/Window` and `Text/Primary`).

### Text

| Row | Kit component | Values | App |
|---|---|---|---|
| Name | `Fields/Text Field/Regular/Filled` 200 | profile name, not empty | yes |
| Colors | Pop-up 256 | Default (follows the appearance), then the ten schemes | yes |
| (under Colors) | five `Wells/Color Well/Minimal` 44 × 24, pitch 64, caption below | Background, Text, Bold, Selection, Cursor: the scheme's colours | read-only in the app (no per-colour editing yet) |
| Font | Text field 200 | JetBrains Mono | yes |
| Font size | `Steppers/With Field/Regular` + "pt" | 8–48 in the steps 8 9 10 11 12 13 14 16 18 20 24 28 32 40 48; default 12 | yes |
| Text | Checkbox | Use the bold face for bold text, on | yes |
| | Checkbox | Antialias text, on | not implemented |
| | Checkbox | Allow blinking text, on | not implemented |
| | Checkbox | Display ANSI colors, on | not implemented |
| | Checkbox | Use bright colors for bold text, off | not implemented |
| Cursor | Pop-up 140 | Block · Underline · Bar; default Block | yes |
| | Checkbox | Blink the cursor, off | yes |

### Window

| Row | Kit component | Values | App |
|---|---|---|---|
| Columns | Stepper with field | 20–300, default 80 | yes |
| Rows | Stepper with field | 4–100, default 24 | yes |
| Title | Checkbox | Show the size in the window title, on | yes |
| | Checkbox | Show the running process and its arguments, on | not implemented |
| Restore | Checkbox | Restore the text when the window reopens, on | not implemented |
| Lines to restore | Text field 80 + `Steppers/Stepper/Regular` + "lines" | default 10,000 | not implemented |

### Tab

| Row | Kit component | Values | App |
|---|---|---|---|
| Title | Checkbox | Show the directory in the title, on | yes |
| Marks | Checkbox | Show prompt marks, on | yes |

### Shell

| Row | Kit component | Values | App |
|---|---|---|---|
| Run at start | Text field 280, placeholder "A command" | empty: none | yes |
| When the shell exits | Pop-up 280 | Close the window · Close if the shell exited cleanly · Don't close; default Don't close | yes |
| Ask before closing | `Toggles/Radio/With Label` × 3 | Always · Never · Only if there are processes other than the login shell (default) | yes, as a pop-up today: the design uses radios because the longest option does not fit a pop-up |

### Keyboard

| Row | Kit component | Values | App |
|---|---|---|---|
| Option key | Checkbox + note | Use Option as Meta Key, off; note "Option sends Escape before the key…" | yes |

### Advanced

| Row | Kit component | Values | App |
|---|---|---|---|
| Scrollback | Pop-up 140 | Limited · Unlimited; default Limited | yes |
| Lines | Text field 80 + stepper + "lines" (shown when Limited) | 0–100,000, step 1,000, default 10,000 | yes |
| Bell | Checkbox | Flash the window for the bell, on | yes |
| | Checkbox | Notify when a background tab rings the bell, on | yes |

## Window Groups

Section "Saved window groups" (title y 124), list box at y 148 with rows 44 and 1 pt `Line/Separator`
inset 16: "<name> — <n> windows" (`Body/Primary`, x 40) and a Remove push button (80, right-aligned 16 from
the box edge). Empty, the content shows the note "No window groups yet. Use Window › Save Windows as Group
to make one." (`Caption/Secondary`). Groups are saved from the Window menu, not here.

## Encodings

Section "Character encoding": label Encodings, a checked and disabled checkbox "Unicode (UTF-8)" and the
note "Terminal speaks Unicode (UTF-8) with programs. Other encodings are not offered." The app shows only
the note today; the disabled checkbox is display only (no setting).

## Light and dark

Every artboard exists in both appearances; the dark ones use the `Dark/*` masters, text styles and
swatches (sections `Surface/Sidebar` dark `#232428`, selected profile row `Accent/Amalfi Soft` dark
`#3D95FF38`). Scheme previews keep their own colours in both appearances; Basic follows the appearance.

## Not in the app yet

Present in the design (from the spec, `docs/spec/settings.md` ST-APPSET-05/06) but not implemented by
`silica-os/terminal`:

- General: On launch, open (a choice; today the app always opens a new window with the default profile);
  Shell for new windows (the app always runs the login shell); Tab switching ⌘1–⌘9 as a setting.
- Profiles › Text: Antialias text, Allow blinking text, Display ANSI colors, Use bright colors for bold
  text; editing the five scheme colours (the wells are read-only).
- Profiles › Window: Show the running process and its arguments in the title; Restore the text when the
  window reopens, Lines to restore.
- Profiles list: the ten presets as ready-made profiles (the app ships only Basic and offers the presets as
  colour schemes in Colors).
- Code differences to align: window 800 × 680 (app 671 × 628); pane tabs as the Settings Tabs bar under
  the title bar (app: a small segmented control); profile sub-tabs 80 each; regular controls, not small; Ask before closing as radios; profile
  rows 44 with a 40 × 28 preview in Mono 12 (app: 45 and 9 pt text, below Amalfi's 11 pt minimum).
