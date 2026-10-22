# resize.R - Pillow 11.0.0 bilinear resize of an 8-bit image (PORTING.md §3)
#
# Pillow widens the filter support with the down-scale factor (antialiasing), uses 22-bit fixed-point
# weights, runs the horizontal pass first and stores the intermediate image as 8 bit.
# The integer sums are exact in doubles (all below 2^31), so the order of the taps does not matter.

PRECISION_BITS <- 22L

# Fixed-point coefficients for one axis (`precompute_coeffs` + `normalize_coeffs_8bpc`).
# Returns the first input index (0-based) of each output index and a matrix of integer weights
# (one row per output index, one column per tap; taps beyond the window have weight 0).
resize_coeffs <- function(in_size, out_size) {
  scale <- in_size / out_size
  filterscale <- if (scale < 1) 1 else scale
  support <- 1 * filterscale
  ss <- 1 / filterscale
  ksize <- as.integer(ceiling(support)) * 2L + 1L
  xx <- 0:(out_size - 1L)
  center <- 0 + (xx + 0.5) * scale
  # C `(int)` truncates toward zero
  xmin <- pmax(trunc(center - support + 0.5), 0)
  xmax <- pmin(trunc(center + support + 0.5), in_size)
  n <- pmax(xmax - xmin, 0)
  w <- matrix(0, out_size, ksize)
  for (x in 0:(ksize - 1L)) {
    t <- abs((x + xmin - center + 0.5) * ss)
    w[, x + 1L] <- ifelse(x < n & t < 1, 1 - t, 0)
  }
  ww <- numeric(out_size) # sequential sum over the taps
  for (x in seq_len(ksize)) ww <- ww + w[, x]
  w <- w / ifelse(ww != 0, ww, 1) # row by row; rows with ww = 0 stay as they are
  list(xmin = as.integer(xmin), k = trunc(0.5 + w * 2^PRECISION_BITS))
}

# One pass along the columns (horizontal) of an 8-bit matrix (values 0..255 as doubles).
resize_pass_horizontal <- function(src, cf) {
  nc <- ncol(src)
  acc <- matrix(2^(PRECISION_BITS - 1L), nrow(src), length(cf$xmin))
  for (x in seq_len(ncol(cf$k))) {
    idx <- pmin(cf$xmin + x, nc) # taps beyond the window have weight 0
    acc <- acc + src[, idx, drop = FALSE] * rep(cf$k[, x], each = nrow(src))
  }
  # Pillow `clip8`: arithmetic shift, then clamp
  pmin(pmax(floor(acc / 2^PRECISION_BITS), 0), 255)
}

# `Image.resize((out_cols, out_rows), BILINEAR)` on a mode "L" image (matrix of 0..255).
resize_bilinear <- function(src, out_cols, out_rows) {
  img <- src
  if (out_cols != ncol(src)) {
    img <- resize_pass_horizontal(img, resize_coeffs(ncol(src), out_cols))
  }
  if (out_rows != nrow(src)) {
    img <- t(resize_pass_horizontal(t(img), resize_coeffs(nrow(src), out_rows)))
  }
  img
}
