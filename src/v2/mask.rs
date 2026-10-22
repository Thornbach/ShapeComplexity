// src/v2/mask.rs - connected components, largest component, hole filling (PORTING.md §2, §4)
//
// Labels are numbered in raster order of each component's first pixel, as `scipy.ndimage.label`.
// Ties for "largest" go to the lowest label (NumPy `argmax`).

use std::collections::VecDeque;

use super::{Grid, Mask};

const N4: [(i64, i64); 4] = [(-1, 0), (0, -1), (0, 1), (1, 0)];
const N8: [(i64, i64); 8] = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)];

/// Connected components of the true pixels (4- or 8-connected). Returns labels (0 = background) and their number.
pub fn label(m: &Mask, eight: bool) -> (Grid<u32>, u32) {
    let nb: &[(i64, i64)] = if eight { &N8 } else { &N4 };
    let mut lab = Grid::new(m.rows, m.cols);
    let mut n = 0;
    let mut queue = VecDeque::new();
    for start in 0..m.data.len() {
        if !m.data[start] || lab.data[start] != 0 {
            continue;
        }
        n += 1;
        lab.data[start] = n;
        queue.push_back(start);
        while let Some(i) = queue.pop_front() {
            let (r, c) = ((i / m.cols) as i64, (i % m.cols) as i64);
            for (dr, dc) in nb {
                let (rr, cc) = (r + dr, c + dc);
                if rr < 0 || cc < 0 || rr >= m.rows as i64 || cc >= m.cols as i64 {
                    continue;
                }
                let j = rr as usize * m.cols + cc as usize;
                if m.data[j] && lab.data[j] == 0 {
                    lab.data[j] = n;
                    queue.push_back(j);
                }
            }
        }
    }
    (lab, n)
}

/// Pixel count per label (index 0 = background).
pub fn counts(lab: &Grid<u32>, n: u32) -> Vec<usize> {
    let mut k = vec![0usize; n as usize + 1];
    for v in &lab.data {
        k[*v as usize] += 1;
    }
    k
}

/// Label with the most pixels (first of equal ones).
fn largest_label(lab: &Grid<u32>, n: u32) -> u32 {
    let k = counts(lab, n);
    let mut best = 1;
    for l in 2..=n as usize {
        if k[l] > k[best] {
            best = l;
        }
    }
    best as u32
}

fn select(lab: &Grid<u32>, l: u32) -> Mask {
    Grid::from_vec(lab.rows, lab.cols, lab.data.iter().map(|v| *v == l).collect())
}

/// `largest_component`: the largest 4-connected component if there are two or more, else the mask itself.
pub fn largest_component(m: &Mask) -> Mask {
    let (lab, n) = label(m, false);
    if n > 1 {
        select(&lab, largest_label(&lab, n))
    } else {
        m.clone()
    }
}

/// `binary_fill_holes`: background not 4-connected to the outside of the image becomes foreground.
pub fn fill_holes(m: &Mask) -> Mask {
    let (rows, cols) = (m.rows, m.cols);
    let mut outside = vec![false; rows * cols];
    let mut queue = VecDeque::new();
    for r in 0..rows {
        for c in 0..cols {
            let i = r * cols + c;
            if (r == 0 || c == 0 || r + 1 == rows || c + 1 == cols) && !m.data[i] && !outside[i] {
                outside[i] = true;
                queue.push_back(i);
            }
        }
    }
    while let Some(i) = queue.pop_front() {
        let (r, c) = ((i / cols) as i64, (i % cols) as i64);
        for (dr, dc) in N4 {
            let (rr, cc) = (r + dr, c + dc);
            if rr < 0 || cc < 0 || rr >= rows as i64 || cc >= cols as i64 {
                continue;
            }
            let j = rr as usize * cols + cc as usize;
            if !m.data[j] && !outside[j] {
                outside[j] = true;
                queue.push_back(j);
            }
        }
    }
    Grid::from_vec(rows, cols, outside.iter().map(|o| !o).collect())
}

/// `largest_filled`: largest 4-connected component with holes filled; an empty mask stays empty.
pub fn largest_filled(m: &Mask) -> Mask {
    let (lab, n) = label(m, false);
    if n == 0 {
        return m.clone();
    }
    fill_holes(&select(&lab, largest_label(&lab, n)))
}

/// Bounding box (row0, row1, col0, col1), inclusive, of the true pixels; None if there are none.
pub fn bbox(m: &Mask) -> Option<(usize, usize, usize, usize)> {
    let mut b: Option<(usize, usize, usize, usize)> = None;
    for r in 0..m.rows {
        for c in 0..m.cols {
            if m.get(r, c) {
                b = Some(match b {
                    None => (r, r, c, c),
                    Some((r0, r1, c0, c1)) => (r0.min(r), r1.max(r), c0.min(c), c1.max(c)),
                });
            }
        }
    }
    b
}

/// Copy of rows r0..=r1 and columns c0..=c1.
pub fn crop<T: Copy + Default>(g: &Grid<T>, r0: usize, r1: usize, c0: usize, c1: usize) -> Grid<T> {
    let mut out = Grid::new(r1 - r0 + 1, c1 - c0 + 1);
    for r in r0..=r1 {
        for c in c0..=c1 {
            out.set(r - r0, c - c0, g.get(r, c));
        }
    }
    out
}

/// Pad with `p` background pixels on every side.
pub fn pad(m: &Mask, p: usize) -> Mask {
    let mut out = Mask::new(m.rows + 2 * p, m.cols + 2 * p);
    for r in 0..m.rows {
        for c in 0..m.cols {
            out.set(r + p, c + p, m.get(r, c));
        }
    }
    out
}
