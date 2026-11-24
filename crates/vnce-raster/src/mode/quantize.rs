//! Reducing the pixels of one cell to two colors.

use crate::color::Rgba;

/// The pixels of a cell split into two color groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Split {
    /// Bit `k` is set when pixel `k` belongs to the foreground group.
    pub(crate) mask: u32,
    /// Mean color of the foreground group.
    pub(crate) fg: Rgba,
    /// Mean color of the background group.
    pub(crate) bg: Rgba,
}

/// Splits up to 8 opaque pixels into the two groups that represent them best (smallest
/// total squared color error).
///
/// `exact` tries every split; otherwise a fast two-step clustering starting from the two
/// most different pixels is used. Pixel 0 is always in the background group, so equal
/// inputs always produce equal outputs (stable frames).
pub(crate) fn split(pixels: &[Rgba], exact: bool) -> Split {
    debug_assert!(!pixels.is_empty() && pixels.len() <= 8);
    if pixels.iter().all(|&p| p == pixels[0]) {
        return Split {
            mask: 0,
            fg: pixels[0],
            bg: pixels[0],
        };
    }
    let mask = if exact {
        best_mask(pixels)
    } else {
        cluster_mask(pixels)
    };
    finish(pixels, mask)
}

/// Builds the result for `mask`, normalized so that pixel 0 is in the background group.
fn finish(pixels: &[Rgba], mask: u32) -> Split {
    let full = (1u32 << pixels.len()) - 1;
    let mask = if mask & 1 == 1 { !mask & full } else { mask };
    let fg = mean(pixels, |k| mask & (1 << k) != 0);
    let bg = mean(pixels, |k| mask & (1 << k) == 0);
    Split {
        mask,
        fg: fg.unwrap_or(bg.unwrap_or(pixels[0])),
        bg: bg.unwrap_or(pixels[0]),
    }
}

/// Mean color of the selected pixels, or `None` if none is selected.
fn mean(pixels: &[Rgba], selected: impl Fn(usize) -> bool) -> Option<Rgba> {
    let mut sum = [0u32; 3];
    let mut count = 0u32;
    for (k, p) in pixels.iter().enumerate() {
        if selected(k) {
            sum[0] += u32::from(p.r);
            sum[1] += u32::from(p.g);
            sum[2] += u32::from(p.b);
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let channel = |s: u32| u8::try_from((s + count / 2) / count).unwrap_or(u8::MAX);
    Some(Rgba::rgb(channel(sum[0]), channel(sum[1]), channel(sum[2])))
}

/// Exhaustive search. The error of a split is `Σ|p|² - |S₀|²/n₀ - |S₁|²/n₁`; the first
/// term is the same for every split, so the split with the largest
/// `|S₀|²/n₀ + |S₁|²/n₁` wins.
fn best_mask(pixels: &[Rgba]) -> u32 {
    let n = pixels.len();
    let vectors: Vec<[f64; 3]> = pixels
        .iter()
        .map(|p| [f64::from(p.r), f64::from(p.g), f64::from(p.b)])
        .collect();
    let total = vectors.iter().fold([0.0; 3], |acc, v| add(acc, *v));
    let mut best = (f64::NEG_INFINITY, 0);
    // Pixel 0 stays in group 0, so every split is tried once.
    for mask in (2u32..(1 << n)).step_by(2) {
        let mut sum1 = [0.0; 3];
        let mut n1 = 0.0;
        for (k, v) in vectors.iter().enumerate() {
            if mask & (1 << k) != 0 {
                sum1 = add(sum1, *v);
                n1 += 1.0;
            }
        }
        let n0 = count_f64(n) - n1;
        let sum0 = sub(total, sum1);
        let score = dot(sum0, sum0) / n0 + dot(sum1, sum1) / n1;
        if score > best.0 {
            best = (score, mask);
        }
    }
    best.1
}

/// Fast clustering: seeds from the two most different pixels, then two rounds of
/// "assign to the nearest mean, recompute the means".
fn cluster_mask(pixels: &[Rgba]) -> u32 {
    let mut seeds = (pixels[0], pixels[0]);
    let mut farthest = 0;
    for (i, &a) in pixels.iter().enumerate() {
        for &b in &pixels[i + 1..] {
            let d = a.distance_squared(b);
            if d > farthest {
                farthest = d;
                seeds = (a, b);
            }
        }
    }
    let (mut c0, mut c1) = seeds;
    let mut mask = 0;
    for _ in 0..2 {
        mask = 0;
        for (k, p) in pixels.iter().enumerate() {
            if p.distance_squared(c1) < p.distance_squared(c0) {
                mask |= 1 << k;
            }
        }
        if let (Some(m0), Some(m1)) = (
            mean(pixels, |k| mask & (1 << k) == 0),
            mean(pixels, |k| mask & (1 << k) != 0),
        ) {
            c0 = m0;
            c1 = m1;
        }
    }
    mask
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn count_f64(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Total squared error of representing the pixels by the split's two colors.
#[cfg(test)]
pub(crate) fn error(pixels: &[Rgba], split: Split) -> u64 {
    pixels
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let color = if split.mask & (1 << k) != 0 {
                split.fg
            } else {
                split.bg
            };
            u64::from(p.distance_squared(color))
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small deterministic generator for test colors.
    fn colors(seed: u64, count: usize) -> Vec<Rgba> {
        let mut state = seed | 1;
        (0..count)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let [r, g, b, ..] = state.to_le_bytes();
                Rgba::rgb(r, g, b)
            })
            .collect()
    }

    #[test]
    fn uniform_cells_have_one_color() {
        let pixels = [Rgba::rgb(9, 8, 7); 6];
        let split = split(&pixels, true);
        assert_eq!(split.mask, 0);
        assert_eq!(split.bg, Rgba::rgb(9, 8, 7));
    }

    #[test]
    fn two_color_cells_are_exact() {
        let (a, b) = (Rgba::rgb(255, 0, 0), Rgba::rgb(0, 0, 255));
        let pixels = [a, b, b, a, a, b, b, a];
        for exact in [true, false] {
            let split = split(&pixels, exact);
            assert_eq!(split.mask, 0b0110_0110);
            assert_eq!((split.fg, split.bg), (b, a));
            assert_eq!(error(&pixels, split), 0);
        }
    }

    #[test]
    fn pixel_zero_is_always_background() {
        for seed in 1..50 {
            let pixels = colors(seed, 8);
            assert_eq!(split(&pixels, true).mask & 1, 0);
            assert_eq!(split(&pixels, false).mask & 1, 0);
        }
    }

    #[test]
    fn exact_search_is_optimal() {
        for seed in 1..40 {
            let pixels = colors(seed, 6);
            let best = split(&pixels, true);
            let best_error = error(&pixels, best);
            // Compare with every possible split.
            for mask in 0..(1u32 << pixels.len()) {
                let candidate = finish(&pixels, mask);
                assert!(best_error <= error(&pixels, candidate) + 6, "seed {seed}");
            }
        }
    }

    #[test]
    fn fast_clustering_is_close_to_exact() {
        let mut exact_total = 0;
        let mut fast_total = 0;
        for seed in 1..200 {
            let pixels = colors(seed, 8);
            exact_total += error(&pixels, split(&pixels, true));
            fast_total += error(&pixels, split(&pixels, false));
        }
        // On random noise (the worst case) the heuristic stays within 15 %.
        assert!(
            fast_total * 100 <= exact_total * 115,
            "{fast_total} vs {exact_total}"
        );
    }
}
