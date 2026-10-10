//! The M1 mapping: HITO element parameters to IFC4 / IFC4X3 entities.
//!
//! Follows the mapping table in docs/architecture/element-model.md. Each
//! function writes one IFC concept; the attribute order is the schema's.

use crate::args;
use crate::guid;
use crate::model::*;
use crate::step::{Header, Id, StepWriter, V};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Schema {
    Ifc4,
    Ifc4x3,
}

impl Schema {
    pub fn name(self) -> &'static str {
        match self {
            Schema::Ifc4 => "IFC4",
            Schema::Ifc4x3 => "IFC4X3_ADD2",
        }
    }
}

/// What a correct reader should find, written next to the IFC file and
/// checked by validate/check.py.
pub struct Expected {
    pub guid: String,
    pub class: &'static str,
    pub name: String,
    pub storey: String,
    pub type_name: Option<String>,
    pub material: String,
    pub volume: f64,
    pub bbox: ([f64; 3], [f64; 3]),
    /// (pset, property, value as text)
    pub props: Vec<(String, String, String)>,
}

fn typed(t: &'static str, v: impl Into<V>) -> V {
    V::Typed(t, Box::new(v.into()))
}

pub struct Exporter<'m> {
    w: StepWriter,
    schema: Schema,
    /// Write IfcGridPlacement for grid-hosted columns instead of a resolved IfcLocalPlacement.
    use_grid_placement: bool,
    m: &'m Model,
    oh: Id,
    body: Id,
    axis_ctx: Id,
    footprint: Id,
    origin: Id,
    z: Id,
    storey_ids: Vec<Id>,
    storey_pl: Vec<Id>,
    contained: Vec<Vec<Id>>,
    materials: Vec<Id>,
    grid: Option<(Id, Vec<Id>, Vec<Id>)>,
    types: Vec<TypeRec>,
    column_ids: Vec<Id>,
    pub expected: Vec<Expected>,
}

/// A written IFC type and the instances that use it.
struct TypeRec {
    id: Id,
    seed: String,
    profile: Id,
    profile_set: Id,
    instances: Vec<Id>,
}

impl<'m> Exporter<'m> {
    pub fn export(m: &'m Model, schema: Schema, use_grid_placement: bool) -> (String, Vec<Expected>, u64) {
        let placeholder = Id(0);
        let mut e = Exporter {
            w: StepWriter::new(),
            schema,
            use_grid_placement,
            m,
            oh: placeholder,
            body: placeholder,
            axis_ctx: placeholder,
            footprint: placeholder,
            origin: placeholder,
            z: placeholder,
            storey_ids: vec![],
            storey_pl: vec![],
            contained: vec![vec![]; m.levels.len()],
            materials: vec![],
            grid: None,
            types: vec![],
            column_ids: vec![],
            expected: vec![],
        };
        e.project();
        e.materials();
        e.grid();
        e.frame_types();
        e.columns();
        e.beams();
        e.slabs();
        e.walls();
        e.footings();
        e.containment();
        let count = e.w.entity_count();
        let text = e.w.finish(&Header {
            description: "ViewDefinition [ReferenceView_V1.2]",
            file_name: "m1-sample.ifc",
            timestamp: "2026-10-08T12:00:00",
            author: "HITO spike #3",
            organization: "HITO",
            preprocessor: "hito ifc-export-spike 0.0.0",
            originating_system: "HITO",
            schema: schema.name(),
        });
        (text, e.expected, count)
    }

    fn add(&mut self, name: &str, a: Vec<V>) -> Id {
        self.w.add(name, a)
    }

    fn gid(&self, seed: &str) -> String {
        guid::compress(guid::from_seed(seed))
    }

    // ---------- geometry primitives ----------

