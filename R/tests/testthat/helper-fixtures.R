# Fixtures of the Python reference (tests/fixtures/README.md at the repository root), shared with the Rust tests.
# They are not part of the built package; the tests that need them skip when the folder is missing.
# SHAPECOMPLEXITY_FIXTURES may point to another copy of `tests/fixtures/public`.

fixtures <- function() {
  p <- Sys.getenv("SHAPECOMPLEXITY_FIXTURES", test_path("..", "..", "..", "tests", "fixtures", "public"))
  if (!dir.exists(p)) skip("fixtures missing")
  p
}

read_table <- function(p) utils::read.csv(p, colClasses = "character")

num <- function(s) as.numeric(s)

# CSV without a header: a numeric matrix.
read_grid <- function(p) unname(as.matrix(utils::read.csv(p, header = FALSE)))

# CSV with a header and two numeric columns: a two-column matrix.
read_points <- function(p) unname(as.matrix(utils::read.csv(p)[, 1:2]))

# 8-bit grey PNG as values 0..255.
read_gray <- function(p) {
  x <- png::readPNG(p)
  if (length(dim(x)) == 3L) x <- x[, , 1L]
  round(x * 255)
}

read_fx_mask <- function(p) read_gray(p) > 0

# Exactly equal, bit for bit (both NaN counts as equal).
same_bits <- function(a, b) {
  length(a) == length(b) && all(dim(a) == dim(b)) && all((a == b) | (is.nan(a) & is.nan(b)))
}

# Leaves with stage CSVs: key, rows, cols of the normalised mask.
stage_leaves <- function(fx) {
  v <- read_table(file.path(fx, "leaves", "values.csv"))
  v <- v[v$stages == "True", ]
  data.frame(key = v$key, h = as.integer(v$norm_h), w = as.integer(v$norm_w))
}
