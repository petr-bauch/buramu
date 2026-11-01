use bevy::input::mouse::MouseMotion;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::settings::{Backends, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use buramu_gen::Megastructure;
use buramu_world::collision::{MoveParams, move_body};
use buramu_world::{carve_capsule, regrow, ChunkId, EditStore, LayeredField, Sdf, CHUNK_SIZE};
use std::path::Path;

mod boss;
mod mesher;
use mesher::mesh_chunk;

const GRAVITY: f64 = 20.0;
const WALK_SPEED: f32 = 4.5;
const JUMP_SPEED: f64 = 5.5;
const BODY: MoveParams = MoveParams { radius: 0.35, height: 1.8 };
const CAMERA_BOOM: f32 = 5.5;

const WORLD_SEED: u64 = 7;
const SAVE_PATH: &str = "buramu-save.json";
/// Chunks meshed around the spawn chunk in X/Z (streaming comes later).
const MESH_RADIUS_CHUNKS: i64 = 2;
const GBE_RADIUS: f64 = 1.25;
const GBE_RANGE: f32 = 40.0;
const GBE_PENETRATION: f32 = 3.0;
const GBE_COOLDOWN_S: f32 = 0.25;
const BUILDER_TICK_S: f32 = 0.25;
const BUILDER_CELLS_PER_TICK: usize = 12;
const BUILDER_GRACE_S: f32 = 5.0;
const SAVE_THROTTLE_S: f32 = 2.0;

/// Backend set for wgpu. Defaults to the primary native backends
/// (Vulkan / DX12 / Metal), never OpenGL: wgpu's GL backend fails Bevy
/// 0.16 device creation (`indirect-validation` needs runtime-sized
/// arrays, which Mesa GL / WSLg D3D12-on-GL does not support) and loses
/// the device at startup. `WGPU_BACKEND` still overrides this.
fn backends_for(env_override: Option<Backends>) -> Backends {
    env_override.unwrap_or(Backends::PRIMARY)
}

fn wgpu_settings() -> WgpuSettings {
    WgpuSettings {
        backends: Some(backends_for(Backends::from_env())),
        ..Default::default()
    }
}

/// The world the game starts with: base generator plus any saved edits.
fn initial_field() -> LayeredField<Megastructure> {
    let mut field = LayeredField::new(Megastructure {
        seed: WORLD_SEED,
        stratum_height_cells: 256,
    });
    match EditStore::load(Path::new(SAVE_PATH), WORLD_SEED) {
        Ok(Some(store)) => {
            eprintln!("loaded {} saved edits from {SAVE_PATH}", store.len());
            field.edits = store;
        }
        Ok(None) => {}
        Err(e) => eprintln!("ignoring unreadable save {SAVE_PATH}: {e}"),
    }
    field
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(RenderPlugin {
            render_creation: wgpu_settings().into(),
            ..default()
        }))
        .insert_resource(FieldRes(initial_field()))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (grab_pointer, look, fire_beam, update_beams, reset_edits, builder, boss::boss_update, walk),
        )
        .run();
}

#[derive(Resource)]
struct FieldRes(LayeredField<Megastructure>);

#[derive(Component, Clone, Copy)]
struct ChunkTag(ChunkId);

#[derive(Component)]
struct Beam {
    expires: f32,
}

#[derive(Resource, Default)]
struct FireState {
    cooldown: f32,
    was_locked: bool,
}

#[derive(Resource)]
struct BeamMaterial(Handle<StandardMaterial>);

#[derive(Component)]
struct Builder {
    home: Vec3,
}

#[derive(Resource, Default)]
struct BuilderState {
    tick: f32,
    last_shot_at: f32,
    repair_site: Option<Vec3>,
    save_dirty: bool,
    save_timer: f32,
}

#[derive(Component, Clone, Copy)]
struct Player {
    vel: [f64; 3],
    grounded: bool,
    yaw: f32,
    pitch: f32,
}

/// Feet position in world meters. Scans for an empty corridor spot so the
/// player never spawns inside a noise blob.
fn find_spawn(field: &LayeredField<Megastructure>) -> [f64; 3] {
    for x in 8..56 {
        for z in [8.5, 16.5, 24.5, 32.5] {
            if field.sample([x as f64 + 0.5, 1.0, z]) > 0.5 {
                return [x as f64 + 0.5, 0.0, z];
            }
        }
    }
    [8.5, 0.0, 8.5]
}

