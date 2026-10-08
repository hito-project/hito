# 0007. Standalone desktop app with a UI familiar to current users

- **Status:** Accepted
- **Date:** 2026-10-08
- **Supersedes:** [0001](0001-standalone-desktop-app.md)

## Context

ADR 0001 assumed the target users came from Blender. They don't: the founding user only uses Revit, and Blender was only mentioned as an alternative. Target users come from Revit, AutoCAD and CYPE. Any unfamiliar UX pattern is a switching cost, even when it's objectively better, because many users don't adapt easily.

## Decision

- Build a **standalone native desktop application**. The decision not to build on Blender or Bonsai stands.
- The UI follows the conventions of the programs users are replacing: Revit, AutoCAD and CYPE. Examples include Revit's two-letter keyboard shortcuts and AutoCAD's command line.
- **Deviations require a recorded decision.** Any UX pattern from elsewhere, including Blender, is adopted only when it is clearly superior, and only through an ADR approved by the project owner.

## Consequences

- Lower switching cost for the target users.
- Every UX innovation carries the burden of proof and leaves a written record.
- We still need our own UI framework layer, viewport and command system.
