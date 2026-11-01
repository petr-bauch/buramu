//! First boss (M4): a scripted arena colossus.
//!
//! Articulated primitive build (body hierarchy + four leg links) until
//! a rigged glTF exists. Feet are planted on the SDF via
//! [`find_ground`], weak-point sockets take GBE hits, and destroying
//! every socket downs the boss.

use bevy::prelude::*;
use buramu_gen::Megastructure;
use buramu_world::{LayeredField, Sdf};

use crate::{FieldRes, Player};

/// Arena chamber carved by `buramu_gen` (x 8..40, z -40..-8, y 0..13).
pub const BOSS_HOME: Vec3 = Vec3::new(24.0, -0.5, -24.0);
const ARENA_CLAMP_X: (f32, f32) = (13.0, 35.0);
const ARENA_CLAMP_Z: (f32, f32) = (-35.0, -13.0);
const HIP_HEIGHT: f32 = 4.2;
const HIP_OFFSETS: [Vec3; 4] = [
    Vec3::new(2.2, HIP_HEIGHT, 1.3),
    Vec3::new(-2.2, HIP_HEIGHT, 1.3),
    Vec3::new(2.2, HIP_HEIGHT, -1.3),
    Vec3::new(-2.2, HIP_HEIGHT, -1.3),
];
const STEP_TIME_S: f32 = 0.45;
const STEP_TRIGGER_M: f32 = 1.3;
const BOSS_SPEED: f32 = 1.0;
const WAKE_RANGE: f32 = 28.0;
const STOP_RANGE: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossPhase {
    Dormant,
    Advance,
    Defeated,
}

#[derive(Clone, Copy)]
struct Foot {
    pos: Vec3,
    from: Vec3,
    to: Vec3,
    step_t: Option<f32>,
}

#[derive(Component)]
pub struct Boss {
    pub phase: BossPhase,
    yaw: f32,
    fall: f32,
    feet: [Foot; 4],
}

impl Boss {
    #[cfg(test)]
    pub fn feet_positions(&self) -> [Vec3; 4] {
        self.feet.map(|f| f.pos)
    }
}

#[derive(Component)]
pub struct BossLeg(pub usize);

#[derive(Component)]
pub struct WeakPoint {
    pub hits: u8,
    pub destroyed: bool,
    pub material: Handle<StandardMaterial>,
}

impl WeakPoint {
    /// Register one GBE hit. Returns true on the destroying hit.
    pub fn damage(&mut self) -> bool {
        if self.destroyed || self.hits == 0 {
            return false;
        }
        self.hits -= 1;
        if self.hits == 0 {
            self.destroyed = true;
            return true;
        }
        false
    }
}

/// March the field downward from `top` until solid; returns the
/// surface height. Falls back to `top - 20` in open air.
pub fn find_ground(field: &dyn Sdf, x: f64, z: f64, top: f64) -> f64 {
    let mut y = top;
    while y > top - 24.0 {
        if field.sample([x, y, z]) < 0.0 {
            return y + 0.1;
        }
        y -= 0.1;
    }
    top - 24.0
}

/// Ray vs sphere: nearest positive hit distance, if any.
pub fn ray_sphere(
    origin: Vec3,
    dir: Vec3,
    center: Vec3,
    radius: f32,
) -> Option<f32> {
    let oc = center - origin;
    let tca = oc.dot(dir);
    let d2 = oc.length_squared() - tca * tca;
    let r2 = radius * radius;
    if d2 > r2 {
        return None;
    }
    let t = tca - (r2 - d2).sqrt();
    if t > 0.0 {
        Some(t)
    } else {
        None
    }
}