    fn pt3(&mut self, x: f64, y: f64, z: f64) -> Id {
        self.add("IFCCARTESIANPOINT", args![vec![x, y, z]])
    }
    fn pt2(&mut self, x: f64, y: f64) -> Id {
        self.add("IFCCARTESIANPOINT", args![vec![x, y]])
    }
    fn dir(&mut self, d: &[f64]) -> Id {
        self.add("IFCDIRECTION", args![d.to_vec()])
    }
    fn axis3(&mut self, loc: (f64, f64, f64), z: Option<[f64; 3]>, x: Option<[f64; 3]>) -> Id {
        let p = if loc == (0.0, 0.0, 0.0) { self.origin } else { self.pt3(loc.0, loc.1, loc.2) };
        let z = z.map(|d| self.dir(&d));
        let x = x.map(|d| self.dir(&d));
        self.add("IFCAXIS2PLACEMENT3D", args![p, z, x])
    }
    fn local_placement(&mut self, rel: Option<Id>, loc: (f64, f64, f64), z: Option<[f64; 3]>, x: Option<[f64; 3]>) -> Id {
        let a = self.axis3(loc, z, x);
        self.add("IFCLOCALPLACEMENT", args![rel, a])
    }
    fn polyline2(&mut self, pts: &[(f64, f64)], close: bool) -> Id {
        let mut ids: Vec<Id> = pts.iter().map(|&(x, y)| self.pt2(x, y)).collect();
        if close {
            ids.push(ids[0]);
        }
        self.add("IFCPOLYLINE", args![ids])
    }
    fn profile(&mut self, name: &str, s: Section, centre: Option<(f64, f64)>) -> Id {
        let pos = centre.map(|(x, y)| {
            let p = self.pt2(x, y);
            self.add("IFCAXIS2PLACEMENT2D", args![p, V::Null])
        });
        match s {
            Section::Rect { b, h } => self.add("IFCRECTANGLEPROFILEDEF", args![V::Enum("AREA"), name, pos, b, h]),
            Section::Circle { r } => self.add("IFCCIRCLEPROFILEDEF", args![V::Enum("AREA"), name, pos, r]),
        }
    }
    fn extrude(&mut self, profile: Id, pos: Option<Id>, depth: f64) -> Id {
        let z = self.z;
        self.add("IFCEXTRUDEDAREASOLID", args![profile, pos, z, depth])
    }
    fn shape(&mut self, ctx: Id, ident: &str, kind: &str, items: Vec<Id>) -> Id {
        self.add("IFCSHAPEREPRESENTATION", args![ctx, ident, kind, items])
    }
    fn product_shape(&mut self, reps: Vec<Id>) -> Id {
        self.add("IFCPRODUCTDEFINITIONSHAPE", args![V::Null, V::Null, reps])
    }

    // ---------- relationships and property sets ----------

    fn rel(&mut self, entity: &str, seed: &str, tail: Vec<V>) -> Id {
        let mut a = args![self.gid(seed), self.oh, V::Null, V::Null];
        a.extend(tail);
        self.add(entity, a)
    }
    /// A property set on its own. Types reference it in HasPropertySets.
    fn property_set(&mut self, owner_seed: &str, name: &str, props: Vec<(&str, V)>) -> Id {
        let ids: Vec<Id> = props
            .into_iter()
            .map(|(n, v)| self.add("IFCPROPERTYSINGLEVALUE", args![n, V::Null, v, V::Null]))
            .collect();
        self.add("IFCPROPERTYSET", args![self.gid(&format!("{owner_seed}/{name}")), self.oh, name, V::Null, ids])
    }
    /// A property set on an occurrence, through IfcRelDefinesByProperties.
    /// (Rule NoRelatedTypeObject forbids this relationship for types.)
    fn pset(&mut self, owner_seed: &str, related: Id, name: &str, props: Vec<(&str, V)>) {
        let ps = self.property_set(owner_seed, name, props);
        self.rel("IFCRELDEFINESBYPROPERTIES", &format!("{owner_seed}/{name}/rel"), args![vec![related], ps]);
    }
    fn qto(&mut self, owner_seed: &str, related: Id, name: &str, q: Vec<(&str, &str, f64)>) {
        let ids: Vec<Id> = q
            .into_iter()
            .map(|(n, kind, v)| self.add(kind, args![n, V::Null, V::Null, v, V::Null]))
            .collect();
        let qs = self.add(
            "IFCELEMENTQUANTITY",
            // Rule QTY001: standard Qto_ sets say MethodOfMeasurement 'BaseQuantities'.
            args![self.gid(&format!("{owner_seed}/{name}")), self.oh, name, V::Null, "BaseQuantities", ids],
        );
        self.rel("IFCRELDEFINESBYPROPERTIES", &format!("{owner_seed}/{name}/rel"), args![vec![related], qs]);
    }
    fn associate_material(&mut self, seed: &str, related: Vec<Id>, material: Id) {
        self.rel("IFCRELASSOCIATESMATERIAL", &format!("{seed}/material"), args![related, material]);
    }

    // ---------- project, units, contexts, spatial structure ----------

