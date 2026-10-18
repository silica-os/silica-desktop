# Composed file and folder icons

`Thumbnails1.Icon` (docs/daemon.md) draws the icon of a file or folder at the exact pixel size
the client asks for: sharp at any scale, rendered once, shared by every app and the shell.
silica-icons provides only the **layers** (SVG) of `file-generic` (`back`, `front`) and
`folder-generic` (`back`, `middle`, `front`). Every composition rule below belongs to the
generator and lives here, in `crates/silica-assets/data/compose.toml` (a build-time source,
compiled into `silica.assets`, see [`assets-format.md`](assets-format.md); nothing parses it at
run time). Numbers are on the 1024 canvas and scale with the side; the numbers were seeded from
silica-icons a8b2a27 (`files/file-generic/icon.json` and `docs/icon-format.md`).

The code is `crates/daemon/src/icons/`: `rules.rs` (rules read from the assets and the geometry,
pure functions), `mime.rs`, `apps.rs`, `folder.rs` (what a path is), `thumbnailer.rs` (freedesktop thumbnailers),
`compose.rs` (drawing).

## Pipeline

1. **Resolve** what the path is: a folder (its colour, symbol, whether it has items), or a file
   (its extension and the app that opens it).
2. **Plan**: all decisions for one icon at one size (layers with their tint, symbol with its
   box and weight, app icon box, label text, size and baseline) as plain data.
3. **Cache**: the file is `$XDG_CACHE_HOME/silica/icons/<size_px>/<md5(plan)>.png`, the MD5 of
   the plan and the `silica.assets` content version. Equal looks share one file; a change of
   style, content of a folder, handler or assets gives another plan and so another file, nothing
   is invalidated. Old files are not removed by the service.
4. **Render** (resvg for the SVG layers and symbols, ab_glyph for the label) and write the PNG.

The icon side in points is `size_px / scale`; the symbol weight and the label depend on it.

## Files

A file has **no symbol of its own type**. Its icon is decided by the app that opens it, as on the
reference desktop; a file nobody opens is an unknown extension.

Order: `back`, glyph, `front`, label. The front is the folded corner; nothing drawn reaches it.
Colors are the same in every theme (no dark layers).

| Element | Box (x, y, w, h) | Details |
| --- | --- | --- |
| Glyph (document foreground) | 262, 262, 500, 500 | an SVG of the app, fitted (aspect kept) and centered |
| App icon | body on 303.67, 303.67, 416.67, 416.67 | scale 0.50566: the 1024 app canvas becomes 517.8 at (253.10, 253.10); composed, not recolored |
| Label with a glyph | 219.08, 744.5, 585.84, 170.05 | Inter 500, size 139.59 (25 % over the former 111.67) |

- **Choice**, in order: (1) a **real preview** (image, PDF) from the built-in generators; (2) the
  **preview an app's thumbnailer draws** ("Thumbnailers" below); (3) the glyph of the **default
  app**: its **document foreground** for the file's type if it declares one ("Document icons"
  below), else its **app icon**; (4) with no handler, or a handler without a usable icon (a
  third-party app), the **unknown glyph** (`files/system/unknown.svg`, a circled question mark) on the
  generic sheet, with the extension label when it has one of 1 to 5 characters. A file with no
  extension, or one of more than 5 characters, has the glyph and no label.
- **Label**: the extension without the dot, uppercase, at most 5 characters (a `.tar.gz` is
  `GZ`), never truncated; wider than the box it shrinks proportionally (`XHTML`, `QCOW2` fit
  at full size); centered horizontally, baseline at `y + h/2 - 0.605 size + 0.96875 size`
  (880.30); not drawn below 32 pt. The box sits under the glyph box
  (which ends at 762) and inside the sheet. **One colour for every file**: `Text/Secondary`
  (light) of `docs/design/amalfi/tokens.json`, `#5f636b` (`[file.label].color` of `compose.toml`; files have no dark variant).
