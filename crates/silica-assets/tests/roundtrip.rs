//! Compile a small source tree, open the result and check everything the format promises.
#![cfg(feature = "compile")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use silica_assets::*;

fn open(fixture: &Fixture) -> (Assets, std::path::PathBuf) {
    let path = fixture.root().join("silica.assets");
    std::fs::write(&path, compile_fixture(fixture)).unwrap();
    // `Assets::open` maps the file; `from_bytes` is covered by the malformed-file tests.
    (Assets::open(&path).unwrap(), path)
}

#[test]
fn every_key_is_found_and_misses_are_rejected() {
    let fixture = fixture();
    let (assets, _) = open(&fixture);
    assert_eq!(assets.content_version(), "silica-assets test");

    let present: &[(KeySpace, &str, &str)] = &[
        (KeySpace::App, "mail", "mail"),
        (KeySpace::App, "clock", "clock"),
        (KeySpace::App, "calendar", "calendar"),
        (KeySpace::App, "dev.frascella.Mail", "mail"),
        (KeySpace::Type, "file-generic", "file-generic"),
        (KeySpace::Role, "folder", "folder.generic"),
        (KeySpace::Role, "folder.generic", "folder.generic"),
        (KeySpace::Role, "trash.empty", "trash.empty"),
        (KeySpace::Role, "volume.external", "volume.external"),
        (KeySpace::Role, "dock.stack-close", "dock.stack-close"),
        (KeySpace::Role, "badge.alias", "badge.alias"),
        (KeySpace::Role, "badge.unreadable", "badge.unreadable"),
        (KeySpace::Symbol, "regular/key", "regular/key"),
        (KeySpace::Symbol, "light/key", "light/key"),
        (KeySpace::Symbol, "light/gear", "light/gear"),
        (KeySpace::Font, "Inter-Regular", "Inter-Regular"),
        (KeySpace::Font, "Mono", "Mono"),
        (KeySpace::Sound, "alert-drop", "alert-drop"),
        (KeySpace::Wallpaper, "silica", "silica"),
    ];
    for (space, key, name) in present {
        let id = assets
            .lookup(*space, key)
            .unwrap_or_else(|| panic!("{space:?} {key} not found"));
        let entry = assets.entry(id).unwrap();
        assert_eq!(entry.name(), *name, "{space:?} {key}");
        assert_ne!(entry.kind(), EntryKind::Alias, "aliases are resolved");
    }

    // The same key in the wrong key space, near misses and junk.
    let absent: &[(KeySpace, &str)] = &[
        (KeySpace::App, "file-pdf"),
        (KeySpace::App, "Mail"),
        (KeySpace::App, ""),
        (KeySpace::Type, "mail"),
        (KeySpace::Type, "file-pd"),
        (KeySpace::Role, "trash.full"),
        (KeySpace::Type, "kind.pdf"),
        (KeySpace::Mime, "application/pdf"),
        (KeySpace::Ext, "pdf"),
        (KeySpace::Uti, "com.adobe.pdf"),
        (KeySpace::Symbol, "bold/key"),
        (KeySpace::Symbol, "key"),
        (KeySpace::Font, "Inter"),
        (KeySpace::Sound, "alert-crystal"),
        (KeySpace::Wallpaper, "other"),
        (KeySpace::Wallpaper, "silica-light"),
    ];
    for (space, key) in absent {
        assert!(
            assets.lookup(*space, key).is_none(),
            "{space:?} {key} must be absent"
        );
    }
    // A generous sweep of keys that are not there: none may be accepted.
    for i in 0..2000 {
        for space in KeySpace::ALL {
            assert!(assets.lookup(*space, &format!("missing-{i}")).is_none());
        }
    }
}

