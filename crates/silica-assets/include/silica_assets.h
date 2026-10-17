/*
 * silica_assets.h: C ABI of libsilica_assets.so, the reader of silica.assets
 * (docs/assets-format.md in silica-desktop). Meant for dart:ffi, the shell and C programs.
 *
 * Conventions
 *  - A handle (silica_assets*) comes from silica_assets_open and is released once with
 *    silica_assets_close. Handles are read-only: any number of threads may use one concurrently.
 *  - Every pointer returned through these functions (names, formats, fonts, pixel and font
 *    data, the content version) points into the memory-mapped file and stays valid until the
 *    handle is closed. Strings are UTF-8 and NUL-terminated (the *_len fields do not count
 *    the NUL).
 *  - An entry is a 32-bit index. SILICA_ASSETS_NONE (UINT32_MAX) means "no entry".
 *  - Functions that return int return 1 on success and 0 on failure (nothing is written to the
 *    output structure on failure). No function aborts on malformed files or arguments.
 *  - A lookup that hits an alias (a MIME type, a UTI, an extension, a desktop application id)
 *    returns the entry it points to, the one that holds the images.
 *  - Images are premultiplied RGBA, 8 bits per channel: SILICA_ENCODING_RGBA8_PREMUL is the raw
 *    size_px x size_px bytes (4 bytes per pixel, rows top to bottom, no header);
 *    SILICA_ENCODING_QOI is a QOI file of the same pixels.
 */
#ifndef SILICA_ASSETS_H
#define SILICA_ASSETS_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SILICA_ASSETS_FORMAT_VERSION 1u
#define SILICA_ASSETS_NONE 0xFFFFFFFFu

typedef struct silica_assets silica_assets;

/* Key spaces for silica_assets_lookup. */
enum {
  SILICA_SPACE_APP = 0,       /* icon id ("mail") or desktop application id ("dev.frascella.Mail") */
  SILICA_SPACE_TYPE = 1,      /* "file-generic", a kind of file ("kind.pdf") or a category ("category.data") */
  SILICA_SPACE_MIME = 2,      /* MIME type ("application/pdf"), or a wildcard key ("image/star") */
  SILICA_SPACE_UTI = 3,       /* Uniform Type Identifier ("com.adobe.pdf") */
  SILICA_SPACE_ROLE = 4,      /* "folder", "folder.generic", "volume.external", "trash.empty",
                                 "trash.full", "dock.stack-close", "badge.alias" ... */
  SILICA_SPACE_SYMBOL = 5,    /* "<weight>/<name>": "regular/key" */
  SILICA_SPACE_FONT = 6,      /* "Inter-Regular" */
  SILICA_SPACE_SOUND = 7,     /* "alert-drop" */
  SILICA_SPACE_WALLPAPER = 8, /* "silica" */
  SILICA_SPACE_EXT = 9        /* extension without the dot, lowercase: "pdf" */
};

/* Entry kinds (silica_entry_info.kind). */
enum {
  SILICA_KIND_APP = 0,
  SILICA_KIND_FILE = 1,
  SILICA_KIND_FOLDER = 2,
  SILICA_KIND_VOLUME = 3,
  SILICA_KIND_BADGE = 4,
  SILICA_KIND_SYMBOL = 5,
  SILICA_KIND_FONT = 6,
  SILICA_KIND_SOUND = 7,
  SILICA_KIND_WALLPAPER = 8,
  SILICA_KIND_ALIAS = 9, /* never returned by lookup: aliases are resolved */
  SILICA_KIND_FILE_KIND = 10 /* a kind ("kind.pdf") or category ("category.data") of file */
};

/* Entry flags (silica_entry_info.flags). */
enum {
  SILICA_FLAG_HAS_DYNAMIC = 1, /* the entry has slots */
  SILICA_FLAG_TINTABLE = 2,    /* a template image the shell may tint (symbols) */
  SILICA_FLAG_HAS_DARK = 4     /* some variant has a dark theme */
};

