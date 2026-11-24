//! Splitting areas: [`Layout`] and [`Constraint`].

use vnce_core::Rect;

/// The direction in which a [`Layout`] splits an area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    /// Side by side, left to right.
    Horizontal,
    /// Stacked, top to bottom.
    #[default]
    Vertical,
}

/// How much space one part of a [`Layout`] gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Constraint {
    /// Exactly this many cells (less only when the area is too small).
    Length(u16),
    /// This percentage of the area (`0..=100`).
    Percentage(u16),
    /// `numerator / denominator` of the area.
    Ratio(u32, u32),
    /// At least this many cells; takes a share of the free space like `Fill(1)`.
    Min(u16),
    /// At most this many cells; takes a share of the free space like `Fill(1)`.
    Max(u16),
    /// A share of the free space, proportional to the weight. `Fill(0)` stays empty.
    Fill(u16),
}

/// Splits an area into parts along one direction.
///
/// Sizes are computed in this order:
///
/// 1. `Length`, `Percentage`, `Ratio` and `Min` get their size.
/// 2. If that is more than the area, they shrink proportionally (`Min` last).
/// 3. The free space goes to `Fill`, `Min` and `Max` parts by weight (`Max` never grows
///    past its limit). Without such parts, rounding leftovers of `Percentage` and `Ratio`
///    go to the parts with the largest fractions, and the rest stays empty at the end.
///
/// The result never leaves the area, and the same input always gives the same result.
///
/// ```
/// use vnce_core::Rect;
/// use vnce_ui::{Constraint, Layout};
///
/// let [header, body, footer] = Layout::vertical([
///     Constraint::Length(1),
///     Constraint::Fill(1),
///     Constraint::Length(1),
/// ])
/// .areas(Rect::new(0, 0, 80, 24));
/// assert_eq!(header, Rect::new(0, 0, 80, 1));
/// assert_eq!(body, Rect::new(0, 1, 80, 22));
/// assert_eq!(footer, Rect::new(0, 23, 80, 1));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Layout {
    direction: Direction,
    constraints: Vec<Constraint>,
    spacing: u16,
    margin: (u16, u16),
}

impl Layout {
    /// A layout with a direction and constraints.
    pub fn new(direction: Direction, constraints: impl IntoIterator<Item = Constraint>) -> Self {
        Self {
            direction,
            constraints: constraints.into_iter().collect(),
            spacing: 0,
            margin: (0, 0),
        }
    }

    /// Parts side by side, left to right.
    pub fn horizontal(constraints: impl IntoIterator<Item = Constraint>) -> Self {
        Self::new(Direction::Horizontal, constraints)
    }

    /// Parts stacked, top to bottom.
    pub fn vertical(constraints: impl IntoIterator<Item = Constraint>) -> Self {
        Self::new(Direction::Vertical, constraints)
    }

    /// Empty cells between two parts.
    #[must_use]
    pub fn spacing(mut self, spacing: u16) -> Self {
        self.spacing = spacing;
        self
    }

    /// Empty columns on the left and right and rows on the top and bottom of the area,
    /// removed before splitting.
    #[must_use]
    pub fn margin(mut self, horizontal: u16, vertical: u16) -> Self {
        self.margin = (horizontal, vertical);
        self
    }

    /// Splits `area`: one rectangle per constraint, in order.
    #[must_use]
    pub fn split(&self, area: Rect) -> Vec<Rect> {
        let area = area.inner(self.margin.0, self.margin.1);
        let (start, length, cross_start, cross_length) = match self.direction {
            Direction::Horizontal => (area.x, area.width, area.y, area.height),
            Direction::Vertical => (area.y, area.height, area.x, area.width),
        };
        let gaps = u16::try_from(self.constraints.len().saturating_sub(1)).unwrap_or(u16::MAX);
        let available = length.saturating_sub(gaps.saturating_mul(self.spacing));
        let sizes = solve(&self.constraints, available);
        let mut position = start;
        let end = start.saturating_add(length);
        sizes
            .into_iter()
            .map(|size| {
                let begin = position.min(end);
                let size = size.min(end - begin);
                position = begin.saturating_add(size).saturating_add(self.spacing);
                match self.direction {
                    Direction::Horizontal => Rect::new(begin, cross_start, size, cross_length),
                    Direction::Vertical => Rect::new(cross_start, begin, cross_length, size),
                }
            })
            .collect()
    }

    /// Like [`Layout::split`], as an array for destructuring.
    ///
    /// # Panics
    ///
    /// Panics if the layout does not have exactly `N` constraints.
    #[must_use]
    pub fn areas<const N: usize>(&self, area: Rect) -> [Rect; N] {
        self.split(area)
            .try_into()
            .unwrap_or_else(|parts: Vec<Rect>| {
                panic!("the layout has {} constraints, not {N}", parts.len())
            })
    }
}

