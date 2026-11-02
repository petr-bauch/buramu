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

/// Per-stratum generation parameters, derived deterministically
/// from `(seed, stratum)`. Stratum 0 keeps the original M1 rules so
/// spawn and the boss arena never change underfoot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StratumParams {
    /// Vertical spacing of gallery decks within the stratum.
    pub deck_period: i64,
    /// Horizontal spacing of 3x3 vertical shaft wells.
    pub shaft_period: i64,
    /// Noise-room cutoff (higher = fewer random rooms).
    pub noise_threshold: f64,
    /// Chance a 32-cell region grows a tall chamber over its deck.
    pub chamber_chance: f64,
}

impl Megastructure {
    pub fn stratum_params(&self, stratum: StratumId) -> StratumParams {
        if stratum.0 == 0 {
            return StratumParams {
                deck_period: 16,
                shaft_period: 64,
                noise_threshold: 0.93,
                chamber_chance: 0.0,
            };
        }
        let s = stratum.0;
        let pick = |salt: i64, n: i64| (hash01(s, salt, 0x5EED, self.seed) * n as f64) as i64;
        StratumParams {
            deck_period: [12, 16, 20, 24][pick(11, 4) as usize],
            shaft_period: [48, 64, 96][pick(23, 3) as usize],
            noise_threshold: 0.90 + hash01(s, 37, 0x5EED, self.seed) * 0.07,
            chamber_chance: 0.10 + hash01(s, 53, 0x5EED, self.seed) * 0.20,
        }
    }

    fn distance_stratified(&self, cell: CellId, stratum: StratumId) -> f32 {
        let [x, y, z] = cell.0;
        let p = self.stratum_params(stratum);
        let yl = y.rem_euclid(self.stratum_height_cells);
        // Gallery decks, three cells tall like stratum 0.
        let deck = yl % p.deck_period < 3;
        // Real shaft wells (3x3) running the full stratum height:
        // these are how levels connect vertically.
        let shaft =
            x.rem_euclid(p.shaft_period) < 3 && z.rem_euclid(p.shaft_period) < 3;
        // Noise rooms, re-salted per stratum so levels don't repeat.
        let salt = (stratum.0 as u64).wrapping_mul(0x9E3779B97F4A7C15);
        let noise = hash01(x / 8, y / 8, z / 8, self.seed ^ salt) > p.noise_threshold;
        // Chambers: some 32-cell regions rise 7 cells over their deck.
        let region = hash01(
            x.div_euclid(32),
            stratum.0,
            z.div_euclid(32),
            self.seed ^ 0xC04A4BE5,
        );
        let deck_base = yl / p.deck_period * p.deck_period;
        let chamber = region < p.chamber_chance
            && (deck_base..deck_base + 7).contains(&yl)
            && (4..28).contains(&x.rem_euclid(32))
            && (4..28).contains(&z.rem_euclid(32));
        if deck || shaft || noise || chamber { 1.0 } else { -1.0 }
    }
}

impl Sdf for Megastructure {
    fn distance(&self, cell: CellId) -> f32 {
        let stratum = stratum_of(cell, self.stratum_height_cells);
        if stratum.0 != 0 {
            return self.distance_stratified(cell, stratum);
        }
        let [x, y, z] = cell.0;
        // Vertical shaft every 64 cells.
        let shaft = x.rem_euclid(64) == 0 && z.rem_euclid(64) == 0;
        // Horizontal gallery band every 16 cells. Three cells tall so a
        // ~1.8 m capsule fits: interior surface spans y in [-0.5, 2.5].
        let gallery = y.rem_euclid(16) < 3;
        // Sparse noise rooms so it is not a perfect grid.
        let noise = hash01(x / 8, y / 8, z / 8, self.seed) > 0.93;
        // Scripted boss arena (M4): a tall chamber north of spawn.
        let arena = (8..40).contains(&x) && (-40..-8).contains(&z) && (0..13).contains(&y);
        if shaft || gallery || noise || arena { 1.0 } else { -1.0 }
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

    #[test]
    fn strata_have_varied_params() {
        let g = Megastructure { seed: 7, stratum_height_cells: 256 };
        let p0 = g.stratum_params(StratumId(0));
        assert_eq!(p0.deck_period, 16, "stratum 0 keeps the M1 rules");
        let mut distinct = std::collections::HashSet::new();
        for s in -4..8 {
            let p = g.stratum_params(StratumId(s));
            distinct.insert((p.deck_period, p.shaft_period));
            assert!((12..=24).contains(&p.deck_period));
            assert!((0.90..0.97).contains(&p.noise_threshold));
        }
        assert!(distinct.len() > 1, "strata differ: {distinct:?}");
        // Deterministic: same stratum, same params, every call.
        assert_eq!(
            g.stratum_params(StratumId(3)),
            g.stratum_params(StratumId(3))
        );
    }

    /// Flood fill over empty cells (6-neighbourhood) inside a box.
    fn reachable(
        g: &Megastructure,
        start: [i64; 3],
        lo: [i64; 3],
        hi: [i64; 3],
    ) -> std::collections::HashSet<[i64; 3]> {
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![start];
        seen.insert(start);
        while let Some(c) = stack.pop() {
            for d in [
                [1, 0, 0],
                [-1, 0, 0],
                [0, 1, 0],
                [0, -1, 0],
                [0, 0, 1],
                [0, 0, -1],
            ] {
                let n = [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
                if (0..3).any(|i| n[i] < lo[i] || n[i] > hi[i]) {
                    continue;
                }
                if seen.contains(&n) || g.distance(CellId(n)) <= 0.0 {
                    continue;
                }
                seen.insert(n);
                stack.push(n);
            }
        }
        seen
    }

    #[test]
    fn shafts_connect_decks_vertically() {
        let g = Megastructure { seed: 7, stratum_height_cells: 256 };
        // Stratum 1, two decks worth of volume including a shaft well.
        let p = g.stratum_params(StratumId(1));
        let seen = reachable(
            &g,
            [8, 256, 8],
            [0, 256, 0],
            [47, 256 + p.deck_period + 8, 47],
        );
        assert!(
            seen.iter().any(|c| c[1] >= 256 + p.deck_period),
            "flood fill climbed from deck 0 to the next deck (period {})",
            p.deck_period
        );
    }

    #[test]
    fn spawn_reaches_boss_arena() {
        let g = Megastructure { seed: 7, stratum_height_cells: 256 };
        let seen = reachable(&g, [8, 1, 8], [0, 0, -40], [47, 3, 16]);
        assert!(
            seen.contains(&[24, 1, -24]),
            "arena centre reachable from spawn corridor"
        );
    }

    #[test]
    fn empty_fraction_is_sane_per_stratum() {
        let g = Megastructure { seed: 7, stratum_height_cells: 256 };
        for s in -2..3 {
            let y0 = s * 256;
            let (mut empty, mut total) = (0u32, 0u32);
            for x in 0..48 {
                for y in y0..y0 + 48 {
                    for z in 0..48 {
                        total += 1;
                        if g.distance(CellId([x, y, z])) > 0.0 {
                            empty += 1;
                        }
                    }
                }
            }
            let frac = empty as f64 / total as f64;
            assert!(
                (0.03..0.65).contains(&frac),
                "stratum {s} empty fraction {frac:.3} is playable"
            );
        }
    }
}
