// src/v2/resize.rs - Pillow 11.0.0 bilinear resize of an 8-bit image (PORTING.md §3)
//
// Pillow widens the filter support with the down-scale factor (antialiasing), uses 22-bit fixed-point
// weights, runs the horizontal pass first and stores the intermediate image as 8 bit.

use super::Grid;

const PRECISION_BITS: u32 = 22;

/// Bilinear (triangle) filter, support 1.
fn tri(t: f64) -> f64 {
    let t = t.abs();
    if t < 1.0 {
        1.0 - t
    } else {
        0.0
    }
}

/// Fixed-point coefficients for one axis (`precompute_coeffs` + `normalize_coeffs_8bpc`).
/// Returns, for each output index, the first input index and the integer weights.
fn coeffs(in_size: usize, out_size: usize) -> Vec<(usize, Vec<i32>)> {
    let scale = in_size as f64 / out_size as f64;
    let filterscale = if scale < 1.0 { 1.0 } else { scale };
    let support = 1.0 * filterscale;
    let ss = 1.0 / filterscale;
    (0..out_size)
        .map(|xx| {
            let center = 0.0 + (xx as f64 + 0.5) * scale;
            // C `(int)` truncates toward zero
            let xmin = ((center - support + 0.5) as i64).max(0);
            let xmax = ((center + support + 0.5) as i64).min(in_size as i64);
            let n = (xmax - xmin).max(0) as usize;
            let mut w: Vec<f64> = (0..n)
                .map(|x| tri(((x as i64 + xmin) as f64 - center + 0.5) * ss))
                .collect();
            let mut ww = 0.0;
            for v in &w {
                ww += *v;
            }
            if ww != 0.0 {
                for v in w.iter_mut() {
                    *v /= ww;
                }
            }
            let k = w
                .iter()
                .map(|v| (0.5 + v * (1u32 << PRECISION_BITS) as f64) as i32)
                .collect();
            (xmin as usize, k)
        })
        .collect()
}

#[inline]
fn clip8(acc: i32) -> u8 {
    (acc >> PRECISION_BITS).clamp(0, 255) as u8
}

/// One pass along the columns (horizontal) of an 8-bit grid.
fn pass_horizontal(src: &Grid<u8>, cf: &[(usize, Vec<i32>)]) -> Grid<u8> {
    let mut out = Grid::new(src.rows, cf.len());
    for r in 0..src.rows {
        let row = &src.data[r * src.cols..(r + 1) * src.cols];
        for (xx, (xmin, k)) in cf.iter().enumerate() {
            let mut acc: i32 = 1 << (PRECISION_BITS - 1);
            for (x, kx) in k.iter().enumerate() {
                acc += row[xmin + x] as i32 * kx;
            }
            out.set(r, xx, clip8(acc));
        }
    }
    out
}

/// One pass along the rows (vertical) of an 8-bit grid.
fn pass_vertical(src: &Grid<u8>, cf: &[(usize, Vec<i32>)]) -> Grid<u8> {
    let mut out = Grid::new(cf.len(), src.cols);
    for (yy, (ymin, k)) in cf.iter().enumerate() {
        for c in 0..src.cols {
            let mut acc: i32 = 1 << (PRECISION_BITS - 1);
            for (y, ky) in k.iter().enumerate() {
                acc += src.get(ymin + y, c) as i32 * ky;
            }
            out.set(yy, c, clip8(acc));
        }
    }
    out
}

/// `Image.resize((out_cols, out_rows), BILINEAR)` on a mode "L" image.
pub fn resize_bilinear(src: &Grid<u8>, out_cols: usize, out_rows: usize) -> Grid<u8> {
    if out_cols == src.cols && out_rows == src.rows {
        return src.clone();
    }
    let mut img = src.clone();
    if out_cols != src.cols {
        img = pass_horizontal(&img, &coeffs(src.cols, out_cols));
    }
    if out_rows != src.rows {
        img = pass_vertical(&img, &coeffs(src.rows, out_rows));
    }
    img
}
