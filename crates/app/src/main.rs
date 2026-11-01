use bevy::input::mouse::MouseMotion;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use buramu_gen::Megastructure;
use buramu_world::collision::{MoveParams, move_body};

const GRAVITY: f64 = 20.0;
const WALK_SPEED: f32 = 4.5;
const JUMP_SPEED: f64 = 5.5;
const BODY: MoveParams = MoveParams { radius: 0.35, height: 1.8 };
const CAMERA_BOOM: f32 = 5.5;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(WorldGen(Megastructure { seed: 7, stratum_height_cells: 256 }))
        .add_systems(Startup, setup)
        .add_systems(Update, (grab_pointer, look, walk))
        .run();
}

#[derive(Resource)]
struct WorldGen(Megastructure);

#[derive(Component, Clone, Copy)]
struct Player {
    vel: [f64; 3],
    grounded: bool,
    yaw: f32,
    pitch: f32,
}

/// Feet position in world meters. Scans for an empty corridor spot so the
/// player never spawns inside a noise blob.
fn find_spawn(gen: &Megastructure) -> [f64; 3] {
    for x in 8..56 {
        for z in [8.5, 16.5, 24.5, 32.5] {
            if gen.sample([x as f64 + 0.5, 1.0, z]) > 0.5 {
                return [x as f64 + 0.5, 0.0, z];
            }
        }
    }
    [8.5, 0.0, 8.5]
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    gen: Res<WorldGen>,
) {
    let spawn = find_spawn(&gen.0);

    // Gallery shell placeholders. Real mesher will replace these with the
    // extracted SDF surface; y values match the field zeros (floor -0.5,
    // ceiling 2.5 for the band at y in {0, 1, 2}).
    let concrete = materials.add(StandardMaterial {
        base_color: Color::srgb(0.10, 0.10, 0.12),
        perceptual_roughness: 0.95,
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(240.0, 240.0))),
        MeshMaterial3d(concrete.clone()),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(240.0, 240.0))),
        MeshMaterial3d(concrete.clone()),
        Transform::from_xyz(0.0, 2.5, 0.0).with_rotation(Quat::from_rotation_x(std::f32::consts::PI)),
    ));

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
        MeshMaterial3d(coat),
        Transform::from_translation(Vec3::new(
            spawn[0] as f32,
            (spawn[1] + BODY.height * 0.5) as f32,
            spawn[2] as f32,
        )),
    ));

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

fn walk(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gen: Res<WorldGen>,
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
        app.insert_resource(WorldGen(Megastructure { seed: 7, stratum_height_cells: 256 }));
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.add_systems(Startup, setup);
        // Fixed-step clock: each update advances exactly 1/60 s, so the
        // tests are deterministic and independent of wall time.
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
        app.add_systems(Update, (grab_pointer, look, walk));
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
        let grounded = one::<Player, With<Player>>(&mut app)
            .grounded;
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
            if one::<Player, With<Player>>(&mut app)
                .grounded
            {
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
        app.world_mut().send_event(MouseButtonInput {
            button: MouseButton::Left,
            state: ButtonState::Pressed,
            window: Entity::PLACEHOLDER,
        });
        step(&mut app);
        assert_eq!(grab_mode(&mut app), CursorGrabMode::Locked);

        // Mouse motion turns the player.
        let yaw0 = one::<Player, With<Player>>(&mut app)
            .yaw;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_is_in_open_air() {
        let gen = Megastructure { seed: 7, stratum_height_cells: 256 };
        let s = find_spawn(&gen);
        // Head room and foot room: corridor band must be empty here.
        assert!(gen.sample([s[0], 1.0, s[2]]) > 0.5, "head room at {s:?}");
        assert!(gen.sample([s[0], 2.0, s[2]]) > 0.0, "upper room at {s:?}");
        // Solid ground below the floor surface.
        assert!(gen.sample([s[0], -1.0, s[2]]) < 0.0, "ground under {s:?}");
    }
}
