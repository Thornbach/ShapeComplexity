# v2 steps against the fixtures of the Python reference. Each library step must match the reference bit for bit.

test_that("round() rounds half to even, as Python", {
  expect_identical(round_half_even(c(0.5, 1.5, 2.5, 3.5, -0.5, -1.5)), c(0, 2, 2, 4, 0, -2))
})

test_that("np_sum follows NumPy's pairwise order", {
  # 1e16 + 1 rounds back to 1e16. A plain loop gives 15, NumPy's eight lanes give 1 + 7 * 4 = 29,
  # R's long-double sum() gives the exact 30.
  x <- c(1e16, rep(1, 15), -1e16, rep(1, 15))
  expect_identical(np_sum(x), 29)
  expect_identical(np_sum(numeric(0)), 0)
  expect_identical(np_sum(1:5 + 0), 15)
})

test_that("labels are 4- or 8-connected and numbered in raster order", {
  # reference: flood fill pixel by pixel in raster order
  flood <- function(m, eight) {
    lab <- matrix(0L, nrow(m), ncol(m))
    nb <- if (eight) as.matrix(expand.grid(-1:1, -1:1))[-5, ] else rbind(c(-1, 0), c(0, -1), c(0, 1), c(1, 0))
    n <- 0L
    for (r in seq_len(nrow(m))) for (c in seq_len(ncol(m))) {
      if (!m[r, c] || lab[r, c] != 0L) next
      n <- n + 1L
      lab[r, c] <- n
      queue <- list(c(r, c))
      while (length(queue)) {
        p <- queue[[1L]]
        queue <- queue[-1L]
        for (k in seq_len(nrow(nb))) {
          q <- p + nb[k, ]
          if (q[1] < 1 || q[2] < 1 || q[1] > nrow(m) || q[2] > ncol(m)) next
          if (m[q[1], q[2]] && lab[q[1], q[2]] == 0L) {
            lab[q[1], q[2]] <- n
            queue[[length(queue) + 1L]] <- q
          }
        }
      }
    }
    list(lab = lab, n = n)
  }
  set.seed(20261022)
  for (i in 1:40) {
    nr <- sample(5:25, 1)
    nc <- sample(5:25, 1)
    m <- matrix(runif(nr * nc) < runif(1, 0.3, 0.7), nr, nc)
    for (eight in c(FALSE, TRUE)) expect_identical(label(m, eight), flood(m, eight))
  }
})

test_that("windowed squared distances are exact below w + 1 and at least (w + 1)^2 above", {
  set.seed(20261022)
  w <- 3L
  for (i in 1:30) {
    nr <- sample(4:30, 1)
    nc <- sample(4:30, 1)
    target <- matrix(runif(nr * nc) < runif(1, 0.005, 0.1), nr, nc)
    need <- matrix(runif(nr * nc) < 0.7, nr, nc)
    got <- sq_distance_to(target, w, need)
    tp <- which(target, arr.ind = TRUE)
    for (r in seq_len(nr)) for (c in seq_len(nc)) {
      true <- if (nrow(tp)) min((tp[, 1] - r)^2 + (tp[, 2] - c)^2) else (r - 1 + 1)^2 + (c - 1)^2 # phantom (-1, 0)
      g <- got[r, c]
      if (!need[r, c]) expect_identical(g, Inf)
      else if (true < (w + 1)^2 || !nrow(tp)) expect_identical(g, true)
      else expect_gte(g, (w + 1)^2)
    }
  }
})

test_that("fill_holes fills background not 4-connected to the border", {
  m <- matrix(FALSE, 7, 7)
  m[2:6, 2:6] <- TRUE
  m[4, 4] <- FALSE # hole
  m[3, 6] <- FALSE # notch in the edge, open to the outside: stays background
  out <- fill_holes(m)
  expect_true(out[4, 4])
  expect_false(out[3, 6])
  d <- matrix(FALSE, 5, 5)
  d[2, 3] <- d[3, 2] <- d[3, 4] <- d[4, 3] <- TRUE # diagonal gaps: centre is a 4-connected hole
  expect_true(fill_holes(d)[3, 3])
})