fn chunk_of(p: [f64; 3]) -> ChunkId {
    ChunkId([
        (p[0].floor() as i64).div_euclid(CHUNK_SIZE),
        (p[1].floor() as i64).div_euclid(CHUNK_SIZE),
        (p[2].floor() as i64).div_euclid(CHUNK_SIZE),
    ])
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    field: Res<FieldRes>,
) {
    let spawn = find_spawn(&field.0);

    // Meshed Megastructure around the spawn chunk. Surface Nets chunk
    // meshes replace the M1 placeholder planes; the field zeros give
    // gallery floors at y = -0.5 and ceilings at y = 2.5 as before.
    let concrete = materials.add(StandardMaterial {
        base_color: Color::srgb(0.10, 0.10, 0.12),
        perceptual_roughness: 0.95,
        cull_mode: None,
        ..default()
    });
    let center = chunk_of(spawn);
    for dx in -MESH_RADIUS_CHUNKS..=MESH_RADIUS_CHUNKS {
        for dy in -1..=0 {
            for dz in -MESH_RADIUS_CHUNKS..=MESH_RADIUS_CHUNKS {
                let chunk = ChunkId([center.0[0] + dx, center.0[1] + dy, center.0[2] + dz]);
                commands.spawn((
                    ChunkTag(chunk),
                    Mesh3d(meshes.add(mesh_chunk(&field.0, chunk))),
                    MeshMaterial3d(concrete.clone()),
                    Transform::from_xyz(
                        (chunk.0[0] * CHUNK_SIZE) as f32,
                        (chunk.0[1] * CHUNK_SIZE) as f32,
                        (chunk.0[2] * CHUNK_SIZE) as f32,
                    ),
                ));
            }
        }
    }
    commands.insert_resource(BeamMaterial(materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::new(2.0, 6.0, 8.0, 1.0),
        ..default()
    })));
    commands.insert_resource(FireState::default());

    // Emissive guide strips along the corridor, BLAME! style.
    let strip = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::new(0.4, 0.9, 1.0, 1.0),
        ..default()
    });
    let strip_mesh = meshes.add(Cuboid::new(2.0, 0.06, 0.12));
    for i in -5..=5 {
        commands.spawn((
            Mesh3d(strip_mesh.clone()),
            MeshMaterial3d(strip.clone()),
            Transform::from_xyz(spawn[0] as f32 + i as f32 * 8.0, 2.44, spawn[2] as f32 + 3.0),
        ));
    }
    commands.spawn((
        PointLight { intensity: 60.0, range: 25.0, ..default() },
        Transform::from_xyz(spawn[0] as f32, 2.0, spawn[2] as f32),
    ));

    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.35, 0.45, 0.6),
        brightness: 0.4,
        ..default()
    });
    commands.spawn((
        DirectionalLight { shadows_enabled: false, illuminance: 1500.0, ..default() },
        Transform::from_xyz(10.0, 20.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // The wanderer. Dark coat, pale head: Killy first, detail later.
    let coat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.08, 0.10),
        perceptual_roughness: 0.8,
        ..default()
    });
    commands.spawn((
        Player { vel: [0.0, 0.0, 0.0], grounded: false, yaw: 0.0, pitch: -0.15 },
        Mesh3d(meshes.add(Capsule3d::new(
            BODY.radius as f32,
            (BODY.height - 2.0 * BODY.radius) as f32,
        ))),
        MeshMaterial3d(coat.clone()),
        Transform::from_translation(Vec3::new(
            spawn[0] as f32,
            (spawn[1] + BODY.height * 0.5) as f32,
            spawn[2] as f32,
        )),
    ));

    // The builder: a hovering drone that regrows GBE damage (M3).
    // Dark hull, emissive core, parked above the spawn corridor.
    let home = Vec3::new(spawn[0] as f32 + 3.0, 3.4, spawn[2] as f32 + 3.0);
    commands
        .spawn((
            Builder { home },
            Mesh3d(meshes.add(Cuboid::new(0.55, 0.32, 0.55))),
            MeshMaterial3d(coat),
            Transform::from_translation(home),
        ))
        .with_child((
            Mesh3d(meshes.add(Cuboid::new(0.2, 0.12, 0.2))),
            MeshMaterial3d(strip.clone()),
            Transform::from_xyz(0.0, -0.22, 0.0),
        ));
    commands.insert_resource(BuilderState {
        last_shot_at: -BUILDER_GRACE_S,
        ..default()
    });

    boss::spawn_boss(&mut commands, &mut meshes, &mut materials, &field.0);

    commands.spawn((
        Camera3d::default(),
        DistanceFog {
            color: Color::srgb(0.02, 0.03, 0.05),
            directional_light_color: Color::NONE,
            directional_light_exponent: 1.0,
            falloff: FogFalloff::from_visibility(70.0),
        },
    ));
}

/// Rebuild the meshes of already-spawned chunks after an edit.
fn remesh_chunks(
    chunks: &[ChunkId],
    field: &LayeredField<Megastructure>,
    meshes: &mut Assets<Mesh>,
    query: &mut Query<(&ChunkTag, &mut Mesh3d)>,
) {
    for chunk in chunks {
        for (tag, mut mesh) in query.iter_mut() {
            if tag.0 == *chunk {
                let old = mesh.0.clone();
                *mesh = Mesh3d(meshes.add(mesh_chunk(field, *chunk)));
                meshes.remove(&old);
            }
        }
    }
}

/// Click to capture the mouse for look control. Esc releases.
fn grab_pointer(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    if buttons.just_pressed(MouseButton::Left) {
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
        window.cursor_options.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        window.cursor_options.grab_mode = CursorGrabMode::None;
        window.cursor_options.visible = true;
    }
}

