//! Animation curves: CSS cubic Béziers and measured tables (docs/spec).

/// A CSS `cubic-bezier(x1, y1, x2, y2)` easing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bezier(pub f32, pub f32, pub f32, pub f32);

impl Bezier {
    /// The eased value at progress `t` (0–1).
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let Bezier(x1, y1, x2, y2) = self;
        let coord = |a: f32, b: f32, s: f32| {
            let u = 1.0 - s;
            3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
        };
        // Bisection on x(s) = t: monotonic for 0 ≤ x1, x2 ≤ 1.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if coord(x1, x2, mid) < t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        coord(y1, y2, (lo + hi) / 2.0)
    }
}

/// Linear interpolation in `table`, sampled every `step` from 0; clamped at both ends.
pub fn sample(table: &[f32], step: f32, x: f32) -> f32 {
    let (Some(first), Some(last)) = (table.first(), table.last()) else {
        return 0.0;
    };
    if !x.is_finite() || x <= 0.0 {
        return *first;
    }
    let i = x / step;
    let k = i.floor() as usize;
    match (table.get(k), table.get(k + 1)) {
        (Some(a), Some(b)) => a + (b - a) * (i - k as f32),
        _ => *last,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_hit_their_ends() {
        let b = Bezier(0.38, 0.0, 0.64, 1.0);
        assert!(b.at(0.0).abs() < 1e-4);
        assert!((b.at(1.0) - 1.0).abs() < 1e-4);
        assert!((b.at(0.5) - 0.5).abs() < 0.05);
        assert_eq!(sample(&[0.0, 1.0, 3.0], 10.0, 15.0), 2.0);
        assert_eq!(sample(&[0.0, 1.0], 10.0, 50.0), 1.0);
    }
}
