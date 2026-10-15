//! The IFC side of candidate B: placements, units, profiles and the common
//! representation items turned into triangle meshes, on top of `p21`.
//!
//! This is the work HITO would own if it wrote its own reader. Profiles and
//! extrusions are evaluated here, as ADR 0014 assigns them to our own code.
//! Booleans (openings, clippings, `IfcBooleanResult`) go to Manifold, standing
//! in for the geometry port.
//!
//! Anything not handled returns `Err(Unsupported(type))`, so the corpus run
//! can count what is missing.

use crate::p21::{File, Instance, Value};
use manifold_csg::{Manifold, MeshGL64};
use std::cell::RefCell;
use std::collections::HashMap;

pub type V3 = [f64; 3];
pub type V2 = [f64; 2];

/// Recursion budget for placements, mapped items and boolean trees.
const MAX_NEST: usize = 64;
/// Segments for a full circle.
const CIRCLE_SEGMENTS: usize = 48;

#[derive(Debug, Clone)]
pub enum GeomError {
    Unsupported(String),
    Invalid(String),
    Boolean(String),
}

impl GeomError {
    pub fn label(&self) -> String {
        match self {
            GeomError::Unsupported(t) => format!("unsupported {t}"),
            GeomError::Invalid(t) => format!("invalid {t}"),
            GeomError::Boolean(t) => format!("boolean failed: {t}"),
        }
    }
}

type R<T> = Result<T, GeomError>;

fn inv(s: &str) -> GeomError {
    GeomError::Invalid(s.to_string())
}

/// Affine transform, rows of [x y z t].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xf(pub [[f64; 4]; 3]);

impl Xf {
    pub const ID: Xf = Xf([[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]]);
    pub fn apply(&self, p: V3) -> V3 {
        let m = &self.0;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
        ]
    }
    pub fn dir(&self, p: V3) -> V3 {
        let m = &self.0;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2],
        ]
    }
    /// self * o (apply o first)
    pub fn mul(&self, o: &Xf) -> Xf {
        let a = &self.0;
        let b = &o.0;
        let mut r = [[0.0; 4]; 3];
        for i in 0..3 {
            for j in 0..4 {
                r[i][j] = a[i][0] * b[0][j]
                    + a[i][1] * b[1][j]
                    + a[i][2] * b[2][j]
                    + if j == 3 { a[i][3] } else { 0.0 };
            }
        }
        Xf(r)
    }
    fn from_axes(x: V3, y: V3, z: V3, o: V3) -> Xf {
        Xf([
            [x[0], y[0], z[0], o[0]],
            [x[1], y[1], z[1], o[1]],
            [x[2], y[2], z[2], o[2]],
        ])
    }
    fn det(&self) -> f64 {
        let m = &self.0;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }
    fn to_manifold(self) -> [f64; 12] {
        let m = self.0;
        [
            m[0][0], m[1][0], m[2][0], m[0][1], m[1][1], m[2][1], m[0][2], m[1][2], m[2][2],
            m[0][3], m[1][3], m[2][3],
        ]
    }
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn norm(a: V3) -> Option<V3> {
    let l = dot(a, a).sqrt();
    (l > 1e-12 && l.is_finite()).then(|| scale(a, 1.0 / l))
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub pos: Vec<V3>,
    pub tri: Vec<[u32; 3]>,
    /// Built as a closed solid (extrusions, closed shells, booleans).
    pub closed: bool,
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.tri.is_empty()
    }
    fn transformed(mut self, x: &Xf) -> Mesh {
        for p in &mut self.pos {
            *p = x.apply(*p);
        }
        if x.det() < 0.0 {
            for t in &mut self.tri {
                t.swap(1, 2);
            }
        }
        self
    }
    fn append(&mut self, o: Mesh) {
        let base = self.pos.len() as u32;
        self.closed = (self.is_empty() || self.closed) && o.closed;
        self.pos.extend(o.pos);
        self.tri.extend(
            o.tri
                .into_iter()
                .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
        );
    }
    /// Signed volume, taken about the bounding box corner so that georeferenced
    /// coordinates don't cancel out.
    pub fn volume(&self) -> f64 {
        let o = self.bbox().map(|b| b.0).unwrap_or([0.0; 3]);
        let mut v = 0.0;
        for t in &self.tri {
            let (a, b, c) = (
                sub(self.pos[t[0] as usize], o),
                sub(self.pos[t[1] as usize], o),
                sub(self.pos[t[2] as usize], o),
            );
            v += dot(a, cross(b, c));
        }
        v / 6.0
    }
    pub fn bbox(&self) -> Option<(V3, V3)> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for t in &self.tri {
            for &i in t {
                let p = self.pos[i as usize];
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        (lo[0] <= hi[0]).then_some((lo, hi))
    }
    fn to_manifold(&self) -> R<Manifold> {
        if self.pos.iter().any(|p| !p.iter().all(|c| c.is_finite())) {
            return Err(GeomError::Boolean("non-finite coordinates".into()));
        }
        let verts: Vec<f64> = self.pos.iter().flat_map(|p| *p).collect();
        let idx: Vec<u64> = self.tri.iter().flat_map(|t| t.map(u64::from)).collect();
        let gl =
            MeshGL64::new(&verts, 3, &idx).map_err(|e| GeomError::Boolean(format!("{e:?}")))?;
        Manifold::from_meshgl64(&gl.merge())
            .map_err(|_| GeomError::Boolean("operand is not a closed manifold".into()))
    }
    fn from_manifold(m: &Manifold) -> Mesh {
        let (v, n, idx) = m.to_mesh_f64();
        let pos = v.chunks(n.max(3)).map(|c| [c[0], c[1], c[2]]).collect();
        let tri = idx
            .chunks(3)
            .map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
            .collect();
        Mesh {
            pos,
            tri,
            closed: true,
        }
    }
}

/// One element's result.
pub struct ElementGeometry {
    pub mesh: Mesh,
    pub openings_applied: usize,
    pub openings_failed: usize,
}

pub struct Ifc<'a> {
    pub f: &'a File<'a>,
    pub length_scale: f64,
    pub angle_scale: f64,
    placements: RefCell<HashMap<u32, Xf>>,
    maps: RefCell<HashMap<u32, Mesh>>,
}

fn get<'a>(f: &File<'a>, id: u32) -> R<Instance<'a>> {
    f.get(id).map_err(|e| inv(&e.to_string()))
}

fn ty(i: &Instance) -> String {
    String::from_utf8_lossy(i.ty).to_string()
}