    fn project(&mut self) {
        let person = self.add("IFCPERSON", args![V::Null, "", V::Null, V::Null, V::Null, V::Null, V::Null, V::Null]);
        let org = self.add("IFCORGANIZATION", args![V::Null, "HITO", V::Null, V::Null, V::Null]);
        let pao = self.add("IFCPERSONANDORGANIZATION", args![person, org, V::Null]);
        let app = self.add("IFCAPPLICATION", args![org, "0.0.0", "HITO", "HITO"]);
        self.oh = self.add(
            "IFCOWNERHISTORY",
            // Rule CorrectChangeAction: ADDED needs LastModifiedDate.
            args![pao, app, V::Null, V::Enum("ADDED"), 1_791_460_800i64, V::Null, V::Null, 1_791_460_800i64],
        );

        self.origin = self.pt3(0.0, 0.0, 0.0);
        self.z = self.dir(&[0.0, 0.0, 1.0]);
        let wcs = self.add("IFCAXIS2PLACEMENT3D", args![self.origin, V::Null, V::Null]);
        let ctx = self.add("IFCGEOMETRICREPRESENTATIONCONTEXT", args![V::Null, "Model", 3i64, 1e-5, wcs, V::Null]);
        let sub = |s: &mut Self, ident: &str, view: &'static str| {
            s.add(
                "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
                args![ident, "Model", V::Derived, V::Derived, V::Derived, V::Derived, ctx, V::Null, V::Enum(view), V::Null],
            )
        };
        self.body = sub(self, "Body", "MODEL_VIEW");
        self.axis_ctx = sub(self, "Axis", "GRAPH_VIEW");
        self.footprint = sub(self, "FootPrint", "SKETCH_VIEW");

