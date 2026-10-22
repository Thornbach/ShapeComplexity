# Input rules and the user-facing functions (no fixtures needed).

disk <- function(n, r) {
  g <- (seq_len(n) - (n + 1) / 2)
  outer(g, g, function(y, x) x^2 + y^2 <= r^2)
}

test_that("mask sources follow the rules of the ShapeComplexity program (D113, D117)", {
  m <- disk(40, 15)
  rgba <- array(0, c(40, 40, 4))
  rgba[, , 1:3] <- 0.2 # dark colour everywhere
  rgba[, , 4] <- m # transparent outside the leaf
  f_alpha <- tempfile(fileext = ".png")
  png::writePNG(rgba, f_alpha)
  a <- read_mask(f_alpha)
  expect_identical(attr(a, "mask_source"), "alpha")
  expect_identical(as.vector(a), as.vector(m))
  # opaque everywhere: auto resolves to dark, the dark pixels are the leaf
  grey <- ifelse(m, 0, 1)
  f_grey <- tempfile(fileext = ".png")
  png::writePNG(grey, f_grey)
  d <- read_mask(f_grey)
  expect_identical(attr(d, "mask_source"), "dark")
  expect_identical(as.vector(d), as.vector(m))
  expect_error(read_mask(f_grey, "alpha"), "no alpha channel")
  # dark rule: transparent pixels are never leaf
  rgba[, , 4] <- 1
  rgba[1:5, , 4] <- 0
  png::writePNG(rgba, f_alpha)
  expect_false(any(read_mask(f_alpha, "dark")[1:5, ]))
  expect_true(all(read_mask(f_alpha, "dark")[6:40, ]))
})

test_that("a disk has MC 0 and almost no EA; small leaves are flagged", {
  r <- measure(disk(700, 300))
  expect_equal(r$MC, 0)
  expect_lt(r$EA, 0.01) # pixel steps of the digital disk; same bound as the Rust test
  expect_equal(r$elongation, 1, tolerance = 1e-3)
  expect_identical(c(r$EA_flag, r$MC_flag), c("ok", "ok"))
  small <- measure(disk(200, 80))
  expect_identical(c(small$EA_flag, small$MC_flag), c("not_resolved", "not_resolved"))
  expect_true(is.nan(small$EA) && is.nan(small$MC))
  expect_null(measure(matrix(FALSE, 5, 5)))
})
