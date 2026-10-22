# morph.R - distance thresholds, opening by a disk, petiole removal (PORTING.md §4)
#
# SciPy's exact Euclidean distance transform is only compared with two thresholds (core: d >= r*, opening:
# d <= r* + TOL). Both are computed exactly from squared integer distances: an exact column pass, then the
# row pass restricted to column offsets |dc| <= w. A target pixel outside that window is at least w + 1 away,
# so every distance d < w + 1 is exact and every larger one stays >= w + 1. With w = floor(r* + TOL) = 31 both
# thresholds (30 and 31.5) are decided exactly.

# TRUE where some element of `x` lies within +-w along its line; `x` holds lines of length `len` one after
# another (the columns of a matrix in column-major order).
near_in_lines <- function(x, len, w) {
  cs <- c(0L, cumsum(as.integer(x)))
  i <- seq_along(x) - 1L
  pos <- i %% len
  base <- i - pos
  cs[base + pmin(pos + w, len - 1L) + 2L] - cs[base + pmax(pos - w, 0L) + 1L] > 0L
}

# Squared distance to the nearest true pixel of `target`, for the pixels where `need` is TRUE (Inf elsewhere).
# Exact where d < w + 1; elsewhere the result is only known to be >= (w + 1)^2. Pixels with no target pixel in
# the box of +-w rows and columns are at least w + 1 away and get Inf. With no target pixel at all, SciPy
# measures to a phantom pixel at (-1, 0); this is copied.
sq_distance_to <- function(target, w, need = TRUE) {
  nr <- nrow(target)
  nc <- ncol(target)
  r <- rep.int(0:(nr - 1L), nc) # row of each element (column-major)
  need <- rep_len(as.vector(need), nr * nc)
  if (!any(target)) {
    return(matrix(ifelse(need, (r + 1)^2 + rep(0:(nc - 1L), each = nr)^2, Inf), nr, nc))
  }
  # column pass: nearest target above and below in the same column, by running max/min over the whole
  # column-major vector; column offsets keep columns apart
  big <- 2 * nr + 2
  base <- rep(0:(nc - 1L), each = nr) * big
  tv <- as.vector(target)
  above <- cummax(ifelse(tv, base + r + nr + 1, base))
  da <- ifelse(above > base, r - (above - base - nr - 1), Inf)
  below <- rev(cummin(rev(ifelse(tv, base + r, base + big - 1))))
  db <- ifelse(below < base + big - 1, (below - base) - r, Inf)
  g <- pmin(da, db)^2
  # pixels with a target pixel in the box: vertical window, then horizontal window (on the transpose)
  near <- near_in_lines(tv, nr, w)
  near <- as.vector(t(matrix(near_in_lines(as.vector(t(matrix(near, nr, nc))), nc, w), nc, nr)))
  idx <- which(near & need)
  # row pass on those pixels; g padded by w columns of Inf, so a shift by dc columns is a shift by dc * nr
  gp <- c(rep(Inf, w * nr), g, rep(Inf, w * nr))
  at <- idx + w * nr
  d <- gp[at]
  for (dc in seq_len(w)) {
    s <- dc * nr
    d <- pmin(d, gp[at - s] + dc * dc, gp[at + s] + dc * dc)
  }
  out <- rep(Inf, nr * nc)
  out[idx] <- d
  matrix(out, nr, nc)
}

# Opening of `m` by a disk of radius `r_px` via distance transforms, with TOL px slack at the outline.
# Distances are compared as SciPy does: sqrt of the squared integer distance, in f64. Only leaf pixels need a
# distance: background pixels are never core (distance 0) and never in the opening.
open_disk <- function(m, r_px) {
  w <- as.integer(floor(r_px + TOL))
  core <- m & sqrt(sq_distance_to(!m, w, m)) >= r_px
  m & (core | sqrt(sq_distance_to(core, w, m & !core)) <= r_px + TOL)
}

# Major and minor axis lengths of each label (skimage 0.24 `regionprops`: 4 sqrt of the inertia eigenvalues).
# `rn` = labelled runs (`label_runs`). Returns a matrix with columns major, minor; one row per label.
region_axes <- function(rn) {
  len <- rn$c1 - rn$c0 + 1L
  rows <- rep.int(rn$row, len)
  cols <- sequence(len, from = rn$c0)
  lab <- factor(rep.int(rn$lab, len), levels = seq_len(rn$n))
  rs <- split(rows, lab)
  cs <- split(cols, lab)
  t(vapply(seq_len(rn$n), function(l) {
    # pixels in raster order, coordinates in the label's bounding-box crop
    r <- as.double(rs[[l]] - min(rs[[l]]))
    c <- as.double(cs[[l]] - min(cs[[l]]))
    m00 <- length(r)
    rbar <- sum(r) / m00
    cbar <- sum(c) / m00
    dr <- r - rbar
    dc <- c - cbar
    mu20 <- sum(dr * dr)
    mu02 <- sum(dc * dc)
    mu11 <- sum(dr * dc)
    # inertia tensor [[mu02, -mu11], [-mu11, mu20]] / mu00; eigenvalues clipped at 0, descending
    a <- mu02 / m00
    b <- -mu11 / m00
    d <- mu20 / m00
    mean <- (a + d) / 2
    rad <- sqrt(((a - d) / 2)^2 + b * b)
    c(major = 4 * sqrt(max(mean + rad, 0)), minor = 4 * sqrt(max(mean - rad, 0)))
  }, c(major = 0, minor = 0)))
}

# D29 petiole removal. Returns L' and the petiole length / sqrt(A) (0 = none found).
remove_petiole <- function(m) {
  opened <- open_disk(m, R_STAR * SQRT_A)
  rn <- label_runs(m & !opened, TRUE)
  if (rn$n == 0L) return(list(lp = largest_filled(m), petiole_len = 0))
  axes <- region_axes(rn)
  best <- which.max(axes[, "major"]) # first of equal ones
  major <- axes[best, "major"]
  minor <- axes[best, "minor"]
  if (major < PET_LEN * SQRT_A || major < PET_ELONG * max(minor, 1e-9)) {
    return(list(lp = largest_filled(m), petiole_len = 0))
  }
  petiole <- runs_to_mask(rn, rn$lab == best)
  list(lp = largest_filled(m & !petiole), petiole_len = unname(major) / SQRT_A)
}
