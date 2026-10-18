//! Which mouse buttons orbit and pan a 3D view.

use serde::{Deserialize, Serialize};

/// What dragging with the mouse wheel held down does. The wheel zooms to
/// the cursor and a double click on it zooms to fit either way.
///
/// The founding user asked to orbit with the wheel, which is the reverse
/// of Revit's default, so it's the default and Revit's is one click away
/// ([ADR 0025](https://github.com/hito-project/hito/blob/main/docs/decisions/0025-m1-small-building.md)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Navigation {
    /// Holding the wheel orbits; Shift and the wheel pans.
    #[default]
    WheelOrbits,
    /// Revit's: holding the wheel pans; Shift and the wheel orbits.
    Revit,
}

/// What a drag with the wheel held down does to the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Orbit,
    Pan,
}

impl Navigation {
    pub const ALL: [Navigation; 2] = [Navigation::WheelOrbits, Navigation::Revit];

    /// The message key of its name.
    pub fn label(self) -> &'static str {
        match self {
            Navigation::WheelOrbits => "navigation-wheel-orbits",
            Navigation::Revit => "navigation-revit",
        }
    }

    /// What dragging with the wheel held down does, with or without Shift.
    pub fn wheel_drag(self, shift: bool) -> Motion {
        match (self, shift) {
            (Navigation::WheelOrbits, false) | (Navigation::Revit, true) => Motion::Orbit,
            (Navigation::WheelOrbits, true) | (Navigation::Revit, false) => Motion::Pan,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wheel_orbits_by_default_and_revit_is_the_reverse() {
        let default = Navigation::default();
        assert_eq!(default.wheel_drag(false), Motion::Orbit);
        assert_eq!(default.wheel_drag(true), Motion::Pan);
        assert_eq!(Navigation::Revit.wheel_drag(false), Motion::Pan);
        assert_eq!(Navigation::Revit.wheel_drag(true), Motion::Orbit);
    }
}