/// Sizes for `constraints` that add up to at most `available`.
fn solve(constraints: &[Constraint], available: u16) -> Vec<u16> {
    let total = u64::from(available);
    // Exact sizes (as fractions of `scale`) of the fixed parts, for rounding.
    let exact: Vec<Option<(u64, u64)>> = constraints
        .iter()
        .map(|constraint| match *constraint {
            Constraint::Percentage(percent) => Some((total * u64::from(percent.min(100)), 100)),
            Constraint::Ratio(numerator, denominator) if denominator > 0 => Some((
                total * u64::from(numerator.min(denominator)),
                u64::from(denominator),
            )),
            Constraint::Ratio(..) => Some((0, 1)),
            _ => None,
        })
        .collect();
    let mut sizes: Vec<u64> = constraints
        .iter()
        .zip(&exact)
        .map(|(constraint, exact)| match (*constraint, exact) {
            (Constraint::Length(length) | Constraint::Min(length), _) => u64::from(length),
            (_, Some((value, scale))) => value / scale,
            _ => 0,
        })
        .collect();

    let used: u64 = sizes.iter().sum();
    if used > total {
        shrink(constraints, &mut sizes, used - total);
        return to_u16(&sizes);
    }

    let mut free = total - used;
    let weights: Vec<u64> = constraints
        .iter()
        .map(|constraint| match *constraint {
            Constraint::Fill(weight) => u64::from(weight),
            Constraint::Min(_) | Constraint::Max(_) => 1,
            _ => 0,
        })
        .collect();
    if weights.iter().any(|&w| w > 0) {
        grow(constraints, &weights, &mut sizes, &mut free);
    } else {
        // Give the rounding leftovers to the largest fractions.
        let mut fractions: Vec<(usize, u64, u64)> = exact
            .iter()
            .enumerate()
            .filter_map(|(index, exact)| exact.map(|(value, scale)| (index, value % scale, scale)))
            .filter(|&(_, remainder, _)| remainder > 0)
            .collect();
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "sizes are at most u16::MAX, far inside f64 precision"
        )]
        let exact_sum: u64 = fractions
            .iter()
            .map(|&(_, remainder, scale)| remainder as f64 / scale as f64)
            .sum::<f64>()
            .round() as u64;
        // Largest fraction first; earlier parts win ties.
        fractions.sort_by(|a, b| (b.1 * a.2).cmp(&(a.1 * b.2)).then(a.0.cmp(&b.0)));
        for &(index, _, _) in fractions
            .iter()
            .take(usize::try_from(exact_sum.min(free)).unwrap_or(usize::MAX))
        {
            sizes[index] += 1;
        }
    }
    to_u16(&sizes)
}

/// Removes `deficit` cells: first proportionally from everything but `Min`, then from
/// `Min`.
fn shrink(constraints: &[Constraint], sizes: &mut [u64], mut deficit: u64) {
    for min_pass in [false, true] {
        let selected: Vec<usize> = (0..sizes.len())
            .filter(|&i| matches!(constraints[i], Constraint::Min(_)) == min_pass)
            .filter(|&i| sizes[i] > 0)
            .collect();
        let pool: u64 = selected.iter().map(|&i| sizes[i]).sum();
        if pool == 0 || deficit == 0 {
            continue;
        }
        let take = deficit.min(pool);
        let mut removed = 0;
        for &i in &selected {
            let share = sizes[i] * take / pool;
            sizes[i] -= share;
            removed += share;
        }
        // Rounding: take the rest from the last parts first.
        for &i in selected.iter().rev() {
            if removed == take {
                break;
            }
            if sizes[i] > 0 {
                sizes[i] -= 1;
                removed += 1;
            }
        }
        deficit -= removed;
    }
}

/// Shares `free` cells by `weights`, never growing `Max` parts past their limit.
fn grow(constraints: &[Constraint], weights: &[u64], sizes: &mut [u64], free: &mut u64) {
    let limit = |i: usize| match constraints[i] {
        Constraint::Max(max) => u64::from(max),
        _ => u64::MAX,
    };
    loop {
        let open: Vec<usize> = (0..sizes.len())
            .filter(|&i| weights[i] > 0 && sizes[i] < limit(i))
            .collect();
        let weight: u64 = open.iter().map(|&i| weights[i]).sum();
        if *free == 0 || weight == 0 {
            return;
        }
        let mut given = 0;
        let mut remainders = Vec::new();
        for &i in &open {
            let exact = *free * weights[i];
            let share = (exact / weight).min(limit(i) - sizes[i]);
            sizes[i] += share;
            given += share;
            remainders.push((i, exact % weight));
        }
        let mut rest = *free - given;
        if given == 0 {
            // Every share rounded down to zero: one cell each, largest remainder first.
            remainders.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            for &(i, _) in &remainders {
                if rest == 0 {
                    break;
                }
                if sizes[i] < limit(i) {
                    sizes[i] += 1;
                    rest -= 1;
                }
            }
            if rest == *free {
                return;
            }
        }
        *free = rest;
    }
}

