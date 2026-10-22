# metrics.R - EA, whitening and elongation, elliptic Fourier error curve and MC (PORTING.md §9-§11)

# Margin profile h(s): signed distance of each outline point from the blade line, along the blade line's normal.
margin_profile <- function(c, bl) {
  n <- nrow(bl)
  nxt <- c(seq_len(n)[-1L], 1L)
  prv <- c(n, seq_len(n - 1L))
  t1 <- bl[nxt, 1L] - bl[prv, 1L]
  t2 <- bl[nxt, 2L] - bl[prv, 2L]
  len <- hypot(t1, t2)
  (c[, 1L] - bl[, 1L]) * (-t2 / len) + (c[, 2L] - bl[, 2L]) * (t1 / len)
}

# Edge amount (D30, "B8") from a margin profile `h` of the blade line `bl`: extra length of h(s) unrolled
# along the blade line.
edge_amount_from_profile <- function(h, bl) {
  n <- nrow(bl)
  nxt <- c(seq_len(n)[-1L], 1L)
  ds <- hypot(bl[nxt, 1L] - bl[, 1L], bl[nxt, 2L] - bl[, 2L])
  np_sum(hypot(ds, h[nxt] - h) - ds) / np_sum(ds)
}

# Edge amount of an outline `c` and its blade line `bl`.
edge_amount <- function(c, bl) edge_amount_from_profile(margin_profile(c, bl), bl)

# Eigen-decomposition of a symmetric 2 x 2 matrix [[a, b], [b, d]]: eigenvalues ascending and unit vectors
# (one per column).
eigh2 <- function(a, b, d) {
  if (b == 0) {
    return(if (a <= d) list(w = c(a, d), v = diag(2)) else list(w = c(d, a), v = matrix(c(0, 1, 1, 0), 2)))
  }
  mean <- (a + d) / 2
  rad <- sqrt(((a - d) / 2)^2 + b * b)
  w <- c(mean - rad, mean + rad)
  vec <- function(l) {
    # the better-conditioned of (b, l - a) and (l - d, b)
    uv <- if (abs(l - a) > abs(l - d)) c(b, l - a) else c(l - d, b)
    uv / hypot(uv[1L], uv[2L])
  }
  list(w = w, v = cbind(vec(w[1L]), vec(w[2L])))
}

# D34/D35/D46: whitened, resampled blade line and the elongation. `bl` is in the L' frame (padding removed).
whiten <- function(bl, rows, cols) {
  poly <- polygon_fill(cols, rows, bl[, 2L], bl[, 1L])
  idx <- which(t(poly)) - 1 # raster order, as np.argwhere
  r <- idx %/% cols
  cc <- idx %% cols
  n <- length(idx)
  # integer sums: exact in any order
  mu <- c(sum(r) / n, sum(cc) / n)
  dr <- r - mu[1L]
  dc <- cc - mu[2L]
  s <- eigh2(sum(dr * dr) / (n - 1), sum(dr * dc) / (n - 1), sum(dc * dc) / (n - 1))
  w <- s$w
  v <- s$v
  # W = v diag(1 / sqrt(w)) v^T * (w0 w1)^(1/4)
  scale <- (w[1L] * w[2L])^0.25
  wm <- matrix(0, 2L, 2L)
  for (i in 1:2) for (j in 1:2) {
    wm[i, j] <- (v[i, 1L] * v[j, 1L] / sqrt(w[1L]) + v[i, 2L] * v[j, 2L] / sqrt(w[2L])) * scale
  }
  a <- bl[, 1L] - mu[1L]
  b <- bl[, 2L] - mu[2L]
  white <- cbind(a * wm[1L, 1L] + b * wm[1L, 2L], a * wm[2L, 1L] + b * wm[2L, 2L])
  list(white = resample(white, 1)$points, elongation = sqrt(w[2L] / w[1L]))
}

# Elliptic Fourier coefficients (Kuhl & Giardina 1982), x = col, y = row, arc-length parameter.
# Vertices whose outgoing segment has zero length are dropped.
efd <- function(c, n_harm) {
  m <- nrow(c)
  nxt <- c(seq_len(m)[-1L], 1L)
  x <- c[, 2L]
  y <- c[, 1L]
  dx <- x[nxt] - x
  dy <- y[nxt] - y
  dt <- hypot(dx, dy)
  keep <- dt > 0
  x <- x[keep]
  y <- y[keep]
  dx <- dx[keep]
  dy <- dy[keep]
  dt <- dt[keep]
  k <- length(x)
  t <- c(0, cumsum_seq(dt))
  period <- t[k + 1L]
  # mean of the piecewise-linear x(t), y(t); axis-0 sums run in order
  nk <- c(seq_len(k)[-1L], 1L)
  sum_seq <- function(v) Reduce(`+`, v, 0)
  dc <- c(sum_seq(0.5 * (x + x[nk]) * dt), sum_seq(0.5 * (y + y[nk]) * dt)) / period
  co <- matrix(0, n_harm, 4L)
  for (n in seq_len(n_harm)) {
    phi <- 2 * n * pi * t / period
    dcos <- diff(cos(phi))
    dsin <- diff(sin(phi))
    kk <- period / (2 * (n * n) * (pi * pi))
    co[n, ] <- kk * c(np_sum(dx / dt * dcos), np_sum(dx / dt * dsin), np_sum(dy / dt * dcos), np_sum(dy / dt * dsin))
  }
  list(xy = cbind(x, y), t = t[seq_len(k)], period = period, dc = dc, co = co)
}

# e(k) / sqrt(A), k = 1, 2, ...: L4 mean of the two nearest-distance sets, running minimum.
error_curve <- function(w) {
  f <- efd(w, K_MAX)
  phi <- 2 * pi * f$t / f$period
  rx <- rep(f$dc[1L], length(phi))
  ry <- rep(f$dc[2L], length(phi))
  e <- numeric(0)
  for (k in seq_len(K_MAX)) {
    co <- f$co[k, ]
    cs <- cos(k * phi)
    sn <- sin(k * phi)
    rx <- rx + (co[1L] * cs + co[2L] * sn)
    ry <- ry + (co[3L] * cs + co[4L] * sn)
    rec <- cbind(rx, ry)
    d1 <- nabor::knn(f$xy, rec, k = 1L)$nn.dists[, 1L]
    d2 <- nabor::knn(rec, f$xy, k = 1L)$nn.dists[, 1L]
    e[k] <- (0.5 * (np_sum(d1^4) / length(d1) + np_sum(d2^4) / length(d2)))^0.25
    if (max(d1, d2) < STOP * SQRT_A) break
  }
  cummin(e / SQRT_A)
}

# D40: MC = sum_k max(0, ln(e(k) / tau)), from an error curve e(k) / sqrt(A).
macro_complexity_from_curve <- function(e) np_sum(pmax(log(e / TAU), 0))

# MC of a whitened blade line.
macro_complexity <- function(w) macro_complexity_from_curve(error_curve(w))
