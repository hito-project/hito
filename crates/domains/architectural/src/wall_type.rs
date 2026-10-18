//! Wall types: a wall's layers, from the exterior face to the interior one
//! (#193).
//!
//! A wall type is a definition element with a name, like Revit's wall types
//! ("Edit Type", then "Structure"). Its layers are definition elements of
//! their own, each linked to its type by [`LAYER_OF`] and to its material by
//! [`LAYER_MATERIAL`], as IFC's `IfcMaterialLayer`s make up an
//! `IfcMaterialLayerSet`. A layer has a [`Position`](POSITION) (the
//! exterior layer first), a [`Thickness`](THICKNESS) and a
//! [`Function`](FUNCTION), with Revit's functions. The wall's thickness is
//! the sum of its layers'.
//!
//! The **core** is the layers from the first to the last structural one, as
//! Revit's core boundaries usually enclose them; a type without a
//! structural layer is all core. Where walls join, cores meet cores and
//! finishes meet finishes (see [`join`](crate::join)).
//!
//! ```
//! use hito_architectural::wall_type::{self, Function, Layer};
//! use hito_core::{ElementStore, Model, ModelKind, SchemaRegistry};
//!
//! # let mut registry = SchemaRegistry::new();
//! # hito_architectural::register(&mut registry)?;
//! # let mut store = ElementStore::new(registry);
//! let mut tx = store.transaction();
//! let definitions = tx.insert_model(Model::new(ModelKind::Definition))?;
//! let brick = wall_type::insert(
//!     &mut tx,
//!     definitions,
//!     "Ladrillo hueco 18 revocado",
//!     &[
//!         Layer::new(Function::Finish1, 0.015),
//!         Layer::new(Function::Structure, 0.18),
//!         Layer::new(Function::Finish2, 0.015),
//!     ],
//! )?;
//! tx.commit()?;
//! assert!((wall_type::width(&store.view(), brick) - 0.21).abs() < 1e-12);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Layer materials can be any definition element until materials (#36)
//! merge; then they narrow to materials.

use std::ops::Range;

use hito_core::{
    ClassDef, ClassId, DataType, Dimension, Element, ElementId, ModelId, OnDelete, Relationship,
    RelationshipDef, Rule, StoreError, StoreView, Transaction, Value,
};

use crate::class;
use crate::read::Read;

/// A wall type.
pub const WALL_TYPE: &str = "WallType";
/// One layer of a wall type.
pub const WALL_LAYER: &str = "WallLayer";
/// From a layer to its wall type. Deleting the type deletes its layers.
pub const LAYER_OF: &str = "LayerOf";
/// From a layer to its material.
pub const LAYER_MATERIAL: &str = "LayerMaterial";

/// A wall type's name, unique among wall types. It's data, not interface
/// text, so it isn't translated.
pub const NAME: &str = "Name";
/// Where a layer sits in its type: layers go from the exterior face, the
/// smallest position, to the interior face. Positions are unique within a
/// type but needn't be consecutive.
pub const POSITION: &str = "Position";
/// A layer's thickness. Zero is allowed, for a membrane.
pub const THICKNESS: &str = "Thickness";
/// What a layer does, as one of [`Function`]'s names. Unset means
/// [`Function::Structure`].
pub const FUNCTION: &str = "Function";

const LENGTH: DataType = DataType::Quantity(Dimension::LENGTH);

/// What a layer does, with Revit's layer functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Function {
    /// Supports the wall, such as brick or concrete.
    Structure,
    /// Supports another layer, such as plywood.
    Substrate,
    /// Insulation or an air gap.
    ThermalAir,
    /// A vapour or damp barrier, usually of zero thickness.
    Membrane,
    /// The exterior finish, such as render.
    Finish1,
    /// The interior finish, such as plaster.
    Finish2,
}

impl Function {
    pub const ALL: [Function; 6] = [
        Self::Structure,
        Self::Substrate,
        Self::ThermalAir,
        Self::Membrane,
        Self::Finish1,
        Self::Finish2,
    ];

    /// The name stored in the [`FUNCTION`] property.
    pub fn name(self) -> &'static str {
        match self {
            Self::Structure => "Structure",
            Self::Substrate => "Substrate",
            Self::ThermalAir => "ThermalAir",
            Self::Membrane => "Membrane",
            Self::Finish1 => "Finish1",
            Self::Finish2 => "Finish2",
        }
    }

    /// The function with this stored name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.name() == name)
    }
}

/// A layer to create with [`insert`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    pub function: Function,
    pub thickness: f64,
    pub material: Option<ElementId>,
}

impl Layer {
    pub fn new(function: Function, thickness: f64) -> Self {
        Self {
            function,
            thickness,
            material: None,
        }
    }

    pub fn with_material(mut self, material: ElementId) -> Self {
        self.material = Some(material);
        self
    }
}

/// The wall type and layer classes.
pub(crate) fn classes() -> [ClassDef; 2] {
    let wall_type =
        ClassDef::new(WALL_TYPE, ClassId::DEFINITION_ELEMENT).property(NAME, DataType::Text);
    let layer = ClassDef::new(WALL_LAYER, ClassId::DEFINITION_ELEMENT)
        .property(POSITION, DataType::Integer)
        .property(THICKNESS, LENGTH)
        .property(FUNCTION, DataType::Text);
    [wall_type, layer]
}

