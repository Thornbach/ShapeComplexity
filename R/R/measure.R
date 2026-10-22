# measure.R - input rules, normalisation, the whole pipeline, results table, port check
#
# Same input rules and output columns as the Rust program (D112, D113, D117).

# --- input -----------------------------------------------------------------------------------------------------

# Channels of a PNG file as 8-bit integer values (0..255), as the Rust `image` crate converts them:
# 16-bit values v become (v + 128) %/% 257. Returns a list with `grey()` (a function giving the luma of the
# colour channels; computed only when needed) and `alpha` (NULL if the image has no alpha channel).
read_png_channels <- function(path) {
  x <- png::readPNG(path, info = TRUE)
  depth <- attr(x, "info")$bit.depth
  dims <- dim(x)
  if (length(dims) == 2L) dim(x) <- c(dims, 1L)
  nch <- dim(x)[3L]
  if (identical(depth, 16L) || identical(depth, 16)) {
    to8 <- function(a) (round(a * 65535) + 128) %/% 257
    luma_raw <- function(r, g, b) (2126 * round(r * 65535) + 7152 * round(g * 65535) + 722 * round(b * 65535)) %/% 10000
  } else {
    to8 <- function(a) round(a * 255)
    luma_raw <- function(r, g, b) (2126 * round(r * 255) + 7152 * round(g * 255) + 722 * round(b * 255)) %/% 10000
  }
  # `image` crate (0.24): luma at the source depth, integer division, then to 8 bit
  grey <- function() {
    g <- if (nch >= 3L) {
      l <- luma_raw(x[, , 1L], x[, , 2L], x[, , 3L])
      if (identical(depth, 16L) || identical(depth, 16)) (l + 128) %/% 257 else l
    } else {
      to8(x[, , 1L])
    }
    matrix(g, dims[1L], dims[2L])
  }
  alpha <- if (nch == 2L || nch == 4L) matrix(to8(x[, , nch]), dims[1L], dims[2L]) else NULL
  list(grey = grey, alpha = alpha)
}

#' Leaf mask of a PNG image
#'
#' @param path PNG file.
#' @param mask_source `"auto"` (default): `"alpha"` if the image has a pixel with alpha <= 127, else `"dark"`.
#'   `"alpha"`: opaque pixels (alpha > 127) of a cut-out; images without alpha are refused.
#'   `"dark"`: dark pixels (grey value < 128) of a binary mask with a dark leaf on a light background;
#'   pixels with alpha <= 127 are never leaf.
#' @return A logical matrix (TRUE = leaf) with attribute `mask_source`, the rule applied.
#' @export
read_mask <- function(path, mask_source = c("auto", "alpha", "dark")) {
  mask_source <- match.arg(mask_source)
  ch <- read_png_channels(path)
  if (mask_source == "auto") {
    mask_source <- if (!is.null(ch$alpha) && any(ch$alpha <= 127)) "alpha" else "dark"
  }
  m <- if (mask_source == "alpha") {
    if (is.null(ch$alpha)) stop(path, ": no alpha channel; the leaf must be the opaque part of the image")
    ch$alpha > 127
  } else {
    if (is.null(ch$alpha)) ch$grey() < 128 else ch$grey() < 128 & ch$alpha > 127
  }
  attr(m, "mask_source") <- mask_source
  m
}

# An already normalised mask (1-bit PNG; any non-zero pixel = leaf).
read_normalised_mask <- function(path) {
  x <- png::readPNG(path)
  if (length(dim(x)) == 3L) x <- x[, , 1L]
  x > 0
}

# --- pipeline --------------------------------------------------------------------------------------------------

#' Normalise a leaf mask
#'
#' Largest 4-connected component, crop, isotropic Pillow-bilinear scale to sqrt(A) = 1000 px, threshold, pad 4 px.
#'
#' @param raw Logical matrix, TRUE = leaf.
#' @return A list with the normalised mask `mask` and the native sqrt(leaf area) `native` in px; NULL for an
#'   empty mask.
#' @export
normalise <- function(raw) {
  m <- largest_component(raw)
  b <- bbox(m)
  if (is.null(b)) return(NULL)
  m <- m[b[1L]:b[2L], b[3L]:b[4L], drop = FALSE]
  native <- sqrt(sum(m))
  f <- SQRT_A / native
  img <- resize_bilinear(m * 255, round_half_even(ncol(m) * f), round_half_even(nrow(m) * f))
  list(mask = pad_mask(img > 127, PAD_NORM), native = native)
}

# Flags for EA and MC from the native sqrt(A) in px (NULL = unknown).
flags <- function(native) {
  if (is.null(native)) return(c("unknown", "unknown"))
  if (native < FLOOR_MIN) return(c("not_resolved", "not_resolved"))
  if (native < FLOOR_EA) return(c("below_recommended", "ok"))
  c("ok", "ok")
}

