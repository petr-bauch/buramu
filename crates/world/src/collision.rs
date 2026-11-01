//! Kinematic capsule-vs-field collision.
//!
//! Pure geometry, no Bevy, no I/O: a body is integrated, then pushed out
//! of solid field regions along the sampled gradient. Tested below with
//! analytic fields; `buramu_gen` tests it against the real Megastructure.

/// Capsule shape, tracked by feet position (bottom of the capsule).
#[derive(Debug, Clone, Copy)]
pub struct MoveParams {
    /// Capsule radius in meters.
    pub radius: f64,
    /// Total capsule height in meters, feet to head.
    pub height: f64,
}

/// Outcome of one `move_body` step.
#[derive(Debug, Clone, Copy, Default)]
pub struct MoveOutcome {
    /// Touching a surface from above this step.
    pub grounded: bool,
    /// Touching a surface from the side this step.
    pub blocked: bool,
}

fn gradient(sample: &dyn Fn([f64; 3]) -> f32, p: [f64; 3]) -> ([f64; 3], f64) {
    let h = 0.08;
    let dx = sample([p[0] + h, p[1], p[2]]) - sample([p[0] - h, p[1], p[2]]);
    let dy = sample([p[0], p[1] + h, p[2]]) - sample([p[0], p[1] - h, p[2]]);
    let dz = sample([p[0], p[1], p[2] + h]) - sample([p[0], p[1], p[2] - h]);
    let g = [dx as f64, dy as f64, dz as f64];
    let len = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
    if len < 1e-6 {
        ([0.0, 1.0, 0.0], 1.0)
    } else {
        ([g[0] / len, g[1] / len, g[2] / len], len / (2.0 * h))
    }
}

/// Integrate `vel` over `dt` from `feet`, then resolve penetration.
///
/// `sample` maps a world position to a signed field value (negative =
/// solid). The capsule is approximated by three spheres (feet, middle,
/// head). Returns `(new_feet, new_vel, outcome)`. Velocity into a contact
/// surface is removed, so bodies rest on floors and slide along walls.
pub fn move_body(
    feet: [f64; 3],
    vel: [f64; 3],
    dt: f64,
    params: MoveParams,
    sample: &dyn Fn([f64; 3]) -> f32,
) -> ([f64; 3], [f64; 3], MoveOutcome) {
    let mut pos = [
        feet[0] + vel[0] * dt,
        feet[1] + vel[1] * dt,
        feet[2] + vel[2] * dt,
    ];
    let mut vel = vel;
    let mut outcome = MoveOutcome::default();

    let cyl = (params.height - 2.0 * params.radius).max(0.0);
    // Sphere centers as heights above the feet point.
    let stations = [params.radius, params.radius + cyl * 0.5, params.radius + cyl];

    for _ in 0..4 {
        // Resolve the deepest penetration each pass. The sampled field
        // is not a true distance field (|grad| != 1), so convert to an
        // approximate distance via the local gradient magnitude before
        // comparing against the radius.
        let mut worst: Option<([f64; 3], f64)> = None;
        for s in stations {
            let c = [pos[0], pos[1] + s, pos[2]];
            let (_, slope) = gradient(sample, c);
            let dist = sample(c) as f64 / slope.clamp(0.25, 8.0);
            let pen = dist - params.radius;
            if pen < 0.0 && worst.is_none_or(|(_, wp)| pen < wp) {
                worst = Some((c, pen));
            }
        }
        let Some((center, pen)) = worst else {
            break;
        };
        let (n, _) = gradient(sample, center);
        pos[0] -= n[0] * pen;
        pos[1] -= n[1] * pen;
        pos[2] -= n[2] * pen;
        let vn = vel[0] * n[0] + vel[1] * n[1] + vel[2] * n[2];
        if vn < 0.0 {
            vel[0] -= n[0] * vn;
            vel[1] -= n[1] * vn;
            vel[2] -= n[2] * vn;
        }
        if n[1] > 0.6 {
            outcome.grounded = true;
        } else if n[1].abs() <= 0.6 {
            outcome.blocked = true;
        }
    }

    (pos, vel, outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: MoveParams = MoveParams { radius: 0.35, height: 1.8 };

    /// Flat floor: surface at y = 0, solid below.
    fn floor(p: [f64; 3]) -> f32 {
        p[1] as f32
    }

    /// Wall: surface at x = 5, solid beyond.
    fn wall(p: [f64; 3]) -> f32 {
        (5.0 - p[0]) as f32
    }

    #[test]
    fn falls_and_lands_on_floor() {
        let mut feet = [0.0, 2.0, 0.0];
        let mut vel = [0.0, 0.0, 0.0];
        let mut grounded = false;
        for _ in 0..240 {
            vel[1] -= 20.0 * 0.016;
            let (p, v, o) = move_body(feet, vel, 0.016, P, &floor);
            feet = p;
            vel = v;
            grounded = o.grounded;
        }
        assert!(grounded, "body should be resting on the floor");
        assert!(feet[1].abs() < 0.05, "feet near y=0, got {}", feet[1]);
        assert!(vel[1].abs() < 0.05, "vertical velocity killed, got {}", vel[1]);
    }

    #[test]
    fn wall_blocks_horizontal_motion() {
        let mut feet = [0.0, 0.5, 0.0];
        let mut vel = [6.0, 0.0, 0.0];
        for _ in 0..120 {
            let (p, v, _) = move_body(feet, vel, 0.016, P, &wall);
            feet = p;
            vel = v;
        }
        assert!(feet[0] <= 5.0 - P.radius + 0.05, "stopped before wall, got {}", feet[0]);
        assert!(vel[0].abs() < 0.05, "horizontal velocity killed, got {}", vel[0]);
    }

    #[test]
    fn slides_along_wall_not_through() {
        // Diagonal approach: x motion blocked, z motion preserved.
        let mut feet = [0.0, 0.5, 0.0];
        let mut vel = [6.0, 0.0, 3.0];
        for _ in 0..120 {
            let (p, v, _) = move_body(feet, vel, 0.016, P, &wall);
            feet = p;
            vel = v;
        }
        assert!(feet[0] <= 5.0 - P.radius + 0.05);
        assert!(feet[2] > 1.0, "slid along wall, z = {}", feet[2]);
    }
}