#[test]
fn aliases_and_entries_describe_themselves() {
    let fixture = fixture();
    let (assets, _) = open(&fixture);
    let mail = assets.lookup(KeySpace::App, "mail").unwrap();
    assert_eq!(
        assets.lookup(KeySpace::App, "dev.frascella.Mail"),
        Some(mail)
    );
    // No file kind, category or type map exists any more.
    assert!(assets.entries().all(|e| e.kind() != EntryKind::FileKind));
    // The raw alias entries exist in the file with `alias_of` set.
    let aliases = assets
        .entries()
        .filter(|e| e.kind() == EntryKind::Alias)
        .count();
    assert_eq!(aliases, 2);
    for alias in assets.entries().filter(|e| e.kind() == EntryKind::Alias) {
        let target = assets.entry(alias.alias_of().unwrap()).unwrap();
        assert_ne!(target.kind(), EntryKind::Alias);
    }
    let kinds = |key: &str, space| {
        assets
            .entry(assets.lookup(space, key).unwrap())
            .unwrap()
            .kind()
    };
    assert_eq!(kinds("folder", KeySpace::Role), EntryKind::Folder);
    assert_eq!(kinds("volume.external", KeySpace::Role), EntryKind::Volume);
    assert_eq!(kinds("badge.alias", KeySpace::Role), EntryKind::Badge);
    assert_eq!(kinds("regular/key", KeySpace::Symbol), EntryKind::Symbol);
    assert_eq!(kinds("Inter-Regular", KeySpace::Font), EntryKind::Font);
    assert_eq!(kinds("alert-drop", KeySpace::Sound), EntryKind::Sound);
    assert_eq!(kinds("silica", KeySpace::Wallpaper), EntryKind::Wallpaper);
    assert_eq!(kinds("mail", KeySpace::App), EntryKind::App);
}

#[test]
fn variant_pick_by_requested_side() {
    let fixture = fixture();
    let (assets, _) = open(&fixture);
    let mail = assets.lookup(KeySpace::App, "mail").unwrap();
    let sides = |wanted: u32| {
        assets
            .pick(mail, Query::composite(Theme::Default, wanted))
            .unwrap()
            .size_px
    };
    assert_eq!(sides(0), 16);
    assert_eq!(sides(1), 16);
    assert_eq!(sides(16), 16);
    assert_eq!(sides(17), 32);
    assert_eq!(sides(32), 32);
    assert_eq!(sides(33), 64);
    assert_eq!(sides(100), 128);
    assert_eq!(sides(128), 128);
    assert_eq!(sides(129), 256);
    assert_eq!(sides(300), 512);
    assert_eq!(sides(1024), 1024);
    assert_eq!(
        sides(5000),
        1024,
        "larger than any stored side: the largest"
    );
    assert_eq!(sides(u32::MAX), 1024);

    let v = assets
        .pick(mail, Query::composite(Theme::Default, 64))
        .unwrap();
    assert_eq!(v.encoding, Encoding::Rgba8Premul);
    let v = assets
        .pick(mail, Query::composite(Theme::Default, 256))
        .unwrap();
    assert_eq!(v.encoding, Encoding::Qoi);

    // Seven sides for each of the six themes.
    assert_eq!(assets.entry(mail).unwrap().variants().count(), 42);

    // Clock: dark and default exist; Clear Dark falls back to Dark, Tinted to Default.
    let clock = assets.lookup(KeySpace::App, "clock").unwrap();
    let theme_of = |theme, layer| {
        assets
            .pick(
                clock,
                Query {
                    theme,
                    layer,
                    state: State::Normal,
                    side: 64,
                },
            )
            .map(|v| v.theme)
    };
    assert_eq!(theme_of(Theme::Dark, Layer::Composite), Some(Theme::Dark));
    assert_eq!(
        theme_of(Theme::ClearDark, Layer::Composite),
        Some(Theme::Dark)
    );
    assert_eq!(
        theme_of(Theme::Tinted, Layer::Composite),
        Some(Theme::Default)
    );
    assert_eq!(
        theme_of(Theme::TintedDark, Layer::Composite),
        Some(Theme::Dark)
    );
    // The second hand has no dark file: Dark falls back to Default.
    assert_eq!(
        theme_of(Theme::Dark, Layer::HandSecond),
        Some(Theme::Default)
    );
    assert_eq!(theme_of(Theme::Dark, Layer::HandHour), Some(Theme::Dark));
    // A layer the icon does not have.
    assert_eq!(theme_of(Theme::Default, Layer::Background), None);
    // Mail has no hands at all.
    assert!(
        assets
            .pick(
                mail,
                Query {
                    theme: Theme::Default,
                    layer: Layer::Cap,
                    state: State::Normal,
                    side: 64
                }
            )
            .is_none()
    );

    // The full folder is a state, not a theme.
    let folder = assets.lookup(KeySpace::Role, "folder").unwrap();
    let full = Query {
        theme: Theme::Dark,
        layer: Layer::Composite,
        state: State::Full,
        side: 128,
    };
    let v = assets.pick(folder, full).unwrap();
    // No dark layers: the default theme answers.
    assert_eq!((v.state, v.theme), (State::Full, Theme::Default));

    // A wallpaper: light 128 wide and a 4 wide preview, dark 64 wide; 100 asks for the larger one.
    let wallpaper = assets.lookup(KeySpace::Wallpaper, "silica").unwrap();
    let v = assets
        .pick(wallpaper, Query::composite(Theme::Default, 100))
        .unwrap();
    assert_eq!((v.encoding, v.width, v.height), (Encoding::Png, 128, 72));
    let v = assets
        .pick(wallpaper, Query::composite(Theme::Default, 4))
        .unwrap();
    assert_eq!((v.encoding, v.width, v.height), (Encoding::Jpeg, 4, 2));
    let v = assets
        .pick(wallpaper, Query::composite(Theme::Dark, 100))
        .unwrap();
    assert_eq!((v.theme, v.width, v.height), (Theme::Dark, 64, 36));
    let title = assets
        .entry(wallpaper)
        .unwrap()
        .slots()
        .find(|s| s.kind == SlotKind::Title)
        .and_then(|s| s.format.map(str::to_owned));
    assert_eq!(title.as_deref(), Some("Silica test"));

    // An id that is not an entry.
    assert!(
        assets
            .pick(EntryId(u32::MAX), Query::composite(Theme::Default, 64))
            .is_none()
    );
    assert!(assets.entry(EntryId(assets.entry_count() as u32)).is_none());
}

