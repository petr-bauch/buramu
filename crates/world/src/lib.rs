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
}

/// Placeholder: everything solid except a horizontal shaft.
/// Real procedural SDF comes from `buramu_gen`.
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
    fn origin_keeps_local_small() {
        let mut origin = Origin::new([1_000_000.0, 0.0, 0.0]);
        let viewer = [1_000_050.0, 0.0, 0.0];
        let local = origin.to_local(viewer);
        assert!((local[0] - 50.0).abs() < 0.01);
        assert!(origin.rebase_if_needed(viewer, 10.0));
        assert_eq!(origin.to_local(viewer), [0.0, 0.0, 0.0]);
    }
}
