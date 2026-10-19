//! Wall geometry: each layer's outline, extruded from the wall's base to
//! its top, minus the openings its doors and windows cut. Doors and
//! windows: their frame and their leaf or glass.

use hito_geometry::{Frame, Generator, Profile, Solid};

use crate::footprint;
use crate::plan::sub;
use crate::{class, opening, wall};

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
        let body = match parts.len() {
            0 => None,
            1 => parts.pop(),
            _ => Some(Solid::Union(parts)),
        }?;
        let voids = opening::wall_voids(inputs, wall.id());
        match voids.is_empty() {
            true => Some(body),
            false => Some(Solid::Difference {
                base: Box::new(body),
                voids,
            }),
        }
    });
    let opening = Generator::new(class(opening::OPENING), |inputs, element| {
        let o = opening::read_opening(inputs, element.id())?;
        opening::body(&o)
    });
    vec![wall, opening]
}