#[test]
fn pixel_bytes_are_what_the_sources_say() {
    let fixture = fixture();
    let (assets, _) = open(&fixture);
    let mail = assets.lookup(KeySpace::App, "mail").unwrap();
    let themes = [
        Theme::Default,
        Theme::Dark,
        Theme::Clear,
        Theme::ClearDark,
        Theme::Tinted,
        Theme::TintedDark,
    ];
    for (i, theme) in themes.into_iter().enumerate() {
        let colored = premultiply(rendered_color(i as u8));
        for side in [16u32, 32, 64, 128] {
            let v = assets.pick(mail, Query::composite(theme, side)).unwrap();
            assert_eq!((v.theme, u32::from(v.size_px)), (theme, side));
            assert_eq!(v.data.len(), (side * side * 4) as usize);
            let at = |x: u32, y: u32| &v.data[((y * side + x) * 4) as usize..][..4];
            assert_eq!(at(0, 0), colored, "{theme:?} {side}");
            assert_eq!(at(side / 2 - 1, side - 1), colored);
            assert_eq!(at(side / 2, 0), [0, 0, 0, 0]);
            assert_eq!(at(side - 1, side - 1), [0, 0, 0, 0]);
        }
        for side in [256u32, 512, 1024] {
            let v = assets.pick(mail, Query::composite(theme, side)).unwrap();
            assert_eq!(v.encoding, Encoding::Qoi);
            let (header, pixels) = qoi::decode_to_vec(v.data).unwrap();
            assert_eq!((header.width, header.height), (side, side));
            let mut expected = Vec::new();
            for _ in 0..side {
                for x in 0..side {
                    expected.extend_from_slice(&if x < side / 2 { colored } else { [0, 0, 0, 0] });
                }
            }
            assert_eq!(pixels, expected, "{theme:?} {side}");
        }
    }

    // Composed from SVG layers: the red square of the clock sits at (100..400) on the canvas,
    // so at 128 px it covers pixels 12..50 and the white background is around it.
    let clock = assets.lookup(KeySpace::App, "clock").unwrap();
    let v = assets
        .pick(clock, Query::composite(Theme::Default, 128))
        .unwrap();
    let at = |x: usize, y: usize| &v.data[(y * 128 + x) * 4..][..4];
    assert_eq!(at(30, 30), [255, 0, 0, 255]);
    assert_eq!(at(100, 100), [255, 255, 255, 255]);
    let v = assets
        .pick(clock, Query::composite(Theme::Dark, 128))
        .unwrap();
    let at = |x: usize, y: usize| &v.data[(y * 128 + x) * 4..][..4];
    assert_eq!(at(30, 30), [0, 255, 0, 255]);
    assert_eq!(at(100, 100), [34, 34, 34, 255]);
}

