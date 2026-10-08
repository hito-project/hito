# 0005. Rust-first stack with Rust-native dependencies

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

[ADR 0007](0007-ui-familiar-to-current-users.md) requires a native, cross-platform (Linux-first) desktop application with a custom UI. Revit's single-threaded performance is a known pain point. The project owner considers Rust the language of the future and prefers dependencies written in Rust.

## Decision

- **Rust** for the whole codebase.
- **Dependency policy:** prefer dependencies written in Rust. A non-Rust dependency (C/C++ bindings, for example) needs a recorded justification: why no Rust option is good enough, and whether it can be replaced later.
- Initial choices: **wgpu** for rendering and **egui** for the UI, both pure Rust. The geometry kernel and IFC libraries are pending an ecosystem review ([tech-stack.md](../architecture/tech-stack.md)).

## Alternatives considered

- **C++ + Qt + OpenCascade (FreeCAD's stack):** more mature geometry and IFC libraries, but slower development and harder contributor onboarding.
- **Web (TypeScript + Three.js, wrapped in Tauri):** easiest distribution, but a lower performance ceiling for large models.

## Consequences

- Memory safety and good multithreading support from the start.
- Pure-Rust builds are simple to cross-compile and package.
- The Rust solid-modelling and IFC ecosystems are immature, so we may write our own components where C++ libraries would otherwise be used.
