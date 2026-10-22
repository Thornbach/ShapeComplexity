# Changelog

## 2.0.0 (unreleased; in work on branch `revision-v2`)

v2.0.0 replaces the v1 method. The numbers of v1.0.0 cannot be reproduced with this version; use v1.0.0
(Zenodo 10.5281/zenodo.22113805) for that.

### Added
- `src/v2/`: the v2 method, edge amount (EA) and macro-shape complexity (MC), plus blade elongation and petiole length.
  A port of the Python reference implementation used for the paper. Each step copies the exact library algorithm
  of the reference (Pillow 11.0.0 bilinear resize and polygon fill, SciPy 1.13.1 Gaussian filter and exact
  distance transform, scikit-image 0.24.0 marching squares, NumPy 1.26.4 interpolation and summation order).
  - `mask.rs`: connected components (raster-order labels), largest component, hole filling.
  - `resize.rs`: Pillow bilinear resize (antialiased support, 22-bit fixed-point weights, 8-bit intermediate).
  - `morph.rs`: exact Euclidean distance transform, opening by a disk, petiole detection (D29).
  - `contour.rs`: Gaussian smoothing, 0.5 contour, arc-length resampling.
  - `fill.rs`: Pillow polygon fill (single-precision scan lines).
  - `metrics.rs`: margin profile h(s), EA, whitening and elongation, elliptic Fourier error curve e(k), MC.
  - `mod.rs`: `measure`, `measure_traced`, `measure_normalised_traced`, resolution flags, input rules.
  - The trace of `measure_traced` also holds h(s) and e(k), for display. Computing them once for both the value
    and the trace does not change any value (all fixture tests still bit-identical).
- Input rule `MaskSource`: `alpha` (alpha > 127; images without alpha are refused) or `dark` (grey value < 128,
  transparent pixels never leaf; for binary masks with a dark leaf on a light background). There is no default:
  the CLI option `--mask-source` is required, and the GUI asks for it before it analyses.
- `src/batch.rs`, shared by the CLI and the GUI: image listing (png, jpg, jpeg, tif, tiff, bmp), `measure_file`,
  the results table, the port check.
- CLI subcommands:
  - `measure <files or folders> --mask-source alpha|dark [-o out.csv] [-r] [-j N]`: one row per image with columns
    file, EA, MC, elongation, petiole_len, sqrtA_native, EA_flag, MC_flag, version (missing values `NA`).
  - `port-check <manifest> <out> <mask_dir>`: the check against the Python reference (replaces the temporary
    `port_check` binary).
- GUI on the same pipeline (`gui_api::analyze` → `v2::measure_traced`): normalised mask with removed petiole,
  outline and blade line; values and flags; plots of h(s) (click to show the point on the leaf) and ln(e(k)/τ);
  export of one CSV with the CLI columns.
- GUI panels after Fig. 1 of the paper and a mask check (D114 A, B; no value changes; fixture tests still bit
  identical):
  - Library: the trace also holds the whitened blade line (`white`) and a `Placement` (crop box, scaled size,
    scale factor, leaf pixels kept and dropped) from `v2::normalise_placed`; `Placement::to_input` maps the
    normalised mask onto the input image. `v2::metrics::efd_reconstruct(w, k)`: the k-harmonic elliptic Fourier
    fit with the coefficients and summation order of `error_curve`. `v2::mask_from_image`, `alpha_mask`,
    `dark_mask` work on a decoded image, so a file is read once.
  - Mask check (`batch::MaskCheck`, D115): warns when > 1 % of the leaf pixels lie outside the largest region, and,
    for `dark` input, when > 50 % of the non-white pixels (grey value ≤ 223) are grey (32–223).
    `batch::measure_file` returns it with the values; the CLI prints the warnings on standard error.
  - GUI: input image with the normalised mask placed over it; margin band inside/outside the blade line; h(s) filled
    by sign, with the stretch on screen shaded; whitened blade line with a k slider and the live Fourier fit;
    ln(e(k)/τ) with the MC area shaded and k marked; dropped area, grey share and warnings in the results; ⚠ in the
    image strip for leaves with a flag other than `ok` or a warning. Mask colours as in Fig. 1 (drawn on white).
  - `leaf_complex_gui/examples/screenshot.rs`: renders the GUI on one leaf and saves a screenshot (development check).
