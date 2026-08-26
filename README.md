# ShapeComplexity v1.0.0

[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.22113804.svg)](https://doi.org/10.5281/zenodo.22113804)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**A powerful tool for analyzing leaf edge and macro-shape complexity from PNG images**

ShapeComplexity provides both command-line and graphical interfaces for comprehensive leaf morphology analysis, measuring edge complexity (EC) and macro-shape complexity (MC) through advanced image processing algorithms.

---

##  Features

### Edge Complexity (EC) Analysis
- **Geodesic path analysis** along leaf edges with adaptive morphological opening
- **Approximate Entropy (ApEn)** calculation for edge irregularity
- **Adaptive petiole filtering** for accurate edge measurements

### Macro-shape Complexity (MC) Analysis
- **Harmonic enhancement** for capturing periodic patterns
- **Spectral entropy** using FFT-based frequency analysis

### Dual Interface
- 🖥️ **GUI Application** - Interactive visual analysis with real-time overlays
- ⌨️ **CLI Tool** - Batch processing for high-throughput analysis

### Advanced Capabilities
- Parallel processing for batch operations
- Configurable reference points (Center of Mass or Emerge Point)
- Comprehensive CSV output with 20+ metrics per image

---

## 📥 Installation

### Windows (Portable - No Installation Required)

Download the latest Windows build from the
[**Releases page**](https://github.com/Thornbach/ShapeComplexity/releases/latest).

Extract the archive and double-click `leaf_complex_gui.exe` to start the GUI,
or run `shapecomplexity_cli.exe` from a terminal for batch processing.

---

## 🚀 Quick Start

### GUI Application
1. Download and extract the latest release archive
2. Double-click `leaf_complex_gui.exe`
3. Click **File → Open Workspace** and select your image folder
4. Select images and click **Analyze**
5. Export results via **File → Export Selected Analysis**

> **Input requirement:** images must be PNGs with a transparent background, and
> ideally share the same resolution.

---

## 📊 Output Format

ShapeComplexity generates:

```
output/
├── EC/                    # Edge Complexity visualizations (pink regions = opaque)
│   └── [filename]_EC.png
├── MC/                    # Macro-shape Complexity visualizations (pink regions = transparent)
│   └── [filename]_MC.png
└── summary/
    ├── EC_summary.csv     # Edge complexity metrics
    └── MC_summary.csv     # Macro-shape complexity metrics
```

**Key Metrics:**
- Geodesic Edge Complexity (length, curvature, entropy)
- Approximate Entropy (ApEn) — reported as **EC**, edge complexity
- Spectral Entropy (SpEn) — reported as **MC**, macro-shape complexity
- Fractal dimension, circularity, shape index and area
- Shape indices (length, width, aspect ratio)

---

## 🔧 Configuration

Edit `config.toml` to customize analysis parameters:

```toml
# Image processing
resize_dimensions = [512, 512]
opening_kernel_size = 9

# Reference point calculation
reference_point_choice = "COM"  # "COM" or "EP"

# Advanced parameters
adaptive_opening_max_density = 75.0
enable_petiole_filter_ec = true
thornfiddle_smoothing_strength = 2.0
```

See [config.toml](config.toml) for full documentation of all parameters.

---

## 📚 Documentation

- **[config.toml](config.toml)** - Parameter reference with explanations

---

## 🧪 Example Workflow

### Academic Research Pipeline
1. Collect leaf images with transparent backgrounds (PNG)
2. Organize images by species/treatment in folders
3. Run batch analysis with CLI tool
4. Import CSV results into R/Python for statistical analysis
5. Use GUI for interactive exploration of interesting cases

### Quality Control
1. Load image set in GUI
2. Visually inspect EC/MC overlays
3. Adjust configuration parameters interactively
4. Re-analyze flagged images
5. Export final results

---

##  Building from Source

If you're on Linux or macOS you may build the tool from source:

```bash
# Install Rust (https://rustup.rs/)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone repository
git clone https://github.com/Thornbach/ShapeComplexity.git
cd ShapeComplexity

# Build CLI
cargo build --release --bin shapecomplexity_cli

# Build GUI
cd leaf_complex_gui
cargo build --release

# Executables will be in target/release/
```

---

## 📖 Citation

If you use ShapeComplexity in your research, please cite **both** the software
and the accompanying paper.

**Software:**

```
Trauden, T., Rakotomalala, A. A. N. A., Junker, R. R., Sauereßig, L.,
Trauden, K., Muñoz Andres, M., Dannoritzer, R., Farwig, N., & Pinkert, S.
(2026). ShapeComplexity: entropy-based metrics for quantifying edge- and
macro-shape complexity (v1.0.0) [Computer software]. Zenodo.
https://doi.org/10.5281/zenodo.22113804
```

The DOI above is the **concept DOI**, which always resolves to the newest
release. To cite the exact version used in the paper, use the version DOI
[10.5281/zenodo.22113805](https://doi.org/10.5281/zenodo.22113805).

**Paper:**

```
Trauden, T., Rakotomalala, A. A. N. A., Junker, R. R., Sauereßig, L.,
Trauden, K., Muñoz Andres, M., Dannoritzer, R., Farwig, N., & Pinkert, S.
(2026). Introducing entropy-based metrics for quantifying edge- and
macro-shape complexity in leaves and beyond.
```

Machine-readable metadata is provided in [CITATION.cff](CITATION.cff).

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## 👤 Author

**Tobias Trauden**
📧 ToTrauden@proton.me

---

## 🐛 Issues & Feedback

Found a bug or have a feature request? Please open an issue on
[GitHub Issues](https://github.com/Thornbach/ShapeComplexity/issues).

---

## 🙏 Acknowledgments

Special thanks to contributors and the Rust community for excellent image processing libraries.

---

**Version:** 1.0.0
**Last Updated:** August 2026
**Status:** Stable Release