        let si = |s: &mut Self, kind: &'static str, prefix: Option<&'static str>, name: &'static str| {
            s.add("IFCSIUNIT", args![V::Derived, V::Enum(kind), prefix.map(V::Enum), V::Enum(name)])
        };
        let units = vec![
            si(self, "LENGTHUNIT", None, "METRE"),
            si(self, "AREAUNIT", None, "SQUARE_METRE"),
            si(self, "VOLUMEUNIT", None, "CUBIC_METRE"),
            si(self, "PLANEANGLEUNIT", None, "RADIAN"),
            si(self, "PRESSUREUNIT", Some("MEGA"), "PASCAL"),
        ];
        let metre = units[0];
        let ua = self.add("IFCUNITASSIGNMENT", args![units]);

        // Georeference (#93), rule GRF003: the project's local origin on a
        // projected CRS. Argentina uses POSGAR 2007 Gauss-Krüger zones.
        if let Some(g) = &self.m.georef {
            let crs = self.add(
                "IFCPROJECTEDCRS",
                args![g.crs.as_str(), g.crs_description.as_str(), V::Null, V::Null, V::Null, V::Null, metre],
            );
            let (c, s) = (g.rotation.cos(), g.rotation.sin());
            self.add(
                "IFCMAPCONVERSION",
                args![ctx, crs, g.eastings, g.northings, g.height, c, s, V::Null],
            );
        }

        let project = self.add(
            "IFCPROJECT",
            args![self.gid("project"), self.oh, self.m.name.as_str(), V::Null, V::Null, V::Null, V::Null, vec![ctx], ua],
        );
        let site_pl = self.local_placement(None, (0.0, 0.0, 0.0), None, None);
        let site = self.add(
            "IFCSITE",
            args![self.gid("site"), self.oh, "Terreno", V::Null, V::Null, site_pl, V::Null, V::Null, V::Enum("ELEMENT"),
                V::Null, V::Null, V::Null, V::Null, V::Null],
        );
        let bldg_pl = self.local_placement(Some(site_pl), (0.0, 0.0, 0.0), None, None);
        let bldg = self.add(
            "IFCBUILDING",
            args![self.gid("building"), self.oh, "Edificio", V::Null, V::Null, bldg_pl, V::Null, V::Null,
                V::Enum("ELEMENT"), V::Null, V::Null, V::Null],
        );
        let m = self.m;
        for (i, l) in m.levels.iter().enumerate() {
            let pl = self.local_placement(Some(bldg_pl), (0.0, 0.0, l.elevation), None, None);
            // IFC 4.3 deprecates Elevation (rule IFC102): the placement carries it.
            let elevation = match self.schema {
                Schema::Ifc4 => V::Real(l.elevation),
                Schema::Ifc4x3 => V::Null,
            };
            let s = self.add(
                "IFCBUILDINGSTOREY",
                args![self.gid(&format!("level/{i}")), self.oh, l.name.as_str(), V::Null, V::Null, pl, V::Null, V::Null,
                    V::Enum("ELEMENT"), elevation],
            );
            self.storey_pl.push(pl);
            self.storey_ids.push(s);
        }
        self.rel("IFCRELAGGREGATES", "agg/project", args![project, vec![site]]);
        self.rel("IFCRELAGGREGATES", "agg/site", args![site, vec![bldg]]);
        let storeys = self.storey_ids.clone();
        self.rel("IFCRELAGGREGATES", "agg/building", args![bldg, storeys]);
    }

    fn materials(&mut self) {
        let m = self.m;
        for mat in m.materials.iter() {
            let id = self.add("IFCMATERIAL", args![mat.name.as_str(), V::Null, "concrete"]);
            let fck = self.add(
                "IFCPROPERTYSINGLEVALUE",
                args!["CompressiveStrength", V::Null, typed("IFCPRESSUREMEASURE", mat.fck_mpa), V::Null],
            );
            self.add("IFCMATERIALPROPERTIES", args!["Pset_MaterialConcrete", V::Null, vec![fck], id]);
            self.materials.push(id);
        }
    }

    fn grid(&mut self) {
        let m = self.m;
        let g = &m.grid;
        let (x0, y0, x1, y1) = g.extent;
        let mut curves = vec![];
        let mut axis = |s: &mut Self, a: &GridAxis, pts: [(f64, f64); 2]| {
            let c = s.polyline2(&pts, false);
            curves.push(c);
            s.add("IFCGRIDAXIS", args![a.tag.as_str(), c, true])
        };
        let u: Vec<Id> = g.u.iter().map(|a| axis(self, a, [(a.offset, y0), (a.offset, y1)])).collect();
        let v: Vec<Id> = g.v.iter().map(|a| axis(self, a, [(x0, a.offset), (x1, a.offset)])).collect();
        let set = self.add("IFCGEOMETRICCURVESET", args![curves]);
        let fp = self.footprint;
        let rep = self.shape(fp, "FootPrint", "GeometricCurveSet", vec![set]);
        let ps = self.product_shape(vec![rep]);
        let pl = self.local_placement(Some(self.storey_pl[0]), (0.0, 0.0, 0.0), None, None);
        let grid = self.add(
            "IFCGRID",
            args![self.gid("grid"), self.oh, g.name.as_str(), V::Null, V::Null, pl, ps, u.clone(), v.clone(), V::Null, V::Null],
        );
        self.contained[0].push(grid);
        self.grid = Some((pl, u, v));
    }

    // ---------- types ----------

    /// One IFC type per frame type. The profile is shared by the type's
    /// material profile set and by every instance's extrusion.
    fn frame_types(&mut self) {
        let m = self.m;
        for (i, t) in m.frame_types.iter().enumerate() {
            let is_column = m.columns.iter().any(|c| c.ty == i);
            let profile = self.profile(&t.name, t.section, None);
            let mp = self.add(
                "IFCMATERIALPROFILE",
                args![t.name.as_str(), V::Null, self.materials[t.material], profile, V::Null, V::Null],
            );
            let mps = self.add("IFCMATERIALPROFILESET", args![t.name.as_str(), V::Null, vec![mp], V::Null]);
            let (class, pset, predef) = if is_column {
                ("IFCCOLUMNTYPE", "Pset_ColumnCommon", "COLUMN")
            } else {
                ("IFCBEAMTYPE", "Pset_BeamCommon", "BEAM")
            };
            let seed = format!("type/{i}");
            let ps = self.property_set(&seed, pset, vec![("Reference", typed("IFCIDENTIFIER", t.name.as_str()))]);
            let id = self.type_object(class, &seed, &t.name, predef, vec![ps]);
            self.associate_material(&seed, vec![id], mps);
            self.types.push(TypeRec { id, seed, profile, profile_set: mps, instances: vec![] });
        }
    }

    fn type_object(&mut self, class: &str, seed: &str, name: &str, predef: &'static str, psets: Vec<Id>) -> Id {
        let psets = if psets.is_empty() { V::Null } else { psets.into() };
        self.add(
            class,
            args![self.gid(seed), self.oh, name, V::Null, V::Null, psets, V::Null, V::Null, V::Null, V::Enum(predef)],
        )
    }

    /// A layer-set type (slab or wall) with one concrete layer.
    /// Returns the index into `self.types` and the layer set.
    fn layer_type(&mut self, class: &str, seed: &str, name: &str, predef: &'static str, material: usize, t: f64) -> (usize, Id) {
        let layer = self.add(
            "IFCMATERIALLAYER",
            args![self.materials[material], t, V::Null, name, V::Null, "LoadBearing", V::Null],
        );
        let set = self.add("IFCMATERIALLAYERSET", args![vec![layer], name, V::Null]);
        let id = self.type_object(class, seed, name, predef, vec![]);
        self.associate_material(seed, vec![id], set);
        self.types.push(TypeRec { id, seed: seed.to_owned(), profile: Id(0), profile_set: set, instances: vec![] });
        (self.types.len() - 1, set)
    }

    // ---------- elements ----------

    fn grid_placement(&mut self, u: usize, v: usize) -> Id {
        let (gpl, ua, va) = self.grid.clone().expect("grid written first");
        let vgi = self.add("IFCVIRTUALGRIDINTERSECTION", args![vec![ua[u], va[v]], vec![0.0, 0.0, 0.0]]);
        match self.schema {
            Schema::Ifc4 => self.add("IFCGRIDPLACEMENT", args![vgi, V::Null]),
            // IFC 4.3 moved PlacementRelTo up to IfcObjectPlacement.
            Schema::Ifc4x3 => self.add("IFCGRIDPLACEMENT", args![gpl, vgi, V::Null]),
        }
    }

    /// `predef` is None when the element's type already sets PredefinedType
    /// (rule OJT001: the occurrence must then leave it empty).
    fn element(&mut self, class: &str, seed: &str, name: &str, pl: Id, ps: Id, predef: Option<&'static str>) -> Id {
        self.add(
            class,
            args![self.gid(seed), self.oh, name, V::Null, V::Null, pl, ps, name, predef.map(V::Enum)],
        )
    }

    fn material_name(&self, i: usize) -> String {
        self.m.materials[i].name.clone()
    }

    fn fck_prop(&self, i: usize) -> (String, String, String) {
        ("Pset_MaterialConcrete".into(), "CompressiveStrength".into(), format!("{}", self.m.materials[i].fck_mpa))
    }

    fn columns(&mut self) {
        let m = self.m;
        for (ci, c) in m.columns.iter().enumerate() {
            let t = &m.frame_types[c.ty];
            let (profile, mps) = (self.types[c.ty].profile, self.types[c.ty].profile_set);
            let base = m.levels[c.base_level].elevation;
            let height = m.levels[c.top_level].elevation - base;
            let (x, y) = m.column_xy(c);
            let pl = match c.at {
                ColumnAt::Grid(u, v) if self.use_grid_placement => self.grid_placement(u, v),
                // Default: the grid position resolved to coordinates (element model §7 fallback).
                ColumnAt::Grid(..) => self.local_placement(Some(self.storey_pl[c.base_level]), (x, y, 0.0), None, None),
                ColumnAt::Point(x, y) => self.local_placement(Some(self.storey_pl[c.base_level]), (x, y, 0.0), None, None),
            };
            let solid = self.extrude(profile, None, height);
            let rep = self.shape(self.body, "Body", "SweptSolid", vec![solid]);
            let ps = self.product_shape(vec![rep]);
            let seed = format!("column/{ci}");
            let id = self.element("IFCCOLUMN", &seed, &c.mark, pl, ps, None);
            let usage = self.add("IFCMATERIALPROFILESETUSAGE", args![mps, 5i64, V::Null]);
            self.associate_material(&seed, vec![id], usage);
            self.pset(&seed, id, "Pset_ColumnCommon", vec![("LoadBearing", typed("IFCBOOLEAN", true))]);
            // Exact quantity from parameters (SR-15): pi r^2 h for the round column.
            let vol = t.section.area() * height;
            self.qto(
                &seed,
                id,
                "Qto_ColumnBaseQuantities",
                vec![
                    ("Length", "IFCQUANTITYLENGTH", height),
                    ("CrossSectionArea", "IFCQUANTITYAREA", t.section.area()),
                    ("NetVolume", "IFCQUANTITYVOLUME", vol),
                ],
            );
            self.types[c.ty].instances.push(id);
            self.contained[c.base_level].push(id);
            self.column_ids.push(id);
            let (hx, hy) = t.section.half_extents();
            self.expected.push(Expected {
                guid: self.gid(&seed),
                class: "IfcColumn",
                name: c.mark.clone(),
                storey: m.levels[c.base_level].name.clone(),
                type_name: Some(t.name.clone()),
                material: self.material_name(t.material),
                volume: vol,
                bbox: ([x - hx, y - hy, base], [x + hx, y + hy, base + height]),
                props: vec![
                    ("Pset_ColumnCommon".into(), "LoadBearing".into(), "True".into()),
                    ("Pset_ColumnCommon".into(), "Reference".into(), t.name.clone()),
                    ("Qto_ColumnBaseQuantities".into(), "NetVolume".into(), format!("{vol}")),
                    self.fck_prop(t.material),
                ],
            });
        }
    }

    fn beams(&mut self) {
        let m = self.m;
        for (bi, b) in m.beams.iter().enumerate() {
            let t = &m.frame_types[b.ty];
            let Section::Rect { b: bw, h } = t.section else { panic!("rectangular beams only in the spike") };
            let (profile, mps) = (self.types[b.ty].profile, self.types[b.ty].profile_set);
            let elev = m.levels[b.level].elevation;
            let (dx, dy) = (b.end.0 - b.start.0, b.end.1 - b.start.1);
            let len = (dx * dx + dy * dy).sqrt();
            let (ux, uy) = (dx / len, dy / len);
            // Local Z along the beam, local Y up, so the profile's YDim is the depth.
            let pl = self.local_placement(
                Some(self.storey_pl[b.level]),
                (b.start.0, b.start.1, -h / 2.0),
                Some([ux, uy, 0.0]),
                Some([-uy, ux, 0.0]),
            );
            let solid = self.extrude(profile, None, len);
            let body = self.shape(self.body, "Body", "SweptSolid", vec![solid]);
            let p0 = self.origin;
            let p1 = self.pt3(0.0, 0.0, len);
            let line = self.add("IFCPOLYLINE", args![vec![p0, p1]]);
            let axis = self.shape(self.axis_ctx, "Axis", "Curve3D", vec![line]);
            let ps = self.product_shape(vec![axis, body]);
            let seed = format!("beam/{bi}");
            let id = self.element("IFCBEAM", &seed, &b.mark, pl, ps, None);
            let usage = self.add("IFCMATERIALPROFILESETUSAGE", args![mps, 5i64, V::Null]);
            self.associate_material(&seed, vec![id], usage);
            self.pset(
                &seed,
                id,
                "Pset_BeamCommon",
                vec![("LoadBearing", typed("IFCBOOLEAN", true)), ("Span", typed("IFCPOSITIVELENGTHMEASURE", len))],
            );
            let vol = t.section.area() * len;
            self.qto(
                &seed,
                id,
                "Qto_BeamBaseQuantities",
                vec![("Length", "IFCQUANTITYLENGTH", len), ("NetVolume", "IFCQUANTITYVOLUME", vol)],
            );
            for &c in &b.frames_into {
                let col = self.column_ids[c];
                self.rel("IFCRELCONNECTSELEMENTS", &format!("{seed}/frames/{c}"), args![V::Null, col, id]);
            }
            self.types[b.ty].instances.push(id);
            self.contained[b.level].push(id);
            let (px, py) = (-uy * bw / 2.0, ux * bw / 2.0);
            let xs = [b.start.0 + px, b.start.0 - px, b.end.0 + px, b.end.0 - px];
            let ys = [b.start.1 + py, b.start.1 - py, b.end.1 + py, b.end.1 - py];
            self.expected.push(Expected {
                guid: self.gid(&seed),
                class: "IfcBeam",
                name: b.mark.clone(),
                storey: m.levels[b.level].name.clone(),
                type_name: Some(t.name.clone()),
                material: self.material_name(t.material),
                volume: vol,
                bbox: ([min(&xs), min(&ys), elev - h], [max(&xs), max(&ys), elev]),
                props: vec![
                    ("Pset_BeamCommon".into(), "Span".into(), format!("{len}")),
                    ("Qto_BeamBaseQuantities".into(), "NetVolume".into(), format!("{vol}")),
                    self.fck_prop(t.material),
                ],
            });
        }
    }

    fn opening(&mut self, seed: &str, host: Id, host_pl: Id, profile: Id, z0: f64, depth: f64) {
        let pos = self.axis3((0.0, 0.0, z0), None, None);
        let solid = self.extrude(profile, Some(pos), depth);
        let rep = self.shape(self.body, "Body", "SweptSolid", vec![solid]);
        let ps = self.product_shape(vec![rep]);
        let pl = self.local_placement(Some(host_pl), (0.0, 0.0, 0.0), None, None);
        let op = self.element("IFCOPENINGELEMENT", seed, "Abertura", pl, ps, Some("OPENING"));
        self.rel("IFCRELVOIDSELEMENT", &format!("{seed}/voids"), args![host, op]);
    }

    fn slabs(&mut self) {
        let m = self.m;
        for (si, s) in m.slabs.iter().enumerate() {
            let seed = format!("slab/{si}");
            let tname = format!("Losa {:.0} cm", s.thickness * 100.0);
            let (ti, set) = self.layer_type("IFCSLABTYPE", &format!("{seed}/type"), &tname, "FLOOR", s.material, s.thickness);
            let elev = m.levels[s.level].elevation;
            let pl = self.local_placement(Some(self.storey_pl[s.level]), (0.0, 0.0, -s.thickness), None, None);
            let outline = self.polyline2(&s.outline, true);
            let profile = self.add("IFCARBITRARYCLOSEDPROFILEDEF", args![V::Enum("AREA"), V::Null, outline]);
            let solid = self.extrude(profile, None, s.thickness);
            let rep = self.shape(self.body, "Body", "SweptSolid", vec![solid]);
            let ps = self.product_shape(vec![rep]);
            let id = self.element("IFCSLAB", &seed, &s.mark, pl, ps, None);
            let usage = self.add(
                "IFCMATERIALLAYERSETUSAGE",
                args![set, V::Enum("AXIS3"), V::Enum("POSITIVE"), 0.0, V::Null],
            );
            self.associate_material(&seed, vec![id], usage);
            let mut holes = 0.0;
            for (oi, o) in s.openings.iter().enumerate() {
                let p = self.profile("", Section::Rect { b: o.w, h: o.h }, Some((o.x + o.w / 2.0, o.y + o.h / 2.0)));
                self.opening(&format!("{seed}/opening/{oi}"), id, pl, p, -0.05, s.thickness + 0.1);
                holes += o.w * o.h;
            }
            let area = polygon_area(&s.outline) - holes;
            let vol = area * s.thickness;
            self.pset(&seed, id, "Pset_SlabCommon", vec![("LoadBearing", typed("IFCBOOLEAN", true))]);
            self.qto(
                &seed,
                id,
                "Qto_SlabBaseQuantities",
                vec![
                    ("Width", "IFCQUANTITYLENGTH", s.thickness),
                    ("NetArea", "IFCQUANTITYAREA", area),
                    ("NetVolume", "IFCQUANTITYVOLUME", vol),
                ],
            );
            self.types[ti].instances.push(id);
            self.contained[s.level].push(id);
            let xs: Vec<f64> = s.outline.iter().map(|p| p.0).collect();
            let ys: Vec<f64> = s.outline.iter().map(|p| p.1).collect();
            self.expected.push(Expected {
                guid: self.gid(&seed),
                class: "IfcSlab",
                name: s.mark.clone(),
                storey: m.levels[s.level].name.clone(),
                type_name: Some(tname),
                material: self.material_name(s.material),
                volume: vol,
                bbox: ([min(&xs), min(&ys), elev - s.thickness], [max(&xs), max(&ys), elev]),
                props: vec![
                    ("Qto_SlabBaseQuantities".into(), "NetArea".into(), format!("{area}")),
                    self.fck_prop(s.material),
                ],
            });
        }
    }

    fn walls(&mut self) {
        let m = self.m;
        for (wi, w) in m.walls.iter().enumerate() {
            let seed = format!("wall/{wi}");
            let tname = format!("Muro H°A° {:.0} cm", w.thickness * 100.0);
            let (ti, set) = self.layer_type("IFCWALLTYPE", &format!("{seed}/type"), &tname, "SOLIDWALL", w.material, w.thickness);
            let base = m.levels[w.base_level].elevation;
            let (dx, dy) = (w.end.0 - w.start.0, w.end.1 - w.start.1);
            let len = (dx * dx + dy * dy).sqrt();
            let (ux, uy) = (dx / len, dy / len);
            let pl = self.local_placement(
                Some(self.storey_pl[w.base_level]),
                (w.start.0, w.start.1, 0.0),
                Some([0.0, 0.0, 1.0]),
                Some([ux, uy, 0.0]),
            );
            let axis_line = self.polyline2(&[(0.0, 0.0), (len, 0.0)], false);
            let axis = self.shape(self.axis_ctx, "Axis", "Curve2D", vec![axis_line]);
            let profile = self.profile("", Section::Rect { b: len, h: w.thickness }, Some((len / 2.0, 0.0)));
            let solid = self.extrude(profile, None, w.height);
            let body = self.shape(self.body, "Body", "SweptSolid", vec![solid]);
            let ps = self.product_shape(vec![axis, body]);
            let id = self.element("IFCWALL", &seed, &w.mark, pl, ps, None);
            let usage = self.add(
                "IFCMATERIALLAYERSETUSAGE",
                args![set, V::Enum("AXIS2"), V::Enum("POSITIVE"), -w.thickness / 2.0, V::Null],
            );
            self.associate_material(&seed, vec![id], usage);
            let mut holes = 0.0;
            for (oi, o) in w.openings.iter().enumerate() {
                let p = self.profile("", Section::Rect { b: o.w, h: w.thickness + 0.1 }, Some((o.x + o.w / 2.0, 0.0)));
                self.opening(&format!("{seed}/opening/{oi}"), id, pl, p, o.y, o.h);
                holes += o.w * o.h * w.thickness;
            }
            let vol = len * w.thickness * w.height - holes;
            self.pset(&seed, id, "Pset_WallCommon", vec![("LoadBearing", typed("IFCBOOLEAN", true))]);
            self.qto(
                &seed,
                id,
                "Qto_WallBaseQuantities",
                vec![
                    ("Length", "IFCQUANTITYLENGTH", len),
                    ("Width", "IFCQUANTITYLENGTH", w.thickness),
                    ("Height", "IFCQUANTITYLENGTH", w.height),
                    ("NetVolume", "IFCQUANTITYVOLUME", vol),
                ],
            );
            self.types[ti].instances.push(id);
            self.contained[w.base_level].push(id);
            let (px, py) = (-uy * w.thickness / 2.0, ux * w.thickness / 2.0);
            let xs = [w.start.0 + px, w.start.0 - px, w.end.0 + px, w.end.0 - px];
            let ys = [w.start.1 + py, w.start.1 - py, w.end.1 + py, w.end.1 - py];
            self.expected.push(Expected {
                guid: self.gid(&seed),
                class: "IfcWall",
                name: w.mark.clone(),
                storey: m.levels[w.base_level].name.clone(),
                type_name: Some(tname),
                material: self.material_name(w.material),
                volume: vol,
                bbox: ([min(&xs), min(&ys), base], [max(&xs), max(&ys), base + w.height]),
                props: vec![("Pset_WallCommon".into(), "LoadBearing".into(), "True".into()), self.fck_prop(w.material)],
            });
        }
    }

    fn footings(&mut self) {
        let m = self.m;
        for (fi, f) in m.footings.iter().enumerate() {
            let seed = format!("footing/{fi}");
            let elev = m.levels[f.level].elevation;
            let pl = self.local_placement(Some(self.storey_pl[f.level]), (f.centre.0, f.centre.1, -f.depth), None, None);
            let p = self.profile("", Section::Rect { b: f.size.0, h: f.size.1 }, None);
            let solid = self.extrude(p, None, f.depth);
            let rep = self.shape(self.body, "Body", "SweptSolid", vec![solid]);
            let ps = self.product_shape(vec![rep]);
            let id = self.element("IFCFOOTING", &seed, &f.mark, pl, ps, Some("PAD_FOOTING"));
            let mat = self.materials[f.material];
            self.associate_material(&seed, vec![id], mat);
            self.pset(&seed, id, "Pset_FootingCommon", vec![("LoadBearing", typed("IFCBOOLEAN", true))]);
            let vol = f.size.0 * f.size.1 * f.depth;
            self.qto(&seed, id, "Qto_FootingBaseQuantities", vec![("NetVolume", "IFCQUANTITYVOLUME", vol)]);
            self.contained[f.level].push(id);
            let (hx, hy) = (f.size.0 / 2.0, f.size.1 / 2.0);
            self.expected.push(Expected {
                guid: self.gid(&seed),
                class: "IfcFooting",
                name: f.mark.clone(),
                storey: m.levels[f.level].name.clone(),
                type_name: None,
                material: self.material_name(f.material),
                volume: vol,
                bbox: ([f.centre.0 - hx, f.centre.1 - hy, elev - f.depth], [f.centre.0 + hx, f.centre.1 + hy, elev]),
                props: vec![("Pset_FootingCommon".into(), "LoadBearing".into(), "True".into()), self.fck_prop(f.material)],
            });
        }
    }

    fn containment(&mut self) {
        for i in 0..self.types.len() {
            if self.types[i].instances.is_empty() {
                continue;
            }
            let (id, seed, inst) = (self.types[i].id, self.types[i].seed.clone(), self.types[i].instances.clone());
            self.rel("IFCRELDEFINESBYTYPE", &format!("{seed}/rel"), args![inst, id]);
        }
        for i in 0..self.storey_ids.len() {
            let (s, elems) = (self.storey_ids[i], self.contained[i].clone());
            if !elems.is_empty() {
                self.rel("IFCRELCONTAINEDINSPATIALSTRUCTURE", &format!("contains/{i}"), args![elems, s]);
            }
        }
    }
}

