# Amalfi lock screen

The hand-off spec of the Silica 1.0 "Amalfi" lock screen.

- **Behaviour comes from the reference:** what appears, the states, the flow and every timing are in
  [`docs/spec/lock.md`](../../spec/lock.md) (L-01), and that file is binding for them.
- **Layout and look are Amalfi:** the 4 pt grid, the type scale, the materials, the components and the colour tokens of
  this folder. Measured sizes are a floor, never a template: no text or control is smaller than its reference
  counterpart, comparing ink, not boxes.

**Kit:** page **Lock Screen** of the Amalfi UI kit (masters `Lock Screen/…` on the Symbols page). The status items are
the shared [Status Bar](status-bar.md) (`Status Bar/Bar/Lock Screen/…`).

**Exports:** [`silica-design/kit/pages/Lock Screen`](https://github.com/silica-os/silica-design/tree/main/kit/pages/Lock%20Screen),
one PNG per artboard, and `kit/symbols/Lock Screen/…`, a JSON layer tree and a 2× PNG per master.

**Provisional** marks Amalfi decisions about behaviour that was never observed (see the end).

## Screen layout (binding)

The rule fits every screen size:

- The top block hangs from the top edge.
- The bottom block stands on the bottom edge.
- Everything is centred on the vertical axis, except the help button (it hangs off the field and is not counted in
  the centring) and the Status Bar.

W and H are the screen size in pt. The kit draws 1920 × 1200 and checks the rule at 2560 × 1440. All frames are on the
4 pt grid; text boxes use the line height of their style.

| Element | Frame (pt) | 1920 × 1200 | Ink (1920 × 1200) |
|---|---|---|---|
| Status Bar | Lock Screen context, right edge at W − 8, y 0, 32 tall (items 28 at y 2): the place the menu bar gives it | x 1724, 188 × 32 | labels 13, glyphs 20 |
| Clock block | 960 × 240, centred, top 104 | x 480 | — |
| Date | box 960 × 40 at the top of the clock block (`Lock Date` 32/40, 600) | y 104 | cap top 113, baseline 136 |
| Time | box 960 × 200 at y 40 in the block (`Lock Clock` 200/200, 600, tracking −6, tabular) | y 144 | digit tops 169, baseline 314 (digits 145) |
| User block | 320 × 108, centred, top H − 284 | x 800, y 916 | — |
| Avatar | 72 circle, top centre of the user block | y 916–988 | — |
| Name | box 320 × 22 at y 84 in the block (`Title 3` 16/22, 600) | y 1000 | cap top 1005, baseline 1016 |
| Password field | 208 × 32 capsule, centred, top H − 160 (bottom 128 above the screen edge) | x 856, y 1040 | — |
| Help button | 32 circle, 8 right of the field, same top | x 1072 | — |
| Hint block (hint state) | 320 × 68, centred, bottom 24 above the field (top H − 252) | x 800, y 948 | — |
| Help address (hint state) | 320 × 16, centred, 12 below the field (top H − 116) | y 1084 | — |
| Banners | 360 wide, right edge W − 16, first top 48 (16 under the Status Bar), next 8 below | x 1544 | — |
| Power row (provisional) | 288 × 72, centred, 32 below the field (top H − 96) | x 816, y 1104 | — |
| Switch-user list (provisional) | user blocks on the user-block row, 256 apart, centred as a group; Cancel action button 80 × 72 centred at the field's top | | — |

Spacing on the grid:

- avatar to name box, 12;
- name box to field, 18 (the name sits on the 4 pt grid as a text box);
- field to help, 8;
- hint to field, 24;
- field to help address, 12;
- field to power row, 32;
- date baseline to digit tops, about 33. The reference measures 32, and the visual rhythm is kept.

Two sizes never change:

- **Field:** its width and height stay fixed. More than 13 dots scroll inside it, and the field does not grow.
- **Name:** one line, ellipsis at 320.

The hint wraps at 320.

**Check at 2560 × 1440:**

| Element | Position |
|---|---|
| Status Bar | x 2364 |
| Clock | x 800, y 104 |
| User block | x 1120, y 1156 |
| Field | x 1176, y 1280 |
| Help button | x 1392 |

## Components (kit masters)

Every master exists in Light and Dark. Swatches and text styles are the kit's (`tokens.json`).

| Master | Size | Content |
|---|---|---|
| `Lock Screen/Clock/Standard/<App>` | 960 × 240 | date `Lock Date`, time `Lock Clock`, both centred; time in `Glass/Clock` |
| `Lock Screen/Clock/Increase Contrast/<App>` | 960 × 240 | solid white, 2 pt black 40 % rim outside the digits, 1 pt outside the date; Light adds `shadows.onPhoto` |
| `Lock Screen/Clock/Reduce Transparency/<App>` | 960 × 240 | white 92 %, no glass, 1 pt black 32 % rim outside the digits |
| `Lock Screen/User/Default/<App>` | 320 × 108 | avatar 72 = `Accent/Amalfi` disc, 1 pt inside edge white 24 %, rim light white 22 %, `person-fill` 40 `Text/On Accent` centred (a user picture replaces disc and glyph, clipped to the circle, same edge); name `Title 3` on-photo white, centred |
| `Lock Screen/User/Increase Contrast/<App>` | 320 × 108 | edge white 60 %, name solid white with a 1 pt black 40 % rim |
| `Lock Screen/Password Field/<State>/<App>` | 208 × 32 | `Placeholder`, `Typing`, `Verifying`, `Wrong`, `Placeholder Increase Contrast`, `Placeholder Reduce Transparency` |
| `Lock Screen/Help Button/Rest/<App>` (+ `Increase Contrast`, `Reduce Transparency`) | 32 × 32 | `Glass/Lock` circle, `?` in `Title 3` (16/22, 600) `Text/Primary`, centred |
| `Lock Screen/Hint/Default/<App>` | 320 × 68 | hint `Headline` (13/18, 600) on-photo white, centred, wraps at 320; 12 below, the reset row centred as a whole: a 16 `Accent/Amalfi` disc with `arrow-right` 12 Semibold `Text/On Accent`, 8 gap, `Reset password…` in `Body Emphasis` white |
| `Lock Screen/Help Address/Rest/<App>` | 320 × 16 | `Callout` 12, `Dark/Text/Secondary` in Dark, white 80 % in Light, centred |
| `Lock Screen/Notification/Collapsed/<App>` | 360 × 60 | the kit banner collapsed: `Glass/Regular` radius 18, app icon 36 at 12, app `Headline`, count `Callout/Secondary`, time `Caption/Tertiary`; the body is never shown |
| `Lock Screen/Action Button/Rest/<App>` | 80 × 72 | 48 `Glass/Lock` circle, glyph 24 `Text/Primary`, label `Callout` 12 on-photo white 8 below |
| `Lock Screen/Power Buttons/Provisional/<App>` | 288 × 72 | Sleep (`moon`), Restart (`restart`), Shut Down (`power`), 24 apart |

Status items: `Status Bar/Bar/Lock Screen/{Default, Increase Contrast, Reduce Transparency}/<App>`, see
[`status-bar.md`](status-bar.md). (Until 2026-10-18 they were the masters `Lock Screen/Status Items/Default/<App>`,
which were renamed to `Status Bar/Bar/Lock Screen/Default/<App>` with the same symbol ids.)

### Password field

All coordinates are inside the 208 × 32 field.

| State | Content |
|---|---|
| Placeholder (empty, focused) | `Glass/Lock` capsule; caret 2 × 16 at x 16, y 8, `Text/Primary`, blinking; `Enter Password` in `Body` 13 `Text/Secondary` at x 22, y 7 |
| Typing | one 8 pt dot per character, first at x 16, pitch 12, y 12, `Text/Primary`; caret 4 after the last dot; **submit**: a 24 `Accent/Amalfi` disc at x 180, y 4 with `arrow-right` 16 `Text/On Accent` (shown while the field is not empty) |
| Verifying | dots, no caret; `Progress/Spinner/Small` (16) at x 184, y 8 in place of the submit |
| Wrong | the dots stay and are **selected**: a `Line/Focus` capsule 16 tall, radius 8, from x 10 to 6 past the last dot (80 for six dots); the submit stays; the field shakes |

The ink inside the field and on the buttons follows the appearance: `Dark/Text/Primary` on the dark frosted glass,
`Light/Text/Primary` on the light one.

## Materials and colours

| Part | Dark | Light |
|---|---|---|
| Wallpaper | the desktop picture at the same framing, not blurred, plus `#0000000F` (6 %) over the whole screen | same, plus `Scrim/Lock Light`: a vertical gradient of black 20 % at the top to 0 at 30 %, 0 until 60 %, black 35 % at the bottom (provisional) |
| Field, help, action buttons | `Dark/Glass/Lock` (background blur 20, saturation 1.1, white 28 %, 1 pt inside edge white 24 %, rim light white 22 %, no refraction, no shadow) | `Light/Glass/Lock` (white 60 %, edge black 12 %, rim light white 70 %) |
| Clock digits | `Glass/Clock`: Amalfi glass clipped to the glyphs, with Glass blur 8, refraction (distortion 0.5, depth 0.6, chromatic 0.05), saturation 1.3, brightness 1.3, specular highlights, fill white 76 %, 1 pt white 80 % line inside, 1 pt black 32 % rim outside | same |
| Date | white 84 % | white 84 % + `shadows.onPhoto` |
| Text over the wallpaper (name, hint, link, address, labels, Status Bar) | `Dark/Text/Primary` (`#F4F5F7`), address `Dark/Text/Secondary` | white + `shadows.onPhoto` (black 40 %, y 1, blur 6) |
| Avatar, submit, reset disc | `Dark/Accent/Amalfi` | `Light/Accent/Amalfi` |
| Selection of the dots | `Dark/Line/Focus` | `Light/Line/Focus` |
| Banners | `Glass/Regular` | `Glass/Regular` + `Glass/Tint` |

The vivid accent is the only colour on the screen: the avatar, the submit button and the reset link. It marks what
belongs to the user and what acts. Everything else is white ink and frosted glass on the photo.

In the compositor, `Glass/Lock` is a glass region with no refraction and no shadow, and its blur radius never animates.
`Glass/Clock` is drawn by the shell: the clock layer samples the blurred backdrop under the glyph mask and adds the edge
lens. Without a blur source it falls back to the Reduce transparency clock.

## States (kit artboards)

| Artboard | What it shows |
|---|---|
| Default / Dark, Default / Light | rest: Status Bar, clock, user block, empty focused field, help button |
| Typing / Dark | dots, caret, Accent submit |
| Verifying / Dark | dots, spinner in place of the submit |
| Wrong password / Dark | selected dots, submit, shake annotation (ghost outlines at −13 and +18) |
| Hint after second failure / Dark | avatar and name replaced by the hint block; the field in place with selected dots; help address below |
| Wake, step 1 / Dark | the display is just on: wallpaper and Status Bar only |
| Wake, step 2 / Dark | +150 ms: date, clock, avatar and name; user block 48 higher; no field |
| With notifications / Dark | two collapsed banners (app and count) at the top right |
| Power buttons (provisional) / Dark | the optional Sleep / Restart / Shut Down row |
| Switch user (provisional) / Dark | the list of users (a second avatar in `Palette/Bouganville`) and Cancel |
| Increase contrast / Dark, / Light | the Increase contrast masters |
| Reduce transparency / Dark, / Light | the Reduce transparency masters |
| Default 2560 × 1440 / Dark | the same rule at the reference size |
| Parts / Dark, Parts / Light | every master on the wallpaper |

Rules between states (behaviour from `docs/spec/lock.md`):

- **Typing** starts from Placeholder at the first key. The submit appears with the first character and goes away when
  the field is emptied.
- **Verifying** follows Return or a click on the submit. Success unlocks; failure goes to **Wrong**.
- **Wrong** keeps the user block after the first failure.
- **Hint:** after the **second** failure, when password hints are on, the avatar and name are replaced by the hint block
  in one frame, with no fade. The field does not move, and typing again replaces the selected dots.
- **Notifications** while locked keep the place and size they have on the desktop, and show the app name and count
  only.
- **Power buttons** (provisional) show only with the setting "Show the sleep, restart and shut down buttons"
  (`docs/spec/misc.md`). Nothing else moves when they show.
- **Switch user** (provisional) shows only when the user switcher setting is "list of users". Esc or Cancel returns.

## Motion

- **Timings and curves:** they are the measured ones in `docs/spec/lock.md` ("Animation timings") and are not repeated
  here.
- **Geometry:** Amalfi scales the distances by Amalfi size ÷ reference size (`motion.md` rule 3).

| Event | Amalfi geometry | Status |
|---|---|---|
| Lock, unlock | as measured: fades only, nothing slides or scales; the user block returns to rest in one frame before the unlock fade | measured |
| Wake | step 2 places the user block **48** higher (one field row: 32 + 16), versus 36 measured with a 28 field | timing measured, distance scaled |
| Wrong password shake | field and help together, horizontally. Keyframes scaled by 208 / 160 = 1.3 (pt @ ms): 0 @ 0, −13 @ 31, +18 @ 94, −14 @ 156, +13 @ 219, −9 @ 281, +8 @ 344, 0 @ 400; ease-in-out between keyframes | timing measured, distances scaled |
| Minute change | only the changing digits roll upwards. Travel = digit height (145), soft mask 24 at the top and bottom of the digit box, ones digit 60 ms after the tens | timing measured; travel, mask and delay assumed |
| Hint state | one-frame replacement | measured |
| Submit button in and out | `fade.quick` (150 ms) with the first character, 60 ms out when emptied | assumption (token) |
| Banner in | the desktop banner motion (`docs/spec/notifications.md`) | measured |
| Caret | blinks like every Amalfi text field | — |
| Buttons (help, submit, action) | hover and press overlays: `fade.quick` in, 60 ms out | token |

**Reduce motion:**

- The shake becomes a single 400 ms change of the field edge to `Palette/Terracotta` and back (provisional).
- The minute roll becomes a `fade.quick` cross-fade.
- Lock, unlock and wake keep their fades and cuts.

## Accessibility

**Sizes, Amalfi against the reference (ink):**

| Element | Amalfi | Reference |
|---|---|---|
| Digits | 145 tall | 142 |
| Name | 16 | ~15 |
| Placeholder, hint | 13 | 13 |
| Address | 12 | 12 |
| Field | 208 × 32 | 160 × 28 |
| Help button | 32 | 28 |
| Dots | 8 | 6.5 |
| Status glyphs | ~17 | 11–14 |
| Status labels | 13 | 13 |

- **Date:** 32 pt Semibold, cap height about 23 pt, against an estimated 40 pt on the reference. It is the one
  deliberate step down: hierarchy under the 200 pt clock, on the user's "date legible" rule. If the floor must apply
  here too, use 40/48: the clock block grows by 8 and the time box moves down by 8.
- **Legibility on bright wallpapers:**
  - The clock keeps a dark 1 pt rim outside the glass.
  - Light adds `shadows.onPhoto` to every text drawn on the photo, plus the scrim.
  - Increase contrast removes the glass from the clock.
- **Increase contrast:**
  - Clock: solid white with a 2 pt rim.
  - Name: rim (`User/Increase Contrast`).
  - Field and help: `Glass/Lock Increase Contrast`. Dark is `#2A2B30` 95 % with a white 60 % edge; Light is white 95 %
    with a `Text/Primary` 60 % edge.
  - Status Bar: on its plate.
  - Placeholder: `Text/Secondary`, at least 7:1 on the dark fill and 6:1 on the light one.
- **Reduce transparency:**
  - Glass: `Glass/Lock Reduce Transparency`, which is opaque. Dark is `#5B5C5E` (the tint over `Surface/Window`) and
    Light is white, with the edges kept.
  - Clock: white 92 % with no glass.
  - Status Bar: on its opaque plate.
- **Keyboard** (provisional):
  - The field has focus at once.
  - Tab moves field → help button → Status Bar input source, then the power row and the switch-user list when they are
    shown.
  - Return submits.
  - Esc clears the field, or closes the switch-user list.
- **Screen reader** (provisional):
  - The field reads "Password, secure field".
  - The submit button reads "Unlock".
  - The help button reads "Show password hint".
  - Collapsed banners read the app name and the count, never the hidden content.

## Provisional values (summary)

These were not observed on the reference, or are Amalfi assumptions:

- the Light scrim and the on-photo shadow;
- the Increase contrast and Reduce transparency treatments;
- the power row and its action button;
- the switch-user list and Cancel;
- the minute-roll travel, mask and ones delay;
- the lock-fade offset between clock and user block (30 ms);
- the submit fade;
- the reduced-motion shake;
- the keyboard order and the screen-reader labels;
- the placeholder, hint and address strings (to be localized: English source, Italian translation).
