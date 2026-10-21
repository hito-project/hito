//! What the properties panel shows and edits of the selected elements
//! (#26), as plain data the shell draws: each class's parameters, grouped
//! and labelled as Revit's properties palette shows them, the values a
//! selection shares, and the types it can change to.
//!
//! [ADR 0015](https://github.com/hito-project/hito/blob/main/docs/decisions/0015-ui-toolkit.md)
//! has property descriptions (name, value, unit, editor) and the shell
//! turning them into rows. The descriptions live here, next to the
//! commands, until a domain outside this repository needs to add its own.

use hito_core::{ClassId, Dimension, ElementId, StoreView, Value};
use hito_structural::{column, footing, level, slab};

/// Revit's parameter groups, in the order its properties palette lists
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Constraints,
    Construction,
    Structural,
    Dimensions,
}

impl Group {
    /// The group's message key.
    pub fn label(self) -> &'static str {
        match self {
            Group::Constraints => "group-constraints",
            Group::Construction => "group-construction",
            Group::Structural => "group-structural",
            Group::Dimensions => "group-dimensions",
        }
    }
}

/// How a parameter's value is shown and typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Editor {
    /// A quantity typed with or without units, such as `20 cm`.
    Quantity(Dimension),
    /// A check box.
    Boolean,
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
    /// The property it shows.
    pub property: &'static str,
    /// Its message key.
    pub label: &'static str,
    pub group: Group,
    pub editor: Editor,
    pub computed: Computed,
    /// When the panel shows it.
    pub shown: Shown,
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
        property,
        label,
        group,
        editor: LENGTH,
        computed: Computed::Always,
        shown: Shown::Always,
    }
}