pub fn spawn_boss(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    field: &LayeredField<Megastructure>,
) {
    let hull = materials.add(StandardMaterial {
        base_color: Color::srgb(0.09, 0.09, 0.11),
        perceptual_roughness: 0.85,
        ..default()
    });
    let weak_mesh = meshes.add(Sphere::new(0.55));
    let weak_positions = [
        Vec3::new(2.9, 5.3, 0.0),
        Vec3::new(-2.9, 5.3, 0.0),
        Vec3::new(0.0, 6.6, -1.6),
    ];

    let mut feet = [Foot {
        pos: Vec3::ZERO,
        from: Vec3::ZERO,
        to: Vec3::ZERO,
        step_t: None,
    }; 4];
    for (i, foot) in feet.iter_mut().enumerate() {
        let hip = BOSS_HOME + HIP_OFFSETS[i];
        let ground = find_ground(field, hip.x as f64, hip.z as f64, HIP_HEIGHT as f64);
        foot.pos = Vec3::new(hip.x, ground as f32, hip.z);
        foot.from = foot.pos;
        foot.to = foot.pos;
    }

    commands
        .spawn((
            Boss {
                phase: BossPhase::Dormant,
                yaw: 0.0,
                fall: 0.0,
                feet,
            },
            Transform::from_translation(BOSS_HOME),
            // Children carry meshes; without Visibility on the root
            // their InheritedVisibility warns (B0004) and misbehaves.
            Visibility::default(),
        ))
        .with_children(|parent| {
            // Torso and head block.
            parent.spawn((
                Mesh3d(meshes.add(Cuboid::new(5.6, 2.2, 3.0))),
                MeshMaterial3d(hull.clone()),
                Transform::from_xyz(0.0, 5.4, 0.0),
            ));
            parent.spawn((
                Mesh3d(meshes.add(Cuboid::new(1.6, 1.4, 1.8))),
                MeshMaterial3d(hull.clone()),
                Transform::from_xyz(0.0, 6.4, 2.2),
            ));
            // Weak-point sockets: emissive, GBE targets.
            for pos in weak_positions {
                let material = materials.add(StandardMaterial {
                    base_color: Color::srgb(0.2, 0.05, 0.02),
                    emissive: LinearRgba::new(6.0, 1.2, 0.25, 1.0),
                    ..default()
                });
                parent.spawn((
                    WeakPoint {
                        hits: 3,
                        destroyed: false,
                        material: material.clone(),
                    },
                    Mesh3d(weak_mesh.clone()),
                    MeshMaterial3d(material),
                    Transform::from_translation(pos),
                ));
            }
        });

    // Leg links are top-level so they can stretch hip -> foot freely.
    let leg_mesh = meshes.add(Cuboid::new(0.38, 1.0, 0.38));
    for i in 0..4 {
        commands.spawn((
            BossLeg(i),
            Mesh3d(leg_mesh.clone()),
            MeshMaterial3d(hull.clone()),
            Transform::default(),
        ));
    }
}

type BossRootQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Boss, &'static mut Transform),
    (Without<Player>, Without<BossLeg>),
>;
type LegQuery<'w, 's> = Query<
    'w,
    's,
    (&'static BossLeg, &'static mut Transform),
    (Without<Player>, Without<Boss>),
>;
type PlayerQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Player, &'static Transform),
    (Without<Boss>, Without<BossLeg>),
>;

