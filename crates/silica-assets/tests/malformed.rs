//! The reader never panics on a malformed file: truncations, flipped bytes, garbage and
//! structures that contradict themselves return an error (or, if they happen to stay
//! consistent, read back without a panic).
#![cfg(feature = "compile")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use silica_assets::compile::{Builder, SlotSpec, VariantSpec};
use silica_assets::*;

fn small_file() -> Vec<u8> {
    let mut b = Builder::new("silica-assets fuzz", [7; 32]);
    let app = b.add_entry("mail", EntryKind::App, FLAG_HAS_DARK | FLAG_HAS_DYNAMIC);
    b.register(KeySpace::App, app).unwrap();
    for (i, theme) in [Theme::Default, Theme::Dark].into_iter().enumerate() {
        for (side, encoding) in [
            (16u16, Encoding::Rgba8Premul),
            (32, Encoding::Rgba8Premul),
            (256, Encoding::Qoi),
        ] {
            b.add_variant(
                app,
                VariantSpec {
                    theme,
                    layer: Layer::Composite,
                    encoding,
                    state: State::Normal,
                    size_px: side,
                    width: side,
                    height: side,
                    data: vec![i as u8 + 1; 40 + side as usize],
                },
            )
            .unwrap();
        }
    }
    b.add_slot(
        app,
        SlotSpec {
            kind: Some(SlotKind::CalendarDay),
            rect: [1.0, 2.0, 3.0, 4.0],
            font: Some("Inter".to_owned()),
            format: Some("day".to_owned()),
            size: 20,
            weight: 700,
            color: [0xFF0000FF, 0x00FF00FF],
            ..SlotSpec::default()
        },
    )
    .unwrap();
    let pdf = b.add_entry("file-pdf", EntryKind::File, 0);
    b.register(KeySpace::Type, pdf).unwrap();
    b.add_alias(KeySpace::Mime, "application/pdf", pdf).unwrap();
    b.add_alias(KeySpace::Ext, "pdf", pdf).unwrap();
    b.add_variant(
        pdf,
        VariantSpec {
            theme: Theme::Default,
            layer: Layer::Composite,
            encoding: Encoding::Svg,
            state: State::Normal,
            size_px: 0,
            width: 0,
            height: 0,
            data: b"<svg/>".to_vec(),
        },
    )
    .unwrap();
    b.finish().unwrap()
}

/// Touches everything a reader can reach.
fn exercise(assets: &Assets) -> usize {
    let mut touched = 0;
    for entry in assets.entries() {
        touched += entry.name().len() + usize::from(entry.flags());
        let _ = entry.alias_of();
        for variant in entry.variants() {
            touched += variant.data.iter().map(|&b| usize::from(b)).sum::<usize>();
            touched += usize::from(variant.size_px);
        }
        for slot in entry.slots() {
            touched += slot.font.map_or(0, str::len) + slot.format.map_or(0, str::len);
        }
        for space in KeySpace::ALL {
            if let Some(id) = assets.lookup(*space, entry.name()) {
                touched += id.0 as usize;
            }
        }
        for theme in [Theme::Default, Theme::ClearDark, Theme::TintedDark] {
            for layer in [Layer::Composite, Layer::Cap] {
                for state in State::ALL {
                    for side in [0, 100] {
                        if let Some(v) = assets.pick(
                            entry.id(),
                            Query {
                                theme,
                                layer,
                                state: *state,
                                side,
                            },
                        ) {
                            touched += v.data.len();
                        }
                    }
                }
            }
        }
    }
    for space in KeySpace::ALL {
        for key in ["", "mail", "pdf", "application/pdf", "\u{0}", "zzzz"] {
            let _ = assets.lookup(*space, key);
        }
    }
    let _ = (assets.content_version(), assets.source_hash());
    touched
}

struct Xorshift(u64);
impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn the_small_file_is_valid() {
    let bytes = small_file();
    let assets = Assets::from_bytes(bytes).unwrap();
    assert!(exercise(&assets) > 0);
    assert_eq!(
        assets.lookup(KeySpace::Mime, "application/pdf"),
        assets.lookup(KeySpace::Type, "file-pdf")
    );
}

#[test]
fn every_truncation_is_an_error() {
    let bytes = small_file();
    for cut in 0..bytes.len() {
        let result = Assets::from_bytes(bytes[..cut].to_vec());
        assert!(
            result.is_err(),
            "cut at {cut} of {} was accepted",
            bytes.len()
        );
    }
    // Extra bytes at the end do not match the length in the header either.
    let mut longer = bytes;
    longer.push(0);
    assert!(Assets::from_bytes(longer).is_err());
}

#[test]
fn every_single_byte_corruption_is_survived() {
    let bytes = small_file();
    let mut accepted = 0;
    for position in 0..bytes.len() {
        for value in [0x00u8, 0xFF, bytes[position] ^ 0x80] {
            let mut copy = bytes.clone();
            copy[position] = value;
            if let Ok(assets) = Assets::from_bytes(copy) {
                accepted += 1;
                exercise(&assets);
            }
        }
    }
    // Corruptions of blob bytes and padding are legitimately accepted; the rest are rejected.
    assert!(accepted > 0);
}

#[test]
fn random_multi_byte_corruptions_are_survived() {
    let bytes = small_file();
    let mut rng = Xorshift(0x9E37_79B9_7F4A_7C15);
    for _ in 0..3000 {
        let mut copy = bytes.clone();
        for _ in 0..(1 + rng.next() % 6) {
            let at = (rng.next() as usize) % copy.len();
            copy[at] = rng.next() as u8;
        }
        // Half of the time make the length field agree again, so the structure is examined.
        if rng.next().is_multiple_of(2) {
            let len = copy.len() as u64;
            copy[96..104].copy_from_slice(&len.to_le_bytes());
        }
        if let Ok(assets) = Assets::from_bytes(copy) {
            exercise(&assets);
        }
    }
}

