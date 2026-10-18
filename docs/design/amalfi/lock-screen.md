# Amalfi lock screen

The hand-off spec of the Silica 1.0 "Amalfi" lock screen. Layout and behaviour follow the reference desktop as
measured in [`docs/spec/lock.md`](../../spec/lock.md) (L-01); the look is Amalfi. Sizes are never smaller than the
measured ones (user rule): every measured size is kept or rounded **up** to the 4 pt grid.

Kit: page **Lock Screen** of the Amalfi UI kit (symbols `Lock Screen/…` on the Symbols page). Exports:
[`silica-design/kit/pages/Lock Screen`](https://github.com/silica-os/silica-design/tree/main/kit/pages/Lock%20Screen) (one
PNG per artboard) and `kit/symbols/Lock Screen/…` (JSON layer tree and 2× PNG per master).

Values marked **provisional** were not measured on the reference and are Amalfi derivations. Replace them when a
measurement exists (open a `measurement` issue in silica-reference first).

## Screen layout (binding)

The rule fits every screen size: **the top block is placed from the top edge, the bottom block from the bottom edge,
and everything is centred on the vertical axis**, except the help button and the status items. W and H are the screen
size in pt. The kit draws 1920 × 1200 (and 2560 × 1440 as the reference size of the measurements).

| Element | Frame (pt) | 1920 × 1200 | Measured (L-01) |
|---|---|---|---|
| Status items | right edge at W − 16, y 0, 32 tall (items 28 at y 2) | x 1740, 164 × 32 | name y 8–18, group ends ~25 from the right |
| Clock block | 960 wide, centred, top y 128, 264 tall | x 480 | — |
| Date | baseline y ≈ 167 | | baseline 168 |
| Time digits | top y ≈ 199, baseline y ≈ 344 (digit height ≈ 145) | | top 200, baseline 341 (142) |
| User block | 320 × 108, centred, top H − 258 | y 942 | — |
| Avatar | 72 circle at the top of the user block | y 942–1014 | 72 |
| Name | text box 22 tall, 12 below the avatar | y 1026–1048 | baseline 19 above the field |
| Password field | 160 × 28, centred, top H − 136 (bottom 108 above the screen edge) | x 880, y 1064 | 160 × 28, bottom 106 |
| Help button | 28 circle, 8 right of the field, same centre; **not counted in the centring** | x 1048 | 28, 6 right |
| Hint block (hint state) | 256 wide, centred, bottom 28 above the field (top H − 248) | x 832, y 952 | link row 29 above the field |
| Help address (hint state) | 256 × 16, centred, 12 below the field (top H − 96) | y 1104 | 12 below |
| Banners | 360 wide, right edge W − 16, first top y 48, next 8 below | x 1544 | 344, 16 from the right, top 46 |
| Power buttons row (provisional) | 288 × 72, centred, 20 below the field (top H − 88) | x 816, y 1112 | not observed |
| Switch user list (provisional) | user blocks at the user block row, 240 pitch, centred as a group; Cancel button 80 × 72 centred at top H − 124 | | not observed |

The field width and height never change; long names and placeholders are clipped or wrapped inside their own boxes
(name: one line, ellipsis; hint: wraps at 256).

## Components (kit masters)

Every master exists in Light and Dark unless noted. Text styles and swatches are the kit's (`tokens.json`).

