//! Assembles the file: strings, entries, variants, slots, lookup tables and data blobs.
//! Everything is laid out in insertion order (or sorted), so the same input gives the same
//! bytes.

use std::collections::{BTreeMap, HashMap};

use crate::format::*;
use crate::mph;

use super::CompileError;

/// A variant to store.
pub struct VariantSpec {
    pub theme: Theme,
    pub layer: Layer,
    pub encoding: Encoding,
    pub state: State,
    pub size_px: u16,
    pub width: u16,
    pub height: u16,
    pub data: Vec<u8>,
}

/// A slot to store.
#[derive(Default)]
pub struct SlotSpec {
    pub kind: Option<SlotKind>,
    pub unit: u8,
    pub min_size: u16,
    pub rect: [f32; 4],
    pub font: Option<String>,
    pub size: u16,
    pub weight: u16,
    pub color: [u32; 2],
    pub format: Option<String>,
    pub tracking: f32,
    pub max_width: f32,
}

struct EntryRec {
    name: String,
    kind: EntryKind,
    flags: u16,
    alias_of: u32,
    variants: Vec<VariantSpec>,
    slots: Vec<SlotSpec>,
}

#[derive(Default)]
struct Strings {
    bytes: Vec<u8>,
    seen: HashMap<String, u32>,
}

impl Strings {
    fn intern(&mut self, text: &str) -> Result<u32, CompileError> {
        if let Some(&offset) = self.seen.get(text) {
            return Ok(offset);
        }
        if text.contains('\0') {
            return Err(CompileError::new(format!("string with a NUL: {text:?}")));
        }
        let offset = u32::try_from(self.bytes.len())
            .map_err(|_| CompileError::new("strings section too large"))?;
        self.bytes.extend_from_slice(text.as_bytes());
        self.bytes.push(0);
        self.seen.insert(text.to_owned(), offset);
        Ok(offset)
    }
}

/// Builds a `silica.assets`.
pub struct Builder {
    entries: Vec<EntryRec>,
    spaces: [BTreeMap<String, u32>; 10],
    content_version: String,
    source_hash: [u8; 32],
}

impl Builder {
    pub fn new(content_version: &str, source_hash: [u8; 32]) -> Builder {
        Builder {
            entries: Vec::new(),
            spaces: Default::default(),
            content_version: content_version.to_owned(),
            source_hash,
        }
    }

    /// Adds an entry (not yet reachable by any key; see [`Builder::register`]).
    pub fn add_entry(&mut self, name: &str, kind: EntryKind, flags: u16) -> u32 {
        self.entries.push(EntryRec {
            name: name.to_owned(),
            kind,
            flags,
            alias_of: NONE,
            variants: Vec::new(),
            slots: Vec::new(),
        });
        (self.entries.len() - 1) as u32
    }

    /// Whether `key` is registered in `space`.
    pub fn has_key(&self, space: KeySpace, key: &str) -> bool {
        self.spaces
            .get(space as usize)
            .is_some_and(|table| table.contains_key(key))
    }

    pub fn set_flags(&mut self, entry: u32, flags: u16) {
        if let Some(rec) = self.entries.get_mut(entry as usize) {
            rec.flags = flags;
        }
    }

    /// Makes the entry reachable by its own name in a key space.
    pub fn register(&mut self, space: KeySpace, entry: u32) -> Result<(), CompileError> {
        let name = self
            .entries
            .get(entry as usize)
            .ok_or_else(|| CompileError::new("register: no such entry"))?
            .name
            .clone();
        let table = self
            .spaces
            .get_mut(space as usize)
            .ok_or_else(|| CompileError::new("register: no such key space"))?;
        if table.insert(name.clone(), entry).is_some() {
            return Err(CompileError::new(format!(
                "duplicate key {name:?} in {space:?}"
            )));
        }
        Ok(())
    }

    /// Adds `name` in `space` as an alias of `target`.
    pub fn add_alias(
        &mut self,
        space: KeySpace,
        name: &str,
        target: u32,
    ) -> Result<(), CompileError> {
        let alias = self.add_entry(name, EntryKind::Alias, 0);
        if let Some(rec) = self.entries.get_mut(alias as usize) {
            rec.alias_of = target;
        }
        self.register(space, alias)
    }

    pub fn add_variant(&mut self, entry: u32, variant: VariantSpec) -> Result<(), CompileError> {
        self.entries
            .get_mut(entry as usize)
            .ok_or_else(|| CompileError::new("variant of an unknown entry"))?
            .variants
            .push(variant);
        Ok(())
    }

    pub fn add_slot(&mut self, entry: u32, slot: SlotSpec) -> Result<(), CompileError> {
        self.entries
            .get_mut(entry as usize)
            .ok_or_else(|| CompileError::new("slot of an unknown entry"))?
            .slots
            .push(slot);
        Ok(())
    }