/* Themes. A pick falls back along: clear-dark -> clear -> dark -> default,
   tinted-dark -> tinted -> dark -> default, dark/clear/tinted -> default. */
enum {
  SILICA_THEME_DEFAULT = 0,
  SILICA_THEME_DARK = 1,
  SILICA_THEME_CLEAR = 2,
  SILICA_THEME_CLEAR_DARK = 3,
  SILICA_THEME_TINTED = 4,
  SILICA_THEME_TINTED_DARK = 5
};

/* Layers of a variant. */
enum {
  SILICA_LAYER_COMPOSITE = 0, /* the whole asset */
  SILICA_LAYER_HAND_HOUR = 1, /* clock hands and cap: drawn pointing to 12, rotated about the pivot */
  SILICA_LAYER_HAND_MINUTE = 2,
  SILICA_LAYER_HAND_SECOND = 3,
  SILICA_LAYER_CAP = 4,
  SILICA_LAYER_BACKGROUND = 5, /* "back" of the generic file and folder (SVG) */
  SILICA_LAYER_FOREGROUND = 6, /* "front" (SVG) */
  SILICA_LAYER_MIDDLE = 7      /* "middle" of the generic folder (SVG) */
};

/* Encodings of variant data. */
enum {
  SILICA_ENCODING_RGBA8_PREMUL = 0,
  SILICA_ENCODING_QOI = 1,
  SILICA_ENCODING_TTF = 2,
  SILICA_ENCODING_OTF = 3,
  SILICA_ENCODING_OGG = 4,
  SILICA_ENCODING_FLAC = 5,
  SILICA_ENCODING_SVG = 6,
  SILICA_ENCODING_PNG = 7,
  SILICA_ENCODING_WAV = 8
};

/* States of a variant. */
enum {
  SILICA_STATE_NORMAL = 0,
  SILICA_STATE_FULL = 1 /* a folder with something inside */
};

/* Slot kinds (silica_slot.kind). */
enum {
  SILICA_SLOT_CLOCK_HAND = 0,     /* unit: 0 hour, 1 minute, 2 second; rect = (pivot x, pivot y, 0, 0);
                                     min_size: smallest icon side in points at which it is drawn */
  SILICA_SLOT_CLOCK_CAP = 1,      /* rect = pivot */
  SILICA_SLOT_CALENDAR_MONTH = 2, /* rect = box on the 1024 canvas; format "month:upper" */
  SILICA_SLOT_CALENDAR_DAY = 3,   /* format "day" */
  SILICA_SLOT_FOLDER_EMBLEM = 4,
  SILICA_SLOT_TYPE_LABEL = 5,     /* extension label of the generic file; unit = max characters;
                                     format = case ("upper") */
  SILICA_SLOT_BADGE = 6,          /* unit: corner (0 top-left, 1 top-right, 2 bottom-left,
                                     3 bottom-right); rect = (corner x, corner y, fraction of the
                                     icon side from min_size up, fraction below it, one badge
                                     only); size = priority; weight = position in the order */
  SILICA_SLOT_TYPE_CATEGORY = 7,  /* label color of a category: format = category name */
  SILICA_SLOT_TITLE = 8,          /* display title: format */
  SILICA_SLOT_LAYER_RULE = 9,     /* unit: role (0 back, 1 middle, 2 front, 3 glyph, 4 symbol,
                                     5 label); weight: flags (1 tint, 2 only when not empty) */
  SILICA_SLOT_GLYPH_BOX = 10,     /* rect */
  SILICA_SLOT_SYMBOL_BAND = 11,   /* min_size pt; format weight name; rect = (scale, highlight, 0, 0) */
  SILICA_SLOT_APP_ICON_BODY = 12, /* rect */
  SILICA_SLOT_FILE_LABEL = 13,    /* unit max chars, min_size pt, rect box, font, size in 1/64
                                     unit, weight, format case, tracking, max_width */
  SILICA_SLOT_FILE_LABEL_ALONE = 14,
  SILICA_SLOT_KIND_STYLE = 15,    /* format kind name, font symbol name, color[0] */
  SILICA_SLOT_FALLBACK_KIND = 16, /* format kind name */
  SILICA_SLOT_FOLDER_EFFECT = 17, /* color[0] engraving, color[1] highlight (0xRRGGBB00),
                                     rect = (opacity, highlight opacity, 0, 0), tracking offset */
  SILICA_SLOT_FOLDER_TINT = 18,   /* rect = (luminance low, high, shade, glow) */
  SILICA_SLOT_TINT_LABEL = 19,    /* format name, color[0] */
  SILICA_SLOT_SPECIAL_FOLDER = 20 /* format key (DOCUMENTS), font symbol name */
};

