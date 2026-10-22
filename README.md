# ShapeComplexity v2.0.0

[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.22113804.svg)](https://doi.org/10.5281/zenodo.22113804)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

ShapeComplexity measures two things about a leaf outline from an image: how much the margin is toothed or
wavy at a small scale (**edge amount, EA**) and how complex the overall blade shape is, for example through
lobes (**macro-shape complexity, MC**). It also reports the blade's elongation and the petiole length. It comes
as a command-line tool, a desktop GUI and a Rust library. All three use the same pipeline function.

> **v2.0.0 replaces the v1 method.** The v1 metrics (edge complexity EC from approximate entropy, MC from
> spectral entropy, the thornfiddle and geodesic paths) are no longer in the code. To reproduce v1 results, use
> v1.0.0: [10.5281/zenodo.22113805](https://doi.org/10.5281/zenodo.22113805).

---

## What is measured

Every leaf is first scaled so that the square root of its area, √A, is 1000 px. All lengths below are fractions
of √A, so the values do not depend on image resolution or leaf size.

1. **Mask.** The largest connected leaf region is kept and scaled to √A = 1000 px.
2. **Petiole.** Thin parts that a disk of radius 0.03 √A cannot enter are found. The longest of them is removed
   as the petiole if it is at least 0.10 √A long and at least 4 times longer than wide (axes from second
   moments). The rest, with holes filled, is L′.
3. **Outline and blade line.** The outline of L′ is traced and resampled at about 1 px steps. The **blade line**
   is the outline smoothed with a Gaussian of σ = 0.03 √A along its length. It follows the lobes but not the teeth.
4. **EA.** The margin profile h(s) is the distance of the outline from the blade line, measured along the blade
   line. EA is the extra length of this profile per unit blade-line length. A smooth margin gives EA ≈ 0; more or
   deeper teeth give a larger EA.
5. **MC.** The blade line is whitened (rescaled so that the blade's spread is the same in every direction, which
   removes plain elongation) and fitted with elliptic Fourier series of k = 1, 2, … harmonics. e(k) is the fitting error.
   MC = Σₖ max(0, ln(e(k) / τ)) with τ = 0.015 √A: it adds up how far each fit stays above the tolerance τ.
   An ellipse gives MC = 0; more lobes give a larger MC.
6. **Elongation** is the major/minor axis ratio of the blade (from the blade-line polygon). **Petiole length**
   is given as a fraction of √A (0 = no petiole found).

**Resolution flags.** From √A of the leaf in the input image (`sqrtA_native`): below 250 px both values are
`not_resolved` and not computed; from 250 to below 500 px EA is `below_recommended` (MC `ok`); from 500 px both are
`ok`. Teeth need more pixels than lobes.

---

## Input

By default, one leaf per image: if there are several separate regions, the largest one is measured. With
`--all-leaves` (GUI: *All leaves*), every region with √A ≥ 250 px is measured as a leaf, one row each, numbered in
raster order; if no region is that large, the largest one is measured. You must say which pixels are leaf; there
is no default:

| `--mask-source` | Leaf pixels | Typical input |
|---|---|---|
| `alpha` | alpha > 127 (opaque) | cut-outs with a transparent background (PNG). Images without an alpha channel are refused. |
| `dark` | grey value < 128; transparent pixels never count | binary masks with a dark leaf on a light background (PNG, JPEG, TIFF, BMP) |

`dark` is meant for black-and-white masks. On a photograph it measures whatever falls below the threshold, so
make a mask first.

**Mask check.** Two warnings help to spot bad input; they change no value. (1) More than 1 % of the leaf pixels lie
outside the largest region, so they were not measured. (2) `dark` input only: more than half of the pixels that are
not white (grey value ≤ 223) are grey (32–223), so the image is probably a photograph, not a binary mask. The
command line prints them as `warning:` lines on standard error; the GUI lists them with the results.

---

## Command line

```bash
# all images in a folder (add -r for subfolders), results to a CSV file
shapecomplexity_cli measure leaves/ --mask-source alpha -o results.csv

# single files, binary masks
shapecomplexity_cli measure leaf1.png leaf2.jpg --mask-source dark > results.csv
```

Options: `-o/--output` (default: standard output), `-r/--recursive`, `-j/--threads` (default: all cores),
`--all-leaves` (see *Input*), `--px-per-mm <scale>` (adds the leaf area in mm²).
Files that fail are reported on standard error and left out of the table; the exit code is then 1.

### Output columns

| Column | Meaning |
|---|---|
| `file` | path of the image (relative to the input folder) |
| `EA` | edge amount (`NA` if not resolved) |
| `MC` | macro-shape complexity (`NA` if not resolved) |
| `elongation` | major/minor axis ratio of the blade |
| `petiole_len` | petiole length / √A (0 = none found) |
| `sqrtA_native` | √A of the leaf in the input image, px |
| `EA_flag`, `MC_flag` | `ok`, `below_recommended` or `not_resolved` |
| `version` | ShapeComplexity version |

Optional columns: with `--all-leaves`, `leaf` (1, 2, …), `leaf_x` and `leaf_y` (centre of the region in input
pixels, 0 = left/top edge of the image) follow `file`; with `--px-per-mm`, `area_mm2` (pixels of the measured
region, petiole included, / scale²) follows `sqrtA_native`.

`shapecomplexity_cli port-check <manifest.csv> <out.csv> <mask_dir>` runs the check against the Python reference
implementation (see *Testing*).

---

## GUI

1. Start `leaf_complex_gui`.
2. **File → Open Workspace** and choose a folder (subfolders are included).
3. Choose the **mask source** in the toolbar.
4. **▶ Analyze** (current image) or **⏩ Analyze All / Selected**.
5. **File → Export Results** writes one CSV with the same columns as the command line.

*All leaves* and *px/mm* in the toolbar work as `--all-leaves` and `--px-per-mm`. With several leaves, pick one
with the *Leaf* buttons; the input view shows the leaf numbers.

The panels follow Fig. 1 of the paper:

- **Image view.** *Normalised mask*: the mask as measured (√A = 1000 px; grey: blade L′, black: removed petiole),
  with the outline (black) and the blade line (blue). *Margin band* colours the leaf outside the blade line orange
  and the background inside it peach. *Input image*: the input with the normalised mask placed over it, to check
  which pixels were measured. Scroll to zoom, drag to pan.
- **Results**: the values, flags, dropped area, the share of grey pixels (`dark` input) and any warnings. Leaves
  with a flag other than `ok` or a warning are marked ⚠ in the image strip.
- **(c) h(s)**: the margin profile along the blade line (orange outside, peach inside); EA. The part of the outline
  on screen in the image view is shaded grey. Click to see that point on the leaf.
- **(d)** the whitened blade line and its elliptic Fourier fit; a slider sets the number of harmonics k.
- **(e)** ln(e(k)/τ); the shaded area is MC; the chosen k is marked.
- **Morphospace** (tab above the right panel): EA against MC for every analysed leaf. Up to three subfolders get
  their own colour; with more, pick one to highlight. Hollow points: a flag other than `ok` or a warning. Click a
  point to open that leaf.

---

## Building from source

```bash
# Install Rust (https://rustup.rs/)
git clone https://github.com/Thornbach/ShapeComplexity.git
cd ShapeComplexity

# Command line
cargo build --release            # target/release/shapecomplexity_cli

# GUI
cd leaf_complex_gui
cargo build --release            # leaf_complex_gui/target/release/leaf_complex_gui
```

The library is the crate `shapecomplexity`: `v2::measure(&mask)` returns the values; `v2::measure_traced(&mask)`
also returns the intermediate stages (normalised mask, L′, outline, blade line, h(s), e(k), whitened blade line,
and where the normalised mask lies in the input). `v2::metrics::efd_reconstruct(&white, k)` gives the k-harmonic
fit; `batch::measure_file` adds the mask check.

---

## Testing

`cargo test` runs two sets of tests:

- `tests/synthetic.rs`: behaviour on synthetic shapes (circle EA ≈ 0 and MC = 0; ellipses MC = 0 with the right
  elongation; teeth raise EA; lobes raise MC; resolution flags) and the input paths.
- `tests/v2_steps.rs`: every step against data files of the Python reference implementation used for the paper,
  bit for bit, and the whole pipeline on 221 masks (201 synthetic shapes, 20 own oak leaves). The data files are
  in `tests/fixtures/` of the repository (23 MB) and not in the published crate; the tests skip without them.

Each step copies the exact library algorithm of the reference (Pillow 11.0.0, SciPy 1.13.1, scikit-image 0.24.0,
NumPy 1.26.4). The full check on 3,881 masks requires every value to agree with the reference to within 10⁻⁹.

---

## Citation

Please cite the software (concept DOI [10.5281/zenodo.22113804](https://doi.org/10.5281/zenodo.22113804),
or the version DOI of the release you used) and the accompanying paper. See [CITATION.cff](CITATION.cff).

## License

MIT, see [LICENSE](LICENSE).