impl<'a> Ifc<'a> {
    pub fn new(f: &'a File<'a>) -> Ifc<'a> {
        let mut s = Ifc {
            f,
            length_scale: 1.0,
            angle_scale: 1.0,
            placements: Default::default(),
            maps: Default::default(),
        };
        s.read_units();
        s
    }

    fn read_units(&mut self) {
        let Some(project) = self
            .f
            .iter()
            .find(|(_, t)| t.eq_ignore_ascii_case(b"IFCPROJECT"))
            .map(|(id, _)| id)
        else {
            return;
        };
        let Ok(p) = self.f.get(project) else { return };
        let Some(ua) = p.attr(8).as_ref().and_then(|id| self.f.get(id).ok()) else {
            return;
        };
        for u in ua.attr(0).as_list().unwrap_or(&[]) {
            let Some(u) = u.as_ref().and_then(|id| self.f.get(id).ok()) else {
                continue;
            };
            let kind = u.attr(1).as_enum().unwrap_or(b"");
            let factor = self.unit_factor(&u, 0);
            match (kind, factor) {
                (b"LENGTHUNIT", Some(f)) => self.length_scale = f,
                (b"PLANEANGLEUNIT", Some(f)) => self.angle_scale = f,
                _ => {}
            }
        }
    }

    fn unit_factor(&self, u: &Instance, depth: usize) -> Option<f64> {
        if depth > 4 {
            return None;
        }
        if u.ty == b"IFCSIUNIT" {
            let prefix = match u.attr(2).as_enum() {
                Some(b"MILLI") => 1e-3,
                Some(b"CENTI") => 1e-2,
                Some(b"DECI") => 1e-1,
                Some(b"KILO") => 1e3,
                _ => 1.0,
            };
            return Some(prefix);
        }
        if u.ty == b"IFCCONVERSIONBASEDUNIT" || u.ty == b"IFCCONVERSIONBASEDUNITWITHOFFSET" {
            let m = self.f.get(u.attr(3).as_ref()?).ok()?;
            let v = m.attr(0).as_f64()?;
            let base = self.f.get(m.attr(1).as_ref()?).ok()?;
            return Some(v * self.unit_factor(&base, depth + 1)?);
        }
        None
    }

    fn num(&self, v: &Value) -> R<f64> {
        let x = v.as_f64().ok_or_else(|| inv("expected a number"))?;
        if x.is_finite() {
            Ok(x)
        } else {
            Err(inv("non-finite number"))
        }
    }
    fn len(&self, v: &Value) -> R<f64> {
        Ok(self.num(v)? * self.length_scale)
    }
    fn opt_len(&self, v: &Value) -> f64 {
        self.len(v).unwrap_or(0.0)
    }

    fn point(&self, id: Option<u32>) -> R<V3> {
        let Some(id) = id else { return Ok([0.0; 3]) };
        let p = get(self.f, id)?;
        let c = p.attr(0).as_list().ok_or_else(|| inv("point"))?;
        let mut r = [0.0; 3];
        for (k, v) in c.iter().take(3).enumerate() {
            r[k] = self.len(v)?;
        }
        Ok(r)
    }
    fn direction(&self, id: Option<u32>, default: V3) -> R<V3> {
        let Some(id) = id else { return Ok(default) };
        let p = get(self.f, id)?;
        let c = p.attr(0).as_list().ok_or_else(|| inv("direction"))?;
        let mut r = [0.0; 3];
        for (k, v) in c.iter().take(3).enumerate() {
            r[k] = self.num(v)?;
        }
        Ok(norm(r).unwrap_or(default))
    }

    /// IfcAxis2Placement3D or 2D (as a 3D transform).
    pub fn axis2(&self, id: Option<u32>) -> R<Xf> {
        let Some(id) = id else { return Ok(Xf::ID) };
        let a = get(self.f, id)?;
        let o = self.point(a.attr(0).as_ref())?;
        if a.ty == b"IFCAXIS2PLACEMENT2D" {
            let x = self.direction(a.attr(1).as_ref(), [1., 0., 0.])?;
            let x = norm([x[0], x[1], 0.0]).unwrap_or([1., 0., 0.]);
            return Ok(Xf::from_axes(x, [-x[1], x[0], 0.0], [0., 0., 1.], o));
        }
        if a.ty != b"IFCAXIS2PLACEMENT3D" {
            return Err(GeomError::Unsupported(ty(&a)));
        }
        let z = self.direction(a.attr(1).as_ref(), [0., 0., 1.])?;
        let xr = self.direction(a.attr(2).as_ref(), [1., 0., 0.])?;
        let x = norm(sub(xr, scale(z, dot(xr, z))))
            .or_else(|| norm(cross([0., 1., 0.], z)))
            .or_else(|| norm(cross([1., 0., 0.], z)))
            .unwrap_or([1., 0., 0.]);
        let y = cross(z, x);
        Ok(Xf::from_axes(x, y, z, o))
    }

    /// World transform of an IfcObjectPlacement.
    pub fn placement(&self, id: Option<u32>, depth: usize) -> R<Xf> {
        let Some(id) = id else { return Ok(Xf::ID) };
        if let Some(x) = self.placements.borrow().get(&id) {
            return Ok(*x);
        }
        if depth > MAX_NEST {
            return Err(inv("placement chain too deep or cyclic"));
        }
        let p = get(self.f, id)?;
        let x = match p.ty {
            b"IFCLOCALPLACEMENT" => {
                let parent = self.placement(p.attr(0).as_ref(), depth + 1)?;
                parent.mul(&self.axis2(p.attr(1).as_ref())?)
            }
            _ => return Err(GeomError::Unsupported(ty(&p))),
        };
        self.placements.borrow_mut().insert(id, x);
        Ok(x)
    }

    // ---------- curves and profiles (2D) ----------

    fn point2(&self, id: Option<u32>) -> R<V2> {
        let p = self.point(id)?;
        Ok([p[0], p[1]])
    }

    fn circle_points(&self, c: V2, r: f64, a0: f64, a1: f64) -> Vec<V2> {
        let sweep = a1 - a0;
        let n = ((sweep.abs() / std::f64::consts::TAU * CIRCLE_SEGMENTS as f64).ceil() as usize)
            .clamp(2, CIRCLE_SEGMENTS);
        (0..=n)
            .map(|i| {
                let a = a0 + sweep * i as f64 / n as f64;
                [c[0] + r * a.cos(), c[1] + r * a.sin()]
            })
            .collect()
    }

    /// A 2D curve as a polyline.
    fn curve2(&self, id: u32, depth: usize) -> R<Vec<V2>> {
        if depth > MAX_NEST {
            return Err(inv("curve nesting"));
        }
        let c = get(self.f, id)?;
        match c.ty {
            b"IFCPOLYLINE" => c
                .attr(0)
                .as_list()
                .ok_or_else(|| inv("polyline"))?
                .iter()
                .map(|v| self.point2(v.as_ref()))
                .collect(),
            b"IFCINDEXEDPOLYCURVE" => {
                let pts = get(
                    self.f,
                    c.attr(0).as_ref().ok_or_else(|| inv("indexed polycurve"))?,
                )?;
                let list = pts.attr(0).as_list().ok_or_else(|| inv("point list"))?;
                let pt = |i: i64| -> R<V2> {
                    let v = list
                        .get((i - 1) as usize)
                        .and_then(|v| v.as_list())
                        .ok_or_else(|| inv("index out of range"))?;
                    Ok([
                        self.len(v.first().unwrap_or(&Value::Null))?,
                        self.len(v.get(1).unwrap_or(&Value::Null))?,
                    ])
                };
                let mut out = Vec::new();
                match c.attr(1) {
                    Value::List(segs) => {
                        for s in segs {
                            let Value::Typed(name, inner) = s else {
                                return Err(inv("segment"));
                            };
                            let idx: Vec<i64> = inner
                                .as_list()
                                .unwrap_or(&[])
                                .iter()
                                .filter_map(|v| v.as_i64())
                                .collect();
                            if *name == b"IFCARCINDEX" && idx.len() == 3 {
                                let (a, b, e) = (pt(idx[0])?, pt(idx[1])?, pt(idx[2])?);
                                out.extend(arc3(a, b, e));
                            } else {
                                for i in idx {
                                    out.push(pt(i)?);
                                }
                            }
                        }
                    }
                    _ => {
                        for i in 1..=list.len() as i64 {
                            out.push(pt(i)?);
                        }
                    }
                }
                Ok(out)
            }
            b"IFCCOMPOSITECURVE" => {
                let mut out: Vec<V2> = Vec::new();
                for s in c.attr(0).as_list().ok_or_else(|| inv("composite"))? {
                    let seg = get(self.f, s.as_ref().ok_or_else(|| inv("segment"))?)?;
                    let same = seg.attr(1).as_enum() != Some(b"F");
                    let mut pts = self.curve2(
                        seg.attr(2).as_ref().ok_or_else(|| inv("parent curve"))?,
                        depth + 1,
                    )?;
                    if !same {
                        pts.reverse();
                    }
                    out.extend(pts);
                }
                Ok(out)
            }
            b"IFCTRIMMEDCURVE" => {
                let basis = get(self.f, c.attr(0).as_ref().ok_or_else(|| inv("basis"))?)?;
                let sense = c.attr(3).as_enum() != Some(b"F");
                let trim = |v: &Value| -> (Option<f64>, Option<V2>) {
                    let mut par = None;
                    let mut pt = None;
                    for t in v.as_list().unwrap_or(&[]) {
                        match t {
                            Value::Typed(n, x) if *n == b"IFCPARAMETERVALUE" => par = x.as_f64(),
                            Value::Ref(r) => pt = self.point2(Some(*r)).ok(),
                            _ => {}
                        }
                    }
                    (par, pt)
                };
                let (p1, q1) = trim(c.attr(1));
                let (p2, q2) = trim(c.attr(2));
                match basis.ty {
                    b"IFCLINE" => match (q1, q2) {
                        (Some(a), Some(b)) => Ok(if sense { vec![a, b] } else { vec![b, a] }),
                        _ => {
                            let o = self.point2(basis.attr(0).as_ref())?;
                            let v =
                                get(self.f, basis.attr(1).as_ref().ok_or_else(|| inv("vector"))?)?;
                            let d = self.direction(v.attr(0).as_ref(), [1., 0., 0.])?;
                            let m = self.len(v.attr(1))?;
                            let at = |t: f64| [o[0] + d[0] * m * t, o[1] + d[1] * m * t];
                            let (a, b) = (at(p1.unwrap_or(0.0)), at(p2.unwrap_or(1.0)));
                            Ok(if sense { vec![a, b] } else { vec![b, a] })
                        }
                    },
                    b"IFCCIRCLE" => {
                        let x = self.axis2(basis.attr(0).as_ref())?;
                        let r = self.len(basis.attr(1))?;
                        let ctr = [x.0[0][3], x.0[1][3]];
                        let rot = x.0[1][0].atan2(x.0[0][0]);
                        let ang = |par: Option<f64>, pt: Option<V2>| -> R<f64> {
                            match (pt, par) {
                                (Some(p), _) => Ok((p[1] - ctr[1]).atan2(p[0] - ctr[0])),
                                (None, Some(t)) => Ok(t * self.angle_scale + rot),
                                _ => Err(inv("trim")),
                            }
                        };
                        let (mut a0, mut a1) = (ang(p1, q1)?, ang(p2, q2)?);
                        if sense {
                            while a1 <= a0 {
                                a1 += std::f64::consts::TAU;
                            }
                        } else {
                            while a0 <= a1 {
                                a0 += std::f64::consts::TAU;
                            }
                        }
                        Ok(self.circle_points(ctr, r, a0, a1))
                    }
                    _ => Err(GeomError::Unsupported(format!(
                        "IFCTRIMMEDCURVE of {}",
                        ty(&basis)
                    ))),
                }
            }
            b"IFCCIRCLE" => {
                let x = self.axis2(c.attr(0).as_ref())?;
                let r = self.len(c.attr(1))?;
                let mut p =
                    self.circle_points([x.0[0][3], x.0[1][3]], r, 0.0, std::f64::consts::TAU);
                p.pop();
                Ok(p)
            }
            _ => Err(GeomError::Unsupported(ty(&c))),
        }
    }

    /// A profile as a list of polygons, each an outer ring plus holes.
    fn profile(&self, id: u32, depth: usize) -> R<Vec<Vec<Vec<V2>>>> {
        if depth > MAX_NEST {
            return Err(inv("profile nesting"));
        }
        let p = get(self.f, id)?;
        let pos = |i: usize| self.axis2(p.attr(i).as_ref());
        let rect = |w: f64, h: f64| {
            vec![
                [-w / 2., -h / 2.],
                [w / 2., -h / 2.],
                [w / 2., h / 2.],
                [-w / 2., h / 2.],
            ]
        };
        let place = |x: Xf, rings: Vec<Vec<V2>>| -> Vec<Vec<Vec<V2>>> {
            vec![
                rings
                    .into_iter()
                    .map(|r| {
                        r.into_iter()
                            .map(|q| {
                                let w = x.apply([q[0], q[1], 0.0]);
                                [w[0], w[1]]
                            })
                            .collect()
                    })
                    .collect(),
            ]
        };
        let l = |i: usize| self.len(p.attr(i));
        match p.ty {
            b"IFCRECTANGLEPROFILEDEF" | b"IFCROUNDEDRECTANGLEPROFILEDEF" => {
                Ok(place(pos(2)?, vec![rect(l(3)?, l(4)?)]))
            }
            b"IFCRECTANGLEHOLLOWPROFILEDEF" => {
                let (w, h, t) = (l(3)?, l(4)?, l(5)?);
                let mut hole = rect(w - 2. * t, h - 2. * t);
                hole.reverse();
                Ok(place(pos(2)?, vec![rect(w, h), hole]))
            }
            b"IFCCIRCLEPROFILEDEF" => {
                let mut c = self.circle_points([0., 0.], l(3)?, 0.0, std::f64::consts::TAU);
                c.pop();
                Ok(place(pos(2)?, vec![c]))
            }
            b"IFCCIRCLEHOLLOWPROFILEDEF" => {
                let r = l(3)?;
                let mut c = self.circle_points([0., 0.], r, 0.0, std::f64::consts::TAU);
                c.pop();
                let mut h = self.circle_points([0., 0.], r - l(4)?, 0.0, std::f64::consts::TAU);
                h.pop();
                h.reverse();
                Ok(place(pos(2)?, vec![c, h]))
            }
            b"IFCELLIPSEPROFILEDEF" => {
                let (a, b) = (l(3)?, l(4)?);
                let c = (0..CIRCLE_SEGMENTS)
                    .map(|i| {
                        let t = std::f64::consts::TAU * i as f64 / CIRCLE_SEGMENTS as f64;
                        [a * t.cos(), b * t.sin()]
                    })
                    .collect();
                Ok(place(pos(2)?, vec![c]))
            }
            b"IFCISHAPEPROFILEDEF" | b"IFCASYMMETRICISHAPEPROFILEDEF" => {
                let (w, d, tw, tf) = (l(3)?, l(4)?, l(5)?, l(6)?);
                let (x, y, a) = (w / 2., d / 2., tw / 2.);
                let ring = vec![
                    [-x, -y],
                    [x, -y],
                    [x, -y + tf],
                    [a, -y + tf],
                    [a, y - tf],
                    [x, y - tf],
                    [x, y],
                    [-x, y],
                    [-x, y - tf],
                    [-a, y - tf],
                    [-a, -y + tf],
                    [-x, -y + tf],
                ];
                Ok(place(
                    pos(2)?,
                    vec![fillet(ring, &[3, 4, 9, 10], self.opt_len(p.attr(7)))],
                ))
            }
            b"IFCLSHAPEPROFILEDEF" => {
                let (d, w, t) = (
                    l(3)?,
                    if p.attr(4).is_null() { l(3)? } else { l(4)? },
                    l(5)?,
                );
                let (x, y) = (w / 2., d / 2.);
                let ring = vec![
                    [-x, -y],
                    [x, -y],
                    [x, -y + t],
                    [-x + t, -y + t],
                    [-x + t, y],
                    [-x, y],
                ];
                Ok(place(
                    pos(2)?,
                    vec![fillet(ring, &[3], self.opt_len(p.attr(6)))],
                ))
            }
            b"IFCUSHAPEPROFILEDEF" => {
                let (d, w, tw, tf) = (l(3)?, l(4)?, l(5)?, l(6)?);
                let (x, y) = (w / 2., d / 2.);
                let ring = vec![
                    [-x, -y],
                    [x, -y],
                    [x, -y + tf],
                    [-x + tw, -y + tf],
                    [-x + tw, y - tf],
                    [x, y - tf],
                    [x, y],
                    [-x, y],
                ];
                Ok(place(
                    pos(2)?,
                    vec![fillet(ring, &[3, 4], self.opt_len(p.attr(7)))],
                ))
            }
            b"IFCTSHAPEPROFILEDEF" => {
                let (d, w, tw, tf) = (l(3)?, l(4)?, l(5)?, l(6)?);
                let (x, y, a) = (w / 2., d / 2., tw / 2.);
                let ring = vec![
                    [-a, -y],
                    [a, -y],
                    [a, y - tf],
                    [x, y - tf],
                    [x, y],
                    [-x, y],
                    [-x, y - tf],
                    [-a, y - tf],
                ];
                Ok(place(
                    pos(2)?,
                    vec![fillet(ring, &[2, 7], self.opt_len(p.attr(7)))],
                ))
            }
            b"IFCCSHAPEPROFILEDEF" => {
                let (d, w, t, g) = (l(3)?, l(4)?, l(5)?, l(6)?);
                let (x, y) = (w / 2., d / 2.);
                Ok(place(
                    pos(2)?,
                    vec![vec![
                        [-x, -y],
                        [x, -y],
                        [x, -y + g],
                        [x - t, -y + g],
                        [x - t, -y + t],
                        [-x + t, -y + t],
                        [-x + t, y - t],
                        [x - t, y - t],
                        [x - t, y - g],
                        [x, y - g],
                        [x, y],
                        [-x, y],
                    ]],
                ))
            }
            b"IFCZSHAPEPROFILEDEF" => {
                let (d, w, tw, tf) = (l(3)?, l(4)?, l(5)?, l(6)?);
                let (y, a) = (d / 2., tw / 2.);
                Ok(place(
                    pos(2)?,
                    vec![vec![
                        [-a, -y],
                        [w - a, -y],
                        [w - a, -y + tf],
                        [a, -y + tf],
                        [a, y],
                        [a - w, y],
                        [a - w, y - tf],
                        [-a, y - tf],
                    ]],
                ))
            }
            b"IFCARBITRARYCLOSEDPROFILEDEF" | b"IFCARBITRARYPROFILEDEFWITHVOIDS" => {
                let mut rings = vec![self.curve2(
                    p.attr(2).as_ref().ok_or_else(|| inv("outer curve"))?,
                    depth + 1,
                )?];
                if p.ty == b"IFCARBITRARYPROFILEDEFWITHVOIDS" {
                    for v in p.attr(3).as_list().unwrap_or(&[]) {
                        rings.push(
                            self.curve2(v.as_ref().ok_or_else(|| inv("inner curve"))?, depth + 1)?,
                        );
                    }
                }
                Ok(vec![rings])
            }
            b"IFCDERIVEDPROFILEDEF" => {
                let parent =
                    self.profile(p.attr(2).as_ref().ok_or_else(|| inv("parent"))?, depth + 1)?;
                let x = self.operator(p.attr(3).as_ref(), true)?;
                Ok(parent
                    .into_iter()
                    .map(|poly| {
                        poly.into_iter()
                            .map(|r| {
                                r.into_iter()
                                    .map(|q| {
                                        let w = x.apply([q[0], q[1], 0.0]);
                                        [w[0], w[1]]
                                    })
                                    .collect()
                            })
                            .collect()
                    })
                    .collect())
            }
            b"IFCCOMPOSITEPROFILEDEF" => {
                let mut out = Vec::new();
                for v in p.attr(2).as_list().unwrap_or(&[]) {
                    out.extend(self.profile(v.as_ref().ok_or_else(|| inv("profile"))?, depth + 1)?);
                }
                Ok(out)
            }
            _ => Err(GeomError::Unsupported(ty(&p))),
        }
    }

    /// IfcCartesianTransformationOperator(2D|3D)(nonUniform).
    fn operator(&self, id: Option<u32>, two_d: bool) -> R<Xf> {
        let Some(id) = id else { return Ok(Xf::ID) };
        let o = get(self.f, id)?;
        let a1 = self.direction(o.attr(0).as_ref(), [1., 0., 0.])?;
        let a2 = self.direction(o.attr(1).as_ref(), [0., 1., 0.])?;
        let org = self.point(o.attr(2).as_ref())?;
        let s = o.attr(3).as_f64().unwrap_or(1.0);
        let (s2, s3) = if o.ty.ends_with(b"NONUNIFORM") {
            (
                o.attr(if two_d { 4 } else { 5 }).as_f64().unwrap_or(s),
                o.attr(6).as_f64().unwrap_or(s),
            )
        } else {
            (s, s)
        };
        let (x, y, z) = if two_d {
            let x = norm([a1[0], a1[1], 0.]).unwrap_or([1., 0., 0.]);
            (x, [-x[1], x[0], 0.], [0., 0., 1.])
        } else {
            let z = self.direction(
                o.attr(4).as_ref(),
                norm(cross(a1, a2)).unwrap_or([0., 0., 1.]),
            )?;
            let x = norm(sub(a1, scale(z, dot(a1, z)))).unwrap_or([1., 0., 0.]);
            (x, cross(z, x), z)
        };
        Ok(Xf::from_axes(scale(x, s), scale(y, s2), scale(z, s3), org))
    }

    // ---------- solids ----------

    fn extrude(&self, polys: &[Vec<Vec<V2>>], dir: V3, depth: f64) -> R<Mesh> {
        let mut m = Mesh {
            closed: true,
            ..Default::default()
        };
        let d = scale(dir, depth);
        for poly in polys {
            let rings: Vec<Vec<V2>> = poly
                .iter()
                .map(|r| clean_ring(r))
                .filter(|r| r.len() >= 3)
                .collect();
            if rings.is_empty() {
                continue;
            }
            // Orient: outer CCW, holes CW.
            let rings: Vec<Vec<V2>> = rings
                .into_iter()
                .enumerate()
                .map(|(i, mut r)| {
                    let ccw = area2(&r) > 0.0;
                    if ccw != (i == 0) {
                        r.reverse();
                    }
                    r
                })
                .collect();
            let mut flat = Vec::new();
            let mut holes = Vec::new();
            for (i, r) in rings.iter().enumerate() {
                if i > 0 {
                    holes.push(flat.len() / 2);
                }
                for q in r {
                    flat.extend_from_slice(q);
                }
            }
            let tris = earcutr::earcut(&flat, &holes, 2)
                .map_err(|e| inv(&format!("triangulation {e:?}")))?;
            let n = flat.len() / 2;
            let base = m.pos.len() as u32;
            for k in 0..n {
                m.pos.push([flat[2 * k], flat[2 * k + 1], 0.0]);
            }
            for k in 0..n {
                m.pos
                    .push([flat[2 * k] + d[0], flat[2 * k + 1] + d[1], d[2]]);
            }
            let flip = dir[2] * depth < 0.0;
            for t in tris.chunks(3) {
                let (a, b, c) = (t[0] as u32 + base, t[1] as u32 + base, t[2] as u32 + base);
                let n32 = n as u32;
                if flip {
                    m.tri.push([a, b, c]);
                    m.tri.push([a + n32, c + n32, b + n32]);
                } else {
                    m.tri.push([a, c, b]);
                    m.tri.push([a + n32, b + n32, c + n32]);
                }
            }
            let mut off = 0u32;
            for r in &rings {
                let len = r.len() as u32;
                for k in 0..len {
                    let (a, b) = (base + off + k, base + off + (k + 1) % len);
                    let (a2, b2) = (a + n as u32, b + n as u32);
                    if flip {
                        m.tri.push([a, b2, b]);
                        m.tri.push([a, a2, b2]);
                    } else {
                        m.tri.push([a, b, b2]);
                        m.tri.push([a, b2, a2]);
                    }
                }
                off += len;
            }
        }
        if m.is_empty() {
            return Err(inv("empty profile"));
        }
        Ok(m)
    }

    fn faces_mesh(
        &self,
        faces: &[Value],
        closed: bool,
        cache: &mut HashMap<u32, u32>,
        m: &mut Mesh,
    ) -> R<()> {
        for fv in faces {
            let face = get(self.f, fv.as_ref().ok_or_else(|| inv("face"))?)?;
            let mut rings: Vec<(Vec<u32>, bool)> = Vec::new();
            for bv in face.attr(0).as_list().unwrap_or(&[]) {
                let b = get(self.f, bv.as_ref().ok_or_else(|| inv("bound"))?)?;
                let outer = b.ty == b"IFCFACEOUTERBOUND";
                let lp = get(self.f, b.attr(0).as_ref().ok_or_else(|| inv("loop"))?)?;
                if lp.ty != b"IFCPOLYLOOP" {
                    return Err(GeomError::Unsupported(ty(&lp)));
                }
                let mut idx = Vec::new();
                for pv in lp.attr(0).as_list().unwrap_or(&[]) {
                    let pid = pv.as_ref().ok_or_else(|| inv("point"))?;
                    let i = match cache.get(&pid) {
                        Some(i) => *i,
                        None => {
                            let i = m.pos.len() as u32;
                            m.pos.push(self.point(Some(pid))?);
                            cache.insert(pid, i);
                            i
                        }
                    };
                    idx.push(i);
                }
                if b.attr(1).as_enum() == Some(b"F") {
                    idx.reverse();
                }
                rings.push((idx, outer));
            }
            // the outer bound first (if none is marked, the first one)
            if let Some(k) = rings.iter().position(|r| r.1) {
                rings.swap(0, k);
            }
            let rings: Vec<Vec<u32>> = rings.into_iter().map(|r| r.0).collect();
            triangulate_face(&rings, m)?;
        }
        m.closed = closed;
        Ok(())
    }

    fn point_list3(&self, id: u32) -> R<Vec<V3>> {
        let pl = get(self.f, id)?;
        pl.attr(0)
            .as_list()
            .ok_or_else(|| inv("point list"))?
            .iter()
            .map(|p| {
                let c = p.as_list().ok_or_else(|| inv("point"))?;
                let mut r = [0.0; 3];
                for (k, v) in c.iter().take(3).enumerate() {
                    r[k] = self.len(v)?;
                }
                Ok(r)
            })
            .collect()
    }

    /// A representation item in its own coordinate system.
    pub fn item(&self, id: u32, depth: usize) -> R<Mesh> {
        if depth > MAX_NEST {
            return Err(inv("item nesting too deep or cyclic"));
        }
        let it = get(self.f, id)?;
        match it.ty {
            b"IFCEXTRUDEDAREASOLID" => {
                let polys = self.profile(
                    it.attr(0).as_ref().ok_or_else(|| inv("swept area"))?,
                    depth + 1,
                )?;
                let pos = self.axis2(it.attr(1).as_ref())?;
                let dir = self.direction(it.attr(2).as_ref(), [0., 0., 1.])?;
                let d = self.len(it.attr(3))?;
                Ok(self.extrude(&polys, dir, d)?.transformed(&pos))
            }
            b"IFCFACETEDBREP" | b"IFCFACETEDBREPWITHVOIDS" => {
                let shell = get(self.f, it.attr(0).as_ref().ok_or_else(|| inv("shell"))?)?;
                let mut m = Mesh::default();
                self.faces_mesh(
                    shell.attr(0).as_list().unwrap_or(&[]),
                    true,
                    &mut HashMap::new(),
                    &mut m,
                )?;
                Ok(m)
            }
            b"IFCSHELLBASEDSURFACEMODEL" | b"IFCFACEBASEDSURFACEMODEL" => {
                let mut m = Mesh::default();
                for s in it.attr(0).as_list().unwrap_or(&[]) {
                    let shell = get(self.f, s.as_ref().ok_or_else(|| inv("shell"))?)?;
                    self.faces_mesh(
                        shell.attr(0).as_list().unwrap_or(&[]),
                        false,
                        &mut HashMap::new(),
                        &mut m,
                    )?;
                }
                m.closed = false;
                Ok(m)
            }
            b"IFCTRIANGULATEDFACESET" | b"IFCTRIANGULATEDIRREGULARNETWORK" => {
                let pos =
                    self.point_list3(it.attr(0).as_ref().ok_or_else(|| inv("coordinates"))?)?;
                // CoordIndex is the first list of integer triples (its position moved between schemas)
                let idx = it
                    .attrs
                    .iter()
                    .skip(1)
                    .find_map(|a| {
                        let l = a.as_list()?;
                        l.first()?.as_list()?.first()?.as_i64()?;
                        Some(l)
                    })
                    .ok_or_else(|| inv("coord index"))?;
                let mut m = Mesh {
                    pos,
                    closed: it.attrs.iter().any(|a| a.as_enum() == Some(b"T")),
                    ..Default::default()
                };
                let n = m.pos.len() as i64;
                for t in idx {
                    let t: Vec<i64> = t
                        .as_list()
                        .unwrap_or(&[])
                        .iter()
                        .filter_map(|v| v.as_i64())
                        .collect();
                    if t.len() != 3 || t.iter().any(|&i| i < 1 || i > n) {
                        return Err(inv("triangle index"));
                    }
                    m.tri
                        .push([(t[0] - 1) as u32, (t[1] - 1) as u32, (t[2] - 1) as u32]);
                }
                Ok(m)
            }
            b"IFCPOLYGONALFACESET" => {
                let pos =
                    self.point_list3(it.attr(0).as_ref().ok_or_else(|| inv("coordinates"))?)?;
                let faces = it
                    .attrs
                    .iter()
                    .skip(1)
                    .find_map(|a| {
                        let l = a.as_list()?;
                        l.first()?.as_ref()?;
                        Some(l)
                    })
                    .ok_or_else(|| inv("faces"))?;
                let mut m = Mesh {
                    pos,
                    closed: it.attr(1).as_enum() == Some(b"T"),
                    ..Default::default()
                };
                let n = m.pos.len() as i64;
                let ring = |v: &Value| -> R<Vec<u32>> {
                    v.as_list()
                        .unwrap_or(&[])
                        .iter()
                        .map(|i| match i.as_i64() {
                            Some(i) if i >= 1 && i <= n => Ok((i - 1) as u32),
                            _ => Err(inv("face index")),
                        })
                        .collect()
                };
                for fv in faces {
                    let f = get(self.f, fv.as_ref().ok_or_else(|| inv("face"))?)?;
                    let mut rings = vec![ring(f.attr(0))?];
                    for h in f.attr(1).as_list().unwrap_or(&[]) {
                        rings.push(ring(h)?);
                    }
                    triangulate_face(&rings, &mut m)?;
                }
                Ok(m)
            }
            b"IFCMAPPEDITEM" => {
                let src = it.attr(0).as_ref().ok_or_else(|| inv("mapping source"))?;
                let target = self.operator(it.attr(1).as_ref(), false)?;
                let cached = self.maps.borrow().get(&src).cloned();
                let m = match cached {
                    Some(m) => m,
                    None => {
                        let map = get(self.f, src)?;
                        let origin = self.axis2(map.attr(0).as_ref())?;
                        let rep = get(
                            self.f,
                            map.attr(1)
                                .as_ref()
                                .ok_or_else(|| inv("mapped representation"))?,
                        )?;
                        let m = self
                            .items(rep.attr(3).as_list().unwrap_or(&[]), depth + 1)?
                            .transformed(&origin);
                        self.maps.borrow_mut().insert(src, m.clone());
                        m
                    }
                };
                Ok(m.transformed(&target))
            }
            b"IFCBOOLEANRESULT" | b"IFCBOOLEANCLIPPINGRESULT" => {
                let op = it.attr(0).as_enum().unwrap_or(b"");
                let a = self.item(
                    it.attr(1).as_ref().ok_or_else(|| inv("operand"))?,
                    depth + 1,
                )?;
                let bid = it.attr(2).as_ref().ok_or_else(|| inv("operand"))?;
                let b_ty = self.f.type_of(bid).unwrap_or(b"");
                let ma = a.to_manifold()?;
                let r = if b_ty == b"IFCHALFSPACESOLID"
                    || b_ty == b"IFCPOLYGONALBOUNDEDHALFSPACE"
                    || b_ty == b"IFCBOXEDHALFSPACE"
                {
                    self.half_space_op(&ma, bid, op)?
                } else {
                    let mb = self.item(bid, depth + 1)?.to_manifold()?;
                    match op {
                        b"DIFFERENCE" => ma.difference(&mb),
                        b"UNION" => ma.union(&mb),
                        b"INTERSECTION" => ma.intersection(&mb),
                        _ => return Err(inv("boolean operator")),
                    }
                };
                Ok(Mesh::from_manifold(&r))
            }
            b"IFCCSGSOLID" => self.item(
                it.attr(0).as_ref().ok_or_else(|| inv("csg root"))?,
                depth + 1,
            ),
            b"IFCBLOCK" => {
                let x = self.axis2(it.attr(0).as_ref())?;
                let c = Manifold::cube(
                    self.len(it.attr(1))?,
                    self.len(it.attr(2))?,
                    self.len(it.attr(3))?,
                    false,
                );
                Ok(Mesh::from_manifold(&c).transformed(&x))
            }
            b"IFCRIGHTCIRCULARCYLINDER" => {
                let x = self.axis2(it.attr(0).as_ref())?;
                let c = Manifold::cylinder(
                    self.len(it.attr(1))?,
                    self.len(it.attr(2))?,
                    self.len(it.attr(2))?,
                    CIRCLE_SEGMENTS as i32,
                    false,
                );
                Ok(Mesh::from_manifold(&c).transformed(&x))
            }
            b"IFCSPHERE" => {
                let x = self.axis2(it.attr(0).as_ref())?;
                Ok(Mesh::from_manifold(&Manifold::sphere(
                    self.len(it.attr(1))?,
                    CIRCLE_SEGMENTS as i32,
                ))
                .transformed(&x))
            }
            b"IFCREVOLVEDAREASOLID" => {
                let polys = self.profile(
                    it.attr(0).as_ref().ok_or_else(|| inv("swept area"))?,
                    depth + 1,
                )?;
                let pos = self.axis2(it.attr(1).as_ref())?;
                let ax = get(self.f, it.attr(2).as_ref().ok_or_else(|| inv("axis"))?)?;
                let o = self.point(ax.attr(0).as_ref())?;
                let d = self.direction(ax.attr(1).as_ref(), [0., 1., 0.])?;
                let angle = self.num(it.attr(3))? * self.angle_scale;
                Ok(revolve(&polys, o, d, angle)?.transformed(&pos))
            }
            b"IFCSWEPTDISKSOLID" => {
                let path = self.curve3(
                    it.attr(0).as_ref().ok_or_else(|| inv("directrix"))?,
                    depth + 1,
                )?;
                let r = self.len(it.attr(1))?;
                sweep_disk(&path, r)
            }
            _ => Err(GeomError::Unsupported(ty(&it))),
        }
    }

    /// 3D polyline directrix for swept disks.
    fn curve3(&self, id: u32, depth: usize) -> R<Vec<V3>> {
        if depth > MAX_NEST {
            return Err(inv("curve nesting"));
        }
        let c = get(self.f, id)?;
        match c.ty {
            b"IFCPOLYLINE" => c
                .attr(0)
                .as_list()
                .ok_or_else(|| inv("polyline"))?
                .iter()
                .map(|v| self.point(v.as_ref()))
                .collect(),
            b"IFCINDEXEDPOLYCURVE" if c.attr(1).is_null() => {
                self.point_list3(c.attr(0).as_ref().ok_or_else(|| inv("points"))?)
            }
            b"IFCCOMPOSITECURVE" => {
                let mut out = Vec::new();
                for s in c.attr(0).as_list().unwrap_or(&[]) {
                    let seg = get(self.f, s.as_ref().ok_or_else(|| inv("segment"))?)?;
                    let mut p = self.curve3(
                        seg.attr(2).as_ref().ok_or_else(|| inv("parent"))?,
                        depth + 1,
                    )?;
                    if seg.attr(1).as_enum() == Some(b"F") {
                        p.reverse();
                    }
                    out.extend(p);
                }
                Ok(out)
            }
            b"IFCTRIMMEDCURVE"
                if self.f.type_of(c.attr(0).as_ref().unwrap_or(0)) == Some(b"IFCCIRCLE") =>
            {
                // Arcs in their own 3D plane (rebar bends).
                let basis = get(self.f, c.attr(0).as_ref().ok_or_else(|| inv("basis"))?)?;
                let x = self.axis2(basis.attr(0).as_ref())?;
                let r = self.len(basis.attr(1))?;
                let o = [x.0[0][3], x.0[1][3], x.0[2][3]];
                let col = |k: usize| [x.0[0][k], x.0[1][k], x.0[2][k]];
                let ang = |v: &Value| -> R<f64> {
                    let list = v.as_list().unwrap_or(&[]);
                    for t in list {
                        if let Value::Ref(p) = t {
                            let d = sub(self.point(Some(*p))?, o);
                            return Ok(dot(d, col(1)).atan2(dot(d, col(0))));
                        }
                    }
                    for t in list {
                        if let Value::Typed(n, v) = t {
                            if *n == b"IFCPARAMETERVALUE" {
                                return Ok(self.num(v)? * self.angle_scale);
                            }
                        }
                    }
                    Err(inv("trim"))
                };
                let (mut a0, mut a1) = (ang(c.attr(1))?, ang(c.attr(2))?);
                let sense = c.attr(3).as_enum() != Some(b"F");
                if sense {
                    while a1 <= a0 {
                        a1 += std::f64::consts::TAU;
                    }
                } else {
                    while a0 <= a1 {
                        a0 += std::f64::consts::TAU;
                    }
                }
                Ok(self
                    .circle_points([0.0, 0.0], r, a0, a1)
                    .into_iter()
                    .map(|q| x.apply([q[0], q[1], 0.0]))
                    .collect())
            }
            b"IFCLINE" | b"IFCTRIMMEDCURVE"
                if self.f.type_of(c.attr(0).as_ref().unwrap_or(0)) == Some(b"IFCLINE") =>
            {
                // straight segments: trimmed by points in 3D
                let pts: Vec<V3> = [c.attr(1), c.attr(2)]
                    .iter()
                    .filter_map(|v| v.as_list()?.iter().find_map(|t| t.as_ref()))
                    .map(|p| self.point(Some(p)))
                    .collect::<R<_>>()?;
                if pts.len() != 2 {
                    return Err(GeomError::Unsupported(
                        "IFCTRIMMEDCURVE of IFCLINE by parameter in 3D".into(),
                    ));
                }
                Ok(if c.attr(3).as_enum() == Some(b"F") {
                    vec![pts[1], pts[0]]
                } else {
                    pts
                })
            }
            _ => {
                // 2D curves in the plane z = 0 (lines, arcs)
                Ok(self
                    .curve2(id, depth + 1)?
                    .into_iter()
                    .map(|p| [p[0], p[1], 0.0])
                    .collect())
            }
        }
    }

    fn half_space_op(&self, a: &Manifold, hid: u32, op: &[u8]) -> R<Manifold> {
        if op != b"DIFFERENCE" {
            return Err(GeomError::Unsupported(
                "half-space union/intersection".into(),
            ));
        }
        let h = get(self.f, hid)?;
        let plane = get(
            self.f,
            h.attr(0).as_ref().ok_or_else(|| inv("base surface"))?,
        )?;
        if plane.ty != b"IFCPLANE" {
            return Err(GeomError::Unsupported(format!(
                "half-space on {}",
                ty(&plane)
            )));
        }
        let px = self.axis2(plane.attr(0).as_ref())?;
        let n = [px.0[0][2], px.0[1][2], px.0[2][2]];
        let o = [px.0[0][3], px.0[1][3], px.0[2][3]];
        // The half-space's material is on the side opposite the normal when AgreementFlag is true.
        let material_neg = h.attr(1).as_enum() != Some(b"F");
        let keep_n = if material_neg { n } else { scale(n, -1.0) };
        match h.ty {
            b"IFCHALFSPACESOLID" | b"IFCBOXEDHALFSPACE" => {
                Ok(a.trim_by_plane(keep_n, dot(keep_n, o)))
            }
            _ => {
                // Polygonal bounded: remove (half-space ∩ prism of the boundary).
                let bx = self.axis2(h.attr(2).as_ref())?;
                let poly = self.curve2(h.attr(3).as_ref().ok_or_else(|| inv("boundary"))?, 0)?;
                let bb = a.bounding_box().ok_or_else(|| inv("empty operand"))?;
                let span = {
                    let (mn, mx) = (bb.min(), bb.max());
                    let mut s = 0.0f64;
                    for k in 0..3 {
                        s = s
                            .max((mx[k] - mn[k]).abs())
                            .max(mn[k].abs())
                            .max(mx[k].abs());
                    }
                    s.max(1.0) * 4.0
                };
                let prism = self
                    .extrude(&[vec![poly]], [0., 0., 1.], 2.0 * span)?
                    .transformed(&Xf::ID);
                let shift = Xf([[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., -span]]);
                let prism = prism.transformed(&bx.mul(&shift)).to_manifold()?;
                let neg_keep = scale(keep_n, -1.0);
                let cutter = prism.trim_by_plane(neg_keep, dot(neg_keep, o));
                Ok(a.difference(&cutter))
            }
        }
    }

    fn items(&self, items: &[Value], depth: usize) -> R<Mesh> {
        let mut m = Mesh::default();
        for v in items {
            let id = v.as_ref().ok_or_else(|| inv("item"))?;
            // styled items etc. are not geometry
            m.append(self.item(id, depth)?);
        }
        Ok(m)
    }

    /// The Body (or Facetation) representation of a product, if any.
    pub fn body_rep(&self, product: &Instance) -> Option<Instance<'a>> {
        let pds = self.f.get(product.attr(6).as_ref()?).ok()?;
        let reps: Vec<Instance> = pds
            .attr(2)
            .as_list()?
            .iter()
            .filter_map(|r| self.f.get(r.as_ref()?).ok())
            .collect();
        for want in ["Body", "Facetation"] {
            if let Some(r) = reps
                .iter()
                .find(|r| r.attr(1).as_str().as_deref() == Some(want))
            {
                return Some(r.clone());
            }
        }
        None
    }

    /// Mesh one product in world coordinates, with its openings cut.
    pub fn element(&self, product: &Instance, openings: &[u32]) -> R<ElementGeometry> {
        let rep = self
            .body_rep(product)
            .ok_or_else(|| inv("no body representation"))?;
        let local = self.items(rep.attr(3).as_list().unwrap_or(&[]), 0)?;
        let x = self.placement(product.attr(5).as_ref(), 0)?;
        let mut mesh = local.transformed(&x);
        let (mut ok, mut failed) = (0, 0);
        if !openings.is_empty() && !mesh.is_empty() {
            let mut cutters = Vec::new();
            for &o in openings {
                let r = self.f.get(o).ok().and_then(|op| {
                    let rep = self.body_rep(&op)?;
                    let m = self.items(rep.attr(3).as_list()?, 0).ok()?;
                    let x = self.placement(op.attr(5).as_ref(), 0).ok()?;
                    m.transformed(&x).to_manifold().ok()
                });
                match r {
                    Some(m) => cutters.push(m),
                    None => failed += 1,
                }
            }
            if !cutters.is_empty() {
                match mesh.to_manifold() {
                    Ok(host) => {
                        let mut all = vec![host];
                        ok = cutters.len();
                        all.extend(cutters);
                        mesh = Mesh::from_manifold(&Manifold::batch_difference(&all));
                    }
                    Err(_) => failed += cutters.len(),
                }
            }
        }
        Ok(ElementGeometry {
            mesh,
            openings_applied: ok,
            openings_failed: failed,
        })
    }

    /// Representation item types used by a product, through mapped items and boolean operands.
    pub fn item_kinds(&self, product: &Instance, out: &mut Vec<String>) {
        let Some(rep) = self.body_rep(product) else {
            return;
        };
        let mut stack: Vec<(u32, usize)> = rep
            .attr(3)
            .as_list()
            .unwrap_or(&[])
            .iter()
            .filter_map(|v| Some((v.as_ref()?, 0)))
            .collect();
        while let Some((id, d)) = stack.pop() {
            if d > MAX_NEST || out.len() > 1000 {
                break;
            }
            let Ok(it) = self.f.get(id) else { continue };
            out.push(ty(&it));
            match it.ty {
                b"IFCMAPPEDITEM" => {
                    if let Some(rep) = self
                        .f
                        .get(it.attr(0).as_ref().unwrap_or(0))
                        .ok()
                        .and_then(|m| self.f.get(m.attr(1).as_ref()?).ok())
                    {
                        stack.extend(
                            rep.attr(3)
                                .as_list()
                                .unwrap_or(&[])
                                .iter()
                                .filter_map(|v| Some((v.as_ref()?, d + 1))),
                        );
                    }
                }
                b"IFCBOOLEANRESULT" | b"IFCBOOLEANCLIPPINGRESULT" => {
                    stack.extend(
                        [it.attr(1), it.attr(2)]
                            .iter()
                            .filter_map(|v| Some((v.as_ref()?, d + 1))),
                    );
                }
                b"IFCCSGSOLID" => stack.extend(it.attr(0).as_ref().map(|r| (r, d + 1))),
                _ => {}
            }
        }
    }
}

