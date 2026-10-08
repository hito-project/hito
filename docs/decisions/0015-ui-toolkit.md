# 0015. UI toolkit: egui on wgpu, behind HITO's own shell layer

- **Status:** Proposed
- **Date:** 2026-10-08

## Context

[ADR 0005](0005-rust-first-stack.md) picked egui as the initial UI toolkit, with its maturity to be reviewed. [ADR 0007](0007-ui-familiar-to-current-users.md) sets what the UI must do: ribbon toolbars, dockable panels, a property grid, an AutoCAD-style command line with autocomplete, Revit-style shortcuts and an embedded 3D viewport. [ADR 0008](0008-argentina-first-international-by-design.md) requires localisation from day one, and the toolkit must fit the license in [ADR 0009](0009-license.md).

Spike [#4](https://github.com/hito-project/hito/issues/4) compared egui with iced, Slint, Xilem and GPUI, and built the riskiest needs with egui. The evaluation and sources are in [Discussion #111](https://github.com/hito-project/hito/discussions/111). The prototype is in [PR #110](https://github.com/hito-project/hito/pull/110).

What the spike found:

| Toolkit | Docking | wgpu viewport | Complex text and RTL | Accessibility | License | Maturity |
|---|---|---|---|---|---|---|
| **egui** 0.36 | ✅ egui_dock | ✅ | Shaping ✅, bidi ❌, editing RTL ❌ | ✅ AccessKit | MIT/Apache | Widely used; breaking release every 2–4 months |
| **iced** 0.14 | Splits only, no tabs | ✅ | ✅ cosmic-text | ❌ None | MIT | Pre-1.0, one release a year |
| **Slint** 1.18 | ❌ | Unstable feature | ✅ Parley | ✅ | GPL-3.0, or a non-open-source royalty-free license | 1.x, stable |
| **Xilem** 0.4 | ❌ | Not documented | ✅ Parley | ✅ | Apache | Experimental |
| **GPUI** + GPUI Kit | ✅ | ❌ No API | Open RTL reports | ✅ (new) | Apache | Snapshots of Zed's repository, no stable releases |

The egui prototype built the ribbon, a dock, a property grid with units, the command line with autocomplete, two-letter shortcuts, a wgpu viewport inside a dock tab, and Fluent i18n, in about 900 lines. Ten headless tests drive it through the AccessKit tree with a real GPU. Two gaps showed up:

- **Right-to-left text.** Hebrew and Arabic are shaped and joined correctly, but egui has no bidi algorithm, so a right-to-left paragraph with Latin text or numbers inside comes out in the wrong order. Editing RTL text misplaces the caret ([egui #8576](https://github.com/emilk/egui/issues/8576)). Fixes are in review upstream.
- **egui_dock's tabs have no accessible name**, so a screen reader can't tell the panels apart.

## Decision

### 1. egui and eframe, on wgpu

The application shell uses **egui** and **eframe** with the **wgpu** renderer, as ADR 0005 proposed. It's the only Rust toolkit that meets every ADR 0007 need today, with a permissive license and an active project. Docking uses **egui_dock**.

The alternatives each miss a core need: iced has no accessibility, Slint has no license compatible with ADR 0009, Xilem is experimental, and GPUI has no stable releases and no way to embed our own wgpu viewport.

### 2. HITO's own shell layer sits between egui and the rest

- A UI crate owns the shell widgets: ribbon, dock, property grid, command line, shortcuts. They're built on egui, but nothing outside the UI crates uses egui types.
- Domains expose **commands** (with their localised names, global names and shortcuts) and **property descriptions** (name, value, unit, editor). The shell turns those into ribbon buttons, command-line completions and property rows. Ribbon buttons, typed commands and shortcuts all run the same command.
- This keeps egui replaceable, as [ADR 0004](0004-unified-core-ports-and-adapters.md) keeps every other adapter replaceable.

### 3. The 3D viewport renders off-screen with its own passes

The viewport renders into its own wgpu textures, with its own depth buffer and, later, MSAA, picking and section passes, on egui's wgpu device. egui only draws the result as an image in a dock tab. The viewport re-renders only when its camera or scene changes.

### 4. Localisation uses Fluent

User-facing text comes from **Fluent** (`fluent-bundle`) message files, one per locale (`es-AR`, `en` first). Commands also have a language-neutral name, so a typed command works in any UI language, like AutoCAD's `_LINE`.

### 5. Right-to-left languages wait

Spanish and English, the languages ADR 0008 needs first, work fully. No RTL locale ships until egui handles bidi text and RTL editing. Before adding one, a spike checks egui's status and, if needed, we contribute the fix upstream (bidi in [#8577](https://github.com/emilk/egui/pull/8577), or Parley in [#5784](https://github.com/emilk/egui/pull/5784)).

### 6. Accessibility and UI tests go through AccessKit

- Every interactive widget has an accessible name. Property values are linked to their labels.
- The egui_dock tab gap is fixed upstream, or in a fork until then.
- UI tests use **egui_kittest**, which finds widgets by role and name through the same AccessKit tree a screen reader uses. Tests that render need a Vulkan device, so CI uses Mesa's software renderer (lavapipe).

## Consequences

- The shell built in M0 and M1 doesn't need to be rebuilt on another toolkit.
- HITO depends on egui's release cadence: a breaking release every 2–4 months. We pin versions and upgrade on purpose. egui 0.36 needs Rust 1.95.
- Ribbons, property grids and the command line are our own widgets, so they look and behave exactly as ADR 0007 asks, but we maintain them.
- RTL languages (Arabic, Hebrew, Persian) are blocked until egui's text work lands or we contribute it.
- Domain code describes commands and properties, never widgets, so it stays testable without a UI.
- [tech-stack.md](../architecture/tech-stack.md) records the choice.
