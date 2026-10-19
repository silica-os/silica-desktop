//! zsync deltas (the format of zsync 0.6, used by AppImageUpdate): a control file lists the
//! weak rolling checksum and a strong MD4 checksum of every block of the new file; the blocks a
//! local seed (the installed version) already has are copied from it, only the others are
//! downloaded (HTTP ranges), and the result must match the file's SHA-1.
//!
//! The control file: text headers (`zsync:`, `Filename:`, `Blocksize:`, `Length:`,
//! `Hash-Lengths: seq,rsum,checksum`, `URL:`, `SHA-1:`), a blank line, then per block the last
//! `rsum` bytes of the big-endian weak checksum `[a, b]` and the first `checksum` bytes of MD4.

use std::collections::HashMap;

use md4::{Digest, Md4};
use sha1::Sha1;

/// A parsed control file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub filename: String,
    pub blocksize: usize,
    pub length: u64,
    pub seq_matches: usize,
    pub rsum_bytes: usize,
    pub checksum_bytes: usize,
    /// The URL of the file (may be relative to the control file's).
    pub url: String,
    pub sha1: [u8; 20],
    /// Per block: the stored weak checksum bytes (as a number) and the MD4 prefix.
    pub blocks: Vec<(u32, Vec<u8>)>,
}

/// The weak checksum of zsync: `a` the sum of the bytes, `b` the sum of the running `a`
/// (16-bit each).
fn rsum(block: &[u8]) -> (u16, u16) {
    let (mut a, mut b) = (0u16, 0u16);
    for &c in block {
        a = a.wrapping_add(u16::from(c));
        b = b.wrapping_add(a);
    }
    (a, b)
}

/// The stored form of a weak checksum: the last `bytes` bytes of `[a_hi a_lo b_hi b_lo]`.
fn stored(r: (u16, u16), bytes: usize) -> u32 {
    let full = (u32::from(r.0) << 16) | u32::from(r.1);
    match bytes {
        0 => 0,
        1..=3 => full & ((1u32 << (8 * bytes)) - 1),
        _ => full,
    }
}

fn md4_prefix(block: &[u8], blocksize: usize, bytes: usize) -> Vec<u8> {
    let mut hasher = Md4::new();
    hasher.update(block);
    // A short last block is checksummed padded with zeros, as zsyncmake does.
    if block.len() < blocksize {
        hasher.update(vec![0u8; blocksize - block.len()]);
    }
    hasher.finalize().iter().take(bytes).copied().collect()
}