fn arc3(a: V2, b: V2, c: V2) -> Vec<V2> {
    // circle through three points
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-12 {
        return vec![a, b, c];
    }
    let sq = |p: V2| p[0] * p[0] + p[1] * p[1];
    let ux = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let uy = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    let r = ((a[0] - ux).powi(2) + (a[1] - uy).powi(2)).sqrt();
    let ang = |p: V2| (p[1] - uy).atan2(p[0] - ux);
    let (a0, am, mut a1) = (ang(a), ang(b), ang(c));
    // pick the direction that passes through b
    let tau = std::f64::consts::TAU;
    let norm_a = |x: f64| ((x - a0) % tau + tau) % tau;
    let ccw = norm_a(am) < norm_a(a1);
    if ccw {
        a1 = a0 + norm_a(a1);
    } else {
        a1 = a0 - (tau - norm_a(a1));
    }
    let n = (((a1 - a0).abs() / tau * CIRCLE_SEGMENTS as f64).ceil() as usize)
        .clamp(2, CIRCLE_SEGMENTS);
    (0..=n)
        .map(|i| {
            let t = a0 + (a1 - a0) * i as f64 / n as f64;
            [ux + r * t.cos(), uy + r * t.sin()]
        })
        .collect()
}

/// Round the given right-angle corners of a ring with radius r (fillets of steel sections).
fn fillet(ring: Vec<V2>, corners: &[usize], r: f64) -> Vec<V2> {
    if !(r > 0.0) || !r.is_finite() {
        return ring;
    }
    let n = ring.len();
    let mut out = Vec::with_capacity(n + corners.len() * 8);
    for (i, p) in ring.iter().enumerate() {
        if !corners.contains(&i) {
            out.push(*p);
            continue;
        }
        let (a, b) = (ring[(i + n - 1) % n], ring[(i + 1) % n]);
        let unit = |q: V2| {
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let l = (dx * dx + dy * dy).sqrt();
            if l > 0.0 {
                [dx / l, dy / l]
            } else {
                [0.0, 0.0]
            }
        };
        let (u1, u2) = (unit(a), unit(b));
        let c = [p[0] + r * (u1[0] + u2[0]), p[1] + r * (u1[1] + u2[1])];
        let (s, e) = (
            [p[0] + r * u1[0], p[1] + r * u1[1]],
            [p[0] + r * u2[0], p[1] + r * u2[1]],
        );
        let (a0, mut a1) = (
            (s[1] - c[1]).atan2(s[0] - c[0]),
            (e[1] - c[1]).atan2(e[0] - c[0]),
        );
        // the short way round (a quarter turn)
        while a1 - a0 > std::f64::consts::PI {
            a1 -= std::f64::consts::TAU;
        }
        while a0 - a1 > std::f64::consts::PI {
            a1 += std::f64::consts::TAU;
        }
        let k = 8;
        for j in 0..=k {
            let t = a0 + (a1 - a0) * j as f64 / k as f64;
            out.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
        }
    }
    out
}