    /// Lays out the file.
    pub fn finish(self) -> Result<Vec<u8>, CompileError> {
        let mut strings = Strings::default();
        // Offset 0 is the empty string.
        strings.intern("")?;
        let mut entries = Vec::new();
        let mut variants = Vec::new();
        let mut slots = Vec::new();
        let mut data = Vec::<u8>::new();
        let mut blobs: HashMap<(u64, usize), Vec<usize>> = HashMap::new();

        for entry in &self.entries {
            let first_variant = (variants.len() / VARIANT_SIZE) as u32;
            let first_slot = (slots.len() / SLOT_SIZE) as u32;
            let name = strings.intern(&entry.name)?;
            push_u32(&mut entries, name);
            push_u16(&mut entries, entry.kind as u16);
            push_u16(&mut entries, entry.flags);
            push_u32(&mut entries, first_variant);
            push_u16(
                &mut entries,
                u16::try_from(entry.variants.len()).map_err(|_| too_many(&entry.name))?,
            );
            push_u16(
                &mut entries,
                u16::try_from(entry.slots.len()).map_err(|_| too_many(&entry.name))?,
            );
            push_u32(&mut entries, first_slot);
            push_u32(&mut entries, entry.alias_of);
            entries.resize(entries.len() + (ENTRY_SIZE - 24), 0);

            for v in &entry.variants {
                // Identical blobs are stored once.
                let key = (xxhash_rust::xxh3::xxh3_64(&v.data), v.data.len());
                let existing = blobs.get(&key).and_then(|offsets| {
                    offsets
                        .iter()
                        .copied()
                        .find(|&o| data.get(o..o + v.data.len()) == Some(v.data.as_slice()))
                });
                let offset = match existing {
                    Some(o) => o,
                    None => {
                        data.resize(align_up(data.len()), 0);
                        let o = data.len();
                        data.extend_from_slice(&v.data);
                        blobs.entry(key).or_default().push(o);
                        o
                    }
                };
                variants.push(v.theme as u8);
                variants.push(v.layer as u8);
                variants.push(v.encoding as u8);
                variants.push(v.state as u8);
                push_u16(&mut variants, v.size_px);
                push_u16(&mut variants, 0);
                push_u64(&mut variants, offset as u64);
                push_u32(
                    &mut variants,
                    u32::try_from(v.data.len()).map_err(|_| too_many(&entry.name))?,
                );
                push_u16(&mut variants, v.width);
                push_u16(&mut variants, v.height);
                variants.resize(variants.len() + 8, 0);
            }

            for s in &entry.slots {
                let kind = s
                    .kind
                    .ok_or_else(|| CompileError::new("slot without a kind"))?;
                slots.push(kind as u8);
                slots.push(s.unit);
                push_u16(&mut slots, s.min_size);
                for f in s.rect {
                    push_u32(&mut slots, f.to_bits());
                }
                let font = match &s.font {
                    Some(text) => strings.intern(text)?,
                    None => NONE,
                };
                push_u32(&mut slots, font);
                push_u16(&mut slots, s.size);
                push_u16(&mut slots, s.weight);
                push_u32(&mut slots, s.color[0]);
                push_u32(&mut slots, s.color[1]);
                let format = match &s.format {
                    Some(text) => strings.intern(text)?,
                    None => NONE,
                };
                push_u32(&mut slots, format);
                push_u32(&mut slots, s.tracking.to_bits());
                push_u32(&mut slots, s.max_width.to_bits());
            }
        }

        let mut tables: Vec<(u32, Vec<u8>)> = Vec::new();
        for (index, table) in self.spaces.iter().enumerate() {
            let pairs: Vec<(&str, u32)> = table.iter().map(|(k, &v)| (k.as_str(), v)).collect();
            let bytes = mph::build(&pairs)
                .map_err(|e| CompileError::new(format!("lookup table {index}: {e}")))?;
            tables.push((SECTION_LOOKUP + index as u32, bytes));
        }

        let mut sections: Vec<(u32, &[u8])> = vec![
            (SECTION_STRINGS, &strings.bytes),
            (SECTION_ENTRIES, &entries),
            (SECTION_VARIANTS, &variants),
            (SECTION_SLOTS, &slots),
        ];
        sections.extend(tables.iter().map(|(k, b)| (*k, b.as_slice())));
        sections.push((SECTION_DATA, &data));

        let table_at = HEADER_SIZE;
        let mut cursor = align_up(table_at + sections.len() * SECTION_SIZE);
        let mut placed = Vec::new();
        for (kind, bytes) in &sections {
            placed.push((*kind, cursor, bytes.len()));
            cursor = align_up(cursor + bytes.len());
        }
        // The file ends where its last section ends.
        let total = placed.last().map_or(cursor, |&(_, at, len)| at + len);

        let mut out = vec![0u8; total];
        let version = self.content_version.as_bytes();
        if version.len() >= 32 {
            return Err(CompileError::new("content version longer than 31 bytes"));
        }
        put(&mut out, 0, &MAGIC);
        put(&mut out, 8, &FORMAT.to_le_bytes());
        put(&mut out, 16, version);
        put(&mut out, 48, &self.source_hash);
        put(&mut out, 80, &(sections.len() as u32).to_le_bytes());
        put(&mut out, 88, &(table_at as u64).to_le_bytes());
        put(&mut out, 96, &(total as u64).to_le_bytes());
        for (i, (kind, at, len)) in placed.iter().enumerate() {
            let rec = table_at + i * SECTION_SIZE;
            put(&mut out, rec, &kind.to_le_bytes());
            put(&mut out, rec + 8, &(*at as u64).to_le_bytes());
            put(&mut out, rec + 16, &(*len as u64).to_le_bytes());
        }
        for ((_, bytes), (_, at, _)) in sections.iter().zip(&placed) {
            put(&mut out, *at, bytes);
        }
        Ok(out)
    }
}

fn too_many(name: &str) -> CompileError {
    CompileError::new(format!("too many variants, slots or bytes in {name:?}"))
}

fn put(out: &mut [u8], at: usize, bytes: &[u8]) {
    if let Some(dst) = out.get_mut(at..at + bytes.len()) {
        dst.copy_from_slice(bytes);
    }
}

fn push_u16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn push_u32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn push_u64(v: &mut Vec<u8>, x: u64) {
    v.extend_from_slice(&x.to_le_bytes());
}