pub fn boss_update(
    time: Res<Time>,
    field: Res<FieldRes>,
    weak_points: Query<&WeakPoint>,
    mut boss_q: BossRootQuery,
    mut legs: LegQuery,
    mut players: PlayerQuery,
) {
    let dt = time.delta_secs();
    let Ok((mut boss, mut root)) = boss_q.single_mut() else {
        return;
    };
    let player_pos = players.single().map(|(_, t)| t.translation).ok();

    // Defeat: every socket destroyed downs the colossus.
    if boss.phase != BossPhase::Defeated
        && !weak_points.is_empty()
        && weak_points.iter().all(|w| w.destroyed)
    {
        boss.phase = BossPhase::Defeated;
        bevy::log::info!("COLOSSUS DOWN");
    }

    if boss.phase == BossPhase::Defeated {
        boss.fall = (boss.fall + dt / 1.8).min(1.0);
        root.rotation = Quat::from_rotation_z(boss.fall * 1.35);
        root.translation.y = BOSS_HOME.y - boss.fall * 1.4;
        return; // legs freeze where they stand
    }

    // Wake and advance on the player, staying inside the arena.
    if let Some(player) = player_pos {
        let to_player = player - root.translation;
        let dist = Vec3::new(to_player.x, 0.0, to_player.z).length();
        if boss.phase == BossPhase::Dormant && dist < WAKE_RANGE {
            boss.phase = BossPhase::Advance;
        }
        if boss.phase == BossPhase::Advance && dist > STOP_RANGE {
            let move_dir = Vec3::new(to_player.x, 0.0, to_player.z).normalize_or_zero();
            root.translation += move_dir * BOSS_SPEED * dt;
            root.translation.x = root.translation.x.clamp(ARENA_CLAMP_X.0, ARENA_CLAMP_X.1);
            root.translation.z = root.translation.z.clamp(ARENA_CLAMP_Z.0, ARENA_CLAMP_Z.1);
            let target_yaw = move_dir.x.atan2(move_dir.z);
            let mut delta = target_yaw - boss.yaw;
            while delta > std::f32::consts::PI {
                delta -= std::f32::consts::TAU;
            }
            while delta < -std::f32::consts::PI {
                delta += std::f32::consts::TAU;
            }
            boss.yaw += delta * (1.0 - (-3.0 * dt).exp());
            root.rotation = Quat::from_rotation_y(boss.yaw);
        }
    }

    // Gait: plant feet, step the ones left behind, ground every target.
    let rotation = Quat::from_rotation_y(boss.yaw);
    let stepping = boss.feet.iter().filter(|f| f.step_t.is_some()).count();
    let mut new_steps = 0;
    for (i, foot) in boss.feet.iter_mut().enumerate() {
        let hip = root.translation + rotation * HIP_OFFSETS[i];
        if let Some(t) = foot.step_t {
            let t = (t + dt / STEP_TIME_S).min(1.0);
            if t >= 1.0 {
                foot.pos = foot.to;
                foot.step_t = None;
                // Stomp: landing near the player shoves them.
                if let Ok((mut player, player_tf)) = players.single_mut() {
                    let d = Vec3::new(
                        player_tf.translation.x - foot.pos.x,
                        0.0,
                        player_tf.translation.z - foot.pos.z,
                    );
                    if d.length() < 2.6 && player_tf.translation.y < foot.pos.y + 2.5 {
                        let push = d.normalize_or_zero() * 6.0;
                        player.vel[0] += push.x as f64;
                        player.vel[2] += push.z as f64;
                        player.vel[1] = player.vel[1].max(4.0);
                    }
                }
            } else {
                foot.step_t = Some(t);
                let arc = (t * std::f32::consts::PI).sin() * 0.9;
                foot.pos = foot.from.lerp(foot.to, t) + Vec3::Y * arc;
            }
        } else if Vec3::new(foot.pos.x - hip.x, 0.0, foot.pos.z - hip.z).length()
            > STEP_TRIGGER_M
            && stepping + new_steps < 2
        {
            let ground = find_ground(
                &field.0,
                hip.x as f64,
                hip.z as f64,
                HIP_HEIGHT as f64,
            );
            foot.from = foot.pos;
            foot.to = Vec3::new(hip.x, ground as f32, hip.z);
            foot.step_t = Some(0.0);
            new_steps += 1;
        }
    }

    // Stretch each leg link between hip and foot.
    for (leg, mut tf) in &mut legs {
        let hip = root.translation + rotation * HIP_OFFSETS[leg.0];
        let foot = boss.feet[leg.0].pos;
        let delta = foot - hip;
        let len = delta.length().max(0.01);
        tf.translation = hip + delta * 0.5;
        tf.rotation = Quat::from_rotation_arc(Vec3::Y, delta / len);
        tf.scale = Vec3::new(1.0, len, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_sphere_hits_and_misses() {
        let origin = Vec3::ZERO;
        let dir = Vec3::new(0.0, 0.0, -1.0);
        let hit = ray_sphere(origin, dir, Vec3::new(0.0, 0.0, -10.0), 1.0);
        assert!(hit.is_some_and(|t| (t - 9.0).abs() < 0.01));
        assert!(ray_sphere(origin, dir, Vec3::new(3.0, 0.0, -10.0), 1.0).is_none());
        assert!(ray_sphere(origin, dir, Vec3::new(0.0, 0.0, 10.0), 1.0).is_none());
    }

    #[test]
    fn weak_point_takes_three_hits() {
        let mut wp = WeakPoint {
            hits: 3,
            destroyed: false,
            material: Handle::default(),
        };
        assert!(!wp.damage());
        assert!(!wp.damage());
        assert!(wp.damage(), "third hit destroys");
        assert!(wp.destroyed);
        assert!(!wp.damage(), "already destroyed");
    }
}
