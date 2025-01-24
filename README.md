rust-life [![Build Status](https://github.com/crazymykl/rust-life/actions/workflows/ci.yml/badge.svg)](https://github.com/crazymykl/rust-life/actions/workflows/ci.yml) [![Crates.io](https://img.shields.io/crates/v/rust-life.svg?logo=rust)](https://crates.io/crates/rust-life/) [![codecov](https://codecov.io/gh/crazymykl/rust-life/branch/main/badge.svg?token=2CXIS1cQrh)](https://codecov.io/gh/crazymykl/rust-life)
=========

Conway's Game of Life, in Rust

Installation
--
`cargo install rust-life`

Running
--
```
Usage: rust-life [OPTIONS]

Options:
  -c, --cols <COLS>
          Number of columns of in the board [default: 640]
  -r, --rows <ROWS>
          Number of rows of in the board [default: 400]
  -t, --template <TEMPLATE>
          A board template string
  -a, --align <ALIGN>
          Alignment of the template within the world [default: center] [possible values: top-left, top, top-right, left, center, right, bottom-left, bottom, bottom-right]
  -p, --padding <PADDING>...
          Custom padding around template, takes 1 to 4 values (overrides alignment)
  -g, --generations <GENERATIONS>
          Number of generations to advance the template for the initial pattern
  -G, --generation-limit <GENERATION_LIMIT>
          Number of generations to display before stopping (runs forever if not given)
  -s, --scale <SCALE>
          Scale factor (pixels per cell side) [default: 2]
  -x, --exit-on-finish
          Close GUI window after final generation
      --no-gui
          Disable GUI
  -u, --ups <UPS>
          Updates per second (target) [default: 120]
  -h, --help
          Print help
  -V, --version
          Print version
```