#[test]
fn blobs_slots_and_dynamic_layers() {
    let fixture = fixture();
    let (assets, _) = open(&fixture);

    let blob = |space, key: &str, encoding| {
        let id = assets.lookup(space, key).unwrap();
        let v = assets.entry(id).unwrap().variant(0).unwrap();
        assert_eq!(v.encoding, encoding);
        v.data.to_vec()
    };
    assert_eq!(
        blob(KeySpace::Font, "Inter-Regular", Encoding::Ttf),
        FONT_BYTES
    );
    assert_eq!(blob(KeySpace::Font, "Mono", Encoding::Otf), b"OTTO mono");
    assert_eq!(
        blob(KeySpace::Sound, "alert-drop", Encoding::Wav),
        SOUND_BYTES
    );
    let svg = blob(KeySpace::Symbol, "regular/key", Encoding::Svg);
    assert!(svg.starts_with(b"<svg"));
    let symbol = assets
        .entry(assets.lookup(KeySpace::Symbol, "regular/key").unwrap())
        .unwrap();
    assert_ne!(symbol.flags() & FLAG_TINTABLE, 0);

    // Blobs are 64-byte aligned inside the file, so they can be handed to the GPU.
    let base = {
        let font = assets
            .entry(assets.lookup(KeySpace::Font, "Inter-Regular").unwrap())
            .unwrap();
        font.variant(0).unwrap().data.as_ptr() as usize
    };
    assert_eq!(base % 64, 0);

    // Clock slots.
    let clock = assets
        .entry(assets.lookup(KeySpace::App, "clock").unwrap())
        .unwrap();
    assert_ne!(clock.flags() & FLAG_HAS_DYNAMIC, 0);
    let slots: Vec<_> = clock.slots().collect();
    assert_eq!(slots.len(), 4);
    assert_eq!(slots[0].kind, SlotKind::ClockHand);
    assert_eq!(slots[0].unit, Unit::Hour as u8);
    assert_eq!(slots[0].rect, [512.0, 512.0, 0.0, 0.0]);
    assert_eq!(slots[2].unit, Unit::Second as u8);
    assert_eq!(slots[2].min_size, 64);
    assert_eq!(slots[3].kind, SlotKind::ClockCap);
    // Each hand is stored as its own layer at every size.
    let hour = assets
        .pick(
            clock.id(),
            Query {
                theme: Theme::Default,
                layer: Layer::HandHour,
                state: State::Normal,
                side: 1024,
            },
        )
        .unwrap();
    let (header, pixels) = qoi::decode_to_vec(hour.data).unwrap();
    assert_eq!((header.width, header.height), (1024, 1024));
    // The hour hand covers x 500..524, y 200..512 in black.
    let px = |x: usize, y: usize| &pixels[(y * 1024 + x) * 4..][..4];
    assert_eq!(px(510, 300), [0, 0, 0, 255]);
    assert_eq!(px(300, 300), [0, 0, 0, 0]);

    // Calendar slots.
    let calendar = assets
        .entry(assets.lookup(KeySpace::App, "calendar").unwrap())
        .unwrap();
    let month = calendar.slot(0).unwrap();
    assert_eq!(month.kind, SlotKind::CalendarMonth);
    assert_eq!(month.rect, [100.0, 206.0, 824.0, 120.0]);
    assert_eq!(
        (month.font, month.size, month.weight),
        (Some("Inter"), 92, 900)
    );
    assert_eq!(month.color, [0xF2382EFF, 0xFF5248FF]);
    assert_eq!(month.format, Some("month:upper"));
    assert_eq!((month.tracking, month.max_width), (4.0, 560.0));
    let day = calendar.slot(1).unwrap();
    assert_eq!((day.kind, day.format), (SlotKind::CalendarDay, Some("day")));
    assert_eq!(day.color, [0xFFFFFFFF, 0xFFFFFFFF]);
    assert!(calendar.slot(2).is_none());

    // Generic file: the stack, the glyph box, the labels and the fallback.
    let generic = assets
        .entry(assets.lookup(KeySpace::Type, "file-generic").unwrap())
        .unwrap();
    let rules: Vec<_> = generic
        .slots()
        .filter(|s| s.kind == SlotKind::LayerRule)
        .map(|s| (s.unit, s.weight))
        .collect();
    assert_eq!(
        rules,
        [
            (LayerRole::Back as u8, 0),
            (LayerRole::Glyph as u8, 0),
            (LayerRole::Front as u8, 0)
        ]
    );
    let of_kind = |kind| generic.slots().find(|s| s.kind == kind).unwrap();
    assert_eq!(
        of_kind(SlotKind::GlyphBox).rect,
        [262.0, 262.0, 500.0, 500.0]
    );
    assert_eq!(
        of_kind(SlotKind::AppIconBody).rect,
        [303.67, 303.67, 416.67, 416.67]
    );
    let label = of_kind(SlotKind::FileLabel);
    assert_eq!(
        (label.unit, label.min_size, label.format),
        (5, 32, Some("upper"))
    );
    assert_eq!((label.font, label.weight), (Some("Inter"), 500));
    // 139.59 in 1/64 units (25 % over the former 111.67), one grey for every file.
    assert_eq!(label.size, 8934);
    assert_eq!(label.rect, [219.08, 744.5, 585.84, 170.05]);
    assert_eq!(label.max_width, 585.84);
    assert_eq!(label.color, [0x5F636BFF, 0x5F636BFF]);
    let alone = of_kind(SlotKind::FileLabelAlone);
    assert_eq!((alone.weight, alone.size), (900, 11234));
    assert_eq!(alone.color, [0x5F636BFF, 0x5F636BFF]);
    // No per-type symbols: the glyph is the icon of the app that opens the file.
    for gone in [
        SlotKind::SymbolBand,
        SlotKind::FallbackKind,
        SlotKind::KindStyle,
        SlotKind::TypeCategory,
    ] {
        assert!(generic.slots().all(|s| s.kind != gone), "{gone:?}");
    }
    // Generic folder: stack with conditional middle and tint flags, symbol, effect, tint ramp.
    let folder = assets
        .entry(assets.lookup(KeySpace::Role, "folder.generic").unwrap())
        .unwrap();
    let rules: Vec<_> = folder
        .slots()
        .filter(|s| s.kind == SlotKind::LayerRule)
        .map(|s| (s.unit, s.weight))
        .collect();
    assert_eq!(
        rules,
        [
            (LayerRole::Back as u8, LAYER_TINT),
            (LayerRole::Middle as u8, LAYER_WHEN_NOT_EMPTY),
            (LayerRole::Front as u8, LAYER_TINT),
            (LayerRole::Symbol as u8, LAYER_TINT),
        ]
    );
    let of_kind = |kind| folder.slots().find(|s| s.kind == kind).unwrap();
    assert_eq!(
        of_kind(SlotKind::FolderEmblem).rect,
        [335.04, 415.04, 353.92, 353.92]
    );
    let effect = of_kind(SlotKind::FolderEffect);
    assert_eq!(effect.color, [0x1A64C800, 0xFFFFFF00]);
    assert_eq!(
        (effect.rect[0], effect.rect[1], effect.tracking),
        (0.78, 0.5, 4.0)
    );
    assert_eq!(of_kind(SlotKind::FolderTint).rect, [0.35, 0.85, 0.72, 0.5]);
    let bands: Vec<_> = folder
        .slots()
        .filter(|s| s.kind == SlotKind::SymbolBand)
        .map(|s| (s.min_size, s.format.unwrap_or(""), s.rect[0], s.rect[1]))
        .collect();
    assert_eq!(
        bands,
        [
            (128, "light", 1.0, 1.0),
            (64, "regular", 1.0, 1.0),
            (32, "semibold", 1.0, 0.0),
            (16, "semibold", 1.272, 0.0)
        ]
    );
    let blue = folder
        .slots()
        .find(|s| s.kind == SlotKind::TintLabel && s.format == Some("blue"))
        .unwrap();
    assert_eq!(blue.color[0], 0x2F7CF6FF);
    let documents = folder
        .slots()
        .find(|s| s.kind == SlotKind::SpecialFolder && s.format == Some("DOCUMENTS"))
        .unwrap();
    assert_eq!(documents.font, Some("gear"));

    // The layers are stored as SVG (back, middle, front), the composites as images.
    let svg_layers: Vec<_> = folder
        .variants()
        .filter(|v| v.encoding == Encoding::Svg)
        .map(|v| v.layer)
        .collect();
    assert_eq!(
        svg_layers,
        [Layer::Background, Layer::Middle, Layer::Foreground]
    );
    let mut normal = Query::composite(Theme::Default, 64);
    let empty = assets.pick(folder.id(), normal).unwrap();
    normal.state = State::Full;
    let full = assets.pick(folder.id(), normal).unwrap();
    assert_ne!(empty.data, full.data);

    // Badge placement.
    let alias = assets
        .entry(assets.lookup(KeySpace::Role, "badge.alias").unwrap())
        .unwrap();
    let slot = alias.slot(0).unwrap();
    assert_eq!(slot.kind, SlotKind::Badge);
    assert_eq!(slot.unit, Corner::BottomLeft as u8);
    assert_eq!(slot.rect[..2], [100.0, 924.0]);
    assert_eq!(slot.min_size, 32);
    assert_eq!(slot.weight, 1, "position in the badge order");
}

