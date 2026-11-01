//! Canonical world representation.
//!
//! Design rule: `i64` lattice coordinates are truth.
//! `f32` is only used for local render / physics offsets.
//! See `docs/tech-decisions.md`.

/// Size of one chunk edge in cells.
pub const CHUNK_SIZE: i64 = 32;
/// Size of one cell in meters.
pub const CELL_SIZE_M: f64 = 1.0;

pub mod collision;

use std::collections::HashMap;
use std::path::Path;

/// Canonical chunk id in lattice space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkId(pub [i64; 3]);

/// Canonical cell coordinate in lattice space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellId(pub [i64; 3]);

impl CellId {
    pub fn chunk(&self) -> (ChunkId, [i64; 3]) {
        let c = [
            self.0[0].div_euclid(CHUNK_SIZE),
            self.0[1].div_euclid(CHUNK_SIZE),
            self.0[2].div_euclid(CHUNK_SIZE),
        ];
        let l = [
            self.0[0].rem_euclid(CHUNK_SIZE),
            self.0[1].rem_euclid(CHUNK_SIZE),
            self.0[2].rem_euclid(CHUNK_SIZE),
        ];
        (ChunkId(c), l)
    }
}

/// Floating origin: logical `f64` world position minus origin gives
/// camera-local `f32` position safe for rendering / physics.
#[derive(Debug, Clone, Copy)]
pub struct Origin {
    pub base: [f64; 3],
}

impl Origin {
    pub fn new(base: [f64; 3]) -> Self {
        Self { base }
    }

    /// Convert a logical position to a render-local `f32` offset.
    pub fn to_local(&self, world: [f64; 3]) -> [f32; 3] {
        [
            (world[0] - self.base[0]) as f32,
            (world[1] - self.base[1]) as f32,
            (world[2] - self.base[2]) as f32,
        ]
    }

    /// Rebase when the viewer drifts too far from the origin.
    /// Returns true if rebased.
    pub fn rebase_if_needed(&mut self, viewer: [f64; 3], threshold_m: f64) -> bool {
        let dx = viewer[0] - self.base[0];
        let dy = viewer[1] - self.base[1];
        let dz = viewer[2] - self.base[2];
        if dx * dx + dy * dy + dz * dz > threshold_m * threshold_m {
            self.base = viewer;
            true
        } else {
            false
        }
    }
}

/// Minimal signed-distance query interface.
/// Negative = solid, positive = empty. Used by meshing,
/// character feet / IK, and later the GBE subtract op.
pub trait Sdf {
    fn distance(&self, cell: CellId) -> f32;

    /// Continuous signed-field sample at a world position in meters.
    ///
    /// Trilinear interpolation of the integer lattice. Cell values sit on
    /// integer corners, so the zero surface between solid (-1) and empty
    /// (+1) cells lies exactly halfway between lattice points.
    fn sample(&self, p: [f64; 3]) -> f32 {
        let x0 = p[0].floor() as i64;
        let y0 = p[1].floor() as i64;
        let z0 = p[2].floor() as i64;
        let tx = (p[0] - x0 as f64) as f32;
        let ty = (p[1] - y0 as f64) as f32;
        let tz = (p[2] - z0 as f64) as f32;
        let c = |x: i64, y: i64, z: i64| self.distance(CellId([x, y, z]));
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let x00 = lerp(c(x0, y0, z0), c(x0 + 1, y0, z0), tx);
        let x10 = lerp(c(x0, y0 + 1, z0), c(x0 + 1, y0 + 1, z0), tx);
        let x01 = lerp(c(x0, y0, z0 + 1), c(x0 + 1, y0, z0 + 1), tx);
        let x11 = lerp(c(x0, y0 + 1, z0 + 1), c(x0 + 1, y0 + 1, z0 + 1), tx);
        lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
    }
}

/// Placeholder: everything solid except a horizontal shaft.
/// Real procedural SDF comes from `buramu_gen`.
/// Sparse per-cell overrides layered over a base generator: GBE holes
/// and (from M3) builder regrowth. Only edited cells are stored.
#[derive(Debug, Clone, Default)]
pub struct EditStore {
    cells: HashMap<CellId, f32>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SaveData {
    seed: u64,
    edits: Vec<SaveEdit>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SaveEdit {
    cell: [i64; 3],
    value: f32,
}

impl EditStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, cell: CellId) -> Option<f32> {
        self.cells.get(&cell).copied()
    }

    pub fn set(&mut self, cell: CellId, value: f32) {
        self.cells.insert(cell, value);
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Save as JSON `{seed, edits}` (sorted, so files diff cleanly).
    pub fn save(&self, path: &Path, seed: u64) -> std::io::Result<()> {
        let mut edits: Vec<SaveEdit> = self
            .cells
            .iter()
            .map(|(cell, value)| SaveEdit { cell: cell.0, value: *value })
            .collect();
        edits.sort_by_key(|e| e.cell);
        let data = SaveData { seed, edits };
        std::fs::write(path, serde_json::to_string_pretty(&data).unwrap())
    }

    /// Load a saved store. `Ok(None)` when the file is missing or the
    /// seed does not match (a different world, not an error).
    pub fn load(path: &Path, seed: u64) -> std::io::Result<Option<EditStore>> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let data: SaveData = serde_json::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if data.seed != seed {
            return Ok(None);
        }
        let mut store = EditStore::new();
        for edit in data.edits {
            store.set(CellId(edit.cell), edit.value);
        }
        Ok(Some(store))
    }
}

/// A base field with an [`EditStore`] layered on top. This is the one
/// field that collision, meshing, and the GBE all sample, so carved
/// holes behave identically in physics and rendering.
#[derive(Debug, Clone)]
pub struct LayeredField<B> {
    pub base: B,
    pub edits: EditStore,
}

