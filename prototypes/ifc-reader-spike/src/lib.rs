//! Spike #139: ifc-lite versus our own Part 21 reader. See README.md.

pub mod geom;
pub mod p21;

use std::collections::HashMap;

use geom::Ifc;
use p21::File;

/// Classes left out of the comparison, as IfcOpenShell's iterator does by default.
pub const SKIP: &[&[u8]] = &[
    b"IFCOPENINGELEMENT",
    b"IFCOPENINGSTANDARDCASE",
    b"IFCSPACE",
    b"IFCANNOTATION",
    b"IFCGRID",
    b"IFCVIRTUALELEMENT",
];

/// Products with a placement and a shape, minus SKIP.
pub fn products(f: &File) -> Vec<u32> {
    f.iter()
        .filter(|(_, t)| !SKIP.contains(t))
        .filter_map(|(id, t)| {
            if !t.starts_with(b"IFC") {
                return None;
            }
            let i = f.get(id).ok()?;
            let rep = i.attrs.get(6)?.as_ref()?;
            (f.type_of(rep)? == b"IFCPRODUCTDEFINITIONSHAPE" && i.attrs.get(5)?.as_ref().is_some())
                .then_some(id)
        })
        .collect()
}

pub fn void_map(f: &File) -> HashMap<u32, Vec<u32>> {
    let mut m: HashMap<u32, Vec<u32>> = HashMap::new();
    for (id, t) in f.iter() {
        if t == b"IFCRELVOIDSELEMENT" {
            if let Ok(r) = f.get(id) {
                if let (Some(h), Some(o)) = (r.attr(4).as_ref(), r.attr(5).as_ref()) {
                    m.entry(h).or_default().push(o);
                }
            }
        }
    }
    m
}

/// Everything our reader does on import: index, decode all, mesh all.
pub fn run_own(data: &[u8]) -> usize {
    let Ok(f) = File::parse(data) else { return 0 };
    let mut n = 0;
    for (id, _) in f.iter() {
        if let Ok(i) = f.get(id) {
            n += i.attrs.len();
            for a in &i.attrs {
                if let Some(s) = a.as_str() {
                    n += s.len();
                }
            }
        }
    }
    let ifc = Ifc::new(&f);
    let voids = void_map(&f);
    for id in products(&f) {
        if let Ok(p) = f.get(id) {
            if let Ok(g) = ifc.element(&p, voids.get(&id).map(|v| v.as_slice()).unwrap_or(&[])) {
                n += g.mesh.tri.len();
            }
        }
    }
    n
}

/// The same for ifc-lite: decode every entity and run its geometry pipeline.
pub fn run_lite(data: &[u8]) -> usize {
    let mut n = 0;
    let mut scanner = ifc_lite_core::EntityScanner::new(data);
    let mut dec = ifc_lite_core::EntityDecoder::new(data);
    while let Some((_, _, s, e)) = scanner.next_entity() {
        if let Ok(ent) = dec.decode_at_uncached(s, e) {
            n += ent.attributes.len();
        }
    }
    let r = ifc_lite_processing::process_geometry(data);
    n + r.meshes.len()
}
