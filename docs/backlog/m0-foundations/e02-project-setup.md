---
title: "E02: Project setup"
labels: type:epic, area:infra
milestone: "M0: Foundations"
---

A repository that any contributor can clone, build, test and contribute to, with a code structure that reflects the architecture (core, domains, adapters, application) from the first commit.

## Story: Build and run HITO with one command
labels: type:story, area:infra

As a contributor, I want to clone the repo and run HITO with one command, so that I can start contributing without setup friction.

### Acceptance criteria
- [ ] A Cargo workspace whose crates mirror the architecture: core, domains, adapters, app
- [ ] `cargo run` opens an empty window on Linux. Windows and macOS also build.
- [ ] Crate dependency rules are enforced, so the core never depends on domains, adapters or the UI
- [ ] README documents prerequisites and the run command

## Story: Automatic checks on every pull request
labels: type:story, area:infra

As a maintainer, I want every PR checked automatically, so that `main` always builds and passes its tests.

### Acceptance criteria
- [ ] CI runs formatting, lints (warnings fail the build) and tests on Linux, Windows and macOS
- [ ] CI checks dependency licenses against MIT/Apache-2.0 compatibility and flags non-Rust dependencies, per ADR 0005 and ADR 0009
- [ ] Status checks are required before merging

## Story: Downloadable development builds
labels: type:story, area:infra

As a tester, I want to download a recent build without compiling, so that non-developers (like the founding user) can try HITO.

### Acceptance criteria
- [ ] Every merge to `main` publishes a Linux build artifact (AppImage or Flatpak, decided in this story)
- [ ] Windows and macOS artifacts are published as well
- [ ] The README links to the latest build
