# Amalfi components

Every component of the UI kit, the binding layout metrics, and the two system surfaces with their own spec (Faro and Launcher).

## Layout metrics (binding)

Every frame x, y, width and height is a multiple of 4 pt. The only exception is vertical centring inside a
bar, and then you choose item heights so the centring lands on whole points: 52 − 16 = 36 → y 18;
52 − 28 → y 12; 52 − 20 → y 16.
- Window: corner 18, toolbar 52, title-only bar 32. Window controls: first dot centre x 26, pitch 23, so the
  group sits at x 18 (or x 19 with a 16 pt dot cell). The vertical centre is the bar's centre.
- Content padding: 20 (regular windows), 16 (compact windows and panels), 24 (documents and settings).
- Sizes are never smaller than macOS (user decision 2026-10-17, readability and accessibility): compare the
  ink, not the box. A symbol of size N draws at most N × 20/24, so a 20 pt icon draws ~17 pt, like macOS.
  Icons are text in the Silica Symbols font (`tokens.json` › `iconFont`): font size = icon size.
  In the Sketch kit the icon text layers use line height N + 1 (16–30 pt) or N + 2 (32–36 pt) to undo Sketch's
  baseline rounding; frames stay N × N. In code the line box is N: centre the glyph in it. No text
  below 11 pt. Default scale on 1920 × 1200 panels stays 1× (user decision 2026-10-17).
- Sidebar: width 240 (min 200), horizontal padding 12, row height 32, row pitch 32 (gap 0: selection capsules
  touch, as on macOS), section header 28 tall (text `Sidebar Header` 12/16), 16 above a header (except the
  first, 8), and 8 between the last row of a section and the next header. The selection capsule is the row
  inset 0 horizontally, radius 8. Row icon 20 at x 12 (y 6), label at x 40 (text `Sidebar` 14/20, selected
  weight 600). Tag dots are 12 pt circles centred in the icon box.
- Sidebar glyph colours (user decision 2026-10-17): glyphs are coloured **by section**, never one colour
  per item; every row of a section shares its colour, and a row keeps it when selected. Source: our Finder
  and Mail captures (`mustache.md` MU-10, `mail.md` Sidebar).

  | App | Section | Glyph token |
  |---|---|---|
  | Mustache | Pinned (favourites), Recents/Shared | `Accent/Amalfi` |
  | Mustache | Places (home, volumes, network, Trash) | `Text/Secondary` |
  | Mustache, Notes | Tags | the tag's own colour (dot) |
  | Mail | Favourites, Smart Mailboxes, account mailboxes, Search | `Accent/Amalfi` |
  | Mail | On My Mac (local folders) | `Palette/Mare` |
  | Notes | Folders (including All Notes) | `Palette/Limone Scuro` |
  | Notes | Smart Folders | `Accent/Amalfi` (not observed on the reference: fallback) |
  | Generic panels (`Sidebars/Panel`, `Windows/Sidebar Body`) | every section | `Accent/Amalfi` |
  | Settings | tiles keep their own colour, white glyph | — |

  Palette (user decision 2026-10-17): `Palette/Arancia` (light `#F98A17`, dark `#FF9F40`) is the orange; the
  Orange tag (dots, tag rows, tag colour row in menus, Mustache sidebar, Settings, Customize Folder) uses it,
  never `Palette/Limone`, which stays the Yellow tag and the decorative yellow. `Palette/Limone Scuro` (light
  `#A67600`, dark `#FFCF3D`, equal to `Limone` dark) is the glyph yellow: it gives at least 3:1 (WCAG 1.4.11)
  on the light sidebar `#F5F6F8` (3.73), on the selected capsule (3.12) and on the window `#FFFFFF` (4.03);
  dark `#FFCF3D` is 10.52 on the sidebar `#232428` and 7.5 on the selected capsule. Use `Limone Scuro`
  for yellow glyphs and text-sized marks, `Limone` for fills and dots.

  The row masters' icon defaults to `Accent/Amalfi`; instances override it per section. Disabled rows use
  `Text/Tertiary`.