/// Mouse look. Yaw/pitch live on the player; the camera follows in `walk`.
fn look(
    mut motion: EventReader<MouseMotion>,
    mut players: Query<&mut Player>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    if window.cursor_options.grab_mode != CursorGrabMode::Locked {
        return;
    }
    let Ok(mut player) = players.single_mut() else {
        return;
    };
    for e in motion.read() {
        player.yaw -= e.delta.x * 0.0025;
        player.pitch = (player.pitch - e.delta.y * 0.0025).clamp(-1.4, 1.4);
    }
}

/// Left mouse fires the GBE while the pointer is locked (the click that
/// locks the pointer does not fire). The beam marches the edited field
/// from the camera, carves a capsule through the hit point, remeshes
/// the touched chunks, and autosaves the edit diff.
#[allow(clippy::too_many_arguments)]
fn fire_beam(
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<&Transform, (With<Camera>, Without<Player>)>,
    beam_material: Option<Res<BeamMaterial>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut field: ResMut<FieldRes>,
    mut state: ResMut<FireState>,
    mut builder_state: ResMut<BuilderState>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut weak_points: Query<(Entity, &GlobalTransform, &mut boss::WeakPoint)>,
    mut chunks: Query<(&ChunkTag, &mut Mesh3d)>,
) {
    state.cooldown = (state.cooldown - time.delta_secs()).max(0.0);
    let locked = windows
        .single()
        .map(|w| w.cursor_options.grab_mode == CursorGrabMode::Locked)
        .unwrap_or(false);
    let just_locked = locked && !state.was_locked;
    state.was_locked = locked;
    if !locked
        || just_locked
        || state.cooldown > 0.0
        || !buttons.just_pressed(MouseButton::Left)
    {
        return;
    }
    let Ok(cam) = cameras.single() else {
        return;
    };
    let origin = cam.translation;
    let dir = cam.forward().as_vec3();

    // March the same field the body collides with until solid.
    let mut end = origin + dir * GBE_RANGE;
    let mut field_t = GBE_RANGE;
    let mut t = 0.5;
    while t < GBE_RANGE {
        let p = origin + dir * t;
        if field.0.sample([p.x as f64, p.y as f64, p.z as f64]) < 0.0 {
            field_t = t;
            end = p + dir * GBE_PENETRATION;
            break;
        }
        t += 0.1;
    }

    // Boss weak points eat the beam before the field does.
    let mut weak_hit: Option<(f32, Entity)> = None;
    for (entity, gt, wp) in &weak_points {
        if wp.destroyed {
            continue;
        }
        if let Some(t) = boss::ray_sphere(origin, dir, gt.translation(), 0.65) {
            if t < field_t && weak_hit.is_none_or(|(best, _)| t < best) {
                weak_hit = Some((t, entity));
            }
        }
    }
    if let Some((t, entity)) = weak_hit {
        if let Ok((_, _, mut wp)) = weak_points.get_mut(entity) {
            if wp.damage() {
                if let Some(m) = std_materials.get_mut(&wp.material) {
                    m.emissive = LinearRgba::new(0.25, 0.04, 0.02, 1.0);
                }
            }
        }
        end = origin + dir * t;
    } else {
        let affected = carve_capsule(
            &mut field.0.edits,
            [origin.x as f64, origin.y as f64, origin.z as f64],
            [end.x as f64, end.y as f64, end.z as f64],
            GBE_RADIUS,
        );
        remesh_chunks(&affected, &field.0, &mut meshes, &mut chunks);
        save_edits(&field.0);
    }
    state.cooldown = GBE_COOLDOWN_S;
    builder_state.last_shot_at = time.elapsed_secs();

    // Brief emissive beam visual from just ahead of the camera.
    if let Some(material) = beam_material {
        let a = origin + dir * 0.5;
        let delta = end - a;
        let len = delta.length().max(0.01);
        commands.spawn((
            Beam { expires: time.elapsed_secs() + 0.15 },
            Mesh3d(meshes.add(Cylinder::new(0.06, 1.0))),
            MeshMaterial3d(material.0.clone()),
            Transform {
                translation: a + delta * 0.5,
                rotation: Quat::from_rotation_arc(Vec3::Y, delta / len),
                scale: Vec3::new(1.0, len, 1.0),
            },
        ));
    }
}

fn update_beams(
    time: Res<Time>,
    mut commands: Commands,
    beams: Query<(Entity, &Beam)>,
) {
    for (entity, beam) in &beams {
        if time.elapsed_secs() >= beam.expires {
            commands.entity(entity).despawn();
        }
    }
}

/// Delete resets the world: clear all edits, drop the save, remesh.
fn reset_edits(
    keys: Res<ButtonInput<KeyCode>>,
    mut field: ResMut<FieldRes>,
    mut builder_state: ResMut<BuilderState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: Query<(&ChunkTag, &mut Mesh3d)>,
) {
    if !keys.just_pressed(KeyCode::Delete) {
        return;
    }
    field.0.edits.clear();
    builder_state.repair_site = None;
    builder_state.save_dirty = false;
    let _ = std::fs::remove_file(SAVE_PATH);
    let all: Vec<ChunkId> = chunks.iter().map(|(tag, _)| tag.0).collect();
    remesh_chunks(&all, &field.0, &mut meshes, &mut chunks);
}

