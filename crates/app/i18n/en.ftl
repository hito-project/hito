# User-facing text of the HITO window, in English.
# Every locale has the same messages; the names follow Revit's English UI
# (ADR 0007).

app-title = HITO

## Ribbon tabs and groups

ribbon-tab-structure = Structure
ribbon-tab-view = View
ribbon-tab-manage = Manage
ribbon-group-structure = Structure
ribbon-group-foundation = Foundation
ribbon-group-datum = Datum
ribbon-group-create = Create
ribbon-group-windows = Windows
ribbon-group-settings = Settings

## Commands

command-beam = Beam
command-wall = Wall
command-column = Column
command-slab = Slab
command-isolated-footing = Isolated
command-level = Level
command-grid = Grid
command-materials = Materials
command-project-units = Project Units
command-unavailable = Not available yet
# A ribbon button's tooltip, with the command's shortcut, as in Revit.
command-tooltip = { $command } ({ $alias })

## The command line, as in AutoCAD

command-line-label = Command
command-line-hint = Type a command
command-unknown = Unknown command: { $input }
command-not-available = { $command } is not available yet
# AutoCAD's message when Esc cancels a command.
command-cancelled = *Cancel*
command-failed = { $command } could not be completed
input-empty = Type an answer, or press Esc to cancel
# The command line reads numbers as AutoCAD does, in every language: a point
# is the decimal separator and a comma separates coordinates.
input-not-a-point = Enter a point as x,y or x,y,z, such as 3.5,2
input-not-a-value = Enter a value, such as 3.5 or 350 cm

## Levels

# New levels are named this word and a number, such as "Level 3". Names are
# data: they don't change when the language does.
level-name = Level
level-prompt-elevation = Enter the new level's elevation, such as 3.5 or 350 cm
level-name-empty = A level needs a name
level-name-taken = Another level is already called { $name }

## Grids

grid-prompt-start = Enter the grid's start point, such as 0,0
grid-prompt-end = Enter the grid's end point, such as 0,10
grid-points-differ = The end point must differ from the start point
grid-name-empty = A grid needs a label
grid-name-taken = Another grid is already labelled { $name }

## Window actions

action-new-3d-view = 3D View
action-close-inactive-views = Close Inactive
action-tab-views = Tab Views
action-tile-views = Tile Views
action-user-interface = User Interface
ui-theme = Theme
theme-light = Light
theme-dark = Dark
theme-system = Use system setting

## Panels

panel-project-browser = Project Browser
panel-properties = Properties

browser-views = Views (all)
browser-3d-views = 3D Views
browser-levels = Levels
browser-grids = Grids
browser-element-types = Element types
browser-none = (none)

properties-no-view = No view is open
properties-view-name = View Name
properties-kind-level = Level
properties-kind-grid = Grid
properties-label = Label
properties-name = Name
properties-elevation = Elevation
properties-edit-failed = The change could not be made

## Deleting elements

action-delete = Delete
action-cancel = Cancel
# Revit warns before deleting elements that others depend on.
delete-also = { $count ->
    [one] Deleting { $name } also deletes 1 element attached to it.
   *[other] Deleting { $name } also deletes { $count } elements attached to it.
}

## Views

view-kind-3d = 3D View
# Revit's name for the default 3D view, the same in every language.
view-default-3d = {"{"}3D{"}"}
view-3d-numbered = 3D View { $number }
viewport-no-gpu = The 3D view needs a graphics device