#' Measure a normalised leaf mask
#'
#' @param m Normalised logical mask (as returned by [normalise()]).
#' @param native sqrt(leaf area) in px before normalisation, for the resolution flags; NULL if unknown.
#' @param trace If TRUE, also return the intermediate stages.
#' @return A list with EA, MC, elongation, petiole_len, sqrtA_native, EA_flag, MC_flag; with `trace = TRUE`
#'   also lp (mask after petiole removal), outline, blade_line, margin_profile, error_curve, white.
#' @export
measure_normalised <- function(m, native = NULL, trace = FALSE) {
  pet <- remove_petiole(m)
  lp <- pet$lp
  b <- blade_line(lp)
  wh <- whiten(b$blade_line - PAD_CONTOUR, nrow(lp), ncol(lp))
  fl <- flags(native)
  h <- margin_profile(b$outline, b$blade_line)
  ea <- if (fl[1L] != "not_resolved") edge_amount_from_profile(h, b$blade_line) else NaN
  e <- if (fl[2L] != "not_resolved") error_curve(wh$white) else numeric(0)
  mc <- if (fl[2L] != "not_resolved") macro_complexity_from_curve(e) else NaN
  out <- list(EA = ea, MC = mc, elongation = wh$elongation, petiole_len = pet$petiole_len,
              sqrtA_native = if (is.null(native)) NaN else native, EA_flag = fl[1L], MC_flag = fl[2L])
  if (trace) {
    out <- c(out, list(lp = lp, outline = b$outline, blade_line = b$blade_line, margin_profile = h,
                       error_curve = e, white = wh$white))
  }
  out
}

#' Measure a leaf mask
#'
#' @param raw Logical matrix, TRUE = leaf (any size).
#' @param trace If TRUE, also return the intermediate stages (see [measure_normalised()]) and the normalised
#'   mask `norm`.
#' @return A list of values (see [measure_normalised()]); NULL for an empty mask.
#' @export
measure <- function(raw, trace = FALSE) {
  nm <- normalise(raw)
  if (is.null(nm)) return(NULL)
  out <- measure_normalised(nm$mask, nm$native, trace)
  if (trace) out$norm <- nm$mask
  out
}

#' Measure leaf images
#'
#' One row per image: file, EA, MC, elongation, petiole_len, sqrtA_native, EA_flag, MC_flag, mask_source,
#' version (the columns of the ShapeComplexity program).
#'
#' @param paths PNG files.
#' @param mask_source See [read_mask()].
#' @return A data frame.
#' @export
measure_files <- function(paths, mask_source = c("auto", "alpha", "dark")) {
  mask_source <- match.arg(mask_source)
  rows <- lapply(paths, function(p) {
    m <- read_mask(p, mask_source)
    r <- measure(m)
    if (is.null(r)) stop(p, ": no leaf pixels")
    data.frame(file = basename(p), EA = r$EA, MC = r$MC, elongation = r$elongation, petiole_len = r$petiole_len,
               sqrtA_native = r$sqrtA_native, EA_flag = r$EA_flag, MC_flag = r$MC_flag,
               mask_source = attr(m, "mask_source"), version = VERSION)
  })
  do.call(rbind, rows)
}

# --- port check (PORTING.md §13) ---------------------------------------------------------------------------------

# One manifest row: values, and the normalised mask (raw mode) and L' written as PNG.
port_check_one <- function(key, path, mode, mask_dir) {
  t0 <- proc.time()[["elapsed"]]
  if (mode == "raw") {
    nm <- normalise(read_mask(path, "alpha"))
    m <- nm$mask
    native <- nm$native
    png::writePNG(m + 0, file.path(mask_dir, paste0(key, "__norm.png")))
  } else {
    m <- read_normalised_mask(path)
    native <- NULL
  }
  r <- measure_normalised(m, native, trace = TRUE)
  png::writePNG(r$lp + 0, file.path(mask_dir, paste0(key, "__lp.png")))
  data.frame(key = key, EA = r$EA, MC = r$MC, elongation = r$elongation, petiole_len = r$petiole_len,
             sqrtA_native = r$sqrtA_native, EA_flag = r$EA_flag, MC_flag = r$MC_flag,
             seconds = proc.time()[["elapsed"]] - t0)
}

# The port contract of `check_port.py`: `port_check(c(manifest.csv, out.csv, mask_dir))`.
# Runs on `SHAPECOMPLEXITY_CORES` processes (default: all cores; forked, not on Windows).
port_check <- function(args) {
  stopifnot(length(args) == 3L)
  man <- utils::read.csv(args[1L], stringsAsFactors = FALSE)
  dir.create(args[3L], showWarnings = FALSE, recursive = TRUE)
  cores <- as.integer(Sys.getenv("SHAPECOMPLEXITY_CORES", parallel::detectCores()))
  rows <- parallel::mclapply(seq_len(nrow(man)), function(i) {
    port_check_one(man$key[i], man$path[i], man$mode[i], args[3L])
  }, mc.cores = cores, mc.preschedule = FALSE)
  bad <- vapply(rows, inherits, TRUE, what = "try-error")
  if (any(bad)) stop("port check failed on ", man$key[which(bad)[1L]], ": ", rows[[which(bad)[1L]]])
  out <- do.call(rbind, rows)
  num <- c("EA", "MC", "elongation", "petiole_len", "sqrtA_native", "seconds")
  out[num] <- lapply(out[num], function(v) ifelse(is.nan(v) | is.na(v), "nan", sprintf("%.17g", v)))
  utils::write.csv(out, args[2L], row.names = FALSE, quote = FALSE)
  invisible(out)
}
