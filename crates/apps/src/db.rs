//! `~/.cache/silica/apps.bin`: what the registry remembers between sessions (when each app was
//! first seen and last launched). Binary, versioned, written atomically.
//!
//! ```text
//! "SAPP" u8 version=1 u64 created u32 count
//! count × { u16 id length, id (UTF-8), u64 added, u64 last_used }
//! ```
//! Little endian; times in unix seconds.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

const MAGIC: &[u8; 4] = b"SAPP";
const VERSION: u8 = 1;
/// More records than this are not read (a damaged count).
const MAX_RECORDS: u32 = 100_000;

/// The memory of one app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// When the registry first saw it.
    pub added: u64,
    /// When it was last launched (0: never).
    pub last_used: u64,
}

/// The database.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Db {
    /// When the database was created: apps already there are not *new*.
    pub created: u64,
    pub records: BTreeMap<String, Record>,
}

impl Db {
    /// A new database created at `now`.
    pub fn new(now: u64) -> Db {
        Db {
            created: now,
            records: BTreeMap::new(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(17 + self.records.len() * 40);
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&self.created.to_le_bytes());
        let records: Vec<_> = self
            .records
            .iter()
            .filter(|(id, _)| id.len() <= usize::from(u16::MAX))
            .take(MAX_RECORDS as usize)
            .collect();
        out.extend_from_slice(&u32::try_from(records.len()).unwrap_or(0).to_le_bytes());
        for (id, record) in records {
            out.extend_from_slice(&u16::try_from(id.len()).unwrap_or(0).to_le_bytes());
            out.extend_from_slice(id.as_bytes());
            out.extend_from_slice(&record.added.to_le_bytes());
            out.extend_from_slice(&record.last_used.to_le_bytes());
        }
        out
    }

    /// Decodes a database; None when damaged or of another version.
    pub fn decode(bytes: &[u8]) -> Option<Db> {
        let mut at = 0usize;
        let mut take = |n: usize| -> Option<&[u8]> {
            let slice = bytes.get(at..at.checked_add(n)?)?;
            at += n;
            Some(slice)
        };
        if take(4)? != MAGIC || take(1)? != [VERSION] {
            return None;
        }
        let created = u64::from_le_bytes(take(8)?.try_into().ok()?);
        let count = u32::from_le_bytes(take(4)?.try_into().ok()?);
        if count > MAX_RECORDS {
            return None;
        }
        let mut records = BTreeMap::new();
        for _ in 0..count {
            let len = u16::from_le_bytes(take(2)?.try_into().ok()?);
            let id = std::str::from_utf8(take(usize::from(len))?)
                .ok()?
                .to_owned();
            let added = u64::from_le_bytes(take(8)?.try_into().ok()?);
            let last_used = u64::from_le_bytes(take(8)?.try_into().ok()?);
            records.insert(id, Record { added, last_used });
        }
        Some(Db { created, records })
    }

    /// Reads the file; None when it is missing or damaged.
    pub fn load(path: &Path) -> Option<Db> {
        let meta = std::fs::metadata(path).ok()?;
        if meta.len() > 16 * 1024 * 1024 {
            return None;
        }
        Db::decode(&std::fs::read(path).ok()?)
    }

    /// Writes the file atomically (a temporary file renamed over it).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        let result = (|| {
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(&self.encode())?;
            file.sync_all()?;
            std::fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    /// Records the apps of a scan: ids not seen before get `added = now`. Returns whether
    /// something changed.
    pub fn observe<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>, now: u64) -> bool {
        let mut changed = false;
        for id in ids {
            if !self.records.contains_key(id) {
                self.records.insert(
                    id.to_owned(),
                    Record {
                        added: now,
                        last_used: 0,
                    },
                );
                changed = true;
            }
        }
        changed
    }

    /// Records a launch.
    pub fn launched(&mut self, id: &str, now: u64) {
        let record = self.records.entry(id.to_owned()).or_insert(Record {
            added: now,
            last_used: 0,
        });
        record.last_used = now;
    }

    /// Whether `id` is new: first seen after the database was created, never launched.
    pub fn is_new(&self, id: &str) -> bool {
        self.records
            .get(id)
            .is_some_and(|r| r.last_used == 0 && r.added > self.created)
    }

    /// Forgets the records of apps no longer installed for more than `keep` seconds, so that a
    /// reinstalled app counts as new again only after a while (a move is not a reinstall).
    pub fn prune(&mut self, installed: &std::collections::HashSet<&str>, now: u64, keep: u64) {
        self.records.retain(|id, r| {
            installed.contains(id.as_str()) || now.saturating_sub(r.last_used.max(r.added)) < keep
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_damage() {
        let mut db = Db::new(100);
        assert!(db.observe(["a", "b"], 100));
        assert!(!db.observe(["a"], 150));
        assert!(db.observe(["c"], 200));
        db.launched("b", 300);
        let bytes = db.encode();
        assert_eq!(Db::decode(&bytes), Some(db.clone()));
        // Apps present at creation are not new; one added later is until launched.
        assert!(!db.is_new("a"));
        assert!(db.is_new("c"));
        db.launched("c", 400);
        assert!(!db.is_new("c"));
        for cut in 0..bytes.len() {
            assert_eq!(Db::decode(&bytes[..cut]), None, "cut at {cut}");
        }
        let mut bad = bytes.clone();
        bad[4] = 9;
        assert_eq!(Db::decode(&bad), None);
    }

    #[test]
    fn saves_atomically_and_prunes() {
        let dir = std::env::temp_dir().join(format!("appsdb-{}", std::process::id()));
        let path = dir.join("silica/apps.bin");
        let mut db = Db::new(1);
        db.observe(["x", "y"], 10);
        db.save(&path).unwrap();
        assert_eq!(Db::load(&path), Some(db.clone()));
        let installed: std::collections::HashSet<&str> = ["x"].into_iter().collect();
        db.prune(&installed, 20, 100);
        assert_eq!(db.records.len(), 2);
        db.prune(&installed, 1000, 100);
        assert_eq!(db.records.keys().collect::<Vec<_>>(), ["x"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