| Master | Size | Content |
|---|---|---|
| `Lock Screen/Clock/Standard/<App>` | 960 × 264 | date `Inter 40 Semibold`, time `Inter 200 Bold`, tabular digits (`tnum` in code; the kit text uses Inter's default figures), both centred |
| `Lock Screen/Clock/Increase Contrast/Dark` | 960 × 264 | provisional, see Accessibility |
| `Lock Screen/User/Default/<App>` | 320 × 108 | avatar 72 (`Glass/Lock` disc + `person-fill` glyph 40, `Text/Secondary` light, white 80 % dark), name `Title 3` (16/22, 600) white, centred |
| `Lock Screen/Password Field/<State>/<App>` | 160 × 28 | states `Placeholder`, `Typing`, `Verifying`, `Wrong`; `Placeholder Increase Contrast` (Dark, provisional) |
| `Lock Screen/Help Button/Rest/<App>`, `…/Increase Contrast/Dark` | 28 × 28 | `Glass/Lock` disc, `question` glyph 28 (its circle draws ~23 pt), `Text/Secondary` |
| `Lock Screen/Hint/Default/<App>` | 256 × 84 | hint `Headline` (13/18, 600) white, centred, wraps at 256; 12 below, the reset link row: 16 white disc with an `arrow-right` 12 cut-out, 8 gap, `Body` 13 white; the row is centred as a whole |
| `Lock Screen/Help Address/Rest/<App>` | 256 × 16 | `Callout` 12, `Text/Secondary` dark, white 72 % light, centred |
| `Lock Screen/Status Items/Default/<App>` | 164 × 32 | input source item (label `Body` 13 + `keyboard` glyph 20, 8 apart, 8 padding), `Menu Bar/Item/Status` Wi-Fi and Battery (36 × 28, glyph 20), 4 apart; always white over the wallpaper |
| `Lock Screen/Notification/Collapsed/<App>` | 360 × 60 | `Glass/Regular` radius 18 (light: plus the `Glass/Tint` layer), app icon 36 at 12, app `Headline`, count `Callout/Secondary` ("3 notifications"), time `Caption/Tertiary` right; the body is never shown |
| `Lock Screen/Action Button/Rest/<App>` | 80 × 72 | provisional: 48 `Glass/Lock` circle, glyph 24 `Text/Primary`, label `Callout` 12 white 8 below |
| `Lock Screen/Power Buttons/Provisional/<App>` | 288 × 72 | provisional: Sleep (`moon`), Restart (`restart`), Shut Down (`power`), 24 apart |

### Password field states

| State | Content |
|---|---|
| Placeholder (empty, focused) | caret 2 × 16 at x 12, y 6, `Text/Primary`, blinking; placeholder "Enter Password" `Body/Secondary` 13 at x 18 |
| Typing | one 8 pt dot per character, first at x 16, 12 pt pitch, centre y 14, `Text/Primary`; caret 4 after the last dot; submit button shown (field not empty): 24 ring at x 134, y 2, 1.5 stroke `Text/Secondary`, `arrow-right` glyph 16 inside |
| Verifying | dots, no caret, `Progress/Spinner/Small` (16) in place of the arrow at x 138, y 6 |
| Wrong | after a failure the dots stay and are **selected**: a `Line/Focus` capsule (radius 4) from x 12 to the end of the dots, 16 tall; the submit button stays; the field shakes (see Motion) |

Measured → Amalfi: caret 2 × 16 kept; dots 6.5 → 8, pitch 8.4 → 12, first 15 → 16; submit ring 22 → 24 (stroke 1.5
kept, 2 from the right edge); help ring 22 → ~23 (glyph 28); help gap 6 → 8.

## Materials and colours

| Part | Dark | Light (provisional) |
|---|---|---|
| Wallpaper | the desktop picture at the same framing, **not blurred**, plus a black overlay of 6 % (`#0000000F`) over the whole screen; no vignette, no shading behind the clock | same |
| Field, help, avatar disc, action buttons | `Dark/Glass/Lock`: background blur σ 20, saturation 1.1, white 28 % (`#FFFFFF47`), 1 pt inside edge white 24 %, rim light white 22 % (inner shadow 0, 1, blur 1), no shadow, no refraction | `Light/Glass/Lock`: same blur, white 60 %, edge black 12 %, rim light white 70 %; ink is `Light/Text/Primary` and `Secondary` |
| Clock digits | fill white 78 % (`#FFFFFFC7`) over a background blur σ 12 (saturation 1.2) clipped to the glyphs, so the wallpaper colour shows through; 1 pt white 90 % line inside the outline; 1 pt black 24 % rim outside; no shadow | same, outer rim black 32 % |
| Date | white 84 % (`#FFFFFFD6`), no rim, no highlight | same, plus a soft shadow (black 18 %, y 1, blur 4) |
| Other text over the wallpaper (name, hint, link, address, status items, button labels) | `Dark/…/Primary` (`#F4F5F7`), address `Secondary` | white (`Light/…/On Accent`) with a soft shadow (black 18 %, y 1, blur 4) |
| Banners | `Glass/Regular` | `Glass/Regular` + `Glass/Tint` |
| Selection of the dots | `Dark/Line/Focus` | `Light/Line/Focus` |

`Glass/Lock` is the new material for the lock screen (`tokens.json` › `materials` › `Glass/Lock`, row in
[`materials.md`](materials.md)): it matches the measured field (a blur that leaves no detail across 28 pt, plus 30–40 %
white, a faint lighter edge and no dark border) instead of the darker Glass/Regular tint. In the compositor it is a
glass region with no refraction and no shadow; the blur radius never animates.

Over a light wallpaper the text over the photo stays white (the reference does the same over photos); the soft shadow
and the darker clock rim in Light are the only legibility aids, and they are provisional.

## States (kit artboards)

| Artboard | What it shows |
|---|---|
| Default / Dark | rest: status items, clock, user block, empty focused field, help button |
| Typing / Dark | dots, caret, submit button |
| Wrong password / Dark | selected dots, submit button, shake annotation (±14 pt extremes, peaks) |
| Hint after second failure / Dark | avatar and name replaced by the hint block; field in place with the selected dots; help address below |
| Wake, step 1 / Dark | display just on: wallpaper (dimmed) and status items only |
| Wake, step 2 / Dark | +150 ms: date, clock, avatar and name, user block 36 pt higher, no field |
| With notifications / Dark | two collapsed banners (app + count) top right |
| Power buttons (provisional) / Dark | the optional Sleep / Restart / Shut Down row |
| Switch user (provisional) / Dark | the list of users and Cancel |
| Default (provisional) / Light | light appearance |
| Increase contrast (provisional) / Dark | increase contrast |
| Default 2560 × 1440 / Dark | the same rule at the reference size |
| Parts / Dark, Parts / Light | every master on the wallpaper |

Rules between states:

- **Typing** starts from Placeholder at the first key; the submit button appears with the first character and goes
  away when the field is emptied.
- **Verifying** follows Return or a click on the submit button; success unlocks, failure goes to **Wrong**.
- **Wrong** after the first failure keeps the user block. After the **second** failure (when password hints are on in
  Settings) the avatar and name are replaced by the hint block **in one frame** (no fade); the field does not move.
  Typing again replaces the selected dots.
- **Notifications** while locked: same place and size as on the desktop; app name and count only.
- **Power buttons** (provisional) show only with the setting "Show the sleep, restart and shut down buttons"
  (`docs/spec/misc.md`, lock screen settings).
- **Switch user** (provisional) shows only when the user switcher setting is "list of users"; Esc or Cancel returns.

## Motion

Curves and times are the measured ones (`docs/spec/lock.md`, "Animation timings", and `motion.md`: measured motion is
binding). Distances are kept in pt because the lock screen geometry keeps the measured sizes.

| Event | Motion | Status |
|---|---|---|
| Lock (appear) | desktop windows fade out slightly faster than the wallpaper changes; clock, date and user block fade in over **~250 ms**, almost linear with a soft end (progress ≈ 0.6 at half time; fit `cubic-bezier(0.4, 0.4, 0.7, 1)`); the user block starts slightly after the clock (offset unmeasured: 30 ms, assumption); nothing slides or scales | measured, offset assumed |
| Unlock (disappear) | first, in one frame, the user block returns to rest (hint gone, avatar back, field empty); then the whole lock screen fades to the desktop, decelerating: 95 % at **~235 ms**, done at **~350 ms** (fit: 1 − e^(−t / 78 ms), cut at 350 ms) | measured |
| Wake | 0 ms: wallpaper and status items; **+150 ms**: date, clock, avatar and name, user block 36 pt higher, no field; **+20 ms**: field and help button, user block in place. All cuts, no animation | measured |
| Display off | cut to black after the idle delay (setting; ~9–10 s after locking with the settings in use) | measured |
| Minute change | only the changing digits roll **upwards** over **~900 ms**, decelerating (fit `cubic-bezier(0.2, 0, 0, 1)`): the old digit moves up and fades out, the new one comes up from below and fades in, through a soft vertical fade mask at the top and bottom of the digit box; the ones digit starts after the tens digit. Travel = the digit height (145 pt), mask 24 pt and ones delay 60 ms are **assumptions** | timing measured, distances assumed |
| Wrong password shake | the field and the help button move together, horizontally only; the user block stays. Damped oscillation, first to the left, ~3 cycles at ~8 Hz (period ~125 ms), **~400 ms** in all. Keyframes (pt, ms): 0 @ 0, −10 @ 31, +14 @ 94, −11 @ 156, +10 @ 219, −7 @ 281, +6 @ 344, 0 @ 400; ease-in-out between keyframes (sine-like) | measured |
| Hint state | replaces avatar and name in one frame | measured |
| Notification banner in | slides in from the right edge with the desktop banner motion (exponential slide, τ 71 ms, ~400 ms; fade ~210 ms; comes into focus from a blur ~215 ms; no overshoot), `docs/spec/notifications.md` | measured |
| Caret | blinks like every Amalfi text field | — |
| Buttons (help, submit, action) | hover/press overlays `fade.quick` in, 60 ms out (`motion.md`) | token |

Reduce motion: the shake becomes a single 400 ms change of the field edge to `Palette/Terracotta` and back (provisional),
the minute roll becomes a `fade.quick` cross-fade; lock, unlock and wake keep their fades and cuts.

## Accessibility

- **Sizes:** no size below the measured one (date 40, time 200 with ≥ 142 pt digits, name 16 ≥ 15, placeholder and hint
  13, address 12, field 160 × 28, help 28, dots 8 ≥ 6.5, status glyphs 20 drawing ~17). Text never below 11 pt.
- **Increase contrast** (provisional, `…/Increase Contrast/Dark` masters): date and digits solid white, digits with a
  2 pt black 40 % rim outside (date 1 pt), no inner line and no blur; the field and the help button use
  `Dark/Glass/Lock Increase Contrast` (opaque `#2A2B30` 95 %, 1 pt white 60 % edge); glyphs `Text/Primary`;
  placeholder `Body/Secondary` (≥ 7:1 on the fill). Light increase contrast follows the same rules with the light
  tokens (no master yet).
- **Reduce transparency** (provisional): `Glass/Lock` becomes the materials fallback (tint over `Surface/Window`,
  edges kept), the clock digits drop the background blur and use white 92 %.
- **Keyboard** (provisional): the field has focus at once; Tab moves field → help button → (power row, switch-user list when shown);
  Return submits; Esc clears the field, or closes the switch-user list.
- **Screen reader** (provisional): the field reads "Password, secure field"; the help button reads "Show password hint"; collapsed
  banners read the app name and count, never the hidden content.

## Provisional values (summary)

Light appearance; Increase contrast and Reduce transparency; the power buttons row and its action button; the
switch-user list and Cancel; the Light text shadows and darker clock rim; the minute-roll travel, mask and ones delay;
the lock fade offset between clock and user block; the reduced-motion shake replacement; keyboard order and screen-reader labels; the placeholder, hint and
address strings (to be localized, English source and Italian translation).
