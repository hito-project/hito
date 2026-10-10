# Development shell for NixOS: `nix-shell`, then `cargo run`.
# rustup installs the toolchain pinned in rust-toolchain.toml.
{ pkgs ? import <nixpkgs> { } }:

let
  # Loaded at run time by the window and graphics stack.
  runtimeLibs = with pkgs; [
    libGL
    libxkbcommon
    vulkan-loader
    wayland
    libx11
    libxcursor
    libxi
    libxrandr
  ];
in
pkgs.mkShell {
  packages = with pkgs; [ rustup gcc ];
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
}
