//! What the properties panel shows and edits of the selected elements
//! (#26), as plain data the shell draws: each class's parameters, grouped
//! and labelled as Revit's properties palette shows them, the values a
//! selection shares, the choices a list offers, the types it can change
//! to, and how each change is made in a transaction.
//!
//! [ADR 0015](https://github.com/hito-project/hito/blob/main/docs/decisions/0015-ui-toolkit.md)
//! has property descriptions (name, value, unit, editor) and the shell
//! turning them into rows. The descriptions live here, next to the
//! commands, until a domain outside this repository needs to add its own.

use hito_architectural::wall::{self, LocationLine};
use hito_core::{
    ClassId, Dimension, Element, ElementId, OnDelete, Relationship, StoreError, StoreView,
    Transaction, Value,
};
use hito_structural::{column, footing, level, material, section, slab};

/// Revit's parameter groups, in the order its properties palette lists
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Constraints,
    Construction,
    MaterialsAndFinishes,
    Structural,
    Dimensions,
}

impl Group {
    /// The group's message key.
    pub fn label(self) -> &'static str {
        match self {
            Group::Constraints => "group-constraints",
            Group::Construction => "group-construction",
            Group::MaterialsAndFinishes => "group-materials-and-finishes",
            Group::Structural => "group-structural",
            Group::Dimensions => "group-dimensions",
        }
    }
}

/// Whose top level a [`Editor::TopLevel`] row sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopOf {
    Wall,
    Column,
}

/// How a parameter's value is shown and changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Editor {
    /// A quantity typed with or without units, such as `20 cm`.
    Quantity(Dimension),
    /// A check box.
    Boolean,
    /// Typed text, such as a type's name.
    Text,
    /// A wall's location line, from Revit's list.
    LocationLine,
    /// A level-based element's level, from the project's levels.
    Level,
    /// A wall's or a column's top level, or "Unconnected".
    TopLevel(TopOf),
    /// A type's structural material, through this structural
    /// relationship, or none.
    Material(&'static str),
}

impl Editor {
    /// Whether it's a list to pick from.
    pub fn is_list(self) -> bool {
        matches!(
            self,
            Editor::LocationLine | Editor::Level | Editor::TopLevel(_) | Editor::Material(_)
        )
    }
}

