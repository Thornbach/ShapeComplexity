# fill.R - Pillow 11.0.0 filled polygon on a mode "1" image (PORTING.md §7)
#
# `ImageDraw.polygon(xy, fill=1)`: vertices truncated toward zero, edge list with merged horizontal runs,
# scan-line fill in single precision (`polygon_generic` in Draw.c). The corner rule writes `xx[k]` with k an
# edge index, as Pillow does; it is copied on purpose.
# Single-precision steps are computed in double and rounded with `f32()`. For +, * and / of two singles this
# gives the correctly rounded single result (53 >= 2 * 24 + 2 bits).

# x of edges on scan line y, in f32 as Pillow computes it.
fill_x_at <- function(y, y0, x0, dx) f32(f32((y - y0) * dx) + x0)

# Draw.c ROUND_UP / ROUND_DOWN: `f + 0.5F` is a float addition; `fabs(f)` promotes to double.
fill_round_up <- function(f) ifelse(f >= 0, floor(f32(f + 0.5)), -floor(abs(f) + 0.5))
fill_round_down <- function(f) ifelse(f >= 0, ceiling(f32(f - 0.5)), -ceiling(abs(f) - 0.5))

# Filled polygon with vertices (x = col, y = row) on a `cols` x `rows` logical matrix.
polygon_fill <- function(cols, rows, xs, ys) {
  img <- matrix(FALSE, rows, cols)
  n <- length(xs)
  if (n == 0L) return(img)
  # `_draw_polygon`: C `(int)` cast
  xi <- trunc(xs)
  yi <- trunc(ys)

  # `ImagingDrawPolygon`: edge list, as start and end vertex (exmin, exmax change when horizontal runs merge)
  from <- integer(n)
  to <- integer(n)
  exmin <- exmax <- numeric(n)
  ne <- 0L
  for (i in seq_len(n - 1L)) {
    x0 <- xi[i]
    x1 <- xi[i + 1L]
    y0 <- yi[i]
    if (y0 == yi[i + 1L] && i != 1L && y0 == yi[i - 1L]) {
      # a horizontal line that follows another horizontal line
      if (x1 > x0 && x0 > xi[i - 1L]) {
        exmax[ne] <- x1
        next
      } else if (x1 < x0 && x0 < xi[i - 1L]) {
        exmin[ne] <- x1
        next
      }
    }
    ne <- ne + 1L
    from[ne] <- i
    to[ne] <- i + 1L
    exmin[ne] <- min(x0, x1)
    exmax[ne] <- max(x0, x1)
  }
  if (xi[n] != xi[1L] || yi[n] != yi[1L]) {
    ne <- ne + 1L
    from[ne] <- n
    to[ne] <- 1L
    exmin[ne] <- min(xi[n], xi[1L])
    exmax[ne] <- max(xi[n], xi[1L])
  }
  keep <- seq_len(ne)
  ex0 <- xi[from[keep]]
  ey0 <- yi[from[keep]]
  ey1 <- yi[to[keep]]
  exmin <- exmin[keep]
  exmax <- exmax[keep]
  eymin <- pmin(ey0, ey1)
  eymax <- pmax(ey0, ey1)
  edx <- ifelse(ey0 == ey1, 0, f32((xi[to[keep]] - ex0) / (ey1 - ey0)))

  # `polygon_generic`, hasAlpha = 0
  ymin <- min(rows - 1, eymin)
  ymax <- max(0, eymax)
  spans <- list()
  hor <- which(eymin == eymax)
  if (length(hor)) spans[[1L]] <- cbind(exmin[hor], eymin[hor], exmax[hor])
  tab <- which(eymin != eymax)
  ty0 <- ey0[tab]
  tx0 <- ex0[tab]
  tdx <- edx[tab]
  tymin <- eymin[tab]
  tymax <- eymax[tab]
  ymin <- max(ymin, 0)
  ymax <- min(ymax, rows)
  xx <- numeric(2L * length(tab)) # not cleared between scan lines, as in Pillow
  for (y in seq(ymin, length.out = max(ymax - ymin + 1, 0))) {
    act <- which(tymin <= y & y <= tymax)
    if (length(act) == 0L) next
    xs_y <- fill_x_at(y, ty0[act], tx0[act], tdx[act])
    cand <- which(tymin == y | tymax == y)
    j <- 0L
    for (q in seq_along(act)) {
      i <- act[q]
      j <- j + 1L
      xx[j] <- xs_y[q]
      if (y == tymax[i] && y < ymax) {
        # needed to draw consistent polygons
        j <- j + 1L
        xx[j] <- xx[j - 1L]
      } else if (tdx[i] != 0 && xx[j] == floor(xx[j])) {
        # connect discontiguous corners: first earlier edge k that meets this one here
        ks <- cand[cand < i]
        if (length(ks)) {
          same_dir <- !((tdx[i] > 0 & tdx[ks] <= 0) | (tdx[i] < 0 & tdx[ks] >= 0))
          meet <- (y == tymin[i] & y == tymin[ks]) | (y == tymax[i] & y == tymax[ks])
          hit <- ks[same_dir & meet & xx[j] == fill_x_at(y, ty0[ks], tx0[ks], tdx[ks])]
          if (length(hit)) {
            k <- hit[1L]
            off <- if (y == ymax) -1 else 1
            a <- fill_x_at(y + off, ty0[i], tx0[i], tdx[i])
            b <- fill_x_at(y + off, ty0[k], tx0[k], tdx[k])
            xx[k] <- if (y == tymax[i]) {
              if (tdx[i] > 0) f32(max(a, b) + 1) else f32(min(a, b) - 1)
            } else if (tdx[i] > 0) {
              min(a, b)
            } else {
              f32(max(a, b) + 1)
            }
          }
        }
      }
    }
    xx[seq_len(j)] <- sort(xx[seq_len(j)])
    if (j >= 2L) {
      odd <- seq(2L, j, by = 2L)
      spans[[length(spans) + 1L]] <- cbind(fill_round_up(xx[odd - 1L]), y, fill_round_down(xx[odd]))
    }
  }

  # `hline` for every span: clip to the image, then fill x0..x1
  sp <- do.call(rbind, spans)
  if (is.null(sp)) return(img)
  x0 <- sp[, 1L]
  y <- sp[, 2L]
  x1 <- sp[, 3L]
  ok <- y >= 0 & y < rows & x0 < cols & x1 >= 0
  x0 <- pmax(x0[ok], 0)
  x1 <- pmin(x1[ok], cols - 1)
  y <- y[ok]
  ok <- x0 <= x1
  len <- (x1 - x0 + 1)[ok]
  img[cbind(rep.int(y[ok], len) + 1, sequence(len, from = x0[ok]) + 1)] <- TRUE
  img
}