#[test]
fn compilation_is_deterministic() {
    let fixture = fixture();
    let first = compile_fixture(&fixture);
    let second = compile_fixture(&fixture);
    assert_eq!(first, second);

    // The same sources in another directory give the same bytes.
    let other = common::fixture();
    assert_eq!(first, compile_fixture(&other));

    // Changing a source changes the hash and the bytes.
    let before = Assets::from_bytes(first.clone()).unwrap();
    std::fs::write(
        fixture.options.fonts.as_ref().unwrap().join("Mono.otf"),
        b"OTTO other",
    )
    .unwrap();
    let changed = compile_fixture(&fixture);
    let after = Assets::from_bytes(changed.clone()).unwrap();
    assert_ne!(first, changed);
    assert_ne!(before.source_hash(), after.source_hash());
    assert_ne!(before.source_hash(), &[0u8; 32]);
}

fn edit_compose(
    fixture: &mut common::Fixture,
    edit: impl FnOnce(&mut serde_json::Value),
) -> String {
    let path = fixture
        .options
        .types
        .clone()
        .unwrap()
        .with_file_name("compose.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut json = serde_json::to_value(text.parse::<toml::Table>().unwrap()).unwrap();
    edit(&mut json);
    let table: toml::Table = serde_json::from_value(json).unwrap();
    let out = fixture.root().join("edited-compose.toml");
    std::fs::write(&out, toml::to_string(&table).unwrap()).unwrap();
    fixture.options.compose = Some(out);
    silica_assets::compile::compile(&fixture.options)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default()
}

#[test]
fn bad_composition_rules_are_errors() {
    // The extension label needs its colour.
    let message = edit_compose(&mut fixture(), |j| {
        j["file"]["label"].as_object_mut().unwrap().remove("color");
    });
    assert!(message.contains("color"), "{message}");
    // A layer the sources do not provide.
    let message = edit_compose(&mut fixture(), |j| {
        j["folder"]["layers"][1]["role"] = "front".into();
    });
    assert!(message.contains("twice"), "{message}");
}

#[test]
fn the_shipped_type_table_parses() {
    let table: toml::Table = silica_assets::compile::DEFAULT_TYPES.parse().unwrap();
    assert!(
        table["app"]
            .as_table()
            .unwrap()
            .contains_key("dev.frascella.Apps")
    );
    // The icon of a file is its app's: no type tables.
    assert!(
        !table.contains_key("uti") && !table.contains_key("mime") && !table.contains_key("ext")
    );
}

/// The real silica-icons layout, when `SILICA_ICONS_A8B2` points at a checkout of it (the
/// commit a8b2a27 of branch amalfi-icons): the layers and the shipped composition rules compile.
#[test]
fn the_real_layers_and_the_shipped_rules_compile() {
    let Some(icons) = std::env::var_os("SILICA_ICONS_A8B2") else {
        return;
    };
    // The real file and folder layers and the symbols over the fixture's other icons.
    let fixture = fixture();
    let real = std::path::PathBuf::from(icons);
    let mut options = fixture.options.clone();
    for dir in ["files/file-generic", "files/folder-generic", "symbols"] {
        let target = options.icons.join(dir);
        std::fs::remove_dir_all(&target).unwrap();
        common::copy_tree(&real.join(dir), &target);
    }
    options.compose = None;
    let (bytes, report) = silica_assets::compile::compile(&options).unwrap();
    let assets = Assets::from_bytes(bytes).unwrap();
    assert!(report.symbols > 0);
    let id = assets.lookup(KeySpace::Role, "folder").unwrap();
    assert_eq!(assets.entry(id).unwrap().name(), "folder.generic");
}
