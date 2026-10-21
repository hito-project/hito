# User-facing text of the HITO window, in English.
# Every locale has the same messages; the names follow Revit's English UI
# (ADR 0007).

app-title = HITO

## Ribbon tabs and groups

ribbon-tab-structure = Structure
ribbon-tab-architecture = Architecture
ribbon-group-build = Build
ribbon-tab-view = View
ribbon-tab-manage = Manage
ribbon-group-structure = Structure
ribbon-group-opening = Opening
ribbon-group-foundation = Foundation
ribbon-group-datum = Datum
ribbon-group-create = Create
ribbon-group-windows = Windows
ribbon-group-settings = Settings

## Commands

command-beam = Beam
command-wall = Wall
command-door = Door
command-window = Window
command-column = Column
command-slab = Slab
command-vertical-opening = Vertical
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
input-not-a-pick = Click the element

## Levels

# New levels are named this word and a number, such as "Level 3". Names are
# data: they don't change when the language does.
level-name = Level
level-prompt-elevation = Enter the new level's elevation, such as 3.5 or 350 cm
level-name-empty = A level needs a name
level-name-taken = Another level is already called { $name }

# The Wall tool (WA). Points are clicked in a plan view or typed.
wall-prompt-start = Specify the wall's start point, such as 0,0
wall-prompt-next = Specify the next point, such as 5,0, @5,0 or @5<90. Esc ends the chain
wall-same-point = The next point must differ from the last one
# The wall type every new project starts with. It's data from then on.
wall-type-generic = Generic - 200 mm
# The generic concrete every new project starts with, with no strength: graded concrete comes with regional packs. It's data from then on.
material-concrete-generic = Concrete

## Slabs

# The Slab tool (SB) and the Vertical opening tool sketch a closed boundary.
# Points are clicked in a plan view or typed; walls and beams can be picked.
slab-prompt-start = Specify the slab boundary's first point, such as 0,0, or pick a wall or a beam
slab-prompt-next = Specify the next point, such as 5,0, @5,0 or @5<90, or pick a wall or a beam. Entering the first point again, or meeting the first line picked, finishes the slab
opening-prompt-slab = Pick the slab to cut the opening in
opening-pick-a-slab = Pick a slab
opening-prompt-start = Specify the opening's first point, inside the slab
opening-prompt-next = Specify the next point, such as 1,0, @1,0 or @1<90. Enter the first point again to finish the opening
sketch-same-point = The next point must differ from the last one
sketch-too-few = A boundary needs at least 3 points before it closes
sketch-crossing = The boundary can't cross or touch itself
sketch-picked-twice = That line is already in the boundary
slab-pick-wall-or-beam = Pick a wall or a beam
# The slab type every new project starts with. It's data from then on.
slab-type-generic = Generic - 150 mm

## Footings

# The Isolated footing tool. Centres are typed until plan views let them be clicked.
footing-prompt-centre = Specify the footing's centre, such as 3,4 or @6,0
# The footing type every new project starts with. It's data from then on.
footing-type-generic = Rectangular - 1800 x 1200 x 450 mm

# The Door (DR) and Window (WN) tools. The point is typed until plan views let it be clicked.
door-prompt = Specify a point on a wall, such as 2,0. The door swings to that side. Esc ends
window-prompt = Specify a point on a wall, such as 2,0. Esc ends
# The door and window types every new project starts with. They're data from then on.
door-type-generic = Single door - 800 × 2050 mm
window-type-generic = Window - 1500 × 1100 mm

## Grids

grid-prompt-start = Enter the grid's start point, such as 0,0
grid-prompt-end = Enter the grid's end point, such as 0,10
grid-points-differ = The end point must differ from the start point
grid-name-empty = A grid needs a label
grid-name-taken = Another grid is already labelled { $name }

## Columns

# The Column tool (CL). On the crossing of two grids, the column follows the grids.
column-prompt-point = Specify the column's centre, such as 0,0. Esc ends
# The column type every new project starts with. It's data from then on.
column-type-generic = Rectangular - 300 x 300 mm