fn hex20(text: &str) -> Option<[u8; 20]> {
    let text = text.trim();
    if text.len() != 40 {
        return None;
    }
    let mut out = [0u8; 20];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(text.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

impl Control {
    /// Parses a control file.
    pub fn parse(bytes: &[u8]) -> Result<Control, String> {
        let end = bytes
            .windows(2)
            .position(|w| w == b"\n\n")
            .ok_or("not a zsync control file: no end of headers")?;
        let head = std::str::from_utf8(bytes.get(..end).unwrap_or_default())
            .map_err(|_| "not a zsync control file: headers are not text")?;
        let body = bytes.get(end + 2..).unwrap_or_default();
        let mut headers: HashMap<&str, &str> = HashMap::new();
        for line in head.lines() {
            if let Some((k, v)) = line.split_once(':') {
                headers.entry(k.trim()).or_insert(v.trim());
            }
        }
        if !headers.get("zsync").is_some_and(|v| v.starts_with("0.")) {
            return Err("not a zsync 0.x control file".to_owned());
        }
        let num = |k: &str| -> Result<u64, String> {
            headers
                .get(k)
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| format!("zsync: missing or bad {k}"))
        };
        let blocksize = usize::try_from(num("Blocksize")?).map_err(|e| e.to_string())?;
        if !(512..=16 * 1024 * 1024).contains(&blocksize) {
            return Err("zsync: unsupported block size".to_owned());
        }
        let length = num("Length")?;
        let lengths: Vec<usize> = headers
            .get("Hash-Lengths")
            .unwrap_or(&"1,4,16")
            .split(',')
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        let [seq_matches, rsum_bytes, checksum_bytes] = lengths.as_slice() else {
            return Err("zsync: bad Hash-Lengths".to_owned());
        };
        if !(1..=2).contains(seq_matches)
            || !(1..=4).contains(rsum_bytes)
            || !(3..=16).contains(checksum_bytes)
        {
            return Err("zsync: unsupported Hash-Lengths".to_owned());
        }
        let count =
            usize::try_from(length.div_ceil(blocksize as u64)).map_err(|e| e.to_string())?;
        let per = rsum_bytes + checksum_bytes;
        if body.len() < count.saturating_mul(per) {
            return Err("zsync: the block checksums are truncated".to_owned());
        }
        let mut blocks = Vec::with_capacity(count);
        for i in 0..count {
            let chunk = body.get(i * per..(i + 1) * per).ok_or("zsync: truncated")?;
            let mut r = 0u32;
            for &b in chunk.get(..*rsum_bytes).unwrap_or_default() {
                r = (r << 8) | u32::from(b);
            }
            blocks.push((r, chunk.get(*rsum_bytes..).unwrap_or_default().to_vec()));
        }
        Ok(Control {
            filename: headers.get("Filename").unwrap_or(&"").to_string(),
            blocksize,
            length,
            seq_matches: *seq_matches,
            rsum_bytes: *rsum_bytes,
            checksum_bytes: *checksum_bytes,
            url: headers.get("URL").unwrap_or(&"").to_string(),
            sha1: hex20(headers.get("SHA-1").unwrap_or(&"")).ok_or("zsync: missing SHA-1")?,
            blocks,
        })
    }

    /// The byte range of block `i` in the target.
    pub fn block_range(&self, i: usize) -> (u64, u64) {
        let start = (i as u64) * self.blocksize as u64;
        (start, (start + self.blocksize as u64).min(self.length))
    }
}

/// Makes a control file for `data` (what `zsyncmake` makes, without gzip maps).
pub fn make(data: &[u8], blocksize: usize, filename: &str, url: &str) -> Vec<u8> {
    let length = data.len() as u64;
    // The weak checksum bytes and sequential matches as zsyncmake picks them.
    let blocks = length.div_ceil(blocksize as u64).max(1);
    let seq_matches = if blocks > 1 { 2 } else { 1 };
    let bits = (length as f64).log2() + (blocksize as f64).log2();
    let rsum_bytes = (((bits - 8.6) / seq_matches as f64 / 8.0).ceil() as usize).clamp(2, 4);
    let checksum_bytes = (((20.0
        + ((1.0 + length as f64 / blocksize as f64).log2() + (length as f64).log2()))
        / seq_matches as f64
        / 8.0)
        .ceil() as usize)
        .clamp(3, 16);
    let sha1: String = Sha1::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut out = format!(
        "zsync: 0.6.2\nFilename: {filename}\nBlocksize: {blocksize}\nLength: {length}\n\
         Hash-Lengths: {seq_matches},{rsum_bytes},{checksum_bytes}\nURL: {url}\nSHA-1: {sha1}\n\n"
    )
    .into_bytes();
    for block in data.chunks(blocksize) {
        let mut padded = block.to_vec();
        padded.resize(blocksize, 0);
        let r = stored(rsum(&padded), rsum_bytes).to_be_bytes();
        out.extend_from_slice(r.get(4 - rsum_bytes..).unwrap_or_default());
        out.extend_from_slice(&md4_prefix(block, blocksize, checksum_bytes));
    }
    out
}

/// For each block of the target, where the seed has it (an offset), if anywhere.
pub fn plan(control: &Control, seed: &[u8]) -> Vec<Option<usize>> {
    let bs = control.blocksize;
    let count = control.blocks.len();
    let mut found: Vec<Option<usize>> = vec![None; count];
    if seed.len() < bs || count == 0 {
        return found;
    }
    let mut index: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, (r, _)) in control.blocks.iter().enumerate() {
        index.entry(*r).or_default().push(i);
    }
    let window = |p: usize| seed.get(p..p + bs);
    let mut p = 0usize;
    let Some(first) = window(0) else {
        return found;
    };
    let mut r0 = rsum(first);
    // The next block's sum, for two sequential matches.
    let next_sum = |p: usize| window(p + bs).map(rsum);
    let mut r1 = next_sum(0);
    let mut remaining = count;
    // The block matched right before this position: its successor needs no second match.
    let mut previous: Option<usize> = None;
    while remaining > 0 {
        let key = stored(r0, control.rsum_bytes);
        let mut matched = false;
        if let Some(candidates) = index.get(&key) {
            for &i in candidates {
                if found.get(i).is_some_and(Option::is_some) {
                    continue;
                }
                let continues = previous.is_some_and(|j| j + 1 == i);
                // Two sequential matches: the next block of the target must follow in the seed
                // (unless this one continues a match).
                if control.seq_matches == 2
                    && !continues
                    && let Some((next_r, _)) = control.blocks.get(i + 1)
                    && r1.is_none_or(|r| stored(r, control.rsum_bytes) != *next_r)
                {
                    continue;
                }
                let Some(block) = window(p) else { continue };
                let Some((_, strong)) = control.blocks.get(i) else {
                    continue;
                };
                // The last block of the target is shorter: compare that much.
                let (start, end) = control.block_range(i);
                let len = usize::try_from(end - start).unwrap_or(bs);
                let piece = block.get(..len).unwrap_or(block);
                if &md4_prefix(piece, bs, control.checksum_bytes) == strong {
                    if let Some(slot) = found.get_mut(i) {
                        *slot = Some(p);
                    }
                    remaining -= 1;
                    matched = true;
                    previous = Some(i);
                    break;
                }
            }
        }
        if !matched {
            previous = None;
        }
        // After a match the next block starts right after it.
        let step = if matched { bs } else { 1 };
        if step == 1 {
            let (Some(&out), Some(&inn)) = (seed.get(p), seed.get(p + bs)) else {
                break;
            };
            let (a, b) = r0;
            let a2 = a.wrapping_sub(u16::from(out)).wrapping_add(u16::from(inn));
            let b2 = b
                .wrapping_sub((bs as u16).wrapping_mul(u16::from(out)))
                .wrapping_add(a2);
            r0 = (a2, b2);
            r1 = match (r1, seed.get(p + bs), seed.get(p + 2 * bs)) {
                (Some((a, b)), Some(&out), Some(&inn)) => {
                    let a2 = a.wrapping_sub(u16::from(out)).wrapping_add(u16::from(inn));
                    let b2 = b
                        .wrapping_sub((bs as u16).wrapping_mul(u16::from(out)))
                        .wrapping_add(a2);
                    Some((a2, b2))
                }
                _ => None,
            };
            p += 1;
        } else {
            p += bs;
            let Some(w) = window(p) else { break };
            r0 = rsum(w);
            r1 = next_sum(p);
        }
    }
    found
}

