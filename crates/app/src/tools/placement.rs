//! What the placement tools share: the level elements go on,
//! the level above it, the project's first type of a class, and the
//! options bar's top for columns and walls.

use hito_core::{ClassId, Dimension, Element, ElementId, StoreView, Value};
use hito_structural::level;

use crate::command::{Choice, OptionValue, ToolOption};

/// How far apart two levels must be to count as different: 1 mm.
const TOLERANCE: f64 = 0.001;

/// The level elements go on: the active one if it still exists, or else
/// the lowest.
pub fn base_level(view: &StoreView<'_>, active: Option<ElementId>) -> Option<ElementId> {
    let levels = level::levels(view);
    let exists = |id: &ElementId| levels.iter().any(|(l, _)| l == id);
    active
        .filter(exists)
        .or_else(|| levels.first().map(|(id, _)| *id))
}

/// The lowest level above `base`, if any.
pub fn level_above(view: &StoreView<'_>, base: ElementId) -> Option<ElementId> {
    levels_above(view, base).first().copied()
}

/// Every level above `base`, lowest first.
pub fn levels_above(view: &StoreView<'_>, base: ElementId) -> Vec<ElementId> {
    let levels = level::levels(view);
    let Some(&(_, elevation)) = levels.iter().find(|(id, _)| *id == base) else {
        return Vec::new();
    };
    levels
        .iter()
        .filter(|(_, e)| *e > elevation + TOLERANCE)
        .map(|(id, _)| *id)
        .collect()
}

/// The type new elements get until the type selector (#26) exists: the
/// project's first of class `class`.
pub fn first_of(view: &StoreView<'_>, class: &ClassId) -> Option<ElementId> {
    let registry = view.registry();
    view.elements()
        .filter(|e| registry.is_a(e.class(), class))
        .map(Element::id)
        .min()
}

/// The options bar's top choice: "Unconnected" or a level above the base.
pub const TOP: &str = "option-top";
/// The options bar's unconnected height.
pub const HEIGHT: &str = "option-unconnected-height";
/// The top choice that leaves the top unconnected.
pub const UNCONNECTED: &str = "option-unconnected";

/// Where a column's or a wall's top goes, as Revit's options bar sets it:
/// at a level above the base, or unconnected at a height. Its top offset
/// stays 0, left to the properties (#26).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TopOption {
    top: TopChoice,
    /// The unconnected height, kept while the top is a level.
    height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TopChoice {
    /// Not set: the level above, or unconnected if there's none.
    LevelAbove,
    Unconnected,
    Level(ElementId),
}

impl Default for TopOption {
    /// Up to the level above, or 3 m high if there's none, as Revit starts.
    fn default() -> Self {
        Self {
            top: TopChoice::LevelAbove,
            height: 3.0,
        }
    }
}

impl TopOption {
    /// The level the top goes to over `base`, or `None` when it's
    /// unconnected, at [`TopOption::height`]. A level that's no longer
    /// above the base falls back to the level above.
    pub fn level(&self, view: &StoreView<'_>, base: ElementId) -> Option<ElementId> {
        match self.top {
            TopChoice::Unconnected => None,
            TopChoice::Level(level) if levels_above(view, base).contains(&level) => Some(level),
            TopChoice::Level(_) | TopChoice::LevelAbove => level_above(view, base),
        }
    }

    pub fn height(&self) -> f64 {
        self.height
    }

    /// The top and the unconnected height, for the options bar. The height
    /// can be changed only while the top is unconnected, as in Revit.
    pub fn options(&self, view: &StoreView<'_>, base: Option<ElementId>) -> [ToolOption; 2] {
        let above = base.map(|b| levels_above(view, b)).unwrap_or_default();
        let level = base.and_then(|b| self.level(view, b));
        let mut choices = vec![Choice::Key(UNCONNECTED)];
        choices.extend(above.iter().map(|&id| Choice::Element {
            id,
            name: match view.property(id, level::NAME) {
                Some(Value::Text(name)) => name.clone(),
                _ => String::new(),
            },
        }));
        let selected = level
            .and_then(|l| above.iter().position(|&a| a == l))
            .map_or(0, |i| i + 1);
        [
            ToolOption {
                key: TOP,
                value: OptionValue::Choice { selected, choices },
                enabled: true,
            },
            ToolOption {
                key: HEIGHT,
                value: OptionValue::Quantity {
                    value: self.height,
                    dimension: Dimension::LENGTH,
                },
                enabled: level.is_none(),
            },
        ]
    }

    /// Sets the top or the height. `None` if `key` is neither.
    pub fn set(&mut self, key: &str, value: &OptionValue) -> Option<Result<(), &'static str>> {
        Some(match key {
            TOP => match value.chosen() {
                Some(Choice::Key(UNCONNECTED)) => {
                    self.top = TopChoice::Unconnected;
                    Ok(())
                }
                Some(Choice::Element { id, .. }) => {
                    self.top = TopChoice::Level(id);
                    Ok(())
                }
                _ => Err("option-unknown"),
            },
            HEIGHT => match value.quantity() {
                Some(height) if height > 0.0 => {
                    self.height = height;
                    Ok(())
                }
                _ => Err("option-height-positive"),
            },
            _ => return None,
        })
    }
}