## Beams

# The Beam tool (BM). An end on a column frames into it and follows it.
beam-prompt-start = Specify the beam's start point, such as 0,0
beam-prompt-end = Specify the beam's end point, such as 5,0, @5,0 or @5<90. Esc ends
beam-same-point = The end point must differ from the start point
# The beam type every new project starts with. It's data from then on.
beam-type-generic = Rectangular - 200 x 400 mm

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
ui-navigation = 3D navigation
navigation-wheel-orbits = Wheel orbits, Shift + wheel pans
navigation-revit = Revit: wheel pans, Shift + wheel orbits
action-zoom-to-fit = Zoom to Fit

## Panels

panel-project-browser = Project Browser
panel-properties = Properties

browser-views = Views (all)
browser-floor-plans = Floor Plans
browser-3d-views = 3D Views
browser-levels = Levels
browser-grids = Grids
browser-element-types = Element types
browser-none = (none)

properties-no-view = No view is open
properties-view-name = View Name
# Several selected elements of different classes, as Revit shows them.
properties-common = Common ({ $count })
properties-kind-level = Level
properties-kind-grid = Grid
properties-label = Label
properties-name = Name
properties-elevation = Elevation
properties-edit-failed = The change could not be made
# The type selector at the top of the panel, and the type editor.
properties-type = Type
# A value that differs between the selected elements.
properties-varies = (varies)
properties-edit-type = Edit Type
type-editor-title = Type Properties
type-editor-warning = { $count ->
    [one] Changes apply to the 1 element of this type.
   *[other] Changes apply to all { $count } elements of this type.
}
action-ok = OK
action-apply = Apply

# The selection's class, as the panel's title shows it.
class-wall = Wall
class-door = Door
class-window = Window
class-column = Column
class-beam = Beam
class-slab = Slab
class-slab-opening = Opening
class-footing = Isolated Footing

# Revit's parameter groups and parameters.
group-constraints = Constraints
group-construction = Construction
group-structural = Structural
group-dimensions = Dimensions
parameter-base-offset = Base Offset
parameter-top-offset = Top Offset
parameter-unconnected-height = Unconnected Height
parameter-structural = Structural
parameter-rotation = Rotation
parameter-level-offset = Level Offset
parameter-height-offset = Height Offset From Level
parameter-elevation-at-top = Elevation at Top
parameter-elevation-at-bottom = Elevation at Bottom
parameter-sill-height = Sill Height
parameter-section-width = Width (b)
parameter-section-depth = Depth (h)
parameter-diameter = Diameter
parameter-default-thickness = Default Thickness
parameter-length = Length
parameter-width = Width
parameter-height = Height
parameter-foundation-thickness = Foundation Thickness
parameter-default-sill-height = Default Sill Height
parameter-frame-width = Frame Width
parameter-frame-depth = Frame Depth

## Deleting elements

action-delete = Delete
action-cancel = Cancel
# Revit warns before deleting elements that others depend on.
delete-also = { $count ->
    [one] Deleting { $name } also deletes 1 element attached to it.
   *[other] Deleting { $name } also deletes { $count } elements attached to it.
}
# The same warning when several elements are selected.
delete-also-selected = { $count ->
    [one] Deleting the { $selected } selected elements also deletes 1 element attached to them.
   *[other] Deleting the { $selected } selected elements also deletes { $count } elements attached to them.
}

## Views

view-kind-3d = 3D View
view-kind-floor-plan = Floor Plan
# Revit's name for the default 3D view, the same in every language.
view-default-3d = {"{"}3D{"}"}
view-3d-numbered = 3D View { $number }
viewport-no-gpu = The 3D view needs a graphics device
# The view cube, as Revit's ViewCube.
view-cube-top = TOP
view-cube-bottom = BOTTOM
view-cube-front = FRONT
view-cube-back = BACK
view-cube-left = LEFT
view-cube-right = RIGHT
view-cube-home = Home
view-cube-go-home = Go Home
view-perspective = Perspective
view-orthographic = Orthographic
