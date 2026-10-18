//! Wall geometry: each layer's outline, extruded from the wall's base to
//! its top.

use hito_geometry::{Frame, Generator, Profile, Solid};

use crate::footprint;
use crate::plan::sub;
use crate::{class, wall};

/// The architectural geometry generators.
pub(crate) fn generators() -> Vec<Generator> {
    let wall = Generator::new(class(wall::WALL), |inputs, wall| {
        let footprint = footprint::compute(inputs, wall.id())?;
        let (start, _) = wall::read_line(inputs, wall.id())?;
        let mut parts = Vec::new();
        for slice in &footprint.slices {
            // Profiles are placed at the wall's start, so a wall far from
            // the origin keeps its precision.
            let frame = Frame::at([start[0], start[1], slice.bottom]);
            let polygons = slice.layers.iter().flat_map(|l| &l.polygons);
            parts.extend(polygons.map(|polygon| Solid::Extrusion {
                profile: Profile::polygon(
                    polygon.iter().map(|&p| sub(p, start)).collect::<Vec<_>>(),
                ),
                frame,
                depth: slice.top - slice.bottom,
            }));
        }
        match parts.len() {
            0 => None,
            1 => parts.pop(),
            _ => Some(Solid::Union(parts)),
        }
    });
    vec![wall]
}