/// When a parameter is computed by the model, so the panel shows it
/// greyed out, as Revit does, instead of letting it be edited.
#[derive(Debug, Clone, Copy)]
pub enum Computed {
    Never,
    Always,
    /// When this says so of the element, such as a wall's height when
    /// its top is at a level.
    When(fn(&StoreView<'_>, ElementId) -> bool),
}

/// When the panel shows a parameter.
#[derive(Debug, Clone, Copy)]
pub enum Shown {
    Always,
    /// Only when the element has a value, as a section type's width or its
    /// diameter, depending on its shape.
    WhenSet,
    /// When this says so of every selected element, such as a column's
    /// offsets from its grids while it's on grids.
    When(fn(&StoreView<'_>, ElementId) -> bool),
}

/// One parameter as the panel shows it.
#[derive(Debug, Clone, Copy)]
pub struct Parameter {
    /// The property it shows, or for a list of elements, the relationship
    /// it sets. Two classes share a row when both this and the label match.
    pub property: &'static str,
    /// Its message key.
    pub label: &'static str,
    pub group: Group,
    pub editor: Editor,
    pub computed: Computed,
    /// When the panel shows it.
    pub shown: Shown,
}

/// Two parameters are the same row when they show the same property under
/// the same label.
impl PartialEq for Parameter {
    fn eq(&self, other: &Self) -> bool {
        self.property == other.property && self.label == other.label
    }
}

const LENGTH: Editor = Editor::Quantity(Dimension::LENGTH);
const ANGLE: Editor = Editor::Quantity(Dimension::ANGLE);

const fn editable(
    property: &'static str,
    label: &'static str,
    group: Group,
    editor: Editor,
) -> Parameter {
    Parameter {
        property,
        label,
        group,
        editor,
        computed: Computed::Never,
        shown: Shown::Always,
    }
}

const fn computed(property: &'static str, label: &'static str, group: Group) -> Parameter {
    Parameter {
        computed: Computed::Always,
        ..editable(property, label, group, LENGTH)
    }
}

const fn computed_when(
    property: &'static str,
    label: &'static str,
    group: Group,
    when: fn(&StoreView<'_>, ElementId) -> bool,
) -> Parameter {
    Parameter {
        computed: Computed::When(when),
        ..editable(property, label, group, LENGTH)
    }
}

const fn read_only_when(
    parameter: Parameter,
    when: fn(&StoreView<'_>, ElementId) -> bool,
) -> Parameter {
    Parameter {
        computed: Computed::When(when),
        ..parameter
    }
}

const fn shown_when(
    parameter: Parameter,
    when: fn(&StoreView<'_>, ElementId) -> bool,
) -> Parameter {
    Parameter {
        shown: Shown::When(when),
        ..parameter
    }
}

const fn when_set(parameter: Parameter) -> Parameter {
    Parameter {
        shown: Shown::WhenSet,
        ..parameter
    }
}

/// A level-based element's level, under the name Revit gives it for the
/// class.
const fn level_row(label: &'static str) -> Parameter {
    editable(
        level::ATTACHED_TO_LEVEL,
        label,
        Group::Constraints,
        Editor::Level,
    )
}

const fn material_row(relationship: &'static str) -> Parameter {
    editable(
        relationship,
        "parameter-structural-material",
        Group::MaterialsAndFinishes,
        Editor::Material(relationship),
    )
}

/// A type's name, which the type editor's Rename changes.
pub const TYPE_NAME: Parameter = editable(
    slab::NAME,
    "properties-type",
    Group::Constraints,
    Editor::Text,
);

/// The parameters of one class, and of its subclasses that have none of
/// their own here.
struct Described {
    schema: &'static str,
    class: &'static str,
    parameters: &'static [Parameter],
}

const STRUCTURAL: &str = hito_structural::SCHEMA;
const ARCHITECTURAL: &str = hito_architectural::SCHEMA;

/// Whether an element has a relationship of this class to something.
fn has(view: &StoreView<'_>, id: ElementId, class: &ClassId) -> bool {
    view.relationships_from(id).any(|r| r.class() == class)
}

fn column_has_top_level(view: &StoreView<'_>, id: ElementId) -> bool {
    column::top_level(view, id).is_some()
}

fn column_has_no_top_level(view: &StoreView<'_>, id: ElementId) -> bool {
    !column_has_top_level(view, id)
}

fn wall_top_is_fixed(view: &StoreView<'_>, id: ElementId) -> bool {
    wall::top_level(view, id).is_some() || wall::top_is_attached(view, id)
}

fn wall_has_no_top_level(view: &StoreView<'_>, id: ElementId) -> bool {
    wall::top_level(view, id).is_none()
}

fn wall_top_is_attached(view: &StoreView<'_>, id: ElementId) -> bool {
    wall::top_is_attached(view, id)
}

/// A wall whose base stands on a slab, so its level doesn't place it.
fn wall_base_is_attached(view: &StoreView<'_>, id: ElementId) -> bool {
    let attached = hito_architectural::class(wall::BASE_ATTACHED_TO);
    has(view, id, &attached)
}

/// A column on two grids: its X and Y follow them, and its offsets from
/// their intersection place it (#215 decision D).
fn column_on_grids(view: &StoreView<'_>, id: ElementId) -> bool {
    column::grids(view, id).is_some()
}

/// A footing under a column: it follows the column.
fn footing_under_column(view: &StoreView<'_>, id: ElementId) -> bool {
    footing::column(view, id).is_some()
}

/// Instance and type parameters, as Revit names, orders and groups them.
/// Levels and grids keep their own rows (`shell::elements`), which check
/// their names are unique.
const DESCRIBED: &[Described] = &[
    Described {
        schema: ARCHITECTURAL,
        class: wall::WALL,
        parameters: &[
            editable(
                wall::LOCATION_LINE,
                "parameter-location-line",
                Group::Constraints,
                Editor::LocationLine,
            ),
            read_only_when(
                level_row("parameter-base-constraint"),
                wall_base_is_attached,
            ),
            editable(
                wall::BASE_OFFSET,
                "parameter-base-offset",
                Group::Constraints,
                LENGTH,
            ),
            read_only_when(
                editable(
                    wall::TOP_LEVEL,
                    "parameter-top-constraint",
                    Group::Constraints,
                    Editor::TopLevel(TopOf::Wall),
                ),
                wall_top_is_attached,
            ),
            computed_when(
                wall::HEIGHT,
                "parameter-unconnected-height",
                Group::Constraints,
                wall_top_is_fixed,
            ),
            computed_when(
                wall::TOP_OFFSET,
                "parameter-top-offset",
                Group::Constraints,
                wall_has_no_top_level,
            ),
            editable(
                wall::STRUCTURAL,
                "parameter-structural",
                Group::Structural,
                Editor::Boolean,
            ),
        ],
    },
    Described {
        schema: STRUCTURAL,
        class: column::COLUMN,
        parameters: &[
            level_row("parameter-base-level"),
            editable(
                column::BASE_OFFSET,
                "parameter-base-offset",
                Group::Constraints,
                LENGTH,
            ),
            editable(
                column::TOP_LEVEL,
                "parameter-top-level",
                Group::Constraints,
                Editor::TopLevel(TopOf::Column),
            ),
            computed_when(
                column::TOP_OFFSET,
                "parameter-top-offset",
                Group::Constraints,
                column_has_no_top_level,
            ),
            computed_when(
                column::HEIGHT,
                "parameter-unconnected-height",
                Group::Constraints,
                column_has_top_level,
            ),
            editable(
                column::ROTATION,
                "parameter-rotation",
                Group::Constraints,
                ANGLE,
            ),
            computed_when(
                column::X,
                "parameter-x",
                Group::Constraints,
                column_on_grids,
            ),
            computed_when(
                column::Y,
                "parameter-y",
                Group::Constraints,
                column_on_grids,
            ),
            shown_when(
                editable(
                    column::GRID_OFFSET_X,
                    "parameter-grid-offset-x",
                    Group::Constraints,
                    LENGTH,
                ),
                column_on_grids,
            ),
            shown_when(
                editable(
                    column::GRID_OFFSET_Y,
                    "parameter-grid-offset-y",
                    Group::Constraints,
                    LENGTH,
                ),
                column_on_grids,
            ),
        ],
    },
    Described {
        schema: STRUCTURAL,
        class: hito_structural::beam::BEAM,
        parameters: &[
            level_row("parameter-reference-level"),
            editable(
                level::LEVEL_OFFSET,
                "parameter-level-offset",
                Group::Constraints,
                LENGTH,
            ),
        ],
    },
    Described {
        schema: STRUCTURAL,
        class: slab::SLAB,
        parameters: &[
            level_row("parameter-level"),
            editable(
                slab::HEIGHT_OFFSET,
                "parameter-height-offset",
                Group::Constraints,
                LENGTH,
            ),
            computed(
                slab::TOP_ELEVATION,
                "parameter-elevation-at-top",
                Group::Dimensions,
            ),
            computed(
                slab::BOTTOM_ELEVATION,
                "parameter-elevation-at-bottom",
                Group::Dimensions,
            ),
        ],
    },
    Described {
        schema: STRUCTURAL,
        class: footing::FOOTING,
        parameters: &[
            read_only_when(level_row("parameter-level"), footing_under_column),
            computed_when(
                footing::HEIGHT_OFFSET,
                "parameter-height-offset",
                Group::Constraints,
                footing_under_column,
            ),
            editable(
                footing::ROTATION,
                "parameter-rotation",
                Group::Constraints,
                ANGLE,
            ),
            computed_when(
                footing::X,
                "parameter-x",
                Group::Constraints,
                footing_under_column,
            ),
            computed_when(
                footing::Y,
                "parameter-y",
                Group::Constraints,
                footing_under_column,
            ),
            computed(
                footing::TOP_ELEVATION,
                "parameter-elevation-at-top",
                Group::Dimensions,
            ),
            computed(
                footing::BOTTOM_ELEVATION,
                "parameter-elevation-at-bottom",
                Group::Dimensions,
            ),
        ],
    },
    Described {
        schema: ARCHITECTURAL,
        class: hito_architectural::opening::OPENING,
        parameters: &[editable(
            hito_architectural::opening::SILL_HEIGHT,
            "parameter-sill-height",
            Group::Constraints,
            LENGTH,
        )],
    },
    // Types, edited in the type editor.
    Described {
        schema: STRUCTURAL,
        class: section::SECTION_TYPE,
        parameters: &[
            material_row(section::SECTION_MATERIAL),
            when_set(editable(
                section::WIDTH,
                "parameter-section-width",
                Group::Dimensions,
                LENGTH,
            )),
            when_set(editable(
                section::DEPTH,
                "parameter-section-depth",
                Group::Dimensions,
                LENGTH,
            )),
            when_set(editable(
                section::DIAMETER,
                "parameter-diameter",
                Group::Dimensions,
                LENGTH,
            )),
        ],
    },
    Described {
        schema: STRUCTURAL,
        class: slab::SLAB_TYPE,
        parameters: &[
            editable(
                slab::THICKNESS,
                "parameter-default-thickness",
                Group::Construction,
                LENGTH,
            ),
            material_row(slab::SLAB_MATERIAL),
        ],
    },
    Described {
        schema: STRUCTURAL,
        class: footing::FOOTING_TYPE,
        parameters: &[
            material_row(footing::FOOTING_MATERIAL),
            editable(
                footing::LENGTH,
                "parameter-length",
                Group::Dimensions,
                LENGTH,
            ),
            editable(footing::WIDTH, "parameter-width", Group::Dimensions, LENGTH),
            editable(
                footing::THICKNESS,
                "parameter-foundation-thickness",
                Group::Dimensions,
                LENGTH,
            ),
        ],
    },
    Described {
        schema: ARCHITECTURAL,
        class: hito_architectural::opening_type::OPENING_TYPE,
        parameters: &[
            editable(
                hito_architectural::opening_type::WIDTH,
                "parameter-width",
                Group::Dimensions,
                LENGTH,
            ),
            editable(
                hito_architectural::opening_type::HEIGHT,
                "parameter-height",
                Group::Dimensions,
                LENGTH,
            ),
            editable(
                hito_architectural::opening_type::SILL_HEIGHT,
                "parameter-default-sill-height",
                Group::Constraints,
                LENGTH,
            ),
            editable(
                hito_architectural::opening_type::FRAME_WIDTH,
                "parameter-frame-width",
                Group::Construction,
                LENGTH,
            ),
            editable(
                hito_architectural::opening_type::FRAME_DEPTH,
                "parameter-frame-depth",
                Group::Construction,
                LENGTH,
            ),
        ],
    },
];

/// Each class's name as the panel's title shows it, by message key.
const CLASS_LABELS: &[(&str, &str, &str)] = &[
    (ARCHITECTURAL, wall::WALL, "class-wall"),
    (
        ARCHITECTURAL,
        hito_architectural::opening::DOOR,
        "class-door",
    ),
    (
        ARCHITECTURAL,
        hito_architectural::opening::WINDOW,
        "class-window",
    ),
    (STRUCTURAL, column::COLUMN, "class-column"),
    (STRUCTURAL, hito_structural::beam::BEAM, "class-beam"),
    (STRUCTURAL, slab::SLAB, "class-slab"),
    (STRUCTURAL, slab::SLAB_OPENING, "class-slab-opening"),
    (STRUCTURAL, footing::FOOTING, "class-footing"),
    (STRUCTURAL, level::LEVEL, "properties-kind-level"),
    (
        STRUCTURAL,
        hito_structural::grid::GRID,
        "properties-kind-grid",
    ),
];

/// The nearest of `class` and its base classes that `find` finds
/// something for, and what it found.
fn nearest<T>(
    view: &StoreView<'_>,
    class: &ClassId,
    find: impl Fn(&ClassId) -> Option<T>,
) -> Option<T> {
    let registry = view.registry();
    let mut current = Some(class);
    while let Some(id) = current {
        if let Some(found) = find(id) {
            return Some(found);
        }
        current = registry.class(id).and_then(|c| c.base());
    }
    None
}

/// The message key of `class`'s name, or of its nearest base class's that
/// has one.
pub fn class_label(view: &StoreView<'_>, class: &ClassId) -> Option<&'static str> {
    nearest(view, class, |id| {
        let found = CLASS_LABELS
            .iter()
            .find(|(schema, name, _)| id.schema() == *schema && id.name() == *name);
        found.map(|(_, _, key)| *key)
    })
}

/// The parameters of `class`: those described for it, or for its nearest
/// base class that has any. In group order, and in the order they're
/// described within a group.
pub fn of_class(view: &StoreView<'_>, class: &ClassId) -> Vec<&'static Parameter> {
    let described = nearest(view, class, |id| {
        DESCRIBED
            .iter()
            .find(|d| id.schema() == d.schema && id.name() == d.class)
    });
    let mut parameters: Vec<_> = described.map_or(&[][..], |d| d.parameters).iter().collect();
    parameters.sort_by_key(|p| p.group);
    parameters
}

/// A value that several elements share, or don't.
#[derive(Debug, Clone, PartialEq)]
pub enum Shared<T> {
    /// Every element has this one. `None` means none of them has a value.
    Same(Option<T>),
    /// They differ: the panel says "varies".
    Varies,
}

impl<T: PartialEq> Shared<T> {
    /// What the values have in common.
    fn of(mut values: impl Iterator<Item = Option<T>>) -> Self {
        let Some(first) = values.next() else {
            return Shared::Same(None);
        };
        match values.all(|v| v == first) {
            true => Shared::Same(first),
            false => Shared::Varies,
        }
    }
}

/// A parameter's value for one element.
#[derive(Debug, Clone, PartialEq)]
pub enum Current {
    /// A property's value.
    Value(Value),
    /// The element a list picks, such as a level, or none, such as an
    /// unconnected top or no material.
    Element(Option<ElementId>),
    LocationLine(LocationLine),
}

impl Current {
    pub fn real(&self) -> Option<f64> {
        match self {
            Current::Value(Value::Real(x)) => Some(*x),
            _ => None,
        }
    }
}

/// `parameter`'s value for `element`, if it has one.
pub fn current(view: &StoreView<'_>, element: ElementId, parameter: &Parameter) -> Option<Current> {
    let structural = hito_structural::class;
    Some(match parameter.editor {
        Editor::Quantity(_) | Editor::Boolean | Editor::Text => {
            Current::Value(view.property(element, parameter.property)?.clone())
        }
        Editor::LocationLine => Current::LocationLine(wall::location_line(view, element)),
        Editor::Level => Current::Element(level::level_of(view, element)),
        Editor::TopLevel(TopOf::Wall) => Current::Element(wall::top_level(view, element)),
        Editor::TopLevel(TopOf::Column) => Current::Element(column::top_level(view, element)),
        Editor::Material(relationship) => {
            let class = structural(relationship);
            let target = view
                .relationships_from(element)
                .find(|r| r.class() == &class)
                .map(Relationship::target);
            Current::Element(target)
        }
    })
}

/// How a choice in a list is labelled.
#[derive(Debug, Clone, PartialEq)]
pub enum Label {
    /// An element's name, such as a level's.
    Text(String),
    /// A message.
    Key(&'static str),
    /// Revit's "Up to level: Level 2", for a wall's top.
    UpToLevel(String),
}

fn name(view: &StoreView<'_>, id: ElementId) -> String {
    match view.property(id, level::NAME) {
        Some(Value::Text(name)) => name.clone(),
        _ => String::new(),
    }
}

/// The choices a list parameter offers, in Revit's order, with the value
/// each one sets. Empty for a parameter that isn't a list.
pub fn choices(view: &StoreView<'_>, parameter: &Parameter) -> Vec<(Current, Label)> {
    let levels = || level::levels(view).into_iter().map(|(id, _)| id);
    match parameter.editor {
        Editor::Quantity(_) | Editor::Boolean | Editor::Text => Vec::new(),
        Editor::LocationLine => LocationLine::ALL
            .into_iter()
            .map(|line| {
                (
                    Current::LocationLine(line),
                    Label::Key(location_line_key(line)),
                )
            })
            .collect(),
        Editor::Level => levels()
            .map(|l| (Current::Element(Some(l)), Label::Text(name(view, l))))
            .collect(),
        Editor::TopLevel(of) => {
            let unconnected = (Current::Element(None), Label::Key("parameter-unconnected"));
            let level = |l| match of {
                TopOf::Wall => Label::UpToLevel(name(view, l)),
                TopOf::Column => Label::Text(name(view, l)),
            };
            let levels = levels().map(|l| (Current::Element(Some(l)), level(l)));
            std::iter::once(unconnected).chain(levels).collect()
        }
        Editor::Material(_) => {
            let registry = view.registry();
            let class = hito_structural::class(material::MATERIAL);
            let mut materials: Vec<_> = view
                .elements()
                .filter(|e| registry.is_a(e.class(), &class))
                .map(|e| (e.id(), name(view, e.id())))
                .collect();
            materials.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
            let none = (Current::Element(None), Label::Key("browser-none"));
            let materials = materials
                .into_iter()
                .map(|(id, name)| (Current::Element(Some(id)), Label::Text(name)));
            std::iter::once(none).chain(materials).collect()
        }
    }
}

/// The message key of a location line, as Revit's list names it.
fn location_line_key(line: LocationLine) -> &'static str {
    match line {
        LocationLine::WallCenterline => "location-line-wall-centerline",
        LocationLine::CoreCenterline => "location-line-core-centerline",
        LocationLine::FinishFaceExterior => "location-line-finish-face-exterior",
        LocationLine::FinishFaceInterior => "location-line-finish-face-interior",
        LocationLine::CoreFaceExterior => "location-line-core-face-exterior",
        LocationLine::CoreFaceInterior => "location-line-core-face-interior",
    }
}

/// Sets `parameter` of `element` to `value` in a transaction, through the
/// domain's operation for it: a level changes the element's level, a wall's
/// location line moves across the wall and leaves it in place, and so on.
pub fn apply(
    tx: &mut Transaction<'_>,
    element: ElementId,
    parameter: &Parameter,
    value: &Current,
) -> Result<(), StoreError> {
    let class = tx.view().get(element).map(|e| e.class().clone());
    let wrong = move || StoreError::WrongValueType {
        class: class.unwrap_or(ClassId::ELEMENT),
        property: parameter.property.to_owned(),
    };
    match (parameter.editor, value) {
        (Editor::Quantity(_) | Editor::Boolean | Editor::Text, Current::Value(value)) => {
            tx.set_property(element, parameter.property, value.clone())
        }
        (Editor::LocationLine, Current::LocationLine(line)) => {
            wall::set_location_line(tx, element, *line)
        }
        (Editor::Level, Current::Element(Some(level))) => level::set_level(tx, element, *level),
        (Editor::TopLevel(TopOf::Wall), Current::Element(level)) => {
            wall::set_top(tx, element, *level)
        }
        (Editor::TopLevel(TopOf::Column), Current::Element(level)) => {
            column::set_top(tx, element, *level)
        }
        (Editor::Material(relationship), Current::Element(material)) => {
            let class = hito_structural::class(relationship);
            level::relate_only(tx, element, &class, *material)
        }
        _ => Err(wrong()),
    }
}

/// One row of the panel: a parameter, the selection's value of it, and
/// whether it can be edited.
#[derive(Debug, Clone)]
pub struct Row {
    pub parameter: &'static Parameter,
    pub value: Shared<Current>,
    /// Computed for at least one of the elements, so shown read-only.
    pub computed: bool,
}

/// The parameters every one of `elements` has, with their values: what
/// Revit's palette shows for a selection. A parameter is shared when it
/// has the same property and label in every element's class.
pub fn rows(view: &StoreView<'_>, elements: &[ElementId]) -> Vec<Row> {
    let classes: Vec<_> = elements
        .iter()
        .filter_map(|&id| Some((id, view.get(id)?.class())))
        .collect();
    let Some((_, first)) = classes.first() else {
        return Vec::new();
    };
    let others: Vec<_> = classes[1..]
        .iter()
        .map(|(_, class)| of_class(view, class))
        .collect();
    of_class(view, first)
        .into_iter()
        .filter(|p| {
            others.iter().all(|parameters| {
                parameters
                    .iter()
                    .any(|o| o.property == p.property && o.label == p.label)
            })
        })
        .map(|parameter| {
            let ids = || classes.iter().map(|(id, _)| *id);
            let value = Shared::of(ids().map(|id| current(view, id, parameter)));
            let computed = ids().any(|id| match parameter.computed {
                Computed::Never => false,
                Computed::Always => true,
                Computed::When(when) => when(view, id),
            });
            Row {
                parameter,
                value,
                computed,
            }
        })
        .filter(|row| match row.parameter.shown {
            Shown::Always => true,
            Shown::WhenSet => row.value != Shared::Same(None),
            Shown::When(when) => classes.iter().all(|(id, _)| when(view, *id)),
        })
        .collect()
}

/// What the type selector offers for a selection.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeChoice {
    /// The selection's type, or [`Shared::Varies`] if the elements have
    /// different ones.
    pub current: Shared<ElementId>,
    /// Every type all the selected elements can take, by name, with its
    /// name.
    pub types: Vec<(ElementId, String)>,
}

/// A type's name, or an empty string if it has none.
pub fn type_name(view: &StoreView<'_>, type_id: ElementId) -> String {
    name(view, type_id)
}

/// The types `elements` can change to, as Revit's type selector lists
/// them: the types every one of them can take, which are those of a class
/// that is, or derives from, each element's class's type class. `None` if
/// one of them takes no type, or there is nothing selected.
pub fn type_choice(view: &StoreView<'_>, elements: &[ElementId]) -> Option<TypeChoice> {
    let registry = view.registry();
    let selected: Vec<_> = elements.iter().filter_map(|&id| view.get(id)).collect();
    let type_classes: Vec<_> = selected
        .iter()
        .map(|e| registry.class(e.class())?.type_class())
        .collect::<Option<_>>()?;
    if type_classes.is_empty() {
        return None;
    }
    let mut types: Vec<_> = view
        .elements()
        .filter(|t| type_classes.iter().all(|tc| registry.is_a(t.class(), tc)))
        .map(|t| (t.id(), type_name(view, t.id())))
        .collect();
    types.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    Some(TypeChoice {
        current: Shared::of(selected.iter().map(|e| e.type_id())),
        types,
    })
}

/// How many elements have `type_id` as their type: whom editing the type
/// changes.
pub fn instance_count(view: &StoreView<'_>, type_id: ElementId) -> usize {
    view.elements()
        .filter(|e| e.type_id() == Some(type_id))
        .count()
}

/// Why a name typed for a type can't be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameProblem {
    Empty,
    /// Another type of the same class has it, as Revit refuses.
    Taken(String),
}

/// Checks a name typed for a copy of `type_id`, or for `type_id` itself
/// when `renaming`. Returns it trimmed.
pub fn check_type_name(
    view: &StoreView<'_>,
    type_id: ElementId,
    text: &str,
    renaming: bool,
) -> Result<String, NameProblem> {
    let text = text.trim();
    if text.is_empty() {
        return Err(NameProblem::Empty);
    }
    let class = view.get(type_id).map(|e| e.class().clone());
    let taken = view.elements().any(|e| {
        Some(e.class()) == class.as_ref()
            && !(renaming && e.id() == type_id)
            && name(view, e.id()) == text
    });
    match taken {
        true => Err(NameProblem::Taken(text.to_owned())),
        false => Ok(text.to_owned()),
    }
}

/// The name Revit's Duplicate suggests for a copy of `type_id`: its name
/// followed by the first number from 2 that no type of its class has.
pub fn copy_name(view: &StoreView<'_>, type_id: ElementId) -> String {
    let original = name(view, type_id);
    (2..)
        .map(|n| format!("{original} {n}"))
        .find(|candidate| check_type_name(view, type_id, candidate, false).is_ok())
        .expect("some number is free")
}

/// Copies a type under a new name, as Revit's Duplicate does: its values,
/// what it refers to (such as its material) and the parts it owns (such as
/// a wall type's layers, with their materials). Returns the copy.
pub fn duplicate_type(
    tx: &mut Transaction<'_>,
    type_id: ElementId,
    new_name: &str,
) -> Result<ElementId, StoreError> {
    let copy = copy_element(tx, type_id)?;
    tx.set_property(copy, level::NAME, new_name)?;
    let view = tx.view();
    let owned: Vec<_> = view
        .relationships_to(type_id)
        .filter(|r| {
            let def = view.registry().relationship(r.class());
            def.is_some_and(|d| d.delete_rule() == OnDelete::Cascade)
        })
        .cloned()
        .collect();
    for part in owned {
        let part_copy = copy_element(tx, part.source())?;
        tx.relate(Relationship::new(part.class().clone(), part_copy, copy))?;
    }
    Ok(copy)
}

/// Inserts a copy of an element, with a new ID and its values, and gives
/// it the element's relationships to others, except those to what it
/// belongs to, which the caller relates the copy to.
fn copy_element(tx: &mut Transaction<'_>, id: ElementId) -> Result<ElementId, StoreError> {
    let view = tx.view();
    let original = view.get(id).ok_or(StoreError::UnknownElement(id))?;
    let mut copy = Element::new(original.class().clone(), original.model());
    for (property, value) in original.properties() {
        copy = copy.with_property(property.clone(), value.clone());
    }
    if let Some(type_id) = original.type_id() {
        copy = copy.with_type(type_id);
    }
    let registry = view.registry();
    let refers: Vec<_> = view
        .relationships_from(id)
        .filter(|r| {
            let def = registry.relationship(r.class());
            def.is_none_or(|d| d.delete_rule() != OnDelete::Cascade)
        })
        .cloned()
        .collect();
    let copy = tx.insert(copy)?;
    for r in refers {
        tx.relate(Relationship::new(r.class().clone(), copy, r.target()))?;
    }
    Ok(copy)
}

#[cfg(test)]
mod tests {
    use hito_core::{ElementStore, ModelKind, SchemaRegistry};
    use hito_structural::class;
    use hito_structural::column::Placement;
    use hito_structural::section::Section;

    use super::*;
    use crate::i18n::{I18n, Locale};
    use crate::project;

    /// A new project, as the shell makes one, with Level 2 3 m above
    /// Level 1 and two 3 m columns of the generic type on Level 1, the
    /// second 20 cm above it.
    struct Fixture {
        store: ElementStore,
        columns: [ElementId; 2],
        column_type: ElementId,
        base: ElementId,
        upper: ElementId,
    }

    fn all_of(store: &ElementStore, schema: &str, name: &str) -> Vec<ElementId> {
        let registry = store.registry();
        let class = ClassId::new(schema, name);
        let mut ids: Vec<_> = store
            .elements()
            .filter(|e| registry.is_a(e.class(), &class))
            .map(|e| e.id())
            .collect();
        ids.sort();
        ids
    }

    fn fixture() -> Fixture {
        let i18n = I18n::new(Locale::En);
        let mut schemas = SchemaRegistry::new();
        hito_structural::register(&mut schemas).unwrap();
        hito_architectural::register(&mut schemas).unwrap();
        let mut store = project::new_project(schemas, &i18n);
        let column_type = all_of(&store, STRUCTURAL, column::COLUMN_TYPE)[0];
        let base = level::levels(&store.view())[0].0;
        let mut tx = store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let upper = Element::new(class(level::LEVEL), model)
            .with_property(level::NAME, "Level 2")
            .with_property(level::ELEVATION, 3.0);
        let upper = tx.insert(upper).unwrap();
        let placement = |x| Placement::new(column_type, base, [x, 0.0]);
        let first = column::insert(&mut tx, model, &placement(0.0)).unwrap();
        let raised = placement(5.0).with_base_offset(0.2);
        let second = column::insert(&mut tx, model, &raised).unwrap();
        tx.commit().unwrap();
        Fixture {
            store,
            columns: [first, second],
            column_type,
            base,
            upper,
        }
    }

    fn row<'a>(rows: &'a [Row], label: &str) -> &'a Row {
        let found = rows.iter().find(|r| r.parameter.label == label);
        found.unwrap_or_else(|| panic!("no {label} row"))
    }

    fn real(x: f64) -> Shared<Current> {
        Shared::Same(Some(Current::Value(Value::Real(x))))
    }

    #[test]
    fn a_column_shows_its_constraints_with_their_values() {
        let f = fixture();
        let rows = rows(&f.store.view(), &f.columns[..1]);
        let labels: Vec<_> = rows.iter().map(|r| r.parameter.label).collect();
        assert_eq!(
            labels,
            [
                "parameter-base-level",
                "parameter-base-offset",
                "parameter-top-level",
                "parameter-top-offset",
                "parameter-unconnected-height",
                "parameter-rotation",
                "parameter-x",
                "parameter-y",
            ]
        );
        assert!(!row(&rows, "parameter-x").computed, "a free column moves");
        let height = row(&rows, "parameter-unconnected-height");
        assert_eq!(height.value, real(3.0));
        assert!(!height.computed, "an unconnected column's height is typed");
        assert!(row(&rows, "parameter-top-offset").computed, "unused");
        let base = row(&rows, "parameter-base-level");
        assert_eq!(
            base.value,
            Shared::Same(Some(Current::Element(Some(f.base))))
        );
        let top = row(&rows, "parameter-top-level");
        assert_eq!(top.value, Shared::Same(Some(Current::Element(None))));
    }

    #[test]
    fn values_that_differ_vary_and_computed_ones_are_read_only() {
        let mut f = fixture();
        let both = rows(&f.store.view(), &f.columns);
        let offset = row(&both, "parameter-base-offset");
        assert_eq!(offset.value, Shared::Varies);
        let height = row(&both, "parameter-unconnected-height");
        assert_eq!(height.value, real(3.0));

        // With a top level, the height is computed and the top offset used.
        let top = row(&both, "parameter-top-level").parameter;
        let mut tx = f.store.transaction();
        apply(&mut tx, f.columns[0], top, &Current::Element(Some(f.upper))).unwrap();
        tx.commit().unwrap();
        let view = f.store.view();
        let both = rows(&view, &f.columns);
        assert!(row(&both, "parameter-unconnected-height").computed);
        assert_eq!(row(&both, "parameter-top-level").value, Shared::Varies);
        let offset = row(&both, "parameter-top-offset");
        assert!(offset.computed, "still unused by the second column");
        let first = rows(&view, &f.columns[..1]);
        assert!(!row(&first, "parameter-top-offset").computed);
    }

    #[test]
    fn lists_offer_the_levels_and_set_them() {
        let mut f = fixture();
        let rows = rows(&f.store.view(), &f.columns[..1]);
        let base = row(&rows, "parameter-base-level").parameter;
        let top = row(&rows, "parameter-top-level").parameter;
        let view = f.store.view();
        let labels = |p| -> Vec<_> { choices(&view, p).into_iter().map(|(_, l)| l).collect() };
        let names = ["Level 1", "Level 2"].map(|n| Label::Text(n.into()));
        assert_eq!(labels(base), names);
        let mut tops = vec![Label::Key("parameter-unconnected")];
        tops.extend(names.clone());
        assert_eq!(labels(top), tops);

        let mut tx = f.store.transaction();
        apply(
            &mut tx,
            f.columns[0],
            base,
            &Current::Element(Some(f.upper)),
        )
        .unwrap();
        tx.commit().unwrap();
        let view = f.store.view();
        assert_eq!(level::level_of(&view, f.columns[0]), Some(f.upper));
        assert_eq!(column::elevations(&view, f.columns[0]), Some((3.0, 6.0)));
    }

    #[test]
    fn a_wall_lists_its_location_line_and_top_constraint_as_revit_does() {
        let mut f = fixture();
        let wall_type = all_of(
            &f.store,
            ARCHITECTURAL,
            hito_architectural::wall_type::WALL_TYPE,
        )[0];
        let mut tx = f.store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let placement = wall::Placement::new(wall_type, f.base, [0.0, 5.0], [5.0, 5.0]);
        let id = wall::insert(&mut tx, model, &placement).unwrap();
        tx.commit().unwrap();
        let view = f.store.view();
        let rows = rows(&view, &[id]);
        let labels: Vec<_> = rows.iter().map(|r| r.parameter.label).collect();
        assert_eq!(
            labels[..4],
            [
                "parameter-location-line",
                "parameter-base-constraint",
                "parameter-base-offset",
                "parameter-top-constraint",
            ]
        );
        let top = row(&rows, "parameter-top-constraint").parameter;
        let tops = choices(&view, top);
        assert_eq!(tops[1].1, Label::UpToLevel("Level 1".into()));
        let line = row(&rows, "parameter-location-line");
        let centre = Current::LocationLine(LocationLine::WallCenterline);
        assert_eq!(line.value, Shared::Same(Some(centre)));
        assert_eq!(choices(&view, line.parameter).len(), 6);
    }

    #[test]
    fn a_column_on_grids_moves_by_its_offsets_from_them() {
        use hito_structural::grid;
        let mut f = fixture();
        let mut tx = f.store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let line = |name: &str, [x0, y0, x1, y1]: [f64; 4]| {
            Element::new(class(grid::GRID), model)
                .with_property(grid::NAME, name)
                .with_property(grid::START_X, x0)
                .with_property(grid::START_Y, y0)
                .with_property(grid::END_X, x1)
                .with_property(grid::END_Y, y1)
        };
        let a = tx.insert(line("A", [0.0, -5.0, 0.0, 5.0])).unwrap();
        let one = tx.insert(line("1", [-5.0, 0.0, 5.0, 0.0])).unwrap();
        let placement = Placement::new(f.column_type, f.base, [0.0, 0.0]).on_grids([a, one]);
        let on_grids = column::insert(&mut tx, model, &placement).unwrap();
        tx.commit().unwrap();

        let view = f.store.view();
        let rows = rows(&view, &[on_grids]);
        assert!(row(&rows, "parameter-x").computed, "#215 decision D");
        assert!(!row(&rows, "parameter-grid-offset-x").computed);
        // A free column has no offsets from grids.
        let free = super::rows(&view, &f.columns[..1]);
        let has = |rows: &[Row], label| rows.iter().any(|r| r.parameter.label == label);
        assert!(!has(&free, "parameter-grid-offset-x"));
        // Nor do they show when only some of the selection is on grids.
        let both = super::rows(&view, &[on_grids, f.columns[0]]);
        assert!(!has(&both, "parameter-grid-offset-y"));
    }

    #[test]
    fn a_parameter_is_shared_only_under_the_same_name() {
        let mut f = fixture();
        let slab_type = all_of(&f.store, STRUCTURAL, slab::SLAB_TYPE)[0];
        let mut tx = f.store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let outline = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let placement = slab::Placement::new(slab_type, f.base, &outline);
        let slab = slab::insert(&mut tx, model, &placement).unwrap();
        tx.commit().unwrap();

        // A column's Base Offset and a slab's Height Offset From Level are
        // the same property, but Revit names them differently.
        let rows = rows(&f.store.view(), &[f.columns[0], slab]);
        assert!(rows.is_empty(), "{rows:?}");
    }

    fn definitions(store: &ElementStore) -> hito_core::ModelId {
        let model = store.models().find(|m| m.kind() == ModelKind::Definition);
        model.unwrap().id()
    }

    #[test]
    fn the_type_selector_lists_the_types_the_selection_can_take() {
        let mut f = fixture();
        let definitions = definitions(&f.store);
        let mut tx = f.store.transaction();
        let round = Section::Circle { diameter: 0.4 };
        let (kind, name) = (column::COLUMN_TYPE, "Round");
        let round = section::insert(&mut tx, definitions, kind, name, round, None).unwrap();
        tx.set_type(f.columns[1], round).unwrap();
        tx.commit().unwrap();
        let view = f.store.view();

        let choice = type_choice(&view, &f.columns).unwrap();
        assert_eq!(choice.current, Shared::Varies);
        let types: Vec<_> = choice.types.iter().map(|(id, _)| *id).collect();
        assert_eq!(types.len(), 2, "only column types: {:?}", choice.types);
        assert!(types.contains(&round) && types.contains(&f.column_type));
        let one = type_choice(&view, &f.columns[..1]).unwrap();
        assert_eq!(one.current, Shared::Same(Some(f.column_type)));
        assert_eq!(instance_count(&view, f.column_type), 1);

        // Levels take no type.
        assert_eq!(type_choice(&view, &[f.upper]), None);
        assert_eq!(type_choice(&view, &[f.columns[0], f.upper]), None);

        // A section type shows its material and the dimensions of its
        // shape.
        let labels = |id| -> Vec<_> {
            let rows = rows(&view, &[id]);
            rows.iter().map(|r| r.parameter.label).collect()
        };
        assert_eq!(
            labels(round),
            ["parameter-structural-material", "parameter-diameter"]
        );
        assert_eq!(
            labels(f.column_type),
            [
                "parameter-structural-material",
                "parameter-section-width",
                "parameter-section-depth"
            ]
        );
    }

    #[test]
    fn a_types_material_is_picked_from_the_projects_materials() {
        let mut f = fixture();
        let rows = rows(&f.store.view(), &[f.column_type]);
        let material = row(&rows, "parameter-structural-material");
        let view = f.store.view();
        let options = choices(&view, material.parameter);
        assert_eq!(
            options[0],
            (Current::Element(None), Label::Key("browser-none"))
        );
        assert!(options.len() >= 2, "the generic concrete");
        let parameter = material.parameter;

        let mut tx = f.store.transaction();
        apply(&mut tx, f.column_type, parameter, &Current::Element(None)).unwrap();
        tx.commit().unwrap();
        assert_eq!(section::material_of(&f.store.view(), f.column_type), None);
    }

    #[test]
    fn duplicating_a_type_copies_its_values_material_and_parts() {
        let mut f = fixture();
        let view = f.store.view();
        assert_eq!(
            copy_name(&view, f.column_type),
            format!("{} 2", type_name(&view, f.column_type))
        );
        let material = section::material_of(&view, f.column_type);
        let mut tx = f.store.transaction();
        let copy = duplicate_type(&mut tx, f.column_type, "C 20 x 40").unwrap();
        tx.commit().unwrap();
        let view = f.store.view();
        assert_eq!(type_name(&view, copy), "C 20 x 40");
        assert_eq!(section::of(&view, copy), section::of(&view, f.column_type));
        assert_eq!(section::material_of(&view, copy), material);
        assert_eq!(
            check_type_name(&view, f.column_type, " C 20 x 40 ", false),
            Err(NameProblem::Taken("C 20 x 40".into()))
        );
        assert_eq!(
            check_type_name(&view, copy, "C 20 x 40", true),
            Ok("C 20 x 40".into())
        );
        assert_eq!(
            check_type_name(&view, copy, "  ", true),
            Err(NameProblem::Empty)
        );

        // A wall type's copy has layers of its own.
        let wall_type = all_of(
            &f.store,
            ARCHITECTURAL,
            hito_architectural::wall_type::WALL_TYPE,
        )[0];
        let mut tx = f.store.transaction();
        let copy = duplicate_type(&mut tx, wall_type, "Copy").unwrap();
        tx.commit().unwrap();
        let view = f.store.view();
        let (original, copied) = (
            hito_architectural::wall_type::layers(&view, wall_type),
            hito_architectural::wall_type::layers(&view, copy),
        );
        assert_eq!(copied.len(), original.len());
        assert!(!copied.is_empty());
        let width = hito_architectural::wall_type::width;
        assert_eq!(width(&view, copy), width(&view, wall_type));
    }

    #[test]
    fn every_class_group_parameter_and_choice_has_a_label_in_every_language() {
        let keys: Vec<_> = DESCRIBED
            .iter()
            .flat_map(|d| d.parameters)
            .flat_map(|p| [p.label, p.group.label()])
            .chain(CLASS_LABELS.iter().map(|(_, _, key)| *key))
            .chain(LocationLine::ALL.map(location_line_key))
            .chain(["parameter-unconnected", "parameter-up-to-level"])
            .collect();
        for locale in Locale::ALL {
            let i18n = I18n::new(locale);
            for key in &keys {
                assert!(!i18n.tr(key).contains('⟨'), "{key} in {locale:?}");
            }
        }
    }
}