typedef struct silica_entry_info {
  const char *name; /* NUL-terminated key of the entry */
  size_t name_len;
  uint32_t kind;
  uint32_t flags;
  uint32_t alias_of; /* SILICA_ASSETS_NONE for a real entry */
  uint32_t variant_count;
  uint32_t slot_count;
} silica_entry_info;

typedef struct silica_variant {
  uint32_t theme;
  uint32_t layer;
  uint32_t encoding;
  uint32_t state;
  uint32_t size_px; /* pixel side of a square image; 0 for fonts, sounds and symbols;
                       the width for wallpapers */
  uint32_t width;
  uint32_t height;
  const uint8_t *data;
  size_t len;
} silica_variant;

typedef struct silica_slot {
  uint32_t kind;
  uint32_t unit;
  uint32_t min_size;
  uint32_t size;   /* font size on the 1024 canvas (text slots) */
  uint32_t weight; /* font weight (text slots) */
  float rect[4];
  const char *font; /* NULL if none */
  size_t font_len;
  uint32_t color[2]; /* light and dark, 0xRRGGBBAA */
  const char *format; /* NULL if none */
  size_t format_len;
  float tracking;
  float max_width;
} silica_slot;

/* Opens and checks a file; NULL on failure (see silica_assets_last_error). */
silica_assets *silica_assets_open(const char *path);

/* Releases a handle; NULL is ignored. Pointers obtained from the handle become invalid. */
void silica_assets_close(silica_assets *handle);

/* Why the last silica_assets_open of the calling thread failed. */
const char *silica_assets_last_error(void);

/* The format version this library reads (SILICA_ASSETS_FORMAT_VERSION). */
uint32_t silica_assets_format_version(void);

/* The content version of the file ("silica-assets 0.1.0"), or NULL. */
const char *silica_assets_content_version(const silica_assets *handle);

/* Number of entries, aliases included. Entries 0..count-1 can be described. */
uint32_t silica_assets_entry_count(const silica_assets *handle);

/* Looks a key up (key_len bytes of UTF-8, no NUL needed). Returns an entry or
   SILICA_ASSETS_NONE. */
uint32_t silica_assets_lookup(const silica_assets *handle, uint32_t space, const uint8_t *key,
                              size_t key_len);

int silica_assets_entry_info(const silica_assets *handle, uint32_t entry, silica_entry_info *out);

/* Variant number `index` (0 <= index < variant_count) of an entry. */
int silica_assets_variant(const silica_assets *handle, uint32_t entry, uint32_t index,
                          silica_variant *out);

/* The variant to draw: of the first theme in the fallback chain that has the layer and state,
   the smallest stored pixel side >= side, otherwise the largest. */
int silica_assets_pick(const silica_assets *handle, uint32_t entry, uint32_t theme, uint32_t layer,
                       uint32_t state, uint32_t side, silica_variant *out);

/* Slot number `index` (0 <= index < slot_count) of an entry. */
int silica_assets_slot(const silica_assets *handle, uint32_t entry, uint32_t index,
                       silica_slot *out);

#ifdef __cplusplus
}
#endif

#endif /* SILICA_ASSETS_H */
