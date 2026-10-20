//! Structural geometry: each element's body, from its parameters.

use hito_geometry::{Frame, Generator, Solid};

use crate::{class, column};

/// The structural geometry generators.
pub(crate) fn generators() -> Vec<Generator> {
    // A column is its section, extruded from its base to its top.
    let column = Generator::new(class(column::COLUMN), |inputs, column| {
        let id = column.id();
        let section = column::read_section(inputs, id)?;
        let [x, y] = column::read_location(inputs, id)?;
        let (bottom, top) = column::read_elevations(inputs, id)?;
        let (sin, cos) = column::read_rotation(inputs, id).sin_cos();
        let frame = Frame::new([x, y, bottom], [0.0, 0.0, 1.0], [cos, sin, 0.0]).ok()?;
        Some(Solid::Extrusion {
            profile: section.profile(),
            frame,
            depth: top - bottom,
        })
    });
    vec![column]
}
