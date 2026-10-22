# mask.R - connected components, largest component, hole filling (PORTING.md §2, §4)
#
# Labels are numbered in raster order of each component's first pixel, as `scipy.ndimage.label`.
# Ties for "largest" go to the lowest label (NumPy `argmax`).
# Components are found on runs of true pixels along each row (union-find over overlapping runs of
# neighbouring rows), which needs a loop over runs only, not over pixels.

# Runs of true pixels of a logical matrix, in raster order. Columns and rows are 0-based.
mask_runs <- function(m) {
  nr <- nrow(m)
  nc <- ncol(m)
  v <- as.vector(t(m)) # raster order
  if (!any(v)) {
    return(list(row = integer(0), c0 = integer(0), c1 = integer(0), nr = nr, nc = nc))
  }
  col <- (seq_along(v) - 1L) %% nc
  prev <- c(FALSE, v[-length(v)])
  nxt <- c(v[-1L], FALSE)
  starts <- which(v & (col == 0L | !prev))
  ends <- which(v & (col == nc - 1L | !nxt))
  list(row = (starts - 1L) %/% nc, c0 = (starts - 1L) %% nc, c1 = (ends - 1L) %% nc, nr = nr, nc = nc)
}

# Connected components of the runs (4- or 8-connected). Adds `lab` (label of each run, 1..n in raster order
# of each component's first pixel) and `n`.
label_runs <- function(m, eight = FALSE) {
  rn <- mask_runs(m)
  k <- length(rn$row)
  if (k == 0L) {
    rn$lab <- integer(0)
    rn$n <- 0L
    return(rn)
  }
  e <- if (eight) 1L else 0L
  big <- rn$nc + 3 # row keys never overlap
  skey <- rn$row * big + rn$c0
  ekey <- rn$row * big + rn$c1
  # runs of the row above that touch each run: a contiguous range of run indices
  y <- rn$row - 1
  lo <- findInterval(y * big + rn$c0 - e - 0.5, ekey) + 1L
  hi <- findInterval(y * big + rn$c1 + e, skey)
  cnt <- pmax(hi - lo + 1L, 0L)
  pa <- sequence(cnt, from = lo)
  pb <- rep.int(seq_len(k), cnt)
  # union-find; the root is the lowest run index of the component
  parent <- seq_len(k)
  for (i in seq_along(pa)) {
    a <- pa[i]
    while (parent[a] != a) {
      parent[a] <- parent[parent[a]]
      a <- parent[a]
    }
    b <- pb[i]
    while (parent[b] != b) {
      parent[b] <- parent[parent[b]]
      b <- parent[b]
    }
    if (a < b) parent[b] <- a else if (b < a) parent[a] <- b
  }
  repeat {
    p2 <- parent[parent]
    if (all(p2 == parent)) break
    parent <- p2
  }
  # runs are in raster order, so first appearance = raster order of each component's first pixel
  rn$lab <- match(parent, unique(parent))
  rn$n <- max(rn$lab)
  rn
}

# Pixel count of each label (1..n).
run_counts <- function(rn) {
  len <- rn$c1 - rn$c0 + 1L
  vapply(split(len, factor(rn$lab, levels = seq_len(rn$n))), sum, 0)
}

# Logical matrix of the runs selected by `keep`.
runs_to_mask <- function(rn, keep = TRUE) {
  keep <- rep_len(keep, length(rn$row))
  len <- (rn$c1 - rn$c0 + 1L)[keep]
  v <- logical(rn$nr * rn$nc)
  v[sequence(len, from = (rn$row * rn$nc + rn$c0)[keep] + 1L)] <- TRUE
  t(matrix(v, rn$nc, rn$nr))
}

# Label image (0 = background) and the number of labels, as `ndimage.label` (4- or 8-connected).
label <- function(m, eight = FALSE) {
  rn <- label_runs(m, eight)
  len <- rn$c1 - rn$c0 + 1L
  v <- integer(rn$nr * rn$nc)
  v[sequence(len, from = rn$row * rn$nc + rn$c0 + 1L)] <- rep.int(rn$lab, len)
  list(lab = t(matrix(v, rn$nc, rn$nr)), n = rn$n)
}

# `largest_component`: the largest 4-connected component if there are two or more, else the mask itself.
largest_component <- function(m) {
  rn <- label_runs(m, FALSE)
  if (rn$n > 1L) runs_to_mask(rn, rn$lab == which.max(run_counts(rn))) else m
}

# `binary_fill_holes`: background not 4-connected to the outside of the image becomes foreground.
fill_holes <- function(m) {
  bg <- label_runs(!m, FALSE)
  if (bg$n == 0L) return(m)
  border <- bg$row == 0L | bg$row == bg$nr - 1L | bg$c0 == 0L | bg$c1 == bg$nc - 1L
  holes <- !(bg$lab %in% bg$lab[border])
  if (!any(holes)) return(m)
  m | runs_to_mask(bg, holes)
}

# `largest_filled`: largest 4-connected component with holes filled; an empty mask stays empty.
largest_filled <- function(m) {
  rn <- label_runs(m, FALSE)
  if (rn$n == 0L) return(m)
  fill_holes(runs_to_mask(rn, rn$lab == which.max(run_counts(rn))))
}

# Bounding box c(row0, row1, col0, col1), 1-based and inclusive, of the true pixels; NULL if there are none.
bbox <- function(m) {
  rows <- which(rowSums(m) > 0)
  if (length(rows) == 0L) return(NULL)
  cols <- which(colSums(m) > 0)
  c(range(rows), range(cols))
}

# Pad with `p` background pixels on every side.
pad_mask <- function(m, p) {
  out <- matrix(FALSE, nrow(m) + 2L * p, ncol(m) + 2L * p)
  out[p + seq_len(nrow(m)), p + seq_len(ncol(m))] <- m
  out
}
