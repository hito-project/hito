# 0017. Development builds: an AppImage for Linux, zips for Windows and macOS, published as a rolling prerelease

- **Status:** Accepted
- **Date:** 2026-10-10

## Context

People who don't compile Rust, starting with the founding user, need to try HITO as it grows. Every merge to `main` should produce a build they can download and open on Linux, Windows and macOS. Linux comes first ([ADR 0001](0001-standalone-desktop-app.md), [ADR 0007](0007-ui-familiar-to-current-users.md)).

These are development builds, not releases: no installer, no updates, no stable file format. They must be easy to get and easy to throw away.

HITO is one Rust binary. The window system and graphics libraries (Wayland, X11, Vulkan, OpenGL) are loaded at run time from the host, and SQLite and Manifold will be compiled into the binary ([ADR 0013](0013-storage-engine-and-file-format.md), [ADR 0014](0014-geometry-kernel.md)).

Linux formats considered:

| Format | What the tester does | Graphics drivers | Notes |
|---|---|---|---|
| **AppImage** | Download one file, make it executable, open it | The host's, like a native app | No install, no runtime to download. Runs on any distribution with a glibc at least as new as the build machine's. Needs FUSE to mount itself, or `--appimage-extract-and-run` without it. |
| **Flatpak** | Install Flatpak, add Flathub, install the bundle, which pulls a runtime of several hundred MB | Through a GL extension matching the host's Mesa or NVIDIA version | Sandboxed: opening and saving files goes through portals. A single-file bundle can't update itself. The right format for Flathub, not for a file passed around. |
| Snap | Needs `snapd`; Ubuntu-centred | Through snap interfaces | Sandboxed, store-centred. |
| `.deb` / `.rpm` | Install with root, one package per distribution family | The host's | Several packages to build and test. |
| A plain binary in a tarball | Unpack, run from a terminal | The host's | No icon or desktop entry. Loses the executable bit when unpacked by some tools. |

Where to publish:

- **Workflow artifacts** need a GitHub login to download, expire after 90 days, and arrive zipped inside another zip.
- **A GitHub release** is public, needs no login, and has stable URLs.

## Decision

### 1. Linux: an AppImage

A tester downloads one file and opens it, with no root, no runtime and no sandbox in the way of their GPU drivers or files. Flatpak suits distribution through Flathub, which is a decision for the first public release, not for development builds.

- The binary is built on the oldest Ubuntu LTS that CI offers (22.04 today), so it runs on distributions with an older glibc.
- `appimagetool` and the AppImage runtime are pinned to fixed versions and checked against their SHA-256 sums.
- The AppImage carries a desktop entry and an icon. The script that builds it, `packaging/linux/build-appimage.sh`, runs the same way on a developer's machine.

### 2. Windows: a zip with `hito.exe`

The C runtime is linked statically, so no Visual C++ redistributable is needed. Release builds open no console window. No installer yet.

### 3. macOS: a universal `HITO.app`, zipped

One bundle runs on Apple silicon and Intel. It is signed ad hoc, not with a Developer ID, and not notarised.

### 4. A rolling `dev` prerelease

Every push to `main` builds all three and replaces the files of one GitHub prerelease, tagged `dev`, which moves to the newest commit. The file names never change, so the README links straight to them. Pull requests that touch the packaging build the files without publishing them.

## Consequences

- A tester on any common Linux distribution downloads one file and opens it. The README links to the latest build.
- Windows SmartScreen and macOS Gatekeeper warn about unsigned apps, and testers must allow HITO once. The release notes say how. Signing (an Apple Developer ID and a Windows code-signing certificate) and notarisation wait for the first public release, which also chooses installers and Flathub.
- On a Linux system without FUSE, the AppImage must be started with `--appimage-extract-and-run`.
- Only the newest build is available. Older builds are rebuilt from their commit if needed.
- C and C++ dependencies are compiled on the build machine, so Ubuntu 22.04's compiler sets the oldest `libstdc++` HITO needs on Linux. When GitHub retires that image, the build moves to the next LTS.
- macOS builds compile twice, once per architecture.
