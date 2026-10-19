//! Local time without a time-zone library: the UTC offset now from the system's TZif file
//! (`$TZ` as a path or a zone name, else `/etc/localtime`), and the calendar from days since
//! the epoch. Without a readable zone the time is UTC.

use std::time::{SystemTime, UNIX_EPOCH};

/// A local date and time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// Year.
    pub year: i64,
    /// Month, 1–12.
    pub month: u32,
    /// Day of the month, 1–31.
    pub day: u32,
    /// Day of the week, 0 Monday … 6 Sunday.
    pub weekday: u32,
    /// Hour, 0–23.
    pub hour: u32,
    /// Minute.
    pub minute: u32,
    /// Second.
    pub second: u32,
}

/// English names (the UI is English, translations come later).
pub const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
/// Month names.
pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn be32(b: &[u8], at: usize) -> Option<i64> {
    let s: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
    Some(i64::from(i32::from_be_bytes(s)))
}

fn be64(b: &[u8], at: usize) -> Option<i64> {
    let s: [u8; 8] = b.get(at..at + 8)?.try_into().ok()?;
    Some(i64::from_be_bytes(s))
}

/// The UTC offset (seconds) at `now` in a TZif file.
pub fn offset_in_tzif(data: &[u8], now: i64) -> Option<i64> {
    if data.get(0..4)? != b"TZif" {
        return None;
    }
    let version = *data.get(4)?;
    let counts = |base: usize| -> Option<[usize; 6]> {
        let mut c = [0usize; 6];
        for (i, v) in c.iter_mut().enumerate() {
            *v = usize::try_from(be32(data, base + 20 + i * 4)?).ok()?;
        }
        Some(c)
    };
    // [isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt]
    let c1 = counts(0)?;
    let v1_len = c1[3] * 5 + c1[4] * 6 + c1[5] + c1[2] * 8 + c1[1] + c1[0];
    let (base, c, tsize) = if version >= b'2' {
        let b2 = 44 + v1_len;
        (b2, counts(b2)?, 8usize)
    } else {
        (0, c1, 4usize)
    };
    let times = base + 44;
    let idx = times + c[3] * tsize;
    let types = idx + c[3];
    let read_t = |i: usize| -> Option<i64> {
        if tsize == 8 {
            be64(data, times + i * 8)
        } else {
            be32(data, times + i * 4)
        }
    };
    // The last transition at or before now; before the first, the first standard type.
    let mut ty = 0usize;
    let mut found = false;
    for i in 0..c[3] {
        if read_t(i)? <= now {
            ty = usize::from(*data.get(idx + i)?);
            found = true;
        } else {
            break;
        }
    }
    if !found {
        ty = 0;
    }
    if ty >= c[4] {
        return None;
    }
    be32(data, types + ty * 6)
}

fn zone_file() -> Option<Vec<u8>> {
    if let Some(tz) = std::env::var_os("TZ").and_then(|t| t.into_string().ok()) {
        let tz = tz.trim_start_matches(':');
        let path = if tz.starts_with('/') {
            tz.to_string()
        } else {
            format!("/usr/share/zoneinfo/{tz}")
        };
        if !tz.contains("..")
            && let Ok(d) = std::fs::read(path)
        {
            return Some(d);
        }
    }
    std::fs::read("/etc/localtime").ok()
}

/// Seconds since the epoch.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// Days since the epoch → (year, month, day) (Howard Hinnant's civil-from-days).
pub fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The local time at `unix` seconds with a UTC offset.
pub fn at(unix: i64, offset: i64) -> LocalTime {
    let t = unix + offset;
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let (year, month, day) = civil(days);
    LocalTime {
        year,
        month,
        day,
        // 1970-01-01 was a Thursday.
        weekday: ((days + 3).rem_euclid(7)) as u32,
        hour: (secs / 3600) as u32,
        minute: ((secs % 3600) / 60) as u32,
        second: (secs % 60) as u32,
    }
}

/// The local zone's UTC offset at `unix` seconds.
pub fn offset_at(unix: i64) -> i64 {
    zone_file()
        .and_then(|d| offset_in_tzif(&d, unix))
        .unwrap_or(0)
}

/// The local time now.
pub fn now() -> LocalTime {
    local_at(unix_now())
}

/// The local time at `unix` seconds.
pub fn local_at(unix: i64) -> LocalTime {
    at(unix, offset_at(unix))
}

/// The epoch seconds of today's local midnight.
pub fn midnight_unix() -> i64 {
    let unix = unix_now();
    let offset = offset_at(unix);
    (unix + offset).div_euclid(86_400) * 86_400 - offset
}

impl LocalTime {
    /// `Monday 19 October` (the lock screen's date).
    pub fn long_date(&self) -> String {
        format!(
            "{} {} {}",
            WEEKDAYS.get(self.weekday as usize).copied().unwrap_or(""),
            self.day,
            MONTHS
                .get(self.month.saturating_sub(1) as usize)
                .copied()
                .unwrap_or("")
        )
    }

    /// `15:13` or `3:13` (12-hour without the period, as the lock screen's clock).
    pub fn clock(&self, hour24: bool) -> String {
        if hour24 {
            format!("{:02}:{:02}", self.hour, self.minute)
        } else {
            let h = match self.hour % 12 {
                0 => 12,
                h => h,
            };
            format!("{h}:{:02}", self.minute)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_and_formats() {
        // 2026-10-19 15:13:20 UTC is a Monday.
        let t = at(1_792_422_800, 0);
        assert_eq!((t.year, t.month, t.day), (2026, 10, 19));
        assert_eq!(t.weekday, 0);
        assert_eq!(t.long_date(), "Monday 19 October");
        assert_eq!(at(1_792_422_800, 7200).clock(true), "17:13");
        assert_eq!(t.clock(false), "3:13");
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
    }

    #[test]
    fn offsets_from_a_tzif_file() {
        // A minimal v1 file: one transition at t=1000 from type 0 (+3600) to type 1 (+7200).
        let mut f = b"TZif".to_vec();
        f.push(0);
        f.extend([0u8; 15]);
        for c in [0u32, 0, 0, 1, 2, 4] {
            f.extend(c.to_be_bytes());
        }
        f.extend(1000i32.to_be_bytes());
        f.push(1);
        f.extend(3600i32.to_be_bytes());
        f.extend([0u8, 0]);
        f.extend(7200i32.to_be_bytes());
        f.extend([1u8, 0]);
        f.extend(b"AB\0\0");
        assert_eq!(offset_in_tzif(&f, 10), Some(3600));
        assert_eq!(offset_in_tzif(&f, 5000), Some(7200));
        assert_eq!(offset_in_tzif(b"nope", 0), None);
    }
}
