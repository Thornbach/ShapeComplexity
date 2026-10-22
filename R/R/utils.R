# utils.R - constants and numeric helpers shared by all steps (PORTING.md §0)
#
# Port of the Python reference implementation `shapecomplexity_v2.py` (analysis archive of the paper).
# Every step copies the exact library algorithm the reference uses (Pillow 11.0.0, SciPy 1.13.1,
# scikit-image 0.24.0, NumPy 1.26.4); the rules are written down in PORTING.md of the analysis archive.
# Acceptance: |difference| <= 1e-9 for EA, MC, elongation and petiole length on every check mask (D111).

# Normalised sqrt(leaf area), px.
SQRT_A <- 1000
# Edge/macro split scale as a fraction of sqrt(A) (blade-line sigma and opening radius).
R_STAR <- 0.03
# Slack (px) of the opening at the outline.
TOL <- 1.5
# Gaussian sigma (px) on the mask before the 0.5 contour.
SIGMA_PRE <- 1.5
PAD_NORM <- 4L
PAD_CONTOUR <- 8L
# Petiole: minimum length (fraction of sqrt(A)) and minimum major/minor axis ratio.
PET_LEN <- 0.10
PET_ELONG <- 4.0
# Harmonic cap of the elliptic Fourier reconstruction.
K_MAX <- 64L
# MC tolerance (fraction of sqrt(A)).
TAU <- R_STAR / 2
# Hausdorff stop rule (fraction of sqrt(A)); changes run time only.
STOP <- R_STAR / 8
# Resolution floors on the native sqrt(A) in px.
FLOOR_MIN <- 250
FLOOR_EA <- 500

VERSION <- "2.0.0"

# Sum in NumPy's order (`np.sum` on a contiguous f64 array): blocks of 8192 added in order,
# each block summed pairwise. R's `sum()` accumulates in long double, which differs in the last bits.
np_sum <- function(a) {
  n <- length(a)
  res <- 0
  start <- 1L
  while (start <= n) {
    end <- min(start + 8191L, n)
    res <- res + pairwise_sum(a[start:end])
    start <- end + 1L
  }
  res
}

pairwise_sum <- function(a) {
  n <- length(a)
  if (n < 8L) {
    res <- 0
    for (v in a) res <- res + v
    res
  } else if (n <= 128L) {
    full <- n - n %% 8L
    r <- a[1:8]
    i <- 9L
    while (i <= full) {
      r <- r + a[i:(i + 7L)]
      i <- i + 8L
    }
    res <- ((r[1] + r[2]) + (r[3] + r[4])) + ((r[5] + r[6]) + (r[7] + r[8]))
    if (full < n) for (v in a[(full + 1L):n]) res <- res + v
    res
  } else {
    m <- n %/% 2L
    m <- m - m %% 8L
    pairwise_sum(a[1:m]) + pairwise_sum(a[(m + 1L):n])
  }
}

# Sequential cumulative sum in double precision (NumPy `cumsum`); R's `cumsum()` accumulates in long double.
cumsum_seq <- function(x) {
  if (length(x) == 0L) return(numeric(0))
  Reduce(`+`, x, accumulate = TRUE)
}

# C `hypot` (libm), as NumPy and Rust use it. R has no `hypot`; `Mod()` of a complex number calls libm `cabs`,
# which is `hypot`. `sqrt(x^2 + y^2)` differs from it in the last bit for about one value in six.
hypot <- function(x, y) Mod(complex(real = x, imaginary = y))

# Round to IEEE single precision (C `(float)` cast of a double). Used where Pillow computes in `float`.
f32 <- function(x) readBin(writeBin(as.double(x), raw(), size = 4L), "double", size = 4L, n = length(x))

# Python `round()` on a float: half to even. R's `round()` with zero digits does the same.
round_half_even <- function(x) round(x)
