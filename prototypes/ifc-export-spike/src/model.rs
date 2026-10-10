//! A stand-in for HITO's M1 element store: parameters only, no meshes
//! (ADR 0014, "the parameters are the truth").

#[derive(Clone, Copy)]
pub enum Section {
    Rect { b: f64, h: f64 },
    Circle { r: f64 },
}

impl Section {
    pub fn area(&self) -> f64 {
        match *self {
            Section::Rect { b, h } => b * h,
            Section::Circle { r } => std::f64::consts::PI * r * r,
        }
    }
    pub fn half_extents(&self) -> (f64, f64) {
        match *self {
            Section::Rect { b, h } => (b / 2.0, h / 2.0),
            Section::Circle { r } => (r, r),
        }
    }
}

pub struct Material {
    pub name: String,
    /// f'c in MPa (CIRSOC 201 grade, e.g. H-30)
    pub fck_mpa: f64,
}

pub struct Level {
    pub name: String,
    pub elevation: f64,
}

pub struct GridAxis {
    pub tag: String,
    /// Offset from the grid origin: x for U axes (vertical lines), y for V axes.
    pub offset: f64,
}

pub struct Grid {
    pub name: String,
    pub u: Vec<GridAxis>,
    pub v: Vec<GridAxis>,
    pub extent: (f64, f64, f64, f64),
}

/// A frame-member type (column or beam): Revit's family type.
pub struct FrameType {
    pub name: String,
    pub section: Section,
    pub material: usize,
}

pub struct Column {
    pub mark: String,
    pub ty: usize,
    pub base_level: usize,
    pub top_level: usize,
    /// Either a grid intersection (u, v) or a free point.
    pub at: ColumnAt,
}

pub enum ColumnAt {
    Grid(usize, usize),
    Point(f64, f64),
}

pub struct Beam {
    pub mark: String,
    pub ty: usize,
    pub level: usize,
    pub start: (f64, f64),
    pub end: (f64, f64),
    /// Columns this beam frames into.
    pub frames_into: Vec<usize>,
}

pub struct Opening {
    /// In the host's local coordinates: slab plan x/y, or wall x along its axis.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub struct Slab {
    pub mark: String,
    pub level: usize,
    pub thickness: f64,
    pub outline: Vec<(f64, f64)>,
    pub material: usize,
    pub openings: Vec<Opening>,
}

pub struct Wall {
    pub mark: String,
    pub base_level: usize,
    pub height: f64,
    pub thickness: f64,
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub material: usize,
    /// x along the axis, y = sill height, w, h
    pub openings: Vec<Opening>,
}

pub struct Footing {
    pub mark: String,
    pub level: usize,
    pub centre: (f64, f64),
    pub size: (f64, f64),
    pub depth: f64,
    pub material: usize,
}

/// Where the project's local origin is on a projected CRS (#93).
pub struct Georef {
    pub crs: String,
    pub crs_description: String,
    pub eastings: f64,
    pub northings: f64,
    pub height: f64,
    /// Angle from the CRS easting axis to the project's x axis, radians.
    pub rotation: f64,
}

pub struct Model {
    pub name: String,
    pub georef: Option<Georef>,
    pub materials: Vec<Material>,
    pub levels: Vec<Level>,
    pub grid: Grid,
    pub frame_types: Vec<FrameType>,
    pub columns: Vec<Column>,
    pub beams: Vec<Beam>,
    pub slabs: Vec<Slab>,
    pub walls: Vec<Wall>,
    pub footings: Vec<Footing>,
}

impl Model {
    pub fn column_xy(&self, c: &Column) -> (f64, f64) {
        match c.at {
            ColumnAt::Grid(u, v) => (self.grid.u[u].offset, self.grid.v[v].offset),
            ColumnAt::Point(x, y) => (x, y),
        }
    }
}

/// Shoelace area of a simple polygon.
pub fn polygon_area(pts: &[(f64, f64)]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[(i + 1) % n];
            x0 * y1 - x1 * y0
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

/// The sample project used by the spike: one bay of a two-level RC frame.
pub fn sample() -> Model {
    let grid = Grid {
        name: "Grilla".into(),
        u: vec![
            GridAxis { tag: "A".into(), offset: 0.0 },
            GridAxis { tag: "B".into(), offset: 6.0 },
        ],
        v: vec![
            GridAxis { tag: "1".into(), offset: 0.0 },
            GridAxis { tag: "2".into(), offset: 5.0 },
        ],
        extent: (-1.0, -1.0, 7.0, 6.0),
    };
    Model {
        name: "Spike #3: pórtico de prueba".into(),
        // A plot in Rosario, Santa Fe: POSGAR 2007 / Argentina 5.
        georef: Some(Georef {
            crs: "EPSG:5347".into(),
            crs_description: "POSGAR 2007 / Argentina 5".into(),
            eastings: 5_438_250.0,
            northings: 6_354_120.0,
            height: 25.0,
            rotation: 0.0,
        }),
        materials: vec![Material { name: "Hormigón H-30".into(), fck_mpa: 30.0 }],
        levels: vec![
            Level { name: "Planta baja".into(), elevation: 0.0 },
            Level { name: "Primer piso".into(), elevation: 3.0 },
        ],
        grid,
        frame_types: vec![
            FrameType { name: "C 30x30".into(), section: Section::Rect { b: 0.3, h: 0.3 }, material: 0 },
            FrameType { name: "C Ø40".into(), section: Section::Circle { r: 0.2 }, material: 0 },
            FrameType { name: "V 20x50".into(), section: Section::Rect { b: 0.2, h: 0.5 }, material: 0 },
        ],
        columns: vec![
            Column { mark: "C1".into(), ty: 0, base_level: 0, top_level: 1, at: ColumnAt::Grid(0, 0) },
            Column { mark: "C2".into(), ty: 0, base_level: 0, top_level: 1, at: ColumnAt::Grid(1, 0) },
            Column { mark: "C3".into(), ty: 0, base_level: 0, top_level: 1, at: ColumnAt::Point(0.0, 5.0) },
            Column { mark: "C4".into(), ty: 1, base_level: 0, top_level: 1, at: ColumnAt::Point(6.0, 5.0) },
        ],
        beams: vec![
            Beam { mark: "V1".into(), ty: 2, level: 1, start: (0.0, 0.0), end: (6.0, 0.0), frames_into: vec![0, 1] },
            Beam { mark: "V2".into(), ty: 2, level: 1, start: (6.0, 0.0), end: (6.0, 5.0), frames_into: vec![1, 3] },
        ],
        slabs: vec![Slab {
            mark: "L1".into(),
            level: 1,
            thickness: 0.15,
            outline: vec![(-0.15, -0.15), (6.15, -0.15), (6.15, 5.15), (-0.15, 5.15)],
            material: 0,
            openings: vec![Opening { x: 2.0, y: 2.0, w: 1.0, h: 1.2 }],
        }],
        walls: vec![Wall {
            mark: "M1".into(),
            base_level: 0,
            height: 3.0,
            thickness: 0.2,
            start: (0.15, 5.0),
            end: (5.8, 5.0),
            material: 0,
            // A door flush with the wall's base: the coplanar case from spike #2.
            openings: vec![Opening { x: 2.0, y: 0.0, w: 0.9, h: 2.1 }],
        }],
        footings: vec![Footing {
            mark: "Z1".into(),
            level: 0,
            centre: (0.0, 0.0),
            size: (1.2, 1.2),
            depth: 0.5,
            material: 0,
        }],
    }
}
