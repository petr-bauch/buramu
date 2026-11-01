//! Deterministic procedural layout for Megastructure strata.
//!
//! Pure functions of canonical coordinates: same input always
//! gives same output, no stored state. Edits (GBE holes,
//! builder regrowth) live in `buramu_world` storage later,
//! layered on top of this base signal.

use buramu_world::{CellId, Sdf};

/// Which stratum / level a position belongs to.
/// Strata are stacked along Y, each `stratum_height_cells` thick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StratumId(pub i64);

pub fn stratum_of(cell: CellId, stratum_height_cells: i64) -> StratumId {
    StratumId(cell.0[1].div_euclid(stratum_height_cells))
}

/// Tiny deterministic hash -> [0, 1). No RNG state, portable.
pub fn hash01(x: i64, y: i64, z: i64, seed: u64) -> f64 {
    let mut h = seed
        .wrapping_add((x as u64).wrapping_mul(0x9E3779B97F4A7C15))
        .wrapping_add((y as u64).wrapping_mul(0xBF58476D1CE4E5B9))
        .wrapping_add((z as u64).wrapping_mul(0x94D049BB133111EB));
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D049BB133111EB);
    h ^= h >> 31;
    (h as f64 / u64::MAX as f64).clamp(0.0, 1.0)
}

/// Base generator: mostly solid mass with carved galleries.
/// Later: replace with real brutalist grammar (shafts, slabs, rooms).
pub struct Megastructure {
    pub seed: u64,
    pub stratum_height_cells: i64,
}

impl Sdf for Megastructure {
    fn distance(&self, cell: CellId) -> f32 {
        let [x, y, z] = cell.0;
        // Vertical shaft every 64 cells.
        let shaft = x.rem_euclid(64) == 0 && z.rem_euclid(64) == 0;
        // Horizontal gallery floor every 16 cells.
        let gallery = y.rem_euclid(16) == 0;
        // Sparse noise rooms so it is not a perfect grid.
        let noise = hash01(x / 8, y / 8, z / 8, self.seed) > 0.93;
        if shaft || gallery || noise { 1.0 } else { -1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let g = Megastructure { seed: 7, stratum_height_cells: 256 };
        let c = CellId([10, 20, 30]);
        assert_eq!(g.distance(c), g.distance(c));
    }

    #[test]
    fn strata_stack() {
        assert_eq!(stratum_of(CellId([0, 0, 0]), 256), StratumId(0));
        assert_eq!(stratum_of(CellId([0, 255, 0]), 256), StratumId(0));
        assert_eq!(stratum_of(CellId([0, 256, 0]), 256), StratumId(1));
        assert_eq!(stratum_of(CellId([0, -1, 0]), 256), StratumId(-1));
    }
}
