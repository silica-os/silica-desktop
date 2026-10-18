# Menu service (context menus, pop-up buttons)

Issue: silica-desktop#241, resolving #237 (dismissal) and #238 (invisible popups). Looks and
timings are those of `menubar.md` (Menus, Menu behaviour); this page is the model, the
transport and the rules that apply to every menu that is not in the bar.

## Why a service

The bar's menus already are a model the app sends and the shell draws (`silica-menu-v1`,
`set_model`). Context menus and pop-up buttons used to be a second implementation: every app
drew them as its own `xdg_popup` (silica_ui `AmalfiMenu`). That produced menus that were never
sized or drawn (#238), menus that stayed open after a context change (#237) and a look that
depended on the app. They now use the same path as the bar: the app describes the menu, the
shell draws it, Ossido positions it and owns its life.

## The model

A menu is a tree of rows, sent in document order.

| Row | Fields |
|---|---|
| item | `id` (non-zero, chosen by the app, unique in the menu), `label`, `flags`, `icon`, `key`, `modifiers` |
| separator | none |
| section | `label`: a heading row (Amalfi `AmalfiMenuSection`) |
| submenu | `label`, `flags`, `icon`, then the rows of the submenu, then the end of the submenu |

- `flags` is a bit set: `1` disabled, `2` checked, `4` mixed (a dash instead of the check),
  `8` destructive.
- `icon` is a symbol name of the Silica symbol set, or an absolute path of an image file the
  shell can read; empty for none. The shell decides whether it can draw it.
- `key` is the shortcut's key name as in `silica-menu-v1` (`A`, `1`, `Left`, `Return`...),
  `modifiers` a bit set `1` meta, `2` ctrl, `4` alt, `8` shift. The service only shows
  shortcuts; the app handles the keys.
- Limits (a violation is a protocol error for that client only): 1024 rows, 4 levels of
  submenu, labels, icons and keys of at most 256 bytes.

A menu has also:

- `surface`: the app `wl_surface` it belongs to (a window or a popup of the app);
- `anchor`: a rectangle in the surface's coordinates (logical pixels): the pointer position as a
  0x0 rectangle for a context menu, the control's rectangle for a pop-up button;
- `kind`: `context` (0) or `pop_up` (1);
- `selected`: the id of the item the pop-up button currently shows (0 for none).

## Transport: a Wayland extension of Ossido

`silica-menu-v1` version 2 adds `silica_menu_manager_v1.get_popup_menu` and the object
`silica_popup_menu_v1`. The model is built with typed requests (`add_item`, `add_separator`,
`add_section`, `begin_submenu`, `end_submenu`) and made visible with `show`; Ossido answers with
`selected(id)` or `dismissed`, once. It goes to the shell as typed events of
`silica_shell_manager_v1` version 9 (`popup_menu`, `popup_menu_row`, `popup_menu_closed`) and
the shell answers with `popup_menu_transient`, `popup_menu_choose` and `popup_menu_close`.
No JSON and no file descriptor: the existing bar model keeps its JSON until it moves to the
same encoding.

Why Wayland and not D-Bus:

- Ossido positions and dismisses. Both need the surface's place on the screen, the stacking
  order and the input path, which only the compositor has; with D-Bus the app would have to
  send screen coordinates it does not know, and the "owner" of a menu would be a bus name that
  Ossido has to map back to a client and a surface (xdg-foreign style handles).
- A menu is tied to a client connection. When the app dies or its surface unmaps the objects go
  with it; a D-Bus request needs separate watching of the name.
- The bar's model already travels on this protocol and Ossido already has the transient
  machinery (`transient.rs`) that decides who sees a press.
- D-Bus stays the path for third-party toolkits (GTK/Qt menus via dbusmenu and the portal), which
  are mapped onto this same model later.

## Placement

Ossido turns the anchor into screen coordinates (the surface's position plus the anchor) and
sends the shell the anchor in screen coordinates. The shell lays the menu out with the bar's
menu code and keeps it on the output:

- context menu: its top left corner at the anchor, 11 pt above the pointer as the desktop menu
  (the first row under the pointer); past the right edge it opens to the left of the pointer,
  past the bottom it slides up;
- submenus: as `menubar.md` (Submenus), on the other side when they do not fit;
- long menus (taller than the output minus 8 pt of margin) scroll inside the menu.

### Pop-up button

The menu opens over the control with the selected row aligned on the control: the row's text
baseline on the control's label baseline and the row's left edge 5 pt left of the control's
(the menu's body is wider than the control by its padding). The rows above the selected one are
over the control, so the control stays visible through the selected row only. When the menu does
not fit (the control is near a screen edge) it is moved to fit, not re-aligned, and the selected
row is highlighted anyway. With no selected item the first row is aligned. A pull-down (no
selected value) is a `context` menu anchored below the control's bottom edge.

Status: the alignment is *provisional*; the reference's pop-up button has not been measured
(request in silica-reference).

## Behaviour

Opening, highlight, blink on choice, 190 ms fade out, submenu delay (295 ms), diagonal
tolerance, keyboard navigation (arrows, Return, Esc, type-to-select) are those of
`menubar.md` (Menu behaviour); they are the same widgets. The menu opens in one frame, with
no scale.

- The shell takes the keyboard while the menu is open. The app's window stays the active one
  (drawn active, the bar keeps its menus).
- The app receives `selected(id)` after the blink, not when the pointer is released.
- One menu per client: a new `show` first ends the open one with `dismissed`.
- A menu with no rows is not shown (`dismissed` at once).

## Dismissal (#237)

Ossido closes the menu, tells the app `dismissed` and the shell `popup_menu_closed`, and the
shell fades it out. All of these are decided in one function (`dismiss_popup_menus`, next to the
transients):

| Trigger | Closes |
|---|---|
| A press outside the menu and its submenus (the rule: modal for the owner app, `pass` for everything else; a press in the owner app only closes) | the menu |
| Keyboard focus moves to a surface that is neither the menu nor the owner app's | all menus |
| App switch (⌘Tab), another window activated, the desktop activated | all menus |
| Space change | all menus |
| Mission Control (Panorama), the Launcher, Faro | all menus |
| The screen locks | all menus |
| The surface the menu hangs from unmaps, is minimized, hidden or its client disconnects | that app's menu |
| The app destroys the `silica_popup_menu_v1` object | that menu, silently |
| Esc, a choice, a click on nothing inside the shell menu | the shell asks with `popup_menu_close` or `popup_menu_choose` |

The bar's own menus follow the same rules through the same machinery (`set_owned_transient`
with the frontmost app as owner); the shell only reacts to `dismissed` and `popup_menu_closed`.

## Fallback

silica_ui uses the service when the compositor offers `silica_menu_manager_v1` version 2 and the
embedder reports `popupMenusAvailable`. Elsewhere (tests, other compositors) it draws the menu
in-process as before. The public API (`showAmalfiMenu`, context menu widgets, `AmalfiPopUpButton`)
does not change.