fn min(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::INFINITY, f64::min)
}
fn max(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

/// expected.json, written by hand to keep the spike dependency-free.
pub fn expected_json(schema: Schema, items: &[Expected]) -> String {
    fn s(x: &str) -> String {
        format!("\"{}\"", x.replace('\\', "\\\\").replace('"', "\\\""))
    }
    fn arr(a: &[f64; 3]) -> String {
        format!("[{:?}, {:?}, {:?}]", a[0], a[1], a[2])
    }
    let mut out = format!("{{\n  \"schema\": {},\n  \"elements\": [\n", s(schema.name()));
    for (i, e) in items.iter().enumerate() {
        let props: Vec<String> = e.props.iter().map(|(p, n, v)| format!("[{}, {}, {}]", s(p), s(n), s(v))).collect();
        out.push_str(&format!(
            "    {{\"guid\": {}, \"class\": {}, \"name\": {}, \"storey\": {}, \"type\": {}, \"material\": {}, \"volume\": {:?}, \"bbox_min\": {}, \"bbox_max\": {}, \"props\": [{}]}}{}\n",
            s(&e.guid),
            s(e.class),
            s(&e.name),
            s(&e.storey),
            e.type_name.as_deref().map_or("null".into(), s),
            s(&e.material),
            e.volume,
            arr(&e.bbox.0),
            arr(&e.bbox.1),
            props.join(", "),
            if i + 1 < items.len() { "," } else { "" }
        ));
    }
    out.push_str("  ]\n}\n");
    out
}