- Several leaves per image and leaf area (D114 E, D116): `--all-leaves` measures every 4-connected region with
  √A ≥ 250 px as a leaf (raster order; the largest region if none reaches 250 px), each exactly as the default run
  measures the largest region; columns `leaf`, `leaf_x`, `leaf_y` after `file`. `--px-per-mm` adds `area_mm2`
  (pixels of the measured region / scale²) after `sqrtA_native`. Without the options the columns are unchanged.
  Library: `v2::leaf_regions`, `batch::measure_file_leaves`, `batch::Leaf`, `batch::Columns`,
  `gui_api::analyze_leaves`. GUI: *All leaves* checkbox, *px/mm* field, leaf buttons, leaf numbers in the input view,
  leaf area in the results; the export uses the same columns.
- GUI morphospace (D114 C): EA–MC scatter of all analysed leaves in a *Morphospace* tab. Colour by subfolder (up to
  three, fixed by name order; with more, one highlighted subfolder, the rest grey); hollow points for leaves with a
  flag other than `ok` or a mask warning; the leaf on screen ringed; hover shows the file, a click opens the leaf.
- `tests/synthetic.rs`: synthetic shapes (circle EA ≈ 0, MC = 0, elongation 1, no petiole; ellipses MC = 0 and
  elongation = aspect ratio within 2 %; teeth raise EA; lobes raise MC; flags at √A = 200, 300, 600 px), the `alpha`
  and `dark` inputs give identical values, the results row; `efd_reconstruct` gives e(1) of the error curve (relative
  difference < 10⁻⁹); the placement maps the outline back onto the input (every point < 1 px from a leaf edge) and
  counts dropped pixels exactly; the mask-check thresholds; several leaves (two leaves kept and a small one
  dropped, the largest gives the default values bit for bit, centroid and crop in the image frame; fallback to the
  largest small region) and the optional columns.
- `tests/v2_steps.rs` and `tests/fixtures/`: every step against fixtures of the reference (bit for bit), and the
  whole pipeline on 221 masks (201 synthetic shapes, 20 own oak leaves). The fixtures are excluded from the
  published crate; the tests skip without them.

### Removed (D112)
- The v1 algorithm: edge complexity EC (approximate entropy of the pink-path signal), MC from spectral entropy of
  the thornfiddle path, harmonic enhancement, the pink (edge mask), golden (lobe scaffold) and diego (geodesic)
  paths, adaptive opening, reference points (COM/EP), `point_analysis`, `feature_extraction`, shape indices,
  circularity, outline counts and the EC/MC overlay images.
- Modules `config`, `errors`, `feature_extraction`, `image_io`, `image_utils`, `morphology`, `output`,
  `path_algorithms`, `pipeline`, `point_analysis`, `shape_analysis`, `thornfiddle`; binary `port_check`.
- `config.toml` and every v1 configuration key (`resize_dimensions`, `opening_kernel_size`, `thornfiddle_*`,
  `harmonic_*`, `approximate_entropy_*`, `spectral_entropy_*`, `ec_scaling_factor`, petiole and pink filters, …).
  v2 has no tunable parameters; its constants are fixed in `src/v2/mod.rs`. The CLI takes its few options as flags.
- GUI: configuration editor, EC/MC overlays and path display, per-image EC/MC curve CSVs.
- Old CLI flags `--input`, `--output` (folder), `--config`, `--reference-point`, `--debug`.
- Dependencies `imageproc`, `nalgebra`, `bresenham`, `rustfft`, `serde`, `serde_json`, `toml`, `csv`, `thiserror`,
  `anyhow`; GUI: `tokio`, `env_logger`, `log`. The `image` crate is used only to decode image files.
- Library crate type `cdylib` (no bindings use it; the R package is pure R).

### Changed
- Version 2.0.0 (library, CLI and GUI).
- README, CITATION.cff, .zenodo.json: describe EA and MC of v2; the paper is cited by its new title (in preparation).

## 1.0.0 (26 Aug 2026)
- First citable release (Zenodo 10.5281/zenodo.22113805).
