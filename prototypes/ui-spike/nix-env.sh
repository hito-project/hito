# Source this on NixOS before `cargo run` or `cargo test`: wgpu and winit load
# Vulkan, Wayland, X11 and xkbcommon at runtime, and nix doesn't put them on
# the library path.
libs=""
for p in vulkan-loader libxkbcommon wayland libGL xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr; do
  libs="$libs:$(nix build "nixpkgs#$p" --no-link --print-out-paths | grep -v -- '-man$' | head -1)/lib"
done
export LD_LIBRARY_PATH="/run/opengl-driver/lib${libs}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