/// The builder drone: steers toward the current repair site (or a slow
/// patrol when the world is pristine) and, on a timer, regrows damaged
/// cells rim-first via `buramu_world::regrow`. Fresh GBE damage gets a
/// grace period so the player can out-carve the repair.
#[allow(clippy::too_many_arguments)]
fn builder(
    time: Res<Time>,
    mut field: ResMut<FieldRes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut state: ResMut<BuilderState>,
    mut chunks: Query<(&ChunkTag, &mut Mesh3d)>,
    mut drones: Query<(&Builder, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();

    // Fly: hover above the repair site, else patrol around home.
    for (drone, mut tf) in &mut drones {
        let patrol = drone.home
            + Vec3::new(
                (now * 0.25).sin() * 5.0,
                (now * 0.8).sin() * 0.3,
                (now * 0.25).cos() * 5.0,
            );
        let goal = state
            .repair_site
            .map(|site| site + Vec3::new(0.0, 1.6, 0.0))
            .unwrap_or(patrol);
        let blend = 1.0 - (-2.0 * dt).exp();
        let delta = goal - tf.translation;
        tf.translation += delta * blend;
    }

    // Throttled autosave while repair is in progress.
    state.save_timer += dt;
    if state.save_dirty && state.save_timer >= SAVE_THROTTLE_S {
        save_edits(&field.0);
        state.save_dirty = false;
        state.save_timer = 0.0;
    }

    state.tick += dt;
    if state.tick < BUILDER_TICK_S {
        return;
    }
    state.tick = 0.0;
    if now - state.last_shot_at < BUILDER_GRACE_S || field.0.edits.is_empty() {
        state.repair_site = None;
        return;
    }

    let LayeredField { edits, base } = &mut field.0;
    let reverted = regrow(edits, base, BUILDER_CELLS_PER_TICK);
    if reverted.is_empty() {
        state.repair_site = None;
        return;
    }
    let mut sum = Vec3::ZERO;
    for cell in &reverted {
        sum += Vec3::new(
            cell.0[0] as f32 + 0.5,
            cell.0[1] as f32 + 0.5,
            cell.0[2] as f32 + 0.5,
        );
    }
    state.repair_site = Some(sum / reverted.len() as f32);

    let mut affected: Vec<ChunkId> =
        reverted.iter().map(|cell| cell.chunk().0).collect();
    affected.sort_by_key(|c| c.0);
    affected.dedup();
    remesh_chunks(&affected, &field.0, &mut meshes, &mut chunks);

    state.save_dirty = true;
    if field.0.edits.is_empty() {
        // Repair complete: persist immediately, go back to patrol.
        save_edits(&field.0);
        state.save_dirty = false;
        state.save_timer = 0.0;
        state.repair_site = None;
    }
}

fn save_edits(field: &LayeredField<Megastructure>) {
    if let Err(e) = field.edits.save(Path::new(SAVE_PATH), WORLD_SEED) {
        bevy::log::warn!("could not save edits to {SAVE_PATH}: {e}");
    }
}

fn walk(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gen: Res<FieldRes>,
    mut players: Query<(&mut Player, &mut Transform)>,
    mut cameras: Query<&mut Transform, (With<Camera>, Without<Player>)>,
) {
    let dt = time.delta_secs_f64().min(0.05);
    let Ok((mut player, mut body)) = players.single_mut() else {
        return;
    };

    // Wish direction in camera space.
    let (sy, cy) = (player.yaw.sin() as f64, player.yaw.cos() as f64);
    let (mut mx, mut mz) = (0.0f64, 0.0f64);
    if keys.pressed(KeyCode::KeyW) {
        mx -= sy;
        mz -= cy;
    }
    if keys.pressed(KeyCode::KeyS) {
        mx += sy;
        mz += cy;
    }
    if keys.pressed(KeyCode::KeyA) {
        mx -= cy;
        mz += sy;
    }
    if keys.pressed(KeyCode::KeyD) {
        mx += cy;
        mz -= sy;
    }
    let mlen = (mx * mx + mz * mz).sqrt().max(1.0);
    let wish = [mx / mlen * WALK_SPEED as f64, mz / mlen * WALK_SPEED as f64];

    // Horizontal velocity eases toward wish; vertical is gravity + jump.
    let ease = 1.0 - (-12.0 * dt).exp();
    player.vel[0] += (wish[0] - player.vel[0]) * ease;
    player.vel[2] += (wish[1] - player.vel[2]) * ease;
    player.vel[1] -= GRAVITY * dt;
    if keys.just_pressed(KeyCode::Space) && player.grounded {
        player.vel[1] = JUMP_SPEED;
    }

    let half = BODY.height as f32 * 0.5;
    let feet = [
        body.translation.x as f64,
        (body.translation.y - half) as f64,
        body.translation.z as f64,
    ];
    let (feet, vel, outcome) = move_body(feet, player.vel, dt, BODY, &|p| gen.0.sample(p));
    player.vel = vel;
    player.grounded = outcome.grounded;
    body.translation = Vec3::new(
        feet[0] as f32,
        (feet[1] + BODY.height * 0.5) as f32,
        feet[2] as f32,
    );
    // Face travel direction, keep it simple.
    if player.vel[0] * player.vel[0] + player.vel[2] * player.vel[2] > 0.5 {
        let face = player.vel[2].atan2(player.vel[0]) as f32 - std::f32::consts::FRAC_PI_2;
        body.rotation = Quat::from_rotation_y(-face);
    }

    // Third-person boom behind the head.
    if let Ok(mut cam) = cameras.single_mut() {
        let (sp, cp) = player.pitch.sin_cos();
        let (sy, cy) = player.yaw.sin_cos();
        let head = body.translation + Vec3::new(0.0, BODY.height as f32 * 0.5 - 0.2, 0.0);
        let offset = Vec3::new(sy * cp, -sp, cy * cp) * CAMERA_BOOM;
        cam.translation = head + offset;
        cam.look_at(head, Vec3::Y);
    }
}

/// Headless end-to-end: drive the real systems with real input resources
/// over frames, no window or GPU needed.
#[cfg(test)]
mod headless {
    use super::*;
    use bevy::input::ButtonState;
    use bevy::input::InputPlugin;
    use bevy::input::keyboard::{Key, KeyboardInput};
    use bevy::input::mouse::MouseButtonInput;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin));
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.insert_resource(FieldRes(LayeredField::new(Megastructure {
            seed: 7,
            stratum_height_cells: 256,
        })));
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.add_systems(Startup, setup);
        // Fixed-step clock: each update advances exactly 1/60 s, so the
        // tests are deterministic and independent of wall time.
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
        app.add_systems(
            Update,
            (grab_pointer, look, fire_beam, update_beams, reset_edits, builder, boss::boss_update, walk),
        );
        app.update(); // runs setup once
        app
    }

    fn step(app: &mut App) {
        app.update();
    }

    fn key(app: &mut App, key_code: KeyCode, logical_key: Key, state: ButtonState) {
        app.world_mut().send_event(KeyboardInput {
            key_code,
            logical_key,
            state,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
    }

    fn mouse(app: &mut App, state: ButtonState) {
        app.world_mut().send_event(MouseButtonInput {
            button: MouseButton::Left,
            state,
            window: Entity::PLACEHOLDER,
        });
    }

    fn one<T: Component + Clone, F: bevy::ecs::query::QueryFilter>(app: &mut App) -> T {
        // Split across statements: QueryState is owned, so the world's
        // mutable borrow ends before the shared reborrow in `single`.
        let mut qs = app.world_mut().query_filtered::<&T, F>();
        qs.single(app.world()).unwrap().clone()
    }

    fn grab_mode(app: &mut App) -> CursorGrabMode {
        let mut qs = app.world_mut().query_filtered::<&Window, With<PrimaryWindow>>();
        qs.single(app.world()).unwrap().cursor_options.grab_mode
    }

    fn player_pos(app: &mut App) -> Vec3 {
        one::<Transform, With<Player>>(app).translation
    }

    type CamFilter = (With<Camera>, Without<Player>);

    fn cam_pos(app: &mut App) -> Vec3 {
        one::<Transform, CamFilter>(app).translation
    }

    fn edit_count(app: &mut App) -> usize {
        app.world().resource::<FieldRes>().0.edits.len()
    }

    #[test]
    fn falls_lands_walks_and_jumps() {
        let mut app = test_app();
        let start = player_pos(&mut app);

        // Gravity pulls the body onto the gallery floor.
        for _ in 0..120 {
            step(&mut app);
        }
        let landed = player_pos(&mut app);
        assert!(landed.y < start.y, "fell from spawn");
        let grounded = one::<Player, With<Player>>(&mut app).grounded;
        assert!(grounded, "resting on the floor after 2 s");

        // Hold W: walk forward (-z at yaw 0).
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..120 {
            step(&mut app);
        }
        let walked = player_pos(&mut app);
        assert!(
            walked.z < landed.z - 2.0,
            "walked forward, z {} -> {}",
            landed.z,
            walked.z
        );

        // Jump: feet leave the floor, then land again.
        key(&mut app, KeyCode::Space, Key::Space, ButtonState::Pressed);
        step(&mut app);
        key(&mut app, KeyCode::Space, Key::Space, ButtonState::Released);
        let mut apex = walked.y;
        let mut relanded = false;
        for _ in 0..180 {
            step(&mut app);
            let p = player_pos(&mut app);
            apex = apex.max(p.y);
            if one::<Player, With<Player>>(&mut app).grounded {
                relanded = true;
                break;
            }
        }
        assert!(apex > walked.y + 0.3, "jump gained height, apex {apex}");
        assert!(relanded, "landed again after the jump");

        // Camera keeps its boom behind the head.
        let cam = cam_pos(&mut app);
        let head = player_pos(&mut app);
        let dist = ((cam.x - head.x).powi(2) + (cam.z - head.z).powi(2)).sqrt();
        assert!((dist - CAMERA_BOOM).abs() < 0.6, "boom length, got {dist}");
        assert!(cam.z > head.z, "camera trails behind forward motion");
    }

    #[test]
    fn click_locks_pointer_and_mouse_turns() {
        let mut app = test_app();
        // Click captures the pointer.
        mouse(&mut app, ButtonState::Pressed);
        step(&mut app);
        assert_eq!(grab_mode(&mut app), CursorGrabMode::Locked);

        // Mouse motion turns the player.
        let yaw0 = one::<Player, With<Player>>(&mut app).yaw;
        app.world_mut().send_event(MouseMotion { delta: Vec2::new(100.0, 50.0) });
        step(&mut app);
        let player = one::<Player, With<Player>>(&mut app);
        assert!((player.yaw - (yaw0 - 0.25)).abs() < 1e-4, "yaw {}", player.yaw);
        assert!((player.pitch - (-0.15 - 0.125)).abs() < 1e-4, "pitch {}", player.pitch);

        // Esc releases.
        key(&mut app, KeyCode::Escape, Key::Escape, ButtonState::Pressed);
        step(&mut app);
        assert_eq!(grab_mode(&mut app), CursorGrabMode::None);
    }

    #[test]
    fn scene_has_fog_and_glow_strips() {
        let mut app = test_app();
        let fog = one::<DistanceFog, With<Camera>>(&mut app);
        assert!(matches!(fog.falloff, FogFalloff::Exponential { .. }));

        let materials = app.world_mut().resource::<Assets<StandardMaterial>>();
        let glowing = materials.iter().filter(|(_, m)| m.emissive != LinearRgba::BLACK).count();
        assert!(glowing >= 1, "at least one emissive strip material");
    }

    #[test]
    fn world_is_meshed_not_placeholder_planes() {
        let mut app = test_app();
        let mut qs = app.world_mut().query::<&ChunkTag>();
        let chunks = qs.iter(app.world()).count();
        assert!(chunks >= 9, "chunk meshes spawned around spawn, got {chunks}");
    }

    #[test]
    fn fire_carves_a_hole_and_delete_resets() {
        let mut app = test_app();
        // Land first so the camera boom is settled.
        for _ in 0..120 {
            step(&mut app);
        }
        assert_eq!(edit_count(&mut app), 0);

        // First click only locks the pointer; it must not fire.
        mouse(&mut app, ButtonState::Pressed);
        step(&mut app);
        mouse(&mut app, ButtonState::Released);
        step(&mut app);
        assert_eq!(grab_mode(&mut app), CursorGrabMode::Locked);
        assert_eq!(edit_count(&mut app), 0, "locking click does not fire");

        // Second click fires down the camera ray and carves the field.
        mouse(&mut app, ButtonState::Pressed);
        step(&mut app);
        mouse(&mut app, ButtonState::Released);
        step(&mut app);
        let edits = edit_count(&mut app);
        assert!(edits > 0, "GBE carved cells, got {edits}");
        let carved = {
            let field = &app.world().resource::<FieldRes>().0;
            // The beam crosses the gallery floor in front of the player:
            // somewhere ahead (-z) and below head height is now carved.
            let mut found = false;
            'outer: for x in 0..40 {
                for z in 0..40 {
                    let cell = buramu_world::CellId([x, 0, z]);
                    if field.edits.get(cell) == Some(1.0) {
                        found = true;
                        break 'outer;
                    }
                }
            }
            found
        };
        assert!(carved, "carve reached the y=0 floor lattice");

        // Delete resets the world.
        key(&mut app, KeyCode::Delete, Key::Delete, ButtonState::Pressed);
        step(&mut app);
        key(&mut app, KeyCode::Delete, Key::Delete, ButtonState::Released);
        assert_eq!(edit_count(&mut app), 0, "delete cleared the edits");
    }

    #[test]
    fn builder_regrows_what_the_gbe_carves() {
        let mut app = test_app();
        let mut drone_q = app.world_mut().query_filtered::<&Transform, With<Builder>>();
        assert_eq!(drone_q.iter(app.world()).count(), 1, "one builder drone");
        let drone_start = *drone_q.single(app.world()).unwrap();

        // Land, lock, and fire (same path as the carve test).
        for _ in 0..120 {
            step(&mut app);
        }
        mouse(&mut app, ButtonState::Pressed);
        step(&mut app);
        mouse(&mut app, ButtonState::Released);
        step(&mut app);
        mouse(&mut app, ButtonState::Pressed);
        step(&mut app);
        mouse(&mut app, ButtonState::Released);
        step(&mut app);
        let carved = edit_count(&mut app);
        assert!(carved > 0, "hole carved before repair");

        // Grace period first, then the builder must close the hole.
        let mut steps = 0;
        while edit_count(&mut app) > 0 && steps < 9000 {
            step(&mut app);
            steps += 1;
        }
        assert_eq!(
            edit_count(&mut app),
            0,
            "builder restored all {carved} cells in {steps} steps"
        );
        let mut drone_q = app.world_mut().query_filtered::<&Transform, With<Builder>>();
        let drone_end = *drone_q.single(app.world()).unwrap();
        assert!(
            drone_end.translation.distance(drone_start.translation) > 0.5,
            "drone flew from {} to {}",
            drone_start.translation,
            drone_end.translation
        );
    }

    #[test]
    fn boss_stands_in_arena_with_grounded_feet() {
        let mut app = test_app();
        for _ in 0..30 {
            step(&mut app);
        }
        let (phase, feet) = {
            let mut boss_q = app.world_mut().query::<&boss::Boss>();
            assert_eq!(boss_q.iter(app.world()).count(), 1, "one boss");
            let boss = boss_q.single(app.world()).unwrap();
            (boss.phase, boss.feet_positions())
        };
        assert_eq!(phase, boss::BossPhase::Dormant);
        // Root must carry Visibility or meshed children warn (B0004).
        let mut vis_q =
            app.world_mut().query_filtered::<&Visibility, With<boss::Boss>>();
        assert_eq!(vis_q.iter(app.world()).count(), 1, "boss root is visible");
        let mut wp_q = app.world_mut().query::<&boss::WeakPoint>();
        assert_eq!(wp_q.iter(app.world()).count(), 3, "three weak points");

        let field = &app.world().resource::<FieldRes>().0;
        for foot in feet {
            let sample = field.sample([foot.x as f64, foot.y as f64, foot.z as f64]);
            assert!(
                sample.abs() < 0.65,
                "foot at {foot:?} rests on the surface, sample {sample}"
            );
        }
    }

    #[test]
    fn destroying_all_weak_points_downs_the_boss() {
        let mut app = test_app();
        for _ in 0..10 {
            step(&mut app);
        }
        // Burn every socket down through the real damage path.
        let mut wp_q = app.world_mut().query::<&mut boss::WeakPoint>();
        for mut wp in wp_q.iter_mut(app.world_mut()) {
            while !wp.destroyed {
                wp.damage();
            }
        }
        for _ in 0..150 {
            step(&mut app);
        }
        let mut boss_q = app.world_mut().query::<&boss::Boss>();
        let boss = boss_q.single(app.world()).unwrap();
        assert_eq!(boss.phase, boss::BossPhase::Defeated);
        let mut tf_q = app
            .world_mut()
            .query_filtered::<&Transform, With<boss::Boss>>();
        let root = tf_q.single(app.world()).unwrap();
        assert!(
            root.rotation.z.abs() > 0.3,
            "colossus tips over, rotation {}",
            root.rotation
        );
    }

    #[test]
    fn gbe_downs_boss_through_real_fire_input() {
        use std::time::Instant;

        let mut app = test_app();
        for _ in 0..10 {
            step(&mut app);
        }

        // Teleport into the arena, 12 m south of the boss.
        {
            let mut q = app.world_mut().query::<(&mut Player, &mut Transform)>();
            let (mut player, mut tf) = q.single_mut(app.world_mut()).unwrap();
            tf.translation = Vec3::new(24.0, 0.4, -12.0);
            player.vel = [0.0, 0.0, 0.0];
        }
        step(&mut app);

        let mut frame_times: Vec<f32> = Vec::new();
        let mut timed_step = |app: &mut App| {
            let start = Instant::now();
            step(app);
            frame_times.push(start.elapsed().as_secs_f32() * 1000.0);
        };

        // Gait: while the boss advances, at least one foot must step,
        // and a player standing at a landing foot gets shoved.
        let feet_before = {
            let mut q = app.world_mut().query::<&boss::Boss>();
            q.single(app.world()).unwrap().feet_positions()
        };
        let mut knocked = false;
        for _ in 0..900 {
            // Stand on a mid-step foot's landing spot.
            let feet = {
                let mut q = app.world_mut().query::<&boss::Boss>();
                q.single(app.world()).unwrap().feet_positions()
            };
            let field_sample = |p: Vec3| {
                app.world()
                    .resource::<FieldRes>()
                    .0
                    .sample([p.x as f64, p.y as f64, p.z as f64])
            };
            if let Some(foot) = feet.iter().find(|f| field_sample(**f) > 0.35) {
                let mut q = app.world_mut().query::<(&mut Player, &mut Transform)>();
                let (mut player, mut tf) = q.single_mut(app.world_mut()).unwrap();
                tf.translation = Vec3::new(foot.x, 0.4, foot.z);
                player.vel = [0.0, 0.0, 0.0];
            }
            timed_step(&mut app);
            let speed = {
                let mut q = app.world_mut().query::<&Player>();
                let p = q.single(app.world()).unwrap();
                (p.vel[0] * p.vel[0] + p.vel[2] * p.vel[2]).sqrt()
            };
            if speed > 1.5 {
                knocked = true;
                break;
            }
        }
        let feet_after = {
            let mut q = app.world_mut().query::<&boss::Boss>();
            q.single(app.world()).unwrap().feet_positions()
        };
        let stepped = feet_before
            .iter()
            .zip(feet_after.iter())
            .any(|(a, b)| a.distance(*b) > 0.3);
        assert!(stepped, "boss gait moved a foot while advancing");
        assert!(knocked, "a landing foot shoved the player");

        // Back off to shooting distance, lock the pointer.
        {
            let mut q = app.world_mut().query::<(&mut Player, &mut Transform)>();
            let (mut player, mut tf) = q.single_mut(app.world_mut()).unwrap();
            tf.translation = Vec3::new(24.0, 0.4, -13.0);
            player.vel = [0.0, 0.0, 0.0];
        }
        mouse(&mut app, ButtonState::Pressed);
        step(&mut app);
        mouse(&mut app, ButtonState::Released);
        step(&mut app);
        assert_eq!(grab_mode(&mut app), CursorGrabMode::Locked);

        let total_hits = |app: &mut App| -> u32 {
            let mut q = app.world_mut().query::<&boss::WeakPoint>();
            q.iter(app.world()).map(|w| (3 - w.hits) as u32).sum()
        };

        // Aim through the real camera path and fire, socket by socket.
        for shot in 0..40 {
            if total_hits(&mut app) >= 9 {
                break;
            }
            // Aim at the first live socket.
            let target = {
                let mut q = app
                    .world_mut()
                    .query::<(&boss::WeakPoint, &GlobalTransform)>();
                let mut found = None;
                for (w, gt) in q.iter(app.world()) {
                    if !w.destroyed {
                        found = Some(gt.translation());
                        break;
                    }
                }
                found.expect("a live weak point")
            };
            {
                let mut q = app.world_mut().query::<(&mut Player, &Transform)>();
                let (mut player, tf) = q.single_mut(app.world_mut()).unwrap();
                let head = tf.translation + Vec3::new(0.0, BODY.height as f32 * 0.5 - 0.2, 0.0);
                let dir = (target - head).normalize();
                player.yaw = (-dir.x).atan2(-dir.z);
                player.pitch = dir.y.asin().clamp(-1.4, 1.4);
            }
            timed_step(&mut app);
            timed_step(&mut app);
            let before = total_hits(&mut app);
            mouse(&mut app, ButtonState::Pressed);
            timed_step(&mut app);
            mouse(&mut app, ButtonState::Released);
            timed_step(&mut app);
            for _ in 0..18 {
                timed_step(&mut app);
            }
            assert!(
                total_hits(&mut app) > before,
                "shot {shot} registered a weak-point hit"
            );
        }
        assert_eq!(total_hits(&mut app), 9, "all sockets burned down");

        // Destroyed sockets went dark.
        let sockets: Vec<(bool, Handle<StandardMaterial>)> = {
            let mut q = app.world_mut().query::<&boss::WeakPoint>();
            q.iter(app.world())
                .map(|wp| (wp.destroyed, wp.material.clone()))
                .collect()
        };
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        for (destroyed, handle) in &sockets {
            assert!(destroyed);
            let m = materials.get(handle).unwrap();
            assert!(
                m.emissive.red < 1.0,
                "socket darkened, emissive {}",
                m.emissive.red
            );
        }

        // The colossus tips over.
        let mut downed = false;
        for _ in 0..300 {
            timed_step(&mut app);
            let mut q = app
                .world_mut()
                .query_filtered::<&Transform, With<boss::Boss>>();
            if q.single(app.world()).unwrap().rotation.z.abs() > 0.3 {
                downed = true;
                break;
            }
        }
        assert!(downed, "boss tipped after losing all sockets");

        // Frame-time stability over the whole encounter.
        frame_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = frame_times.iter().sum::<f32>() / frame_times.len() as f32;
        let p95 = frame_times[frame_times.len() * 95 / 100];
        eprintln!(
            "encounter frames: n={} mean={mean:.2}ms p95={p95:.2}ms max={:.2}ms",
            frame_times.len(),
            frame_times.last().unwrap()
        );
        assert!(mean < 50.0, "mean frame time {mean}ms");
        assert!(p95 < 250.0, "p95 frame time {p95}ms");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_backends_exclude_gl() {
        // Regression: wgpu's GL backend (WSLg / Mesa D3D12-on-GL) fails
        // device creation with an indirect-validation error in Bevy 0.16.
        let backends = backends_for(None);
        assert_eq!(backends, Backends::PRIMARY);
        assert!(!backends.contains(Backends::GL));
        // An explicit WGPU_BACKEND override still wins.
        assert_eq!(backends_for(Some(Backends::GL)), Backends::GL);
    }

    #[test]
    fn spawn_is_in_open_air() {
        let gen = LayeredField::new(Megastructure { seed: 7, stratum_height_cells: 256 });
        let s = find_spawn(&gen);
        // Head room and foot room: corridor band must be empty here.
        assert!(gen.sample([s[0], 1.0, s[2]]) > 0.5, "head room at {s:?}");
        assert!(gen.sample([s[0], 2.0, s[2]]) > 0.0, "upper room at {s:?}");
        // Solid ground below the floor surface.
        assert!(gen.sample([s[0], -1.0, s[2]]) < 0.0, "ground under {s:?}");
    }
}