test_that("resize equals Pillow bilinear on every unit case", {
  d <- file.path(fixtures(), "unit", "resize")
  cases <- read_table(file.path(d, "cases.csv"))
  bad <- character(0)
  for (i in seq_len(nrow(cases))) {
    src <- read_gray(file.path(d, paste0(cases$id[i], "_in.png")))
    want <- read_gray(file.path(d, paste0(cases$id[i], "_out.png")))
    got <- resize_bilinear(src, as.integer(cases$out_w[i]), as.integer(cases$out_h[i]))
    if (!same_bits(got, want)) bad <- c(bad, cases$id[i])
  }
  expect_identical(bad, character(0))
})

test_that("Gaussian filters equal SciPy on every unit case", {
  d <- file.path(fixtures(), "unit", "gauss")
  cases <- read_table(file.path(d, "cases.csv"))
  bad <- character(0)
  for (i in seq_len(nrow(cases))) {
    a <- read_grid(file.path(d, paste0(cases$id[i], "_2d_in.csv")))
    want <- read_grid(file.path(d, paste0(cases$id[i], "_2d_out.csv")))
    ok2 <- same_bits(gaussian_filter_2d(a, num(cases$sigma_2d[i])), want)
    line <- read_points(file.path(d, paste0(cases$id[i], "_1d.csv")))
    ok1 <- same_bits(gaussian_filter1d_wrap(line[, 1], num(cases$sigma_1d[i])), line[, 2])
    if (!(ok1 && ok2)) bad <- c(bad, sprintf("%s (2-D %s, 1-D %s)", cases$id[i], ok2, ok1))
  }
  expect_identical(bad, character(0))
})

test_that("find_contours equals skimage on every unit case", {
  d <- file.path(fixtures(), "unit", "contour")
  cases <- read_table(file.path(d, "cases.csv"))
  bad <- character(0)
  for (id in cases$id) {
    a <- read_grid(file.path(d, paste0(id, "_in.csv")))
    out <- utils::read.csv(file.path(d, paste0(id, "_out.csv")))
    want <- lapply(split(out[, c("r", "c")], out$contour), function(x) unname(as.matrix(x)))
    got <- find_contours(a, 0.5)
    if (length(got) != length(want) || !all(mapply(same_bits, got, want))) bad <- c(bad, id)
  }
  expect_identical(bad, character(0))
})

test_that("polygon fill equals Pillow on every unit case", {
  d <- file.path(fixtures(), "unit", "polygon")
  cases <- read_table(file.path(d, "cases.csv"))
  bad <- character(0)
  for (i in seq_len(nrow(cases))) {
    xy <- read_points(file.path(d, paste0(cases$id[i], "_xy.csv")))
    got <- polygon_fill(as.integer(cases$width[i]), as.integer(cases$height[i]), xy[, 1], xy[, 2])
    if (!identical(got, read_fx_mask(file.path(d, paste0(cases$id[i], "_mask.png"))))) bad <- c(bad, cases$id[i])
  }
  expect_identical(bad, character(0))
})

# Leaf stages: L' -> contour -> resampled outline -> blade line -> filled polygon, each from the previous fixture.
test_that("leaf stages equal the reference bit for bit", {
  fx <- fixtures()
  leaves <- stage_leaves(fx)
  expect_equal(nrow(leaves), 20L)
  bad <- character(0)
  for (i in seq_len(nrow(leaves))) {
    key <- leaves$key[i]
    d <- file.path(fx, "leaves", key)
    lp <- read_fx_mask(file.path(d, "lp.png"))
    contour <- read_points(file.path(d, "contour.csv"))
    if (!same_bits(outline(lp), contour)) bad <- c(bad, paste(key, "contour"))
    rs <- resample(contour, 1)
    want_c <- read_points(file.path(d, "outline.csv"))
    if (!same_bits(rs$points, want_c)) {
      bad <- c(bad, sprintf("%s outline (max |d| %.1e)", key, max(abs(rs$points - want_c))))
    }
    sig <- R_STAR * SQRT_A / rs$step
    bl <- cbind(gaussian_filter1d_wrap(want_c[, 1], sig), gaussian_filter1d_wrap(want_c[, 2], sig))
    want_bl <- read_points(file.path(d, "bl.csv"))
    if (!same_bits(bl, want_bl)) bad <- c(bad, sprintf("%s blade line (max |d| %.1e)", key, max(abs(bl - want_bl))))
    poly <- polygon_fill(leaves$w[i], leaves$h[i], want_bl[, 2] - PAD_CONTOUR, want_bl[, 1] - PAD_CONTOUR)
    if (!identical(poly, read_fx_mask(file.path(d, "polygon.png")))) bad <- c(bad, paste(key, "polygon"))
  }
  expect_identical(bad, character(0))
})