- Sidebar selection (user decision 2026-10-17), the same in every sidebar (Mustache, Mail, Notes, Settings,
  Calculator, panels):

  | State | Capsule | Glyph | Label |
  |---|---|---|---|
  | Selected, active window | `Accent/Amalfi Soft` (light `#0E77E624`, dark `#3D95FF38`) | section colour | `Text/Primary`, weight 600 |
  | Selected, inactive window | `Surface/Control Pressed` light `#E9ECF0`, `Surface/Control` dark `#3A3B40` | `Text/Secondary` | `Text/Secondary`, weight 600 |
  | Any row, inactive window | none | `Text/Secondary` | `Text/Secondary` |

  Masters: `Sidebars/Row/{Plain,Badge}/Selected` and `.../Selected Inactive`. Hover has no background on the
  reference; the kit's hover tint stays a pointer affordance.
- No divider between sidebar and content (user decision 2026-10-17): the sidebar meets the content with
  the tone step `Surface/Sidebar` → `Surface/Window`, as on the reference (`windows.md` Sidebar). No line in
  the body, none across the title bar. Dividers between list and content (Mail, Notes) and before an
  inspector stay.
- Sidebar show/hide (⌃⌘S or the toggle; Sketch cannot show it): the sidebar slides in from / out to the
  leading edge while the content area widens or narrows in step. Hide **240 ms**, show **230 ms** starting
  ~40 ms after the key or the click release, S curve (slow–fast–slow, half way at ~140 ms; use
  `cubic-bezier(0.42, 0, 0.58, 1)`). Content reflows (icon grid columns) during the slide, not at the end.
  - Toolbar items have no animation of their own: in every frame each leading item is placed at
    max(its sidebar-closed x, sidebar edge + its sidebar-open offset). So the item rides the sidebar's
    trailing edge and stops at its closed slot; nothing jumps, fades or slides on its own timing.
  - The sidebar toggle is one button in both states and follows the same rule: open, right-aligned in the
    sidebar's title-bar section (x = sidebar width − 52, slot 44); closed, right after the window controls
    (x 96). Hiding, it moves left with the sidebar edge and stops at 96; showing, it waits at 96 until the
    edge reaches it, then rides the edge to its open place.
  - The title and the other leading items (Back/Forward, a list column's filter) behave the same way; a title
    that runs out of room is truncated, never pushed off.
  - Window controls never move. Trailing items (right-aligned groups, search) never slide; if the content
    width crosses a toolbar width state during the slide, they switch layout in one step.
  Source: Mustache and Mail measurements (`mustache.md` MU-10 and MU-13, `mail.md` › Sidebar toggle,
  `mustache-spec.md` 2.3). Notes measured ~410 ms for the same transition; Amalfi uses the 240/230 ms values
  everywhere.
- Calculator history sidebar: closed, the sidebar toggle sits in the toolbar after the window controls;
  open, it moves inside the sidebar's title-bar area, right-aligned 8 pt from the sidebar edge, as Mail
  (toggle 44 × 52 at x = sidebar width − 52: 168 with the 220 pt sidebar).
- Settings sidebar: the same sidebar as Mustache, Mail and Notes (width 240, rows 32, soft selection
  capsule, sidebar body under a blur-strip title bar section, no divider), with the search field and
  the account block above the rows. Each entry keeps a coloured rounded tile: tile 24 at x 10 (y 4) with a
  white glyph 16 centred in it, label at x 44 (user decision 2026-10-17).
- Sidebar in a split title bar (user decision 2026-10-16; Mustache, Mail, Notes and every `Split Toolbar`
  window):
  - no divider between sidebar and content (2026-10-17, replaces the 1 pt line at x 239).
  - the sidebar body runs from y 0, under the title bar; its first row sits where it did (header at y 60,
    i.e. 52 + 8), and the rows scroll **under** the title bar section.
  - the title bar section over the sidebar (`Titlebars/Section/Sidebar`, 240 × 52, controls and sidebar
    toggle on it) is a **blur strip**, not glass: background blur σ 10 pt, saturation 1.0, no refraction,
    no rim, no shadow, tint `Surface/Sidebar` at 80 % (`#F5F6F8CC` light, `#232428CC` dark), no line under it.
  - unified title bars (one bar over sidebar and content) keep the opaque bar.
- Inspector: width 260, padding 16, groups separated by 20.
- Toolbar items are glass (Glass/Regular), 36 tall at y 8 in the 52 bar (user decision 2026-10-17; macOS 38):
  - single icon button 36 × 36 circle, glyph 20;
  - action capsules 36 tall, made of 36 pt segments with 1 × 16 separators (Back/Forward 73);
  - search capsule 36 tall, glyph 20;
  - 8 between capsules; trailing padding 16 (user decision 2026-10-16: the kit's "Full Inset 8" windows are defects).
  Small 24 (segment 24, icon 16), mini 20 (segment 20, icon 14).
- Icon grids (file manager): tile 104 × 104 (icon 64, label below), gap 16, first tile at the content
  padding.
- Lists and tables: row 28 (regular), 24 (compact), 44 (two-line). Separators are 1 pt Line/Separator,
  inset 16.
- Calculator keypad: keys 48 × 48 (capsule or circle), gap 8 (pitch 56), padding 16, display height 72.
  Key labels Inter Semibold: digits 24, AC/C 20, scientific and programmer keys 18. Operators, % and ± are
  Silica Symbols Semibold 24 (`calc-add`, `calc-subtract`, `calc-multiply`, `calc-divide`, `calc-equal`,
  `calc-percent`, `calc-plus-minus`; about 17 pt of ink like the digits); backspace `delete-left` Semibold 24
  (user decision 2026-10-17). History sidebar: runs under the toolbar like every sidebar (blur strip, no
  divider); each row shows the operation (Callout, secondary) with the result below it (Inter Semibold 16/20),
  both right-aligned. No divider against the keypad; the sidebar toggle moves into the sidebar
  when it is open (see Calculator history sidebar above).
- Terminal: content padding 12, line height 16 (Mono 12/16). Settings window: [`terminal-settings.md`](terminal-settings.md).
- Text baselines: put text frames on whole points, with the line height from the text style.
- Push buttons: the label is centred in the button on both axes (text centre = button centre); the
  kit's text frames are auto-width, centre-aligned and centred in all 200 masters. Do not take the frame x
  from the kit and draw the text left-aligned: centre the measured label width in code.

Pointers are not part of Amalfi: Silica uses the classic Adwaita cursor theme (user decision 2026-10-16).

### Toolbar width states

Mustache's content toolbar (kit masters `Apps/Mustache/Toolbar/Bar/*`, pieces and menus under
`Apps/Mustache/Toolbar/*`) never wraps: as it narrows it drops items in a fixed order. W is the width of the
toolbar right of the sidebar (window width − sidebar; + 240 with the default sidebar).

Item widths: leading 16, Back/Forward 73, 12 to the title, title at least 160, 12 after it, view group 152,
view pop-up 56 (view glyph + `chevron-up-down` 12), Group 56 (`square-stack` + `chevron-down` 12),
Share·Tags·Actions 110, Share alone 36, overflow » 36, search circle 36, gaps 8, trailing 16. A state fits when
W ≥ 289 + its items + gaps (289 = 16 + 73 + 12 + 160 + 12 + 16). With the search field open (240) every
threshold moves by 240 − 36 = 204. Items come back 8 pt later than they leave (hysteresis); the permanent field
and Back/Forward have none.

| State (items right of the title) | Leaves below | Comes back at | Search open: leaves below | Comes back at |
|---|---|---|---|---|
| Wide: views, Group, Share·Tags·Actions, permanent field | 856 | 856 | — | — |
| Full: views, Group, Share·Tags·Actions, search | 667 | 675 | 871 | 879 |
| View group → pop-up | 571 | 579 | 775 | 783 |
| Tags and Actions → » (Share alone) | 541 | 549 | 745 | 753 |
| Share → » | 497 | 505 | 701 | 709 |
| Group → » | 433 | 441 | 637 | 645 |
| View pop-up → » (title, », search) | 369 | 369 | 573 | 573 |
| Back/Forward → » (minimum W 284) | 284 | — | below 573 | — |

- Permanent field: width min(320, 160 + 0.75 × (W − 856)); the title takes the rest. Focused or holding text it
  is at least 240, so between 856 and 870 focusing it collapses the view group.
- Search open below 573: the title shrinks first (down to 24); below W 352 the field narrows (W − 112, 172 at
  the minimum).
- Order never changes: Back/Forward, title, views, Group, Share·Tags·Actions, », search. » sits just before
  the search item. Overflow menu: text items in bar order, menu buttons as submenus (`Overflow Menu/*`).
- Crossing a threshold while resizing swaps the layout in one frame; nothing slides.
- Search circle → field (on release): 36 → 240 in ~160 ms ease-out with most of the distance in the first
  frame; the items to its left move with it and the view group collapses in one frame during the growth. Focus
  goes to the field at once; the focus ring fades in over ~170 ms after the width settles. Search mode starts
  only when text is typed.
- Collapse (Esc, or a click in the content with an empty field): 240 → 36 in ~190 ms ease-out, the view group
  returns in one frame. A click on the sidebar background keeps the field open and focused; a field with text
  stays open without focus (no ring, text secondary); the clear button leaves search.
- Group, Actions, the view pop-up, » and a long press on Back/Forward open their menu on press; Share and Tags
  open their popover on release. ⌥ at the press turns the Group menu into the Sort menu. Menus drop below the
  button; the view pop-up menu puts the current item over the button; the » menu starts on the button row.
- Control states (`<item>/Hover`, `/Pressed`, `/Inactive`; Share·Tags·Actions per segment; `Bar/Full/Inactive`):
  hover is a pill tinted light `#1D1E211A`, dark `#F4F5F71F`, 28 tall inset 4 on a segment of a capsule (in
  the view group 36 × 28 like the chosen segment) and the whole shape on a single button; the separators beside
  the hovered or pressed segment are hidden. Pressed tints light `#1D1E2124`, dark `#F4F5F729` over the full
  36 pt segment. Disabled items show no hover. Inactive window: glyphs and title `Text/Tertiary`; the pointer
  relights a control and a click acts at once (click-through). Hover and press appear in one frame, no fade.
- Menus show a highlighted row with `Menus/Item/*/Highlighted` (`<menu>/Highlighted`). Submenus (`Submenu/View`,
  `Group By`, `Open With`, `Sort By`; attached in `Cascade/*`) open to the right overlapping the parent by 4,
  first row level with the parent item (submenu top = item top − 4); the parent item stays highlighted.
  Open With: the default app with "(default)", separator, other apps, separator, Other….

## Settings Tabs (pane switcher of settings windows)

An app's settings window switches its panes with **Settings Tabs**, never with toolbar buttons
(user decision 2026-10-19). Kit masters `Settings Tabs/Item/<State>/<Light|Dark>` and
`Settings Tabs/Bar/<N> Panes/<Light|Dark>` (N = 2–7); sheet `Settings tabs / <Light|Dark>` on the
Segmented Controls page.

Anatomy: a glass track (the Segmented Control's: `Glass/Regular` + `Fill/Glass/Tint`, rim, shadow 0/4/12)
holding N equal panes in a row; each pane is an icon over a label.

| Part | Value |
|---|---|
| Placement | centred horizontally in the window, top at y 52 (directly under the 52 pt title bar, which keeps only the window controls; no title text) |
| Track | height 52, padding 4, radius 26 (capsule; concentric with the panes: 22 + 4), smooth 0.6 |
| Pane | height 44, radius 22; width W for every pane |
| Pane width W | longest label (Caption, 11/14) + 24, rounded up to a multiple of 8; at least 72. Bar width = N × W + 8 |
| Icon | Silica Symbols 20 (line height 21 in Sketch), centred, top at 4 in the pane |
| Label | `Caption/Primary` 11/14, centred, top at 26 (1 below the icon), never truncated or wrapped |
| Content | the pane's content starts 20–24 below the bar (Terminal 20, Mustache 24: the window's own padding); the window grows by 52 compared with a toolbar-tab layout |

States (per pane, the bar has no state of its own):

| State | Background | Icon | Label |
|---|---|---|---|
| Rest | none | `Text/Primary` | `Text/Primary` |
| Selected | `Surface/Control`, border 1 `Line/Border` inside, shadow `Shadow/1` 0/1/2 (the Segmented Control's selected item) | `Accent/Amalfi` | `Text/Primary` |
| Hover | `Line/Separator` (light `#1D1E211A`, dark `#F4F5F71F`) | `Text/Primary` | `Text/Primary` |
| Pressed | `Line/Border` (light `#1D1E2124`, dark `#F4F5F729`) | `Text/Primary` | `Text/Primary` |
| Disabled | none, whole pane at 35 % | — | — |

The Segmented Control has no Increase Contrast variant, so Settings Tabs has none either. Hover and press
appear in one frame, no fade; the selection moves in one frame when the pane changes (the window then resizes
to the new pane's height).

Segment count: 2–7 panes. With more than 7, or labels so long that N × W + 8 exceeds the window width
minus 2 × 24, use a sidebar settings window (the Settings app) instead.

Building instances: the bar masters are stacks whose panes fill the width equally, so an instance is resized
to N × W + 8 and every pane follows; choose the selected pane by swapping its nested symbol to
`Settings Tabs/Item/Selected/…`, and set icon and label through the overrides.

Which control to use:

- **Settings Tabs**: switching the panes of an app's settings window (General, Profiles, Advanced…). Icon
  and label, one row, top of the window.
- **Segmented Control** (`Segments/Control/One <N> Text|Icon`, 2–6 segments): switching views or sub-pages
  inside content, such as the Terminal profile sub-tabs or list/grid views. Text or icon only.
- **Toolbar buttons** (`Buttons/Toolbar/*`): actions and views of a document or browser window's toolbar.
  Never as pane tabs.

Used in: Terminal settings (4 panes, W 112: General, Profiles, Window Groups, Encodings), Mustache settings
(4 panes, W 80: General, Tags, Sidebar, Advanced).

## Component inventory (3047 symbols, every one in Light and Dark)

Names follow `Category/Component/Variant/Size/State/Appearance`. States are always
Rest · Hover · Pressed · Disabled · Focused unless listed. Sizes: Mini 20 · Small 24 · Regular 28 · Large 32.
Read each one from the kit for exact geometry (see README, "How to read the kit").

| Family | Components and variants | Sizes (pt) |
|---|---|---|
| Alerts | Alert: one / two / three buttons, critical, with field, glass | 280 wide |
| Buttons | Push: Primary, Secondary, Destructive, Borderless, Glass × 4 sizes × 5 states (200); Icon: Round, Square × 4 sizes × 5 states; Help; Link; Toolbar: Icon, Icon + Label; Gradient add/remove; Pop-up and Pull-down × 4 sizes × 5 states + Open | push 48×20 … 88×32; icon 20–32; toolbar 36×36, 56×44 |
| Color Wells | Color Well: Minimal, Default, Expanded × states; Color Panel swatch grid; Swatch Chip; Swatch Row | 44×24, 72×28, 188×28 |
| Combo Boxes | Combo Box: Small, Regular, Large × Rest/Focused/Disabled + Open; Combo Button | 200×24/28/32 |
| Control Center | Panel; Module: Toggle Off/On/On Hover, Tile, Slider (Display, Sound, Keyboard), Now Playing, Privacy Pill, Edit Button; Detail: Wi-Fi, Wi-Fi Off, Bluetooth, Nearby Sharing, Focus, Display, Sound, Now Playing; Row: Item, Network Secure/Open, Slider, Footer, Section; Status: Wi-Fi, Bluetooth, Battery; Status Menu: Battery | panel 292×484; detail 312 wide; modules 64×64, 140×64, 140×140, 292×64 |
| Dialogs | Dialog: Save Panel, Open Panel, Generic; Browser; Footer | 600×348, 600×308, 400×432 |
| Disclosure | Triangle and Button: Collapsed/Expanded × states; Group; Group Header | 24×24; group 280 |
| Dock | Shelf: Default, With Label; Tile (Rest, Running) per app + Stack, Trash Empty/Full; Separator; Indicator; Label (tooltip) | shelf 528×60; tile 52×60; the label is centred on the hovered icon with its bottom edge 8 pt above the icon's top (not above the shelf), also when magnified (use the magnified icon's top) |
| Fields | Text Field: Placeholder, Filled, Focused, Disabled, Error × 3 sizes; Secure Field; Text Area; Token Field; Token; Search Field: Empty, Typing, Focused, Disabled × 3 sizes + Token; Inline Button; Clear Button | 200×24/28/32; area 240×88 |
| Forms | Layout; Section: Profile, Preferences, Validation; Row: Text, Popup, Check, Switch, Help, Error; Footer | 400×496 |
| Group Boxes | Box: Titled, Titled Disabled, Untitled, Nested | |
| Image Wells | Empty (Rest, Drag Over, Disabled), Filled (Rest, Focused, Disabled) × Small/Regular/Large | 72, 120, 168 |
| Lock Screen | Clock: Standard, Increase Contrast, Reduce Transparency; User: Default, Increase Contrast; Password Field: Placeholder, Typing, Verifying, Wrong, Placeholder Increase Contrast, Placeholder Reduce Transparency; Help Button: Rest, Increase Contrast, Reduce Transparency; Hint; Help Address; Notification: Collapsed; Action Button; Power Buttons ([`lock-screen.md`](lock-screen.md)) | clock 960×240; user 320×108; field 208×32; help 32; hint 320×68 |
| Menu Bar | Bar: Closed, Menu Open (the status cluster is a nested `Status Bar/Bar/Menu Bar/Default`); Item: Menu, Name × Rest/Active | 32 tall, items 28 at y 2, status glyph 20 (2026-10-17; macOS 30) |
| Status Bar | Item: Glyph (Rest/Active), Battery, Input Source, Clock; Bar: Menu Bar, Lock Screen × Default/Increase Contrast/Reduce Transparency ([`status-bar.md`](status-bar.md)) | items 28 tall, glyph 36 × 28, battery 72, input source 80, clock 120; menu bar cluster 372, lock 188 |
| Menus | Menu: Simple, Full, Context, Submenu, Cascade; Item: Plain, Shortcut, Submenu, Icon, Destructive, Checkmark × Rest/Highlighted/Checked/Disabled; Header; Separator | items 24 tall; separator 8 |
| Notifications | Banner: Basic, With Actions, Panel, Panel With Actions; Stack Grouped; Centre Panel; App Icon | 360×76, 360×108; centre 380×416 |
| Popovers | Popover: Form, List, Text × Top/Bottom/Leading/Trailing × Start/Center/End + Detached; Content | ~204–268 wide |
| Progress | Bar: Determinate (25/60/100/Disabled), Indeterminate (3 phases) × Small/Regular/Large; Circular; Spinner | bar 4/8 tall; spinner 16/24/32 |
| Scrollbars | Overlay: Thin, Expanded, Dragging × V/H; Legacy: Rest, Hover, Disabled × V/H; Corner; Area | overlay 12; legacy 16 |
| Segmented | Action segments (toolbar, glass): Icon 2–5, Text 2–3 × Large/Small × Rest/Hover/Pressed/Disabled/Inactive (240); Control (select one/any): 2–5 Text/Icon and 6 Text × Small/Regular/Large × Rest/Pressed/Disabled (+ Mini Rest); segments fill the width equally, so instances can be widened; Icon Item; Text Item; Align | segments 32 (Large), 24 (Small) |
| Settings Tabs | Item: Rest, Selected, Hover, Pressed, Disabled; Bar: 2–7 Panes (see Settings Tabs above) | pane 44 tall, bar 52 tall, width N × W + 8 |
| Sidebars | Panel; Row: Plain, Badge × Rest/Hover/Selected/Disabled; Header; Group Open/Closed; Badge; Footer | panel 240; rows 216×32, icon 20 |
| Sliders | Horizontal: Plain, Ticks, Glyphs × states + Dragging; Vertical; Circular dial; Thumb: Round × states, Lens Dragging | 200×20/28/36 |
| Steppers | Stepper × 4 sizes × Rest/Hover Up/Down/Pressed Up/Down/Disabled/Focused; Value Field; With Field | 16×20 … 28×32 |
| Titlebars and toolbars | Bar: Unified, Title Only, Compact × Active/Inactive; Section: Sidebar, Content, Inspector, List; Piece (modular): Controls, Title, Title + Subtitle, Title + Document Icon, Nav, Sidebar Toggle, Item Group 2/3/4, Segmented, Segmented 4, Segmented Text, Search, Share, More, Spacer, Add, app pieces; Controls; Dot: Close/Minimise/Zoom × Rest/Hover/Inactive; Title Bar; Toolbar; Tab; Tab Bar | toolbar 52; title-only 32; compact 40 |
| Toggles | Checkbox: Off/On/Mixed × 3 sizes × 5 states + label + group; Radio: Off/On × 3 sizes × 5 states + label + group; Switch: Off/On × Mini/Small/Regular × states + Dragging + label row | checkbox 16/20; switch 24×16, 32×20, 40×24 |
| Tooltips | One Line, Two Lines, Shortcut; Placement Below/Above/Leading/Trailing | 24 tall |
| Windows | Window: Document, Sidebar Inspector, Inactive, Panel, Utility, Sheet Attached, Full Screen Strip; Unified Toolbar (+ Sidebar), Split Toolbar (+ Inspector, Three Columns), Sidebar Collapsed, Compact Toolbar, Title Only, Sidebar Body | |
| Apps | Faro 50, Mustache 68, Mail 40, Notes 52, Preview 60, Settings 104, Calculator 24, Clock 13, Terminal 18, Text Editor 14, Screenshot 28, Mission Control 8, Wallpaper | full screens, toolbars and parts |

## Faro (search panel)

Behaviour copies the desktop's search panel exactly (state machine and timings below); the measures are Amalfi's.

**Envelope.** Faro never exceeds **640 pt** in width in any state (bar, bar + buttons, results, scopes). Every
Faro symbol is 640 wide; the Launcher is a separate mode with its own size (below). Top edge fixed at 30 % of the screen
height; the panel only grows downwards. All values on the 4 pt grid.

| Element | Amalfi value |
|---|---|
| Bar (rest) | 640 × 56 capsule, radius 28, Glass/Thick |
| Bar beside the buttons | 384 × 56 capsule |
| Round buttons | 4 circles 56 × 56, same glass as the bar, gap 8 (pitch 64); 8 pt between bar and first button: 384 + 8 + 4·56 + 3·8 = 640 |
| Button glyphs | 24 pt (Icon Kit, Regular/24), order: Applications, Files, Actions, Clipboard |
| Magnifier / scope glyph | 24 pt at x 20, centred on the 56 pt header |
| Field text | text style **Field**: Inter 26 / 32 Regular (placeholder in Tertiary), starting at x 56 |
| Handler icon at the right | 28 pt, 20 from the right edge |
| Panel with results | 640 wide, radius 28; header 56; 1 pt separator inset 20; filter row 40 (pills 24 tall, gap 8, inset 20); rows 56; bottom padding 8 |
| Result list | at most 6 rows visible (panel max 440 tall), then it scrolls |
| Row | icon 32 at x 20, title at x 64 (Body Emphasized), subtitle below (Caption); selection inset 8, radius 16 |
| Selected-row extras | hint (Caption, secondary) + key capsule "Tab" 20 tall, both right-aligned, 20 from the row end, vertically centred |
| Scopes (Files, Actions) | fixed 640 × 544; the list scrolls inside |
| Clipboard opt-in, No results | 640 wide, height from content |

**When the four buttons exist** (presence/absence, same as the reference behaviour):

| State | Buttons |
|---|---|
| Just opened, pointer away from the bar | none: single 640 capsule |
| Pointer enters the bar (first time in this opening) | they come out: bar 640 → 384, buttons slide out from behind it; they then stay while Faro is open, also with the pointer outside |
| Typing a query / results shown | none: the bar is 640 and grows into the results panel |
| Query cleared (Esc) | panel shrinks back to the 56 pt bar; buttons return only if they were out (not verified on the reference: mark as assumption) |
| Inside a scope (⌘1–⌘4 or a button) | none: magnifier becomes the scope glyph (back button), placeholder becomes the scope name |
| Closed and reopened | plain capsule again (buttons out only after the pointer enters); the last scope is kept |

Buttons have no hover or pressed look and no tooltip; the scope opens on release. Shortcuts ⌘1 Applications
(the Launcher), ⌘2 Files, ⌘3 Actions, ⌘4 Clipboard, also switching directly between scopes.

**Animations** (timings from the reference behaviour, distances scaled to Amalfi):

| Event | Motion |
|---|---|
| Open | opacity 0.22 → 1 in ~60 ms; height starts at ~0.97, grows with +2 pt overshoot, settled ~190 ms |
| Buttons out (pointer enters) | starts ~60 ms after entry; the capsule narrows while the buttons are still merged into it, splits at ~235 ms; bar undershoots ~20 pt below 384, then +2, settled ~690 ms; button group overshoots ~12 pt to the right, settled ~800 ms; damped spring |
| Results appear | ~500 ms after the last key; bottom edge grows in ~270 ms, +2 pt overshoot, settled ~400 ms; top edge never moves; result content swaps in one frame, no fade |
| Buttons away while typing | reverse of "buttons out" (merge back into the bar, bar widens to 640) — not measured on the reference, use the same spring |
| Enter / leave a scope | height grows downward in ~450 ms with +4 pt overshoot (leave: −4 undershoot), width unchanged |
| Esc with a query | shrinks back to 56 starting ~150 ms after the key, −4 undershoot, settled ~530 ms |
| Close | starts ~35 ms after the key: brightens and shrinks toward the centre, gone at ~100 ms; no slide |

## Launcher

Faro in apps mode (⌘1 or the Dock). It keeps its **own size** and is not limited to Faro's 640 pt (user
decision, 2026-10-16, reverting an earlier 640 request).

| Element | Amalfi value |
|---|---|
| Panel | 844 × 580, Glass/Thick, top edge on Faro's 30 % line |
| Header | 56, like Faro (apps glyph / back at x 20, field text at x 56, "…" view menu at the right) |
| Category row | seven capsules, all visible |
| Grid | 7 columns; recents row, separator, then all apps A–Z; cell, icon and label geometry from the kit masters `Apps/Faro/Launcher/<Light|Dark>` |
| Category chosen | `Apps/Faro/Launcher Category/*`, 844 × 580, token in the field, no capsule row |
| List mode | `Apps/Faro/Launcher List/*`, 640 × 560 |
| Esc | the panel morphs into Faro's 640 × 56 bar (width and height change), as in `docs/spec/launcher.md` |
