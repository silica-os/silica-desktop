# Silica system assets: `silica.assets`

One compiled, memory-mapped file holds the system's shared assets. The shell, the Flutter apps
(through `silica_ui`), the daemon, the portal and Ossido all read the same file. No app ships
its own copy of these images, fonts or sounds.

Status: design accepted by the user on 2026-10-16. Format version 1.

## What is in it

| Asset | Source | Notes |
| --- | --- | --- |
| Shell app icons | silica-icons `apps/` | only the shell's own, by app id: `apps` (Launcher), `mission-control`, `screenshot` and the `silica` mark. The other Silica apps carry their own icon ("App icons" below) |
| Generic file and folder | silica-icons `files/file-generic`, `files/folder-generic` (layer SVGs only) + `crates/silica-assets/data/compose.toml` (composition rules) | the layers and the rules the thumbnail service composes with, see "Generic file and folder"; a file has no type map: its icon is its app's |
| Volumes, Trash | silica-icons `files/` | by role (external, network, optical, disk image, Trash empty/full) |
| Badges | silica-icons `badges/` | alias, cloud, locked, unreadable, Dock stack close… |
| System symbols | silica-icons `symbols/` | Icon Kit, light/regular/semibold; the SVG as it is (tintable template), not rasterized |
| Fonts | Inter, JetBrains Mono | loaded with `FontLoader` from the mapped bytes |
| Sounds | silica-brand (`sounds/`), meanwhile `data/sounds` of silica-desktop | alert sounds, screenshot, Trash; WAV, OGG or FLAC as they are |
| Wallpapers | silica-wallpapers (CC BY-SA 4.0): `<set>/wallpapers.json`, `<set>/<id>/{light,dark}.png`, `<set>/previews/<id>-{light,dark}.jpg` (today the Amalfi set) | the wallpaper layer and Settings; the PNG files as they are (2880 square, light and dark), the 640 px JPEG previews as the small variant of each theme (`Jpeg` encoding), the title as a `Title` slot. `cargo xtask assets --wallpapers DIR` (default: the `silica-wallpapers` checkout next to this repository) |

Not in it: the icons of Silica's other apps (Mustache, Calculator, Clock, Mail, Terminal,
Settings, Preview, Notes, Text Editor, Calendar, Reminders, Photos, Disks, Fonts, Passwords...:
each app repository and bundle carries its own, see "App icons"); cursors (the system Adwaita
theme, user decision); third-party app icons (their desktop entry `Icon=`, through the XDG icon
theme, is the fallback).

## Sources and compilation

