# Changelog

## Unreleased (v2.0.0, in work on branch `revision-v2`)

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
  - `metrics.rs`: EA, whitening and elongation, elliptic Fourier error curve, MC.
  - `mod.rs`: `measure`, `measure_traced`, `measure_normalised_traced`, resolution flags.
- `src/bin/port_check.rs`: runs the pipeline on a manifest for the check against the Python reference
  (temporary; to become a CLI subcommand).
- `tests/v2_steps.rs` and `tests/fixtures/`: every step against fixtures of the reference (bit for bit), and the
  whole pipeline on 221 masks (201 synthetic shapes, 20 own oak leaves).

### Planned before release
- CLI and GUI on the one `measure` function; input option `--mask-source alpha|dark` (no silent default).
- Remove the v1 algorithm (edge complexity EC, thornfiddle path, pink/golden/diego paths, entropy functions,
  `point_analysis`, `feature_extraction`, v1 config keys). v1.0.0 stays citable on Zenodo.
- Output columns: file, EA, MC, elongation, petiole_len, sqrtA_native, EA_flag, MC_flag, version.

## 1.0.0 (26 Aug 2026)
- First citable release (Zenodo 10.5281/zenodo.22113805).