/// The byte ranges of the target to download (consecutive missing blocks merged), as
/// `(start, end)` with `end` exclusive.
pub fn missing(control: &Control, plan: &[Option<usize>]) -> Vec<(u64, u64)> {
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for (i, known) in plan.iter().enumerate() {
        if known.is_some() {
            continue;
        }
        let (start, end) = control.block_range(i);
        match ranges.last_mut() {
            Some(last) if last.1 == start => last.1 = end,
            _ => ranges.push((start, end)),
        }
    }
    ranges
}

/// Puts the target together from the seed and the downloaded ranges (`(start, bytes)`), and
/// checks its SHA-1.
pub fn assemble(
    control: &Control,
    seed: &[u8],
    plan: &[Option<usize>],
    fetched: &[(u64, Vec<u8>)],
) -> Result<Vec<u8>, String> {
    let length = usize::try_from(control.length).map_err(|e| e.to_string())?;
    let mut out = vec![0u8; length];
    for (i, known) in plan.iter().enumerate() {
        let (start, end) = control.block_range(i);
        let (start, end) = (
            usize::try_from(start).map_err(|e| e.to_string())?,
            usize::try_from(end).map_err(|e| e.to_string())?,
        );
        let len = end - start;
        let Some(offset) = known else { continue };
        let from = seed
            .get(*offset..*offset + len)
            .ok_or("zsync: seed too short")?;
        out.get_mut(start..end)
            .ok_or("zsync: block out of range")?
            .copy_from_slice(from);
    }
    for (start, bytes) in fetched {
        let start = usize::try_from(*start).map_err(|e| e.to_string())?;
        out.get_mut(start..start + bytes.len())
            .ok_or("zsync: downloaded range out of the file")?
            .copy_from_slice(bytes);
    }
    let digest: [u8; 20] = Sha1::digest(&out).into();
    if digest != control.sha1 {
        return Err("zsync: the result does not match its SHA-1".to_owned());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/zsync");

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{FIXTURES}/{name}")).unwrap()
    }

    /// The control file made by zsyncmake 0.6.2 itself (tests/fixtures/zsync).
    #[test]
    fn a_real_zsyncmake_control_file_rebuilds_the_new_file() {
        let control = Control::parse(&fixture("new.bin.zsync")).unwrap();
        let (old, new) = (fixture("old.bin"), fixture("new.bin"));
        assert_eq!(control.blocksize, 2048);
        assert_eq!(control.length, new.len() as u64);
        assert_eq!(
            (
                control.seq_matches,
                control.rsum_bytes,
                control.checksum_bytes
            ),
            (2, 2, 3)
        );
        let plan = plan(&control, &old);
        let reused = plan.iter().filter(|p| p.is_some()).count();
        // Every block made only of old bytes is reused (blocks 0, 1, 4-7, 9-11): only those
        // that hold changed, inserted or appended bytes are downloaded.
        assert_eq!(reused, 9, "reused {reused} of {}", plan.len());
        let ranges = missing(&control, &plan);
        let downloaded: u64 = ranges.iter().map(|(s, e)| e - s).sum();
        assert!(downloaded < new.len() as u64 / 2, "{downloaded} bytes");
        let fetched: Vec<(u64, Vec<u8>)> = ranges
            .iter()
            .map(|(s, e)| (*s, new[*s as usize..*e as usize].to_vec()))
            .collect();
        assert_eq!(assemble(&control, &old, &plan, &fetched).unwrap(), new);
        // A wrong range fails the SHA-1.
        let mut bad = fetched.clone();
        if let Some((_, bytes)) = bad.first_mut() {
            bytes[0] ^= 1;
        }
        assert!(assemble(&control, &old, &plan, &bad).is_err());
    }

    #[test]
    fn our_control_files_match_zsyncmakes() {
        let new = fixture("new.bin");
        let ours = Control::parse(&make(&new, 2048, "new.bin", "new.bin")).unwrap();
        let theirs = Control::parse(&fixture("new.bin.zsync")).unwrap();
        assert_eq!(ours.blocks, theirs.blocks);
        assert_eq!(ours.sha1, theirs.sha1);
        assert_eq!(
            (ours.seq_matches, ours.rsum_bytes, ours.checksum_bytes),
            (theirs.seq_matches, theirs.rsum_bytes, theirs.checksum_bytes)
        );
    }

    #[test]
    fn no_seed_downloads_everything() {
        let data: Vec<u8> = (0..10_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let control = Control::parse(&make(&data, 1024, "x", "x")).unwrap();
        let plan = plan(&control, &[]);
        assert!(plan.iter().all(Option::is_none));
        assert_eq!(missing(&control, &plan), [(0, 10_000)]);
        assert_eq!(
            assemble(&control, &[], &plan, &[(0, data.clone())]).unwrap(),
            data
        );
        assert!(Control::parse(b"garbage").is_err());
    }
}
