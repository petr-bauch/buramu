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
        // Horizontal gallery band every 16 cells. Three cells tall so a
        // ~1.8 m capsule fits: interior surface spans y in [-0.5, 2.5].
        let gallery = y.rem_euclid(16) < 3;
        // Sparse noise rooms so it is not a perfect grid.
        let noise = hash01(x / 8, y / 8, z / 8, self.seed) > 0.93;
        if shaft || gallery || noise { 1.0 } else { -1.0 }
    }
}

impl Megastructure {
    /// Continuous signed-field sample at a world position in meters.
    ///
    /// Trilinear interpolation of the integer lattice. Cell values sit on
    /// integer corners, so the zero surface between solid (-1) and empty
    /// (+1) cells lies exactly halfway: gallery floors at y = -0.5,
    /// ceilings at y = 2.5 for the band at y in {0, 1, 2}.
    pub fn sample(&self, p: [f64; 3]) -> f32 {
        <Self as Sdf>::sample(self, p)
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
    fn body_rests_on_gallery_floor() {
        use buramu_world::collision::{MoveParams, move_body};
        let g = Megastructure { seed: 7, stratum_height_cells: 256 };
        // Find an empty corridor spot away from shafts.
        let mut sx = None;
        for x in 8..56 {
            if g.sample([x as f64 + 0.5, 1.0, 8.5]) > 0.5 {
                sx = Some(x as f64 + 0.5);
                break;
            }
        }
        let sx = sx.expect("an empty corridor exists");
        let p = MoveParams { radius: 0.35, height: 1.8 };
        let mut feet = [sx, 1.0, 8.5];
        let mut vel = [0.0, 0.0, 0.0];
        let mut grounded = false;
        for _ in 0..240 {
            vel[1] -= 20.0 * 0.016;
            let (np, nv, o) = move_body(feet, vel, 0.016, p, &|q| g.sample(q));
            feet = np;
            vel = nv;
            grounded = o.grounded;
        }
        assert!(grounded, "body should rest on the gallery floor");
        assert!(
            (feet[1] - (-0.5)).abs() < 0.1,
            "feet on gallery floor y=-0.5, got {}",
            feet[1]
        );
    }

    #[test]
    fn strata_stack() {
        assert_eq!(stratum_of(CellId([0, 0, 0]), 256), StratumId(0));
        assert_eq!(stratum_of(CellId([0, 255, 0]), 256), StratumId(0));
        assert_eq!(stratum_of(CellId([0, 256, 0]), 256), StratumId(1));
        assert_eq!(stratum_of(CellId([0, -1, 0]), 256), StratumId(-1));
    }
}