fn to_u16(sizes: &[u64]) -> Vec<u16> {
    sizes
        .iter()
        .map(|&size| u16::try_from(size).unwrap_or(u16::MAX))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use Constraint::{Fill, Length, Max, Min, Percentage, Ratio};

    fn widths(constraints: &[Constraint], width: u16) -> Vec<u16> {
        Layout::horizontal(constraints.iter().copied())
            .split(Rect::new(0, 0, width, 1))
            .iter()
            .map(|r| r.width)
            .collect()
    }

    #[test]
    fn lengths_and_fill() {
        assert_eq!(widths(&[Length(3), Fill(1), Length(2)], 10), [3, 5, 2]);
        assert_eq!(widths(&[Fill(1), Fill(2)], 9), [3, 6]);
        assert_eq!(widths(&[Fill(1), Fill(1), Fill(1)], 10), [4, 3, 3]);
        assert_eq!(widths(&[Fill(0), Fill(1)], 10), [0, 10]);
    }

    #[test]
    fn percentages_and_ratios_round_to_the_largest_fraction() {
        assert_eq!(widths(&[Percentage(50), Percentage(50)], 7), [4, 3]);
        assert_eq!(
            widths(&[Ratio(1, 3), Ratio(1, 3), Ratio(1, 3)], 10),
            [4, 3, 3]
        );
        // Leftovers that are not rounding stay empty.
        assert_eq!(widths(&[Percentage(25), Percentage(25)], 8), [2, 2]);
        assert_eq!(widths(&[Ratio(1, 0)], 8), [0]);
    }

    #[test]
    fn min_and_max_share_the_free_space() {
        assert_eq!(widths(&[Min(2), Fill(1)], 10), [6, 4]);
        assert_eq!(widths(&[Max(3), Fill(1)], 10), [3, 7]);
        assert_eq!(widths(&[Max(3), Max(4)], 20), [3, 4]);
    }

    #[test]
    fn too_small_areas_shrink_min_last() {
        assert_eq!(widths(&[Length(6), Length(6)], 6), [3, 3]);
        assert_eq!(widths(&[Min(4), Length(6)], 6), [4, 2]);
        assert_eq!(widths(&[Min(4), Min(4)], 6), [3, 3]);
        assert_eq!(widths(&[Length(5), Fill(1)], 0), [0, 0]);
    }

    #[test]
    fn parts_never_leave_the_area() {
        let area = Rect::new(3, 2, 17, 5);
        let constraint_sets: [&[Constraint]; 4] = [
            &[Length(10), Length(10), Length(10)],
            &[Percentage(100), Percentage(100)],
            &[Min(20), Fill(3), Max(2)],
            &[Ratio(2, 3), Fill(1), Length(1)],
        ];
        for constraints in constraint_sets {
            for spacing in [0, 1, 5, 40] {
                for direction in [Direction::Horizontal, Direction::Vertical] {
                    let parts = Layout::new(direction, constraints.iter().copied())
                        .spacing(spacing)
                        .split(area);
                    assert_eq!(parts.len(), constraints.len());
                    let mut previous_end = 0;
                    for part in parts {
                        assert!(
                            part.left() >= area.left()
                                && part.right() <= area.right()
                                && part.top() >= area.top()
                                && part.bottom() <= area.bottom(),
                            "{constraints:?} {part:?}"
                        );
                        let start = match direction {
                            Direction::Horizontal => part.left(),
                            Direction::Vertical => part.top(),
                        };
                        assert!(start >= previous_end, "{constraints:?}");
                        previous_end = match direction {
                            Direction::Horizontal => part.right(),
                            Direction::Vertical => part.bottom(),
                        };
                    }
                }
            }
        }
    }

    #[test]
    fn spacing_and_margin() {
        let [a, b] = Layout::horizontal([Fill(1), Fill(1)])
            .spacing(2)
            .margin(1, 1)
            .areas(Rect::new(0, 0, 12, 5));
        assert_eq!(a, Rect::new(1, 1, 4, 3));
        assert_eq!(b, Rect::new(7, 1, 4, 3));
    }

    #[test]
    fn vertical_layout() {
        let [top, rest] = Layout::vertical([Length(2), Fill(1)]).areas(Rect::new(5, 5, 4, 6));
        assert_eq!(top, Rect::new(5, 5, 4, 2));
        assert_eq!(rest, Rect::new(5, 7, 4, 4));
    }

    #[test]
    #[should_panic(expected = "the layout has 2 constraints, not 3")]
    fn areas_checks_the_count() {
        let _: [Rect; 3] = Layout::vertical([Length(1), Length(1)]).areas(Rect::new(0, 0, 1, 1));
    }
}
