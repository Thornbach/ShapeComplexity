# shapecomplexity (R package)

Edge amount (EA) and macro-shape complexity (MC) of leaf outlines, plus blade elongation and petiole length,
in pure R. It implements the same method as the ShapeComplexity program (Rust) in the parent folder.
Both are ports of the Python reference implementation used for the paper. Each step copies the exact
library algorithm of the reference, so all three give the same numbers (|difference| ≤ 10⁻⁹).

## Install

From a clone of the repository:

```sh
R CMD INSTALL R
```

Dependencies: `png` (reading images) and `nabor` (exact nearest neighbours). Tests use `testthat`.

## Use

```r
library(shapecomplexity)

# one row per image: file, EA, MC, elongation, petiole_len, sqrtA_native, EA_flag, MC_flag, mask_source, version
measure_files(c("leaf1.png", "leaf2.png"))

# or step by step
m <- read_mask("leaf1.png")      # logical matrix, TRUE = leaf
r <- measure(m, trace = TRUE)    # values and intermediate stages (outline, blade line, h(s), e(k), ...)
```

**Which pixels are leaf** (`mask_source`, the same rules as the program):
- `"auto"` (default): `"alpha"` if the image has a pixel with alpha ≤ 127, else `"dark"`.
- `"alpha"`: opaque pixels (alpha > 127) of a cut-out on a transparent background.
- `"dark"`: dark pixels (grey value < 128) of a binary mask with a dark leaf on a light background.

The largest 4-connected region is measured. **Resolution flags** from the leaf's native size (√area in pixels):
below 250 px EA and MC are not computed (`not_resolved`); from 250 to 500 px EA is `below_recommended`;
from 500 px both are `ok`.

Only PNG input is read. A leaf at about 1,000 × 1,000 px takes about 4–5 s.

## Tests

`tests/testthat` checks every step against data files of the Python reference in `../tests/fixtures`
(shared with the Rust tests; not part of the built package, so these tests skip there). By default the
whole-pipeline test runs on every 10th fixture mask; set `SHAPECOMPLEXITY_FULL_TESTS=true` for all 221.