pub(crate) fn relationships() -> [RelationshipDef; 2] {
    let layer_of = RelationshipDef::new(LAYER_OF, class(WALL_LAYER), class(WALL_TYPE))
        .on_delete(OnDelete::Cascade);
    let material = RelationshipDef::new(
        LAYER_MATERIAL,
        class(WALL_LAYER),
        ClassId::DEFINITION_ELEMENT,
    );
    [layer_of, material]
}

/// The wall type rules:
///
/// - `WallTypeName`: every wall type has a name, and no two share one, as
///   in Revit.
/// - `WallLayer`: every layer belongs to exactly one wall type, has a
///   position no other layer of its type has, a thickness that isn't
///   negative, and a known function if it has one.
pub(crate) fn rules() -> Vec<Rule> {
    let name = Rule::new("WallTypeName", class(WALL_TYPE), |store, wall_type| {
        let name = match wall_type.properties().get(NAME) {
            Some(Value::Text(name)) if !name.trim().is_empty() => name,
            _ => return Err("the wall type has no name".into()),
        };
        let registry = store.registry();
        let same_name = store.elements().find(|other| {
            other.id() != wall_type.id()
                && registry.is_a(other.class(), &class(WALL_TYPE))
                && other.properties().get(NAME) == Some(&Value::Text(name.clone()))
        });
        match same_name {
            Some(other) => Err(format!("wall type {} is also called `{name}`", other.id())),
            None => Ok(()),
        }
    });

    let layer = Rule::new("WallLayer", class(WALL_LAYER), |store, layer| {
        let types = store.targets(layer.id(), &class(LAYER_OF));
        let [wall_type] = types[..] else {
            return Err(format!("the layer belongs to {} wall types", types.len()));
        };
        let Some(&Value::Integer(position)) = layer.properties().get(POSITION) else {
            return Err("the layer has no position".into());
        };
        let same_position = store
            .sources(wall_type, &class(LAYER_OF))
            .into_iter()
            .find(|&other| {
                other != layer.id()
                    && store.property(other, POSITION) == Some(&Value::Integer(position))
            });
        if let Some(other) = same_position {
            return Err(format!("layer {other} is also at position {position}"));
        }
        match layer.properties().get(THICKNESS) {
            Some(Value::Real(t)) if *t >= 0.0 => {}
            Some(Value::Real(t)) => return Err(format!("the layer is {t} m thick")),
            _ => return Err("the layer has no thickness".into()),
        }
        match layer.properties().get(FUNCTION) {
            Some(Value::Text(f)) if Function::from_name(f).is_none() => {
                Err(format!("`{f}` isn't a layer function"))
            }
            _ => Ok(()),
        }
    });

    vec![name, layer]
}

/// Adds a wall type called `name` to `model`, with `layers` from the
/// exterior face to the interior one. Returns the type.
pub fn insert(
    tx: &mut Transaction<'_>,
    model: ModelId,
    name: &str,
    layers: &[Layer],
) -> Result<ElementId, StoreError> {
    let wall_type = tx.insert(Element::new(class(WALL_TYPE), model).with_property(NAME, name))?;
    for (position, layer) in (1i64..).zip(layers) {
        let element = Element::new(class(WALL_LAYER), model)
            .with_property(POSITION, position)
            .with_property(THICKNESS, layer.thickness)
            .with_property(FUNCTION, layer.function.name());
        let id = tx.insert(element)?;
        tx.relate(Relationship::new(class(LAYER_OF), id, wall_type))?;
        if let Some(material) = layer.material {
            tx.relate(Relationship::new(class(LAYER_MATERIAL), id, material))?;
        }
    }
    Ok(wall_type)
}

/// A wall type's layer, as walls read it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerInfo {
    pub id: ElementId,
    pub thickness: f64,
    pub function: Function,
    pub material: Option<ElementId>,
}

/// A wall type's layers, from the exterior face to the interior one.
pub fn layers(store: &StoreView<'_>, wall_type: ElementId) -> Vec<LayerInfo> {
    read_layers(store, wall_type)
}

/// A wall type's thickness: the sum of its layers'.
pub fn width(store: &StoreView<'_>, wall_type: ElementId) -> f64 {
    layers(store, wall_type).iter().map(|l| l.thickness).sum()
}

/// Which of `layers` form the core: from the first structural layer to the
/// last, or all of them if none is structural.
pub fn core(layers: &[LayerInfo]) -> Range<usize> {
    let structural = |l: &LayerInfo| l.function == Function::Structure;
    let first = layers.iter().position(structural);
    let last = layers.iter().rposition(structural);
    match (first, last) {
        (Some(first), Some(last)) => first..last + 1,
        _ => 0..layers.len(),
    }
}

pub(crate) fn read_layers<'a>(store: &impl Read<'a>, wall_type: ElementId) -> Vec<LayerInfo> {
    let mut layers: Vec<_> = store
        .sources(wall_type, &class(LAYER_OF))
        .into_iter()
        .map(|id| {
            let position = match store.property(id, POSITION) {
                Some(Value::Integer(p)) => *p,
                _ => i64::MAX,
            };
            let function = store.text(id, FUNCTION).and_then(Function::from_name);
            let layer = LayerInfo {
                id,
                thickness: store.real(id, THICKNESS).unwrap_or(0.0).max(0.0),
                function: function.unwrap_or(Function::Structure),
                material: store.targets(id, &class(LAYER_MATERIAL)).first().copied(),
            };
            (position, layer)
        })
        .collect();
    layers.sort_by_key(|(position, layer)| (*position, layer.id));
    layers.into_iter().map(|(_, layer)| layer).collect()
}