fn area2(r: &[V2]) -> f64 {
    let mut a = 0.0;
    for i in 0..r.len() {
        let (p, q) = (r[i], r[(i + 1) % r.len()]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    a
}

fn clean_ring(r: &[V2]) -> Vec<V2> {
    let mut out: Vec<V2> = Vec::with_capacity(r.len());
    for p in r {
        if !p.iter().all(|c| c.is_finite()) {
            continue;
        }
        if out
            .last()
            .is_none_or(|q| (q[0] - p[0]).abs() > 1e-9 || (q[1] - p[1]).abs() > 1e-9)
        {
            out.push(*p);
        }
    }
    while out.len() > 1 {
        let (f, l) = (out[0], out[out.len() - 1]);
        if (f[0] - l[0]).abs() <= 1e-9 && (f[1] - l[1]).abs() <= 1e-9 {
            out.pop();
        } else {
            break;
        }
    }
    out
}

/// Triangulate a planar face (outer ring + holes, vertex indices into m.pos).
fn triangulate_face(rings: &[Vec<u32>], m: &mut Mesh) -> R<()> {
    let Some(outer) = rings.first() else {
        return Ok(());
    };
    if outer.len() < 3 {
        return Ok(());
    }
    if rings.len() == 1 && outer.len() == 3 {
        m.tri.push([outer[0], outer[1], outer[2]]);
        return Ok(());
    }
    // Newell normal
    let mut n = [0.0; 3];
    for i in 0..outer.len() {
        let (p, q) = (
            m.pos[outer[i] as usize],
            m.pos[outer[(i + 1) % outer.len()] as usize],
        );
        n[0] += (p[1] - q[1]) * (p[2] + q[2]);
        n[1] += (p[2] - q[2]) * (p[0] + q[0]);
        n[2] += (p[0] - q[0]) * (p[1] + q[1]);
    }
    let Some(n) = norm(n) else { return Ok(()) };
    let u = norm(cross(
        if n[0].abs() < 0.9 {
            [1., 0., 0.]
        } else {
            [0., 1., 0.]
        },
        n,
    ))
    .unwrap_or([1., 0., 0.]);
    let v = cross(n, u);
    let mut flat = Vec::new();
    let mut holes = Vec::new();
    let mut map = Vec::new();
    for (i, r) in rings.iter().enumerate() {
        if i > 0 {
            holes.push(map.len());
        }
        for &k in r {
            let p = m.pos[k as usize];
            flat.push(dot(p, u));
            flat.push(dot(p, v));
            map.push(k);
        }
    }
    let tris =
        earcutr::earcut(&flat, &holes, 2).map_err(|e| inv(&format!("triangulation {e:?}")))?;
    for t in tris.chunks(3) {
        // earcut returns CCW in (u, v); (u, v, n) is right-handed, so this matches the face normal
        let (a, b, c) = (map[t[0]], map[t[1]], map[t[2]]);
        let (pa, pb, pc) = (m.pos[a as usize], m.pos[b as usize], m.pos[c as usize]);
        if dot(cross(sub(pb, pa), sub(pc, pa)), n) >= 0.0 {
            m.tri.push([a, b, c]);
        } else {
            m.tri.push([a, c, b]);
        }
    }
    Ok(())
}

fn revolve(polys: &[Vec<Vec<V2>>], o: V3, d: V3, angle: f64) -> R<Mesh> {
    // Revolve each ring about the axis (o, d) in the profile plane, as a closed tube of quads plus end caps.
    let steps = ((angle.abs() / std::f64::consts::TAU * CIRCLE_SEGMENTS as f64).ceil() as usize)
        .clamp(1, CIRCLE_SEGMENTS);
    let mut m = Mesh {
        closed: true,
        ..Default::default()
    };
    for poly in polys {
        for ring in poly {
            let ring = clean_ring(ring);
            let n = ring.len();
            if n < 3 {
                continue;
            }
            let base = m.pos.len() as u32;
            for s in 0..=steps {
                let a = angle * s as f64 / steps as f64;
                for p in &ring {
                    m.pos.push(rotate([p[0], p[1], 0.0], o, d, a));
                }
            }
            for s in 0..steps as u32 {
                for k in 0..n as u32 {
                    let a = base + s * n as u32 + k;
                    let b = base + s * n as u32 + (k + 1) % n as u32;
                    let (a2, b2) = (a + n as u32, b + n as u32);
                    m.tri.push([a, b, b2]);
                    m.tri.push([a, b2, a2]);
                }
            }
        }
    }
    if m.is_empty() {
        return Err(inv("empty revolution"));
    }
    m.closed = false; // caps not generated: the share counts it as meshed, booleans won't accept it
    Ok(m)
}

fn rotate(p: V3, o: V3, d: V3, a: f64) -> V3 {
    let v = sub(p, o);
    let (c, s) = (a.cos(), a.sin());
    let r = [
        v[0] * c + cross(d, v)[0] * s + d[0] * dot(d, v) * (1.0 - c),
        v[1] * c + cross(d, v)[1] * s + d[1] * dot(d, v) * (1.0 - c),
        v[2] * c + cross(d, v)[2] * s + d[2] * dot(d, v) * (1.0 - c),
    ];
    [r[0] + o[0], r[1] + o[1], r[2] + o[2]]
}

fn sweep_disk(path: &[V3], r: f64) -> R<Mesh> {
    let path: Vec<V3> = path
        .iter()
        .copied()
        .filter(|p| p.iter().all(|c| c.is_finite()))
        .collect();
    if path.len() < 2 || !(r > 0.0) {
        return Err(inv("swept disk"));
    }
    let n = 12usize;
    let mut m = Mesh {
        closed: false,
        ..Default::default()
    };
    for (i, p) in path.iter().enumerate() {
        let t = norm(sub(
            path[(i + 1).min(path.len() - 1)],
            path[i.saturating_sub(1)],
        ))
        .unwrap_or([0., 0., 1.]);
        let u = norm(cross(
            if t[2].abs() < 0.9 {
                [0., 0., 1.]
            } else {
                [1., 0., 0.]
            },
            t,
        ))
        .unwrap_or([1., 0., 0.]);
        let v = cross(t, u);
        for k in 0..n {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            let off = [
                u[0] * a.cos() + v[0] * a.sin(),
                u[1] * a.cos() + v[1] * a.sin(),
                u[2] * a.cos() + v[2] * a.sin(),
            ];
            m.pos
                .push([p[0] + off[0] * r, p[1] + off[1] * r, p[2] + off[2] * r]);
        }
    }
    for i in 0..(path.len() - 1) as u32 {
        for k in 0..n as u32 {
            let a = i * n as u32 + k;
            let b = i * n as u32 + (k + 1) % n as u32;
            m.tri.push([a, b, b + n as u32]);
            m.tri.push([a, b + n as u32, a + n as u32]);
        }
    }
    // end caps
    let last = (path.len() - 1) as u32 * n as u32;
    let (c0, c1) = (m.pos.len() as u32, m.pos.len() as u32 + 1);
    m.pos.push(path[0]);
    m.pos.push(path[path.len() - 1]);
    for k in 0..n as u32 {
        let k2 = (k + 1) % n as u32;
        m.tri.push([c0, k2, k]);
        m.tri.push([c1, last + k, last + k2]);
    }
    m.closed = true;
    Ok(m)
}