impl<B> LayeredField<B> {
    pub fn new(base: B) -> Self {
        Self { base, edits: EditStore::new() }
    }
}

impl<B: Sdf> Sdf for LayeredField<B> {
    fn distance(&self, cell: CellId) -> f32 {
        self.edits.get(cell).unwrap_or_else(|| self.base.distance(cell))
    }
}

fn dist_point_segment(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    let t = if len2 < 1e-12 {
        0.0
    } else {
        ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / len2).clamp(0.0, 1.0)
    };
    let d = [ap[0] - ab[0] * t, ap[1] - ab[1] * t, ap[2] - ab[2] * t];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// GBE subtract: set every lattice point inside the capsule `a`->`b`
/// (world meters) to empty. Returns the chunks the capsule touches.
pub fn carve_capsule(
    edits: &mut EditStore,
    a: [f64; 3],
    b: [f64; 3],
    radius: f64,
) -> Vec<ChunkId> {
    let lo = [
        (a[0].min(b[0]) - radius).floor() as i64,
        (a[1].min(b[1]) - radius).floor() as i64,
        (a[2].min(b[2]) - radius).floor() as i64,
    ];
    let hi = [
        (a[0].max(b[0]) + radius).floor() as i64,
        (a[1].max(b[1]) + radius).floor() as i64,
        (a[2].max(b[2]) + radius).floor() as i64,
    ];
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                if dist_point_segment([x as f64, y as f64, z as f64], a, b) <= radius {
                    edits.set(CellId([x, y, z]), 1.0);
                }
            }
        }
    }
    let mut chunks = Vec::new();
    for cx in lo[0].div_euclid(CHUNK_SIZE)..=hi[0].div_euclid(CHUNK_SIZE) {
        for cy in lo[1].div_euclid(CHUNK_SIZE)..=hi[1].div_euclid(CHUNK_SIZE) {
            for cz in lo[2].div_euclid(CHUNK_SIZE)..=hi[2].div_euclid(CHUNK_SIZE) {
                chunks.push(ChunkId([cx, cy, cz]));
            }
        }
    }
    chunks
}

pub struct EmptyShaft;

impl Sdf for EmptyShaft {
    fn distance(&self, cell: CellId) -> f32 {
        if cell.0[0] == 0 && cell.0[2] == 0 { 1.0 } else { -1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_to_chunk_roundtrip() {
        let cell = CellId([33, -1, 0]);
        let (chunk, local) = cell.chunk();
        assert_eq!(chunk, ChunkId([1, -1, 0]));
        assert_eq!(local, [1, 31, 0]);
    }

    #[test]
    fn carve_empties_cells_and_reports_chunks() {
        let mut edits = EditStore::new();
        let chunks = carve_capsule(&mut edits, [2.0, -1.0, 8.0], [10.0, -1.0, 8.0], 1.2);
        assert!(edits.len() > 20, "capsule wrote cells, got {}", edits.len());
        assert_eq!(edits.get(CellId([5, -1, 8])), Some(1.0));
        assert_eq!(edits.get(CellId([5, 5, 8])), None);
        assert!(chunks.contains(&ChunkId([0, -1, 0])), "chunks {chunks:?}");
    }

    #[test]
    fn layered_field_passes_through_unedited_cells() {
        let mut field = LayeredField::new(EmptyShaft);
        // EmptyShaft: only the x=0,z=0 line is empty.
        assert_eq!(field.distance(CellId([0, 3, 0])), 1.0);
        assert_eq!(field.distance(CellId([4, 3, 0])), -1.0);
        field.edits.set(CellId([4, 3, 0]), 1.0);
        assert_eq!(field.distance(CellId([4, 3, 0])), 1.0);
    }

    #[test]
    fn sample_interpolates_between_lattice_points() {
        struct Slope;
        impl Sdf for Slope {
            fn distance(&self, cell: CellId) -> f32 {
                cell.0[1] as f32
            }
        }
        assert!((Slope.sample([0.0, 0.5, 0.0]) - 0.5).abs() < 1e-6);
        assert!((Slope.sample([0.0, -0.5, 0.0]) + 0.5).abs() < 1e-6);
    }

    #[test]
    fn edit_store_save_load_roundtrip() {
        let mut store = EditStore::new();
        store.set(CellId([1, -2, 3]), 1.0);
        store.set(CellId([-40, 0, 9]), -1.0);
        let path = std::env::temp_dir().join(format!(
            "buramu-world-test-{}.json",
            std::process::id()
        ));
        store.save(&path, 7).unwrap();
        let loaded = EditStore::load(&path, 7).unwrap().expect("saved store loads");
        assert_eq!(loaded.get(CellId([1, -2, 3])), Some(1.0));
        assert_eq!(loaded.get(CellId([-40, 0, 9])), Some(-1.0));
        // Wrong seed is a different world, not an error.
        assert!(EditStore::load(&path, 99).unwrap().is_none());
        let _ = std::fs::remove_file(&path);
        // Missing file is also `None`.
        assert!(EditStore::load(&path, 7).unwrap().is_none());
    }

    #[test]
    fn origin_keeps_local_small() {
        let mut origin = Origin::new([1_000_000.0, 0.0, 0.0]);
        let viewer = [1_000_050.0, 0.0, 0.0];
        let local = origin.to_local(viewer);
        assert!((local[0] - 50.0).abs() < 0.01);
        assert!(origin.rebase_if_needed(viewer, 10.0));
        assert_eq!(origin.to_local(viewer), [0.0, 0.0, 0.0]);
    }
}
