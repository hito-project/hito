//! Structural geometry: each element's body, from its parameters.

use hito_geometry::{Frame, Generator, Profile, Solid};

use crate::read::Read;
use crate::{class, column, footing, slab};

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
    // A slab is its boundary, less its openings, extruded from its bottom
    // up to its top. Openings are holes in the profile, so no boolean is
    // needed.
    let slab = Generator::new(class(slab::SLAB), |inputs, slab| {
        let outline = slab::read_boundary(inputs, slab.id());
        let (bottom, top) = slab::read_elevations(inputs, slab.id())?;
        let origin = *outline.first()?;
        // Profiles are placed at the first corner, so a slab far from the
        // origin keeps its precision.
        let local = |points: Vec<[f64; 2]>| -> Vec<[f64; 2]> {
            let at = |p: [f64; 2]| [p[0] - origin[0], p[1] - origin[1]];
            points.into_iter().map(at).collect()
        };
        let mut openings = inputs.sources(slab.id(), &class(slab::OPENING_IN));
        openings.sort();
        let holes = openings
            .into_iter()
            .map(|o| local(slab::read_boundary(inputs, o)))
            .collect();
        Some(Solid::Extrusion {
            profile: Profile::Polygon {
                outline: local(outline),
                holes,
            },
            frame: Frame::at([origin[0], origin[1], bottom]),
            depth: top - bottom,
        })
    });
    // A footing is a box: its length along its rotation, its width across,
    // from its bottom up to its top.
    let footing = Generator::new(class(footing::FOOTING), |inputs, footing| {
        let [x, y] = footing::read_centre(inputs, footing.id())?;
        let (bottom, top) = footing::read_elevations(inputs, footing.id())?;
        let angle = inputs.real(footing.id(), footing::ROTATION).unwrap_or(0.0);
        let frame = Frame::new(
            [x, y, bottom],
            [0.0, 0.0, 1.0],
            [angle.cos(), angle.sin(), 0.0],
        );
        Some(Solid::Extrusion {
            profile: Profile::Rectangle {
                width: inputs.real(footing.id(), footing::LENGTH)?,
                height: inputs.real(footing.id(), footing::WIDTH)?,
            },
            frame: frame.ok()?,
            depth: top - bottom,
        })
    });
    vec![column, slab, footing]
}
