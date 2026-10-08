---
title: "E05: Application shell and commands"
labels: type:epic, area:ui
milestone: "M1: Structural BIM MVP"
---

The main window and the command system. It follows the conventions of the programs users already know: Revit's layout and two-letter shortcuts, and AutoCAD's command line ([ADR 0007](docs/decisions/0007-ui-familiar-to-current-users.md)). The toolkit is chosen in the UI spike (E01).

## Story: Main window with familiar layout
labels: type:story, area:ui

As a Revit user, I want a main window with a tool ribbon, project browser, properties panel and view area, so that I can find my way around immediately.

### Acceptance criteria
- [ ] Ribbon-style toolbar grouped by discipline tabs (Structure, Views, Manage…)
- [ ] Project browser (levels, views, element types) and properties panel, both dockable
- [ ] Several views can be open as tabs or tiles
- [ ] The layout is remembered between sessions

## Story: Commands from the command line and shortcuts
labels: type:story, area:ui

As an AutoCAD or Revit user, I want to run any command by typing its name or a short alias, so that I can work quickly from the keyboard.

### Acceptance criteria
- [ ] Every action is a registered command with a name, a localised label and an optional alias
- [ ] A command line with autocomplete accepts command names and Revit-style two-letter aliases (for example `CL` for column)
- [ ] Commands can prompt for input (points, values) on the command line, AutoCAD style
- [ ] Every command can be cancelled with Esc

## Story: Customisable keyboard shortcuts
labels: type:story, area:ui

As a user, I want to change shortcuts and aliases, so that HITO matches my habits.

### Acceptance criteria
- [ ] A shortcut editor lists all commands with their shortcuts and aliases
- [ ] Conflicts are detected
- [ ] Shortcuts can be exported and imported

## Story: Edit element parameters in the properties panel
labels: type:story, area:ui

As a user, I want to see and edit the selected elements' type and instance parameters, so that I can change dimensions, materials and offsets precisely.

### Acceptance criteria
- [ ] The properties panel shows the parameters of the selection, grouped and with units
- [ ] Multi-selection shows shared parameters. Differing values appear as "varies".
- [ ] Editing a value creates one undoable transaction
- [ ] Type parameters can be edited through a type editor, with a warning that every instance is affected