const fn computed_when(
    property: &'static str,
    label: &'static str,
    group: Group,
    when: fn(&StoreView<'_>, ElementId) -> bool,
) -> Parameter {
    Parameter {
        property,
        label,
        group,
        editor: LENGTH,
        computed: Computed::When(when),
        shown: Shown::Always,
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
    has(view, id, &hito_structural::class(column::TOP_LEVEL))
}

fn column_has_no_top_level(view: &StoreView<'_>, id: ElementId) -> bool {
    !column_has_top_level(view, id)
}

fn wall_top_is_fixed(view: &StoreView<'_>, id: ElementId) -> bool {
    use hito_architectural::{class, wall};
    has(view, id, &class(wall::TOP_LEVEL)) || has(view, id, &class(wall::TOP_ATTACHED_TO))
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

fn wall_has_no_top_level(view: &StoreView<'_>, id: ElementId) -> bool {
    use hito_architectural::{class, wall};
    !has(view, id, &class(wall::TOP_LEVEL))
}

/// Instance and type parameters, as Revit names and groups them. Levels
/// and grids keep their own rows (`shell::elements`), which check their
/// names are unique.
const DESCRIBED: &[Described] = &[
    Described {
        schema: ARCHITECTURAL,
        class: hito_architectural::wall::WALL,
        parameters: &[
            editable(
                hito_architectural::wall::BASE_OFFSET,
                "parameter-base-offset",
                Group::Constraints,
                LENGTH,
            ),
            computed_when(
                hito_architectural::wall::TOP_OFFSET,
                "parameter-top-offset",
                Group::Constraints,
                wall_has_no_top_level,
            ),
            computed_when(
                hito_architectural::wall::HEIGHT,
                "parameter-unconnected-height",
                Group::Constraints,
                wall_top_is_fixed,
            ),
            editable(
                hito_architectural::wall::STRUCTURAL,
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
            editable(
                column::BASE_OFFSET,
                "parameter-base-offset",
                Group::Constraints,
                LENGTH,
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
        parameters: &[editable(
            level::LEVEL_OFFSET,
            "parameter-level-offset",
            Group::Constraints,
            LENGTH,
        )],
    },
    Described {
        schema: STRUCTURAL,
        class: slab::SLAB,
        parameters: &[
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
        class: hito_structural::section::SECTION_TYPE,
        parameters: &[
            when_set(editable(
                hito_structural::section::WIDTH,
                "parameter-section-width",
                Group::Dimensions,
                LENGTH,
            )),
            when_set(editable(
                hito_structural::section::DEPTH,
                "parameter-section-depth",
                Group::Dimensions,
                LENGTH,
            )),
            when_set(editable(
                hito_structural::section::DIAMETER,
                "parameter-diameter",
                Group::Dimensions,
                LENGTH,
            )),
        ],
    },
    Described {
        schema: STRUCTURAL,
        class: slab::SLAB_TYPE,
        parameters: &[editable(
            slab::THICKNESS,
            "parameter-default-thickness",
            Group::Construction,
            LENGTH,
        )],
    },
    Described {
        schema: STRUCTURAL,
        class: footing::FOOTING_TYPE,
        parameters: &[
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
    (ARCHITECTURAL, hito_architectural::wall::WALL, "class-wall"),
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

/// One row of the panel: a parameter, the selection's value of it, and
/// whether it can be edited.
#[derive(Debug, Clone)]
pub struct Row {
    pub parameter: &'static Parameter,
    pub value: Shared<Value>,
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
            let value = Shared::of(ids().map(|id| view.property(id, parameter.property).cloned()));
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
    match view.property(type_id, slab::NAME) {
        Some(Value::Text(name)) => name.clone(),
        _ => String::new(),
    }
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

#[cfg(test)]
mod tests {
    use hito_core::{Element, ElementStore, ModelKind, Relationship, SchemaRegistry};
    use hito_structural::class;
    use hito_structural::column::Placement;
    use hito_structural::section::{self, Section};

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
        upper: ElementId,
    }

    fn all_of(store: &ElementStore, name: &str) -> Vec<ElementId> {
        let registry = store.registry();
        let mut ids: Vec<_> = store
            .elements()
            .filter(|e| registry.is_a(e.class(), &class(name)))
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
        let column_type = all_of(&store, column::COLUMN_TYPE)[0];
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
            upper,
        }
    }

    fn row<'a>(rows: &'a [Row], label: &str) -> &'a Row {
        let found = rows.iter().find(|r| r.parameter.label == label);
        found.unwrap_or_else(|| panic!("no {label} row"))
    }

    #[test]
    fn a_column_shows_its_constraints_with_their_values() {
        let f = fixture();
        let rows = rows(&f.store.view(), &f.columns[..1]);
        let labels: Vec<_> = rows.iter().map(|r| r.parameter.label).collect();
        assert_eq!(
            labels,
            [
                "parameter-base-offset",
                "parameter-top-offset",
                "parameter-unconnected-height",
                "parameter-rotation",
                "parameter-x",
                "parameter-y",
            ]
        );
        assert!(!row(&rows, "parameter-x").computed, "a free column moves");
        let height = row(&rows, "parameter-unconnected-height");
        assert_eq!(height.value, Shared::Same(Some(Value::Real(3.0))));
        assert!(!height.computed, "an unconnected column's height is typed");
        assert!(row(&rows, "parameter-top-offset").computed, "unused");
    }

    #[test]
    fn values_that_differ_vary_and_computed_ones_are_read_only() {
        let mut f = fixture();
        let both = rows(&f.store.view(), &f.columns);
        let offset = row(&both, "parameter-base-offset");
        assert_eq!(offset.value, Shared::Varies);
        let height = row(&both, "parameter-unconnected-height");
        assert_eq!(height.value, Shared::Same(Some(Value::Real(3.0))));

        // With a top level, the height is computed and the top offset used.
        let mut tx = f.store.transaction();
        let top = Relationship::new(class(column::TOP_LEVEL), f.columns[0], f.upper);
        tx.relate(top).unwrap();
        tx.commit().unwrap();
        let view = f.store.view();
        let both = rows(&view, &f.columns);
        assert!(row(&both, "parameter-unconnected-height").computed);
        let offset = row(&both, "parameter-top-offset");
        assert!(offset.computed, "still unused by the second column");
        let first = rows(&view, &f.columns[..1]);
        assert!(!row(&first, "parameter-top-offset").computed);
    }

    #[test]
    fn a_column_on_grids_moves_by_its_offsets_from_them() {
        use hito_structural::grid;
        let mut f = fixture();
        let base = level::levels(&f.store.view())[0].0;
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
        let placement = Placement::new(f.column_type, base, [0.0, 0.0]).on_grids([a, one]);
        let on_grids = column::insert(&mut tx, model, &placement).unwrap();
        tx.commit().unwrap();

        let view = f.store.view();
        let rows = rows(&view, &[on_grids]);
        assert!(row(&rows, "parameter-x").computed, "#215 decision D");
        assert!(!row(&rows, "parameter-grid-offset-x").computed);
        // A free column has no offsets from grids.
        let free = rows_of(&view, f.columns[0]);
        assert!(
            free.iter()
                .all(|r| r.parameter.label != "parameter-grid-offset-x")
        );
        // Nor do they show when only some of the selection is on grids.
        let both = super::rows(&view, &[on_grids, f.columns[0]]);
        assert!(
            both.iter()
                .all(|r| r.parameter.label != "parameter-grid-offset-y")
        );
    }

    fn rows_of(view: &StoreView<'_>, id: ElementId) -> Vec<Row> {
        rows(view, &[id])
    }

    #[test]
    fn a_parameter_is_shared_only_under_the_same_name() {
        let mut f = fixture();
        let slab_type = all_of(&f.store, slab::SLAB_TYPE)[0];
        let base = level::levels(&f.store.view())[0].0;
        let mut tx = f.store.transaction();
        let model = project::physical_model(&mut tx).unwrap();
        let outline = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let placement = slab::Placement::new(slab_type, base, &outline);
        let slab = slab::insert(&mut tx, model, &placement).unwrap();
        tx.commit().unwrap();

        // A column's Base Offset and a slab's Height Offset From Level are
        // the same property, but Revit names them differently.
        let rows = rows(&f.store.view(), &[f.columns[0], slab]);
        assert!(rows.is_empty(), "{rows:?}");
    }

    #[test]
    fn the_type_selector_lists_the_types_the_selection_can_take() {
        let mut f = fixture();
        let definitions = f
            .store
            .models()
            .find(|m| m.kind() == ModelKind::Definition)
            .unwrap()
            .id();
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

        // A section type shows the dimensions of its shape.
        let labels = |id| -> Vec<_> {
            let rows = rows(&view, &[id]);
            rows.iter().map(|r| r.parameter.label).collect()
        };
        assert_eq!(labels(round), ["parameter-diameter"]);
        assert_eq!(
            labels(f.column_type),
            ["parameter-section-width", "parameter-section-depth"]
        );
    }

    #[test]
    fn every_class_group_and_parameter_has_a_label_in_every_language() {
        let keys: Vec<_> = DESCRIBED
            .iter()
            .flat_map(|d| d.parameters)
            .flat_map(|p| [p.label, p.group.label()])
            .chain(CLASS_LABELS.iter().map(|(_, _, key)| *key))
            .collect();
        for locale in Locale::ALL {
            let i18n = I18n::new(locale);
            for key in &keys {
                assert!(!i18n.tr(key).contains('⟨'), "{key} in {locale:?}");
            }
        }
    }
}