#[test]
fn garbage_and_wrong_headers_are_errors() {
    assert!(matches!(
        Assets::from_bytes(Vec::new()),
        Err(Error::Truncated)
    ));
    assert!(matches!(
        Assets::from_bytes(vec![0; 4]),
        Err(Error::Truncated)
    ));
    assert!(matches!(
        Assets::from_bytes(vec![0; 10]),
        Err(Error::BadMagic)
    ));
    assert!(matches!(
        Assets::from_bytes(vec![0; 4096]),
        Err(Error::BadMagic)
    ));
    assert!(matches!(
        Assets::from_bytes(
            b"not a silica file at all, but long enough to have a header....".repeat(10)
        ),
        Err(Error::BadMagic)
    ));

    let mut rng = Xorshift(42);
    for _ in 0..200 {
        let garbage: Vec<u8> = (0..1000).map(|_| rng.next() as u8).collect();
        assert!(Assets::from_bytes(garbage).is_err());
    }

    let good = small_file();
    let mut other_format = good.clone();
    other_format[8..12].copy_from_slice(&2u32.to_le_bytes());
    assert!(matches!(
        Assets::from_bytes(other_format),
        Err(Error::UnsupportedFormat(2))
    ));

    let mut bad_magic = good.clone();
    bad_magic[0] = b'X';
    assert!(matches!(
        Assets::from_bytes(bad_magic),
        Err(Error::BadMagic)
    ));

    // A section table that points outside the file.
    let mut outside = good.clone();
    outside[88..96].copy_from_slice(&(u64::MAX - 10).to_le_bytes());
    assert!(Assets::from_bytes(outside).is_err());

    let mut many = good.clone();
    many[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Assets::from_bytes(many).is_err());

    // An entry whose variants run past the variant table: entry 0 starts at the entries
    // section; find it through the section table.
    let table_at = u64::from_le_bytes(good[88..96].try_into().unwrap()) as usize;
    let count = u32::from_le_bytes(good[80..84].try_into().unwrap()) as usize;
    let section = |kind: u32| {
        (0..count)
            .map(|i| table_at + i * 24)
            .find(|&at| u32::from_le_bytes(good[at..at + 4].try_into().unwrap()) == kind)
            .map(|at| u64::from_le_bytes(good[at + 8..at + 16].try_into().unwrap()) as usize)
            .unwrap()
    };
    let entries = section(2);
    let mut far = good.clone();
    far[entries + 12..entries + 14].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(matches!(Assets::from_bytes(far), Err(Error::Corrupt(_))));
    let mut name = good.clone();
    name[entries..entries + 4].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
    assert!(matches!(Assets::from_bytes(name), Err(Error::Corrupt(_))));
    let mut loop_alias = good.clone();
    // The third entry is an alias; point it to itself (alias of an alias).
    let alias_at = entries + 2 * 64;
    loop_alias[alias_at + 20..alias_at + 24].copy_from_slice(&2u32.to_le_bytes());
    assert!(matches!(
        Assets::from_bytes(loop_alias),
        Err(Error::Corrupt(_))
    ));
    let variants = section(3);
    let mut data_far = good.clone();
    data_far[variants + 16..variants + 20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        Assets::from_bytes(data_far),
        Err(Error::Corrupt(_))
    ));
    let mut theme = good;
    theme[variants] = 99;
    assert!(matches!(Assets::from_bytes(theme), Err(Error::Corrupt(_))));
}

#[test]
fn opening_files_that_cannot_be_opened_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        Assets::open(dir.path().join("missing")),
        Err(Error::Io(_))
    ));
    assert!(Assets::open(dir.path()).is_err(), "a directory");
    let empty = dir.path().join("empty");
    std::fs::write(&empty, b"").unwrap();
    assert!(Assets::open(&empty).is_err());
    let cut = dir.path().join("cut");
    let bytes = small_file();
    std::fs::write(&cut, &bytes[..bytes.len() - 1]).unwrap();
    assert!(Assets::open(&cut).is_err());
    let ok = dir.path().join("ok");
    std::fs::write(&ok, &bytes).unwrap();
    assert!(Assets::open(&ok).is_ok());
}

#[test]
fn compiling_broken_sources_is_an_error_not_a_panic() {
    let fixture = common::fixture();
    // A rendered PNG of the wrong size.
    std::fs::write(
        fixture.options.icons.join("apps/mail/icon.png"),
        common::png_bytes(10, 10, |_, _| [1, 2, 3, 4]),
    )
    .unwrap();
    assert!(silica_assets::compile::compile(&fixture.options).is_err());
    // An SVG that is not an SVG.
    let fixture = common::fixture();
    std::fs::write(
        fixture.options.icons.join("files/file-generic/back.svg"),
        b"<not svg",
    )
    .unwrap();
    assert!(silica_assets::compile::compile(&fixture.options).is_err());
    // A manifest that is not JSON.
    let fixture = common::fixture();
    std::fs::write(
        fixture.options.icons.join("files/file-generic/icon.json"),
        b"{",
    )
    .unwrap();
    assert!(silica_assets::compile::compile(&fixture.options).is_err());
    // A missing icons folder.
    let mut options = common::fixture().options;
    options.icons = std::path::PathBuf::from("/nonexistent/icons");
    assert!(silica_assets::compile::compile(&options).is_err());
}
