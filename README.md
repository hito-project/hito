# HITO

**HITO Is Totally Open.** It's an open-source, cross-platform (Linux-first) **2D/3D civil engineering suite**. It will replace the separate programs civil engineers depend on today (Revit, AutoCAD, Civil 3D, CYPE, structural analysis, coordination…) with one unified core. Parity with the programs it replaces is the minimum.

A *hito* is a survey marker: the fixed reference point everything else is measured from.

The project is in the research and design phase. The code so far is the workspace skeleton: running it opens an empty window.

All documentation lives in [`docs/`](docs/README.md). To get involved, read [CONTRIBUTING.md](CONTRIBUTING.md).

## Building and running

### Prerequisites

- **Rust**, installed with [rustup](https://rustup.rs). You don't pick a version: [`rust-toolchain.toml`](rust-toolchain.toml) pins it, and rustup installs it on the first `cargo` command.
- **A C compiler and linker.** Some dependencies are written in C or C++ (SQLite, [ADR 0013](docs/decisions/0013-storage-engine-and-file-format.md); Manifold, [ADR 0014](docs/decisions/0014-geometry-kernel.md)).
- **Linux:** a Wayland or X11 desktop with working Vulkan or OpenGL drivers. The build itself needs no other system libraries.

  | Distribution | Command |
  |---|---|
  | Debian, Ubuntu | `sudo apt install build-essential libxkbcommon0 libwayland-client0 libvulkan1 mesa-vulkan-drivers` |
  | Fedora | `sudo dnf install gcc libxkbcommon libwayland-client vulkan-loader mesa-vulkan-drivers` |
  | Arch | `sudo pacman -S base-devel libxkbcommon wayland vulkan-icd-loader` |
  | NixOS | Run `nix-shell` in the repository first. [`shell.nix`](shell.nix) provides rustup, the compiler and the runtime libraries. |

- **Windows:** the [Visual Studio C++ build tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/), which rustup offers to install.
- **macOS:** the Xcode command line tools (`xcode-select --install`).

### Run

```sh
cargo run
```

### Test

```sh
cargo test --workspace
```

The tests include the architecture's dependency rules: the core never depends on domains, adapters or a UI toolkit. See the [workspace layout](Cargo.toml) and [`tools/architecture-check`](tools/architecture-check/src/lib.rs).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
