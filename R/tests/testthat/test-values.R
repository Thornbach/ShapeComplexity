# Whole pipeline on the public fixture masks: A1 (normalised mask, L') and A2 (|d| <= 1e-9, flags) as in D111.
# About 4 s per mask; by default every 10th mask, all 221 with SHAPECOMPLEXITY_FULL_TESTS=true.

test_that("public leaves give the reference values (D111 A1, A2)", {
  fx <- fixtures()
  v <- read_table(file.path(fx, "leaves", "values.csv"))
  rows <- if (identical(Sys.getenv("SHAPECOMPLEXITY_FULL_TESTS"), "true")) seq_len(nrow(v)) else seq(1L, nrow(v), by = 10L)
  bad <- character(0)
  worst <- 0
  for (i in rows) {
    d <- file.path(fx, "leaves", v$key[i])
    input <- read_fx_mask(file.path(d, "input.png"))
    if (v$mode[i] == "raw") {
      nm <- normalise(input)
      if (!identical(nm$mask, read_fx_mask(file.path(d, "norm.png")))) bad <- c(bad, paste(v$key[i], "normalised mask"))
      m <- nm$mask
      native <- nm$native
    } else {
      m <- input
      native <- NULL
    }
    r <- measure_normalised(m, native, trace = TRUE)
    if (!identical(r$lp, read_fx_mask(file.path(d, "lp.png")))) bad <- c(bad, paste(v$key[i], "L'"))
    for (k in c("EA", "MC", "elongation", "petiole_len")) {
      want <- if (v[[k]][i] == "") NaN else num(v[[k]][i])
      diff <- if (is.nan(r[[k]]) && is.nan(want)) 0 else abs(r[[k]] - want)
      worst <- max(worst, if (is.na(diff)) Inf else diff)
      if (is.na(diff) || diff > 1e-9) bad <- c(bad, sprintf("%s: %s %.17g vs %.17g", v$key[i], k, r[[k]], want))
    }
    if (r$EA_flag != v$EA_flag[i] || r$MC_flag != v$MC_flag[i]) bad <- c(bad, paste(v$key[i], "flags"))
  }
  message(sprintf("leaf values: %d masks, max |difference| %.1e", length(rows), worst))
  expect_identical(bad, character(0))
})
