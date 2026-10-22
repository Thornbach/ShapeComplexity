# contour.R - Gaussian smoothing, 0.5 contour and arc-length resampling (PORTING.md §5, §6, §8)
#
# Copies SciPy 1.13.1 `gaussian_filter` / `gaussian_filter1d` (symmetric correlation, kernel normalised with
# NumPy's pairwise sum), scikit-image 0.24.0 `find_contours` (marching squares and segment assembly) and
# NumPy 1.26.4 `interp` / `linspace`. Points are (row, col) in two-column matrices.

# Normalised Gaussian kernel of radius trunc(4 sigma + 0.5), as SciPy `_gaussian_kernel1d`.
gaussian_kernel <- function(sigma) {
  radius <- as.integer(4 * sigma + 0.5)
  x <- -radius:radius
  phi <- exp((-0.5 / (sigma * sigma)) * (x * x))
  phi / np_sum(phi)
}

# Indices (1-based) of positions -r .. n + r - 1 of a line of length n, extended by `mode`.
extend_index <- function(n, r, mode) {
  i <- (-r):(n + r - 1L)
  if (mode == "reflect") { # d c b a | a b c d
    m <- i %% (2L * n)
    ifelse(m >= n, 2L * n - 1L - m, m) + 1L
  } else { # wrap
    i %% n + 1L
  }
}

# Symmetric correlation along the columns of matrix `a` (each row is one line); `NI_Correlate1D`,
# symmetric branch: centre tap, then the outer taps first.
correlate_rows <- function(a, kernel, mode) {
  n <- ncol(a)
  r <- (length(kernel) - 1L) %/% 2L
  ext <- a[, extend_index(n, r, mode), drop = FALSE]
  mid <- r + seq_len(n)
  out <- ext[, mid, drop = FALSE] * kernel[r + 1L]
  for (j in rev(seq_len(r))) {
    out <- out + (ext[, mid - j, drop = FALSE] + ext[, mid + j, drop = FALSE]) * kernel[r + 1L - j]
  }
  out
}

# `ndimage.gaussian_filter(a, sigma)` (mode reflect): axis 0 (along the columns) first, then axis 1.
gaussian_filter_2d <- function(a, sigma) {
  kernel <- gaussian_kernel(sigma)
  b <- t(correlate_rows(t(a), kernel, "reflect"))
  correlate_rows(b, kernel, "reflect")
}

# `ndimage.gaussian_filter1d(x, sigma, mode = "wrap")`.
gaussian_filter1d_wrap <- function(x, sigma) {
  as.vector(correlate_rows(matrix(x, 1L), gaussian_kernel(sigma), "wrap"))
}

# Segments of each marching-squares case (1..14): point types 1 top, 2 bottom, 3 left, 4 right.
MS_FROM <- c(1L, 4L, 4L, 3L, 1L, 4L, 4L, 2L, 1L, 2L, 2L, 3L, 1L, 3L)
MS_TO <- c(3L, 1L, 3L, 2L, 2L, 1L, 2L, 4L, 3L, 1L, 3L, 4L, 4L, 1L)

# `skimage.measure.find_contours(a, level)` with the defaults (fully_connected and positive_orientation "low").
# Returns a list of contours (two-column matrices, row and col), in creation order.
find_contours <- function(a, level) {
  nr <- nrow(a)
  nc <- ncol(a)
  if (nr < 2L || nc < 2L) return(list())
  hi <- a > level # a value equal to the level counts as low
  case <- hi[-nr, -nc] + 2L * hi[-nr, -1L] + 4L * hi[-1L, -nc] + 8L * hi[-1L, -1L]
  # squares in raster order
  sel <- which(t(case != 0L & case != 15L)) - 1L
  r0 <- sel %/% (nc - 1L)
  c0 <- sel %% (nc - 1L)
  cs <- t(case)[sel + 1L]
  ul <- a[cbind(r0 + 1L, c0 + 1L)]
  ur <- a[cbind(r0 + 1L, c0 + 2L)]
  ll <- a[cbind(r0 + 2L, c0 + 1L)]
  lr <- a[cbind(r0 + 2L, c0 + 2L)]
  frac <- function(from, to) ifelse(to == from, 0, (level - from) / (to - from))
  # points by type: top, bottom, left, right
  pr <- cbind(r0 + 0, r0 + 1, r0 + frac(ul, ll), r0 + frac(ur, lr))
  pc <- cbind(c0 + frac(ul, ur), c0 + frac(ll, lr), c0 + 0, c0 + 1)
  k <- seq_along(sel)
  ft <- MS_FROM[cs]
  tt <- MS_TO[cs]
  # cases 6 and 9 have a second segment: 6 left -> bottom, 9 bottom -> right
  two <- which(cs == 6L | cs == 9L)
  ord <- order(c(k, two + 0.5))
  seg_k <- c(k, two)[ord]
  seg_f <- c(ft, ifelse(cs[two] == 6L, 3L, 2L))[ord]
  seg_t <- c(tt, ifelse(cs[two] == 6L, 2L, 4L))[ord]
  assemble_contours(pr[cbind(seg_k, seg_f)], pc[cbind(seg_k, seg_f)], pr[cbind(seg_k, seg_t)], pc[cbind(seg_k, seg_t)])
}

