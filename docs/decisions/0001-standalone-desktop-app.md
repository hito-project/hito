# 0001. Standalone desktop app with a Blender-like UI

- **Status:** Superseded by [0007](0007-ui-familiar-to-current-users.md)
- **Date:** 2026-10-08

## Context

The target users are civil engineers, some coming from Blender. Bonsai already provides BIM inside Blender, but it requires learning Blender itself first and inherits Blender's architecture and constraints. A web app is easier to distribute but has a lower performance ceiling for large models.

## Decision

Build a standalone, native desktop application whose interaction model mimics Blender (workspaces, operators, keyboard-first, F3 search) without depending on Blender.

## Consequences

- We control the architecture and the UX for BIM work.
- We must build our own UI framework layer, viewport and operator system.
- Blender users get a familiar experience, and newcomers don't have to learn Blender first.
