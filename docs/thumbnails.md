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
pure functions), `mime.rs`, `apps.rs`, `folder.rs` (what a path is), `compose.rs` (drawing).

## Pipeline

1. **Resolve** what the path is: a folder (its colour, symbol, whether it has items), or a file
   (kind or category, handling app, extension).
2. **Plan**: all decisions for one icon at one size (layers with their tint, symbol with its
   box and weight, app icon box, label text, size and baseline) as plain data.
3. **Cache**: the file is `$XDG_CACHE_HOME/silica/icons/<size_px>/<md5(plan)>.png`, the MD5 of
   the plan and the `silica.assets` content version. Equal looks share one file; a change of
   style, content of a folder, handler or assets gives another plan and so another file, nothing
   is invalidated. Old files are not removed by the service.
4. **Render** (resvg for the SVG layers and symbols, ab_glyph for the label) and write the PNG.

The icon side in points is `size_px / scale`; the symbol weight and the label depend on it.

## Files

Order: `back`, glyph, `front`, label. The front is the folded corner; nothing drawn reaches it.
Colors are the same in every theme (no dark layers).

| Element | Box (x, y, w, h) | Details |
| --- | --- | --- |
| Glyph | 262, 262, 500, 500 | the symbol's 24 u canvas fills it |
| App icon | body on 303.67, 303.67, 416.67, 416.67 | scale 0.50566: the 1024 app canvas becomes 517.8 at (253.10, 253.10); drawn as it is, not recolored |
| Label with a glyph | 270.64, 763.7, 482.72, 136.04 | Inter 500, size 111.67 |
| Label alone | 219.08, 362.8, 585.84, 212.83 | Inter 900, size 175.53 |

- **Symbol** weight by icon side in points: Light from 128, Regular from 64, Semibold from 16;
  below 16 no glyph. Flat, opacity 1, `currentColor` replaced by the kind's color.
- **Label**: the extension without the dot, uppercase, at most 5 characters (a `.tar.gz` is
  `GZ`), never truncated; wider than the box it shrinks proportionally (`XHTML`, `QCOW2` fit at
  full size); centered horizontally, baseline at `y + h/2 - 0.605 size + 0.96875 size`
  (872.34 with a glyph, 533.06 alone); not drawn below 32 pt. No extension or more than 5
  characters: no label, and when there is no glyph either, the `fallback` kind's symbol.
- **Choice**: a real preview (image, PDF) first, from the `Thumbnail` generators. Else the glyph
  is, in order: the Silica icon of the app that opens the file (below); the symbol of the file's
  kind; none (label alone). The label is the kind's color next to its symbol, the category's color
  next to an app icon or alone.
- **Default app**: for the file's type and then its parents (shared-mime-info `subclasses`), the
  first desktop id of `[Default Applications]` in the `mimeapps.list` files (XDG order), else
  `[Added Associations]`, else the first desktop entry by file name whose `MimeType=` lists it.
  Only that app counts: its icon is `X-Silica-Icon=` (an absolute icon folder, whose `icon.png`
  is drawn; or a plain name of `silica.assets`), else an absolute `Icon=` PNG. An app without a
  Silica icon (any third-party app) gives none, and the file shows its kind's symbol.
  Desktop entries are re-read every 60 s.
- **Type resolution** (the map in `silica.assets`, `[file.types]` of `compose.toml`): the type
  from shared-mime-info `globs2` by name (heaviest glob, then longest; `*.tar.gz` before `*.gz`)
  or, with no match, a few magic numbers (PDF, ELF, `#!`); aliases are canonicalized; then the
  type and its parents in order, each exact then `major/*`; then the lowercase extension; then no
  kind. A MIME entry may give a kind or only a category (`application/json` is data, so it does
  not become code through its parents). The category of a file with no kind: the extension
  override, the MIME entry's category, else by type (`text/*` and office documents document,
  data formats data, sources and scripts code, archives, media, other). Known gap: the
  ambiguous names (`.ts`) are not disambiguated by content.
- **Kinds** (symbol, color): pdf `doc-text` #e2332b, text `text-align-left` #8e8e93, document
  `doc-text` #2f7cf6, spreadsheet `chart-bar` #1f9d55, presentation `chart-pie` #f2780c, font
  `text-size` #ff2d55, disk-image `disc` #8e8e93, executable `terminal` #ff6a00, web `globe`
  #2f7cf6, code `code` #7a5af8, archive `archive` #8e8e93, image `photo` #2f7cf6, audio
  `music-note` #ff2d55, video `film` #8e8e93, unknown `question` #8e8e93. **Categories**:
  document #2f7cf6, data #1f9d55, code #ff6a00, archive #8e8e93, media #e8336d, other #636366.
  The complete map (about 150 MIME types, 170 extensions) is `[file.types]` in `compose.toml`.
- **Fonts**: Inter from `silica.assets` (`Inter-Regular|Medium|SemiBold|Bold|Black`); the face
  closest to the asked weight is used. Today silica-ui ships no Black, so the label alone is
  Bold until `Inter-Black.ttf` is added to `silica-ui/fonts`.

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
- **Symbol** box 354, 434, 316, 316 (center 512, 592), bands by points: Light from 128, Regular
  from 64, Semibold from 32 (no highlight), Semibold from 16 (no highlight, scale 1.272 around the
  center); below 16 none.
- **Engraving**: first the symbol in white at 0.5 shifted down 4 canvas units (at least half a
  pixel), then the symbol in #1a64c8 at 0.78 over it; both go through the tint ramp.
- **Style** (what Mustache and the shell store): `user.silica.folder` extended attribute, JSON
  `{"tint": "#rrggbb"|label, "symbol": name}`; on file systems without user attributes the GIO
  metadata `metadata::silica-folder`; else the first colour tag of `user.xdg.tags` (English or
  Italian names) tints. Invalid values are dropped. A symbol that is not in the set keeps the
  special folder's own.
- **Special folders** (symbol when no custom one): the `xdg-user-dirs` keys (`DESKTOP` display,
  `DOCUMENTS` doc-text, `DOWNLOAD` arrow-down-circle, `MUSIC` music-note, `PICTURES` photo,
  `VIDEOS` film, `TEMPLATES` doc-plus, `PUBLICSHARE` share) from `user-dirs.dirs`; `HOME` house
  (the home folder); `APPLICATIONS` square-grid-2x2 (`~/Applications` and the `applications`
  folders of the XDG data dirs); `LIBRARY` books-vertical (`~/.local/share`). `SHARED` and
  `NETWORK` have no fixed path and are known only by the file manager: not resolved by the service.

## What silica_ui and the apps must change

- `SilicaThumbnail` / icon widgets ask `Icon(uri, size_px, scale, flags)` for files and folders
  and show the PNG; the old per-type icons and `folder-generic` manifest fields (`folders`,
  `tint`, `effect`, symbol bands) no longer exist in silica-icons.
- `resolveFolderIcon` stops reading the special-folder map and the tint from `folder-generic`:
  the service does it; for a path that is not a local folder (a network share) the widget keeps
  its drawn fallback or requests the symbol by a custom style.
- `SilicaAssets` slots changed (kinds 9 to 20, `silica_assets.h`); the `type-label`/`type-category`
  slots of the generic file are gone, kinds and categories are `file-kind` entries
  (`kind.pdf`, `category.data`), TYPE/MIME/EXT keys point to them.
- Mustache and the Dock use `Icon`; `silica_ui` composes nothing itself.