The sources stay readable in their repositories: SVG layers, rendered PNGs and `icon.json`
([silica-icons `docs/icon-format.md`](https://github.com/silica-os/silica-icons/blob/main/docs/icon-format.md)).
A compiler turns them into `silica.assets`:

- `cargo xtask assets [--icons DIR] [--brand DIR] [--fonts DIR] [--sounds DIR] [--out FILE]`
  (crate `silica-assets`, binary `silica-assets-compile`). It is deterministic: the same sources
  give the same bytes (sources are read in sorted order, the pixel math is integer, the header
  has no timestamp). Defaults: icons = branch `amalfi-icons` of the silica-icons checkout next
  to the repository, read with `git archive` into a temporary directory (the checkout is not
  touched; without that branch the checked-out files are used), brand = the silica-brand
  checkout, fonts = `silica-ui/fonts`, sounds = `data/sounds`, out = `target/silica.assets`.
  The old `cargo xtask assets` (the ASSETS.md license check) is now `cargo xtask assets check`.
- System install: `/usr/share/silica/silica.assets` (package `silica-assets`, noarch).
- Development: `target/silica.assets`, built by `cargo xtask assets` and copied by
  `cargo xtask os deploy` (which builds it unless `--no-build`) into the share, where the dev
  session script exports `SILICA_ASSETS`. `SILICA_ASSETS=<file>` overrides the path. `SILICA_ICONS_DIR`
  stays as a fallback for the old loader until the migration ends.

## File layout

Little-endian. Every offset is from the start of the file; every section and every data blob
starts on a 64-byte boundary so pixel data can be handed to the GPU without copying. The file
is written to a temporary name and renamed over the old one, so a process that has the old file
mapped keeps reading the old bytes.

```
Header (128 bytes)
  magic          [u8; 8]  "SILASSET"
  format         u32      1
  flags          u32      0
  content_version [u8; 32] UTF-8, NUL-padded, at least one NUL (e.g. "silica-assets 0.1.0")
  source_hash    [u8; 32] two xxh3-128 digests (different seeds) of the source files read
                          and their names, for rebuild checks (not a security hash)
  section_count  u32      at most 64
  reserved       u32
  sections       offset u64 of the section table
  file_length    u64      total length; a mismatch means a truncated or padded file
  reserved       to 128
Section table: { kind u32, reserved u32, offset u64, length u64 } x section_count
Sections (any order, by kind; unknown kinds are skipped):
  1 STRINGS   UTF-8 strings, NUL-terminated; referenced by offset (offset 0 is "")
  2 ENTRIES   fixed-size Entry records
  3 VARIANTS  fixed-size Variant records
  4 SLOTS     fixed-size dynamic-content records
  5 DATA      pixel data, font files, sound files (each blob 64-byte aligned, identical
              blobs stored once)
  16 + n      LOOKUP, one minimal perfect hash table per key space n (below)
```

The reader checks the whole structure when it opens the file (section bounds and alignment,
record sizes, enum values, string offsets, variant and slot ranges, data ranges, alias targets,
lookup values) and returns an error for anything inconsistent; after that no accessor can be
led out of range.

### Lookup tables

One table per key space, numbered as in the C header: 0 `APP` (icon id such as `mail`, and
desktop application id such as `dev.frascella.Mail` as an alias), 1 `TYPE` (`file-generic`; the kinds and categories of earlier drafts are gone), 2 `MIME`
(`application/pdf`; wildcard keys such as `image/*` are stored literally and the reader tries
the exact type first), 3 `UTI` (`com.adobe.pdf`), 4 `ROLE` (`folder`, `folder.generic`,
`volume.internal|external|network|optical|disk-image`, `trash.empty`, `trash.full`,
`dock.stack-close`, `badge.alias|cloud|locked|unreadable`), 5 `SYMBOL` (`<weight>/<name>`,
e.g. `regular/key`), 6 `FONT` (file stem, `Inter-Regular`), 7 `SOUND` (file stem), 8
`WALLPAPER` (`silica`), 9 `EXT` (extension without the dot, lowercase: `pdf`). `EXT` is an
addition to the first draft: the `[file.types.extensions]` and `[file.overrides]` tables of
`data/compose.toml` were the map from extension to kind or category; it is empty now and the
key space stays reserved.

Each table is a minimal perfect hash (compress, hash and displace; PTHash was not needed at
this size): `{ seed u64, bucket_count u32, key_count u32, displacements [u32; bucket_count],
slots [u32; key_count] }`. A lookup hashes the key with xxh3-64 and the table seed to `h`; the
bucket is `(h >> 32) % bucket_count`; the slot is `mix(h ^ displacement * 0x9E3779B97F4A7C15) %
key_count` (`mix` is the splitmix64 finalizer); the slot holds an entry index. The key string
stored in that entry is compared with the requested key to reject misses. No parsing, no
allocation. The compiler uses the first seed (0, 1, ...) that gives a perfect hash, so the
tables are deterministic. A table with no keys has `bucket_count = key_count = 0`.

Desktop-id keys, and the role names that differ from the icon id, are
`alias` entries (`alias_of` is the entry that holds the images; an alias never points to an
alias). `lookup` resolves them: the caller gets the real entry. The app aliases come from
`crates/silica-assets/data/types.toml` (section `[app]`). The compiler fails if a target does not exist.
The MIME, UTI and EXT key spaces are reserved and empty: no type map ships (the icon of a file is
the icon of the app that opens it, see [`thumbnails.md`](thumbnails.md)). The top-level key `shell_apps` of the
same file lists the app icons that are compiled (`apps`, `mission-control`, `screenshot`,
`silica`): the other `apps/` folders of silica-icons are skipped and `[app]` aliases only those
four. A table without `shell_apps` compiles every app icon (the test fixtures do).

### Entry (64 bytes)

```
name        u32  string offset (the key)
kind        u16  app | file | folder | volume | badge | symbol | font | sound | wallpaper | alias
flags       u16  has_dynamic (1), tintable (2), has_dark (4)
variants    u32  first Variant index
variant_count u16
slot_count  u16
slots       u32  first Slot index
alias_of    u32  entry index this one points to (aliases), or u32::MAX
reserved    [u8; 40]
```

### Variant (32 bytes)

```
theme       u8   default | dark | clear | clear-dark | tinted | tinted-dark
layer       u8   composite | hand-hour | hand-minute | hand-second | cap | background | foreground
encoding    u8   rgba8-premul | qoi | ttf | otf | ogg | flac | svg | png | wav
state       u8   normal | full (a folder with something inside)
size_px     u16  pixel side (square images), 0 for non-images; the width for wallpapers
scale       u16  reserved
data        u64  offset into DATA
length      u32  bytes
width       u16  pixels, 0 for non-images
height      u16  pixels, 0 for non-images
reserved    [u8; 8]
```

Images are stored for pixel sides 16, 32, 64, 128 (raw `rgba8-premul`, ready for
`ImageDescriptor.raw` / GPU upload) and 256, 512, 1024 (`qoi`, of the same premultiplied
pixels). Smaller sides are made from the 1024 image by repeated 2x2 box filtering in integer
arithmetic. A reader picks the smallest stored side >= requested logical size x device pixel
ratio, or the largest if none is, and scales down. `pick` looks for the wanted layer and state
in the requested theme and, if it has none, in the next theme of the fallback chain:
clear-dark -> clear -> dark -> default, tinted-dark -> tinted -> dark -> default, and
dark, clear, tinted -> default.

The entry kind `file-kind` (10) is reserved: no entry has it any more.

Where the images come from: an app icon uses its `rendered` PNGs (six themes, 1024 px); an app
without them is composed from its SVG layers for default and dark; file, folder, volume, Trash
and badge icons are rasterized from their SVG with resvg (`icon.svg`, `icon-dark.svg`). The `background` and
`foreground` layers of an app are not stored: the rendered composites cover the themes. The
generic file and folder are the exception, see below.

### Dynamic slots (48 bytes)

```
kind        u8   clock-hand | clock-cap | calendar-month | calendar-day | folder-emblem |
                 type-label | badge | type-category
unit        u8   clock-hand: hour 0 | minute 1 | second 2; type-label: max characters;
                 badge: corner (top-left 0, top-right 1, bottom-left 2, bottom-right 3)
min_size    u16  smallest icon side (pt) at which the slot is drawn
pivot / box f32 x 4  pivot (x, y, 0, 0) for hands and caps, or box (x, y, w, h) on the 1024 canvas
font        u32  string offset (font entry name) for text slots, u32::MAX if none
size, weight u16 x 2  font size and weight on the 1024 canvas
color       u32 x 2  light and dark 0xRRGGBBAA
format      u32  string offset, u32::MAX if none: "month:upper", "day" (calendar), the case
                 of the label ("upper"), the category name (type-category)
tracking    f32  letter spacing on the 1024 canvas
max_width   f32  widest text on the 1024 canvas (shrinks proportionally beyond it)
```

The first draft said 32 bytes, but its fields add up to 40 and the manifests also need
`tracking` and `max_width`, hence 48.

Clock hands and caps are stored as their own image variants (layer field, themes default and,
when the source has a `-dark` file, dark) and rotated about the pivot at draw time. Calendar
text, file-type labels and folder emblems are drawn at draw time with the font and symbol
entries of the same file. The generic file's label and the category colors are described in "Generic file and folder"
(`type-label` is superseded by `file-label`). A badge
carries one `badge` slot: `unit` the corner, `rect` = (corner x, corner y, the badge's fraction
of the icon side from `min_size` up, its fraction below that), `size` the priority, `weight`
its position in the badge order of `badges.json`. Not stored yet: the extension-to-category
`overrides` of `file-generic`, and the shared-mime-info resolution order (aliases, parents),
which stay with the reader that has the system MIME database.

## Generic file and folder

silica-icons provides only the layer SVGs of `files/file-generic` (`back`, `front`) and
`files/folder-generic` (`back`, `middle`, `front`), listed in their `icon.json` `layers` (and
an informative `content_area`, not stored). Every composition rule belongs to the thumbnail
generator and lives in silica-desktop, in `crates/silica-assets/data/compose.toml`, a
build-time source (TOML, one table `[file]`, one `[folder]`; no JSON anywhere at run time). The
compiler stores it in the entries; the rules and their numbers are in
[`thumbnails.md`](thumbnails.md). `--compose FILE` overrides the source.

What the compiler writes:

- `file-generic` (TYPE) and `folder.generic` (ROLE, with the alias `folder`): an `svg` variant
  per layer (layer `background` = back, `middle`, `foreground` = front; theme default; no dark
  layers), and raster composites for readers that draw no layers (state normal: back + front;
  state full of the folder: with the middle).
- Slots (kinds 9 to 20, `silica_assets.h`): `LayerRule` per step in drawing order (`unit` the
  role, `weight` flags: 1 tint reaches the step, 2 only when the folder has items; the folder
  ends with the `symbol` step, the file's stack lists `glyph` and the label follows it);
  `GlyphBox`, `AppIconBody`, (folder only) `SymbolBand` (from `min_size` pt: weight name, scale, highlight),
  `FileLabel` and `FileLabelAlone` (`size` in 1/64 of a canvas unit, since the box metrics are
  fractional; `color` the one grey of the extension label, `[file.label].color` and
  `[file.label_alone].color`, required); for the folder `FolderEmblem` (the symbol box), `FolderEffect`,
  `FolderTint`, `TintLabel`, `SpecialFolder` (key to symbol name).
- No kind, category or type map: the slots `SymbolBand` (on the file), `FallbackKind` (16), `KindStyle` (15)
  and `TypeCategory` (7) are no longer written for the generic file and
  readers must ignore them; the numbers stay reserved. `[file.glyph.symbol]`, `[file.kinds]`,
  `[file.colors]`, `[file.overrides]`, `[file.types]`, `fallback` and the `[uti]` table of
  `types.toml` are gone from the sources.
- The compiler checks that every symbol named in `compose.toml` exists (light, regular and
  semibold) when the sources have symbols.

## App icons

The icon of every Silica app other than the shell's own is not in `silica.assets`: the app
carries it, so an app can be installed, updated or removed without touching the system file.

Layout, in the app repository:

```
icon/                    the silica-icons apps/<id>/ folder, as it is
  icon.json              manifest (docs/icon-format.md of silica-icons)
  background.svg, foreground.svg, *-dark.svg, layers of dynamic content (hands, cap...)
  icon.png               rendered, default (1024 px); icon-dark.png, icon-clear.png,
  icon-clear-dark.png, icon-tinted.png, icon-tinted-dark.png: the six themes
```

- `cargo xtask bundle <app> [--release]` copies `icon/` to `<bundle>/icon/`, next to
  `flutter_assets` (`build/icon/` for the debug bundle, `build/release/icon/` for the release
  one), replacing an older copy. An app without `icon/` (no `icon.json` in it) is bundled as
  before.
- `cargo xtask install-app <app>` and `cargo xtask os deploy --apps` write the desktop entry
  with `X-Silica-Icon=<absolute path of the installed icon folder>` and `Icon=<that folder>/icon.png`
  (the file `rendered.default` of `icon.json`; 1024 px, launchers scale it), replacing the keys the
  template has in `[Desktop Entry]`. The `@ICON@` and `@ICON_PNG@` placeholders of a template are
  filled with the same values. Templates need neither.
- `X-Silica-Icon` takes two kinds of value. An absolute path is an app icon folder: the shell
  reads `icon.json` there, draws the rendered PNG of the current theme (default, dark, clear,
  tinted, ...) and the dynamic layers (clock hands, calendar text) from the same folder. A plain
  name (`apps`, `mission-control`, ...) is looked up as before in `silica.assets` and then in the
  icons folder (`SILICA_ICONS_DIR`, `silica/icons` in the XDG data folders). A path whose folder
  has no valid icon gives no Silica icon: the entry's `Icon=` is used.
- Fallback: an app whose repository has no `icon/` keeps the entry it has, a plain name looked
  up in the icons folder, so nothing breaks until each app gets its folder.
- Packaging (silica-packages): the app package installs the same folder with its bundle,
  `/usr/lib64/silica/apps/<id>/icon/` next to `/usr/lib64/silica/apps/<id>/flutter_assets/`
  (`<id>` is the desktop application id, e.g. `dev.frascella.Clock`), and its desktop entry in
  `/usr/share/applications/<id>.desktop` has `X-Silica-Icon=/usr/lib64/silica/apps/<id>/icon` and
  `Icon=/usr/lib64/silica/apps/<id>/icon/icon.png`. Optionally the package also installs
  `icon.png` as `/usr/share/icons/hicolor/512x512/apps/<id>.png`, for tools that only read the
  XDG icon theme.

### App icon file: `icon/icon.assets`

`cargo xtask bundle`, `install-app` and `os deploy` also compile the icon folder into
`<bundle>/icon/icon.assets` (`silica-assets-compile --app-icon DIR --out FILE`; crate
`silica-assets`, modules `appicon` and `compile::app_icon`). `icon.json` and the SVG layers are
build-time sources: nothing reads JSON at runtime. The fallback PNGs (`icon.png`,
`icon-dark.png`) and `icon.json` are still copied, for launchers and for a client that cannot
reach the thumbnail service. A compile error is a warning: the app keeps its flat icon.

The thumbnail service (`AppIcon` of `dev.frascella.Silica.Thumbnails1`, docs/thumbnails.md)
composes light, dark, clear and tinted from this file. Both formats of silica-icons compile:
format 2 as it is; format 1 as the background file plus one `body` layer (the foreground, ramp
in clear and tinted, neutral shadow).

Binary layout, little endian, deterministic (same sources, same bytes):

| Field | Type | Meaning |
| --- | --- | --- |
| magic | `"SAIC"` | |
| version | u32 | 1 |
| digest | u64 | xxh3-64 of the payload; names the icon in caches |
| background | 2 x background | light, dark. Gradient: `0`, top rgb (3 bytes), bottom rgb; SVG: `1`, u32 blob index |
| glass | 3 x f32 | opacity at the top, at the bottom, blur sigma (canvas units) |
| layers | u32 count (max 16), then per layer | id (u32 length + UTF-8), role u8 (body, detail, highlight, dynamic), dynamic flag u8 and, if 1: kind u8 (clock-hand, clock-cap, calendar-month, calendar-day), unit u8 (0 none, hour, minute, second), pivot 2 x f32; then 2 forms: light, dark |
| form | | blob index u32, opacity f32, blend u8 (normal, multiply, screen), shadow u8 (none, neutral, chromatic), min_size f32, then clear and tinted specs |
| spec | | mode u8 (ramp, light, glass, accent, keep, hide), opacity f32 (multiplies the layer opacity; role factor already applied), luma flag u8 and, if 1, 2 x f32 |
| blobs | u32 count, then each u32 length + bytes | the SVG files, deduplicated; at most 16 MB each |

Every default of the manifest is resolved by the compiler (role modes, opacity factors such as the
0.6 of `highlight`, shadows, `layers/<id>-dark.svg`, the `dark` override block merged into the
dark form), so a reader applies no rules of its own. `decode` checks the digest, every length
and every blob index and returns nothing for malformed input; it never panics.

## Readers

- **Rust**: crate `silica-assets` (in silica-desktop `crates/`): `Assets::open(path)` maps the
  file read-only; lookups return borrowed slices. It never panics on a malformed file: a bad
  magic, an unknown format or an out-of-range offset returns an error, and the caller falls
  back. A C ABI (`libsilica_assets.so`, header
  `crates/silica-assets/include/silica_assets.h`: open, close, lookup by key space, entry and
  slot info, variant by index, variant pick, blob pointer and length) serves `dart:ffi`; the
  only `unsafe` in the crate is the memory mapping and that ABI (`src/ffi/`).
- **silica_ui**: `SilicaAssets` resolves app, folder role, volume,
  badge, symbol, font, sound and wallpaper requests through the C ABI; widgets get pixels as
  `ui.ImageDescriptor.raw` (no decode) or decode QOI; a size-bounded cache keyed by
  (entry, theme, side); theme follows `AmalfiTheme` at runtime; missing file or unknown format
  gives drawn fallbacks (generic document, folder, app), never a crash.

All processes map the same file, so its pages are shared in memory across the whole session.

## Versioning

- `format` changes only when the layout changes; readers accept the formats they know and fall
  back otherwise.
- `content_version` follows the `silica-assets` package version; apps never pin it.
- A newer `silica.assets` with the same format is picked up at the next process start.