# `_assemble_contours`: joins segments (from -> to) into contours, matching points by exact equality.
# A contour is kept as two point-index stacks: `front` (last element = first point) and `back`.
assemble_contours <- function(fr, fc, tr, tc) {
  ns <- length(fr)
  if (ns == 0L) return(list())
  pr <- c(fr, tr)
  pc <- c(fc, tc)
  key <- sprintf("%a %a", pr + 0, pc + 0) # bit pattern; + 0 turns -0 into +0, as Python's float equality
  starts <- new.env(hash = TRUE)
  ends <- new.env(hash = TRUE)
  front <- list()
  back <- list()
  alive <- logical(0)
  first_pt <- function(f, b) if (length(f)) f[length(f)] else b[1L]
  last_pt <- function(f, b) if (length(b)) b[length(b)] else f[1L]
  for (s in seq_len(ns)) {
    fk <- key[s]
    tk <- key[ns + s]
    if (fk == tk) next
    tail <- starts[[tk]]
    if (!is.null(tail)) rm(list = tk, envir = starts)
    head <- ends[[fk]]
    if (!is.null(head)) rm(list = fk, envir = ends)
    if (!is.null(tail) && !is.null(head)) {
      if (tail == head) {
        back[[head]] <- c(back[[head]], ns + s) # closes the loop
      } else if (tail > head) {
        back[[head]] <- c(back[[head]], rev(front[[tail]]), back[[tail]])
        alive[tail] <- FALSE
        front[[tail]] <- integer(0)
        back[[tail]] <- integer(0)
        assign(key[first_pt(front[[head]], back[[head]])], head, envir = starts)
        assign(key[last_pt(front[[head]], back[[head]])], head, envir = ends)
      } else {
        hf <- key[first_pt(front[[head]], back[[head]])]
        front[[tail]] <- c(front[[tail]], rev(back[[head]]), front[[head]])
        if (exists(hf, envir = starts, inherits = FALSE)) rm(list = hf, envir = starts)
        alive[head] <- FALSE
        front[[head]] <- integer(0)
        back[[head]] <- integer(0)
        assign(key[first_pt(front[[tail]], back[[tail]])], tail, envir = starts)
        assign(key[last_pt(front[[tail]], back[[tail]])], tail, envir = ends)
      }
    } else if (is.null(tail) && is.null(head)) {
      id <- length(alive) + 1L
      alive[id] <- TRUE
      front[[id]] <- integer(0)
      back[[id]] <- c(s, ns + s)
      assign(fk, id, envir = starts)
      assign(tk, id, envir = ends)
    } else if (!is.null(tail)) {
      front[[tail]] <- c(front[[tail]], s)
      assign(fk, tail, envir = starts)
    } else {
      back[[head]] <- c(back[[head]], ns + s)
      assign(tk, head, envir = ends)
    }
  }
  lapply(which(alive), function(id) {
    p <- c(rev(front[[id]]), back[[id]])
    cbind(pr[p], pc[p])
  })
}

# `np.allclose(a, b)` for two points.
allclose_pt <- function(a, b) all(abs(a - b) <= 1e-8 + 1e-5 * abs(b))

# Outline of L': first longest 0.5 contour of the Gaussian-smoothed mask, padded by PAD_CONTOUR px,
# repeated end point dropped. Coordinates in the padded frame.
outline <- function(lp) {
  a <- pad_mask(lp, PAD_CONTOUR) + 0
  contours <- find_contours(gaussian_filter_2d(a, SIGMA_PRE), 0.5)
  if (length(contours) == 0L) return(matrix(numeric(0), 0L, 2L))
  c <- contours[[which.max(vapply(contours, nrow, 0L))]] # first of equal ones
  n <- nrow(c)
  if (n > 1L && allclose_pt(c[1L, ], c[n, ])) c <- c[-n, , drop = FALSE]
  c
}

# Closed polygon resampled by arc length at about `step` px. Returns the points and the exact step.
resample <- function(c, step = 1) {
  m <- nrow(c)
  cc <- rbind(c, c[1L, ])
  seg <- c(0, cumsum_seq(hypot(diff(cc[, 1L]), diff(cc[, 2L]))))
  total <- seg[m + 1L]
  n <- max(16, round_half_even(total / step))
  ds <- total / n
  s <- (0:(n - 1)) * ds + 0
  # np.interp: j = last index with seg[j] <= s
  j <- findInterval(s, seg)
  exact <- j == m + 1L | seg[j] == s
  jn <- pmin(j + 1L, m + 1L)
  d <- seg[jn] - seg[j]
  interp <- function(p) ifelse(exact, p[j], (p[jn] - p[j]) / d * (s - seg[j]) + p[j])
  list(points = cbind(interp(cc[, 1L]), interp(cc[, 2L])), step = ds)
}

# Outline c (1 px steps) and blade line BL (c smoothed along arc length, sigma = r*), same padded frame.
blade_line <- function(lp) {
  rs <- resample(outline(lp), 1)
  sig <- R_STAR * SQRT_A / rs$step
  c <- rs$points
  list(outline = c, blade_line = cbind(gaussian_filter1d_wrap(c[, 1L], sig), gaussian_filter1d_wrap(c[, 2L], sig)),
       step = rs$step)
}
