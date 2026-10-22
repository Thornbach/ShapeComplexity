# Fixtures for the ShapeComplexity v2 ports

Made by `analysis/v2/export_fixtures.py` from `analysis/v2/shapecomplexity_v2.py` (Pillow 11.0.0, SciPy 1.13.1,
scikit-image 0.24.0, NumPy 1.26.4). Rules for each step: `analysis/v2/PORTING.md`. Numbers in CSV are `%.17g`.

- `public/leaves/values.csv`: one row per mask (key, mode, EA, MC, elongation, petiole_len, flags, intermediate numbers).
  `EA` is the v2 edge amount (the Python module calls it `EC`, D84). Empty cells are NaN.
- `public/leaves/<key>/`: `input.png` (normalised mask if mode = normalised; raw alpha > 127 mask if mode = raw),
  `resized.png` and `norm.png` (raw only), `lp.png`, `polygon.png`; stage CSVs where `stages` is True.
- `public/unit/resize`: `<id>_in.png` → Pillow bilinear → `<id>_out.png` at (out_w, out_h).
- `public/unit/polygon`: `<id>_xy.csv` (x = col, y = row, float) → Pillow polygon fill on a mode "1" image (width, height) → `<id>_mask.png`.
- `public/unit/contour`: `<id>_in.csv` (array) → `find_contours(a, 0.5)` → `<id>_out.csv` (all contours in order; contour index, r, c).
- `public/unit/gauss`: `<id>_2d_in.csv` → `gaussian_filter(σ = 1.5)` → `<id>_2d_out.csv`; `<id>_1d.csv` x → `gaussian_filter1d(σ = sigma_1d, mode wrap)` → y.

Public = own data only (synthetic shapes; oak cut-outs of CS1). It may be copied to `tests/fixtures/` of the code repository.