- **Default app**: for the file's type and then its parents (shared-mime-info `subclasses`), the
  first desktop id of `[Default Applications]` in the `mimeapps.list` files (XDG order), else
  `[Added Associations]`, else the first desktop entry by file name whose `MimeType=` lists it.
  Only that app counts. Its **app icon** is composed with the `AppIcon` composition (below), light
  appearance, from the `icon.assets` of its `X-Silica-Icon=` folder, at the size of the body box
  (so `min_size` layers follow the size in points); a folder whose `icon.assets` is missing or
  damaged gives its `icon.png`; a plain name of `silica.assets` (the shell's own apps) gives
  that embedded icon; else an absolute `Icon=` PNG. An app without any of these (any third-party
  app) gives none and the file is an unknown extension. Desktop entries and thumbnailers are
  re-read every 60 s.
- **Type resolution** only finds the handler and the label text: the type from shared-mime-info
  `globs2` by name (heaviest glob, then longest; `*.tar.gz` before `*.gz`) or, with no match, a
  few magic numbers (PDF, ELF, `#!`); aliases are canonicalized; the chain is the type and its
  parents. Nothing maps a type to a kind, category, symbol or colour. Known gap: the ambiguous
  names (`.ts`) are not disambiguated by content.
- **Fonts**: Inter from `silica.assets` (`Inter-Regular|Medium|SemiBold|Bold|Black`); the face
  closest to the asked weight is used. silica-ui ships `Inter-Black.ttf` since the composed-icons change
  (until that silica_ui is in the image the label is Medium).

### Thumbnailers

An app that opens a type can draw the preview of its documents with a freedesktop thumbnailer
(Thumbnail Managing Standard): a file `thumbnailers/<name>.thumbnailer` in `$XDG_DATA_HOME` or
`$XDG_DATA_DIRS` (the first with a given file name wins):

```ini
[Thumbnailer Entry]
TryExec=/usr/libexec/silica-notes/thumbnail
Exec=/usr/libexec/silica-notes/thumbnail -s %s %i %o
MimeType=application/x-silica-note;text/markdown;
```

`Exec` words are split on spaces (double quotes group) and run **without a shell**: `%i` the input
path, `%u` its URI, `%o` the output PNG path, `%s` the edge in pixels, `%%` a percent; `%o` is
required. `TryExec` (or the program of `Exec`) must exist, as an absolute path or in `PATH`. The
type is matched against the file's chain (type, then parents), the nearest type first.

Bounded: the program gets no stdin, its output is discarded, it runs in a fresh temporary folder,
is **killed after 15 s**, and its picture is refused above 64 MiB. Order: the built-in generators
(images, PDF) always win; the thumbnailers run only for what they cannot draw. The result is
cached like any other preview (`$XDG_CACHE_HOME/thumbnails/<bucket>/`, `Thumb::MTime`), and a
failure (no program, non-zero exit, timeout, not a PNG/image) is recorded in
`fail/silica-daemon/` so the program does not run again until the file changes. It is used by
`Thumbnail` and `Icon` alike; `Icon` with `NO_PREVIEW` never runs one.

### Document icons

An app can give its documents a dedicated icon without a thumbnailer, in two forms: a **document
foreground** (the sheet and the label stay the system's) or a **full icon** (it replaces the whole
sheet). The convention:

1. In the app's desktop entry, `X-Silica-Document-Icons=<mime>=<name>;<mime>=<name>;...`, with
   `<mime>` an exact type (`application/pdf`) or a group (`text/*`), and `<name>` made of
   letters, digits, `.`, `_`, `-` (not starting with a dot).
2. For each name, under `<X-Silica-Icon folder>/documents/` (that is, `icon/documents/` in the
   app's repository, next to `icon.json`/`icon.assets`), one of:
   - `<name>.svg` or `<name>/foreground.svg`: a foreground, composed between `back` and `front`
     in the 500 x 500 glyph box; any aspect ratio (fitted and centered).
   - `<name>/icon.svg`, or `<name>/back.svg` + `<name>/front.svg` (drawn in that order): a full
     icon, each layer fitted and centered in the whole 1024 canvas (draw the sheet yourself; a
     square SVG fills the canvas exactly). The extension label is **not** drawn on a full icon,
     unless the folder also holds a file named `label` (any content, usually empty): then the
     system label (the one with a glyph, under the glyph box) is drawn over the icon.
   Every SVG is drawn as it is: no recolouring, one appearance (files have no dark form), up to
   2 MiB each. A folder wins over a plain `<name>.svg`; in a folder a full icon wins over
   `foreground.svg`; `back.svg` without `front.svg` (or the opposite) is not a full icon.

```ini
X-Silica-Icon=/usr/lib64/silica/apps/notes/icon
X-Silica-Document-Icons=text/markdown=markdown;application/x-silica-note=note;
```

The entry must be the file's default handler. The type is looked up through the file's chain
(type, then parents), exact first and then the group, so `text/markdown` also covers its
subtypes. A name whose SVG is missing, too big or cannot be parsed gives nothing (for a full icon
this is checked before drawing) and the file falls to the next step of the precedence. The
source is part of the cache key with each SVG's mtime and size, so replacing a file regenerates
the icon.

### System types

Structural types with no app (executable, archive, disk image, link, package, generic data) get a
**system foreground** in the glyph box of the generic sheet. They are listed in `[file.system]` of
`crates/silica-assets/data/compose.toml` (compiled into `silica.assets`): per name, `mime` (a type
or `major/*`, matched against the file's chain) and `ext` (an extension without the dot, matched
first). The artwork is `files/system/<name>.svg` of silica-icons, embedded when the build has it
(`file.system.<name>` in the `Role` key space); a name without artwork is skipped silently and the
file stays an unknown extension. Used only when no app icon applies (no handler, or a handler
without a Silica icon).

### Precedence

What the service answers for a file, first match wins:

1. a real preview (images, PDF: built-in generators);
2. a thumbnailer of an app (the two are skipped by `Icon` with `NO_PREVIEW`);
3. the full icon of the default app (`documents/<name>/icon.svg` or `back.svg` + `front.svg`);
4. the document foreground of the default app (`documents/<name>.svg`, `<name>/foreground.svg`);
5. the app icon on the generic sheet, with the label;
6. the system type foreground on the generic sheet, with the label;
7. the generic sheet with the `unknown` glyph and the extension label (unknown extension; no label
   without an extension or with more than 5 characters).

## Folders

Order: `back`, `middle`, `front`, then the symbol.

- `middle` (the white sheet) only when the folder has at least one item; an unreadable folder
  counts as empty.
- **Tint** on `back`, `front` and the symbol (with its highlight), never `middle`: per pixel, with
  L = 0.2126 R + 0.7152 G + 0.0722 B (sRGB 0..1), from `luminance` 0.35 to 0.85 the color goes
  from tint x 0.72 (`shade`) to the tint lightened toward white by 0.5 (`glow`); clamped outside.
  Affine, so tinting the layers equals tinting the composed folder. No tint: the blue drawing.
  A tint is `#rrggbb` or a label: red #f2382e, orange #f98a17, yellow #f5c400, green #2fb350,
  blue #2f7cf6, purple #a052e0, gray #8e8e93.
- **Symbol** box 335.04, 415.04, 353.92, 353.92 (center 512, 592: the former 354, 434, 316, 316
  enlarged by 12 % around the same centre), bands by points: Light from 128, Regular
  from 64, Semibold from 32 (no highlight), Semibold from 16 (no highlight, scale 1.272 around the
  center); below 16 none.
- **Engraving**: first the symbol in white at 0.5 shifted down 4 canvas units (at least half a
  pixel), then the symbol in #1a64c8 at 0.78 over it; both go through the tint ramp.
- **Style** (what Mustache and the shell store): `user.silica.folder` extended attribute, JSON
  `{"tint": "#rrggbb"|label, "symbol": name}`; on file systems without user attributes the GIO
  metadata `metadata::silica-folder`; else the first colour tag of `user.xdg.tags` (English or
  Italian names) tints. Invalid values are dropped. A symbol that is not in the set keeps the
  special folder's own.
- **Icon styles** (silica-desktop#232): `Icon` flags bits 1 to 3 carry the system's icon style as
  the `AppIcon` appearance word (0 Default, 1 clear, 2 tinted, plus 4 for the dark forms; bit 0 is
  `NO_PREVIEW`) and bits 8 to 31 the accent (`0xRRGGBB`, 0 the default `#0e77e6`). Only folders
  change; a file is the same in every style. silica-icons has no dark or clear folder artwork, so
  `[folder.styles.<style>]` in `compose.toml` recolours the same three layers (`FolderStyle` slots):
  per layer role a base colour or the accent, the luminance ramp of the labelled tints and an
  opacity, and for the symbol the engraving. Dark is graphite; tinted is the accent through the
  tint ramp (deeper in the dark form); clear and clear dark are white or graphite glass drawn
  translucent, with no colour of their own (the folder's tint is ignored; in every other style it
  replaces the accent or base colour of the back and front layers, never of the paper). The values
  are ours, by analogy with the app icons (nothing measured). A style the assets lack is Default.
- **Special folders** (symbol when no custom one): the `xdg-user-dirs` keys (`DESKTOP` display,
  `DOCUMENTS` doc-text, `DOWNLOAD` arrow-down-circle, `MUSIC` music-note, `PICTURES` photo,
  `VIDEOS` film, `TEMPLATES` doc-plus, `PUBLICSHARE` share) from `user-dirs.dirs`; `HOME` house
  (the home folder); `APPLICATIONS` square-grid-2x2 (`~/Applications` and the `applications`
  folders of the XDG data dirs); `LIBRARY` books-vertical (`~/.local/share`). `SHARED` and
  `NETWORK` have no fixed path and are known only by the file manager: not resolved by the service.

## App icons

`AppIcon(s app, u size_px, d scale, u appearance, u accent_rgb) -> s path` composes the icon of
an app from its `icon/icon.assets` (docs/assets-format.md, "App icon file"), the port of
`tools/compose.py` of silica-icons (docs/icon-format.md, Format 2):

- `app`: a desktop id (`dev.frascella.Clock`, with or without `.desktop`; its entry's
  `X-Silica-Icon=` folder) or an absolute icon folder.
- `appearance`: 0 light, 1 clear, 2 tinted; add 4 for the dark form (4 dark, 5 clear dark,
  6 tinted dark), 8 for bare (below). Any other value gives "".
- `appearance` bit 8 (value 8, with 1 or 2 and 4: 9, 10, 13, 14): the **bare** composition, only
  the foreground layers on a transparent body, without the generated background. The host puts
  the compositor's glass behind a Clear icon (a glass region with the body shape) and draws this
  on top, so the body is the compositor's blur and not a baked translucent fill (silica-desktop#232).
  The cache name has a `-bare` suffix.
- `accent_rgb`: 0xRRGGBB, used by the tinted looks only (0 is `Accent/Amalfi`, `#0e77e6`).
- Result: the path of a PNG of exactly `size_px` (at most 2048) pixels, `scale` giving the side
  in points for `min_size`; "" when the app has no `icon.assets`, it is damaged or the request
  is invalid, and the client then shows the bundled `icon.png` / `icon-dark.png`.
- Not in the PNG: layers with `dynamic` data (Clock hands and cap, Calendar date: the host draws
  them live on top, from the layer data of the same `icon.assets`), the cast shadows of the
  layers and the rim (the host draws them, as in the reference). The clip is the Sketch body outline
  (`tools/body.svg` of silica-icons: 824 at (100, 100), radius 214, smoothing 0.6). Layers are
  rendered at 4 x 4 samples per pixel in premultiplied float, with the filter regions widened to the
  reach of their shadows (the Sketch export cuts some), as `compose.py` does.
- Cache: `$XDG_CACHE_HOME/silica/icons/app/<digest>-<appearance>-<px>-<pt*10>[-<accent>].png`;
  the digest is the icon's, so an updated app regenerates, and the accent is part of the key only
  for the tinted looks. `Ready(app, size_px, path)` follows a success.
- Tests: `crates/daemon/tests/appicon_golden.rs` compares the output with PNGs rendered by
  `compose.py` (`tests/fixtures/appicon/gen.py`): a synthetic icon always, and the pilot icons
  (Mustache, Terminal, Clock) when `SILICA_ICONS_DIR` points at silica-icons on `amalfi-icons`.

## What silica_ui and the apps must change

- `SilicaThumbnail` / icon widgets ask `Icon(uri, size_px, scale, flags)` for files and folders
  and show the PNG; the old per-type icons and `folder-generic` manifest fields (`folders`,
  `tint`, `effect`, symbol bands) no longer exist in silica-icons.
- `resolveFolderIcon` stops reading the special-folder map and the tint from `folder-generic`:
  the service does it; for a path that is not a local folder (a network share) the widget keeps
  its drawn fallback or requests the symbol by a custom style.
- `SilicaAssets` slots changed (kinds 9 to 20, `silica_assets.h`); the file kinds, categories and
  the MIME/UTI/extension map no longer exist (a file's icon is its app's, so there is nothing to
  resolve on the client); the `type-label`/`type-category` slots of the generic file are gone.
- Mustache and the Dock use `Icon`; `silica_ui` composes nothing itself.
