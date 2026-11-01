use bevy::prelude::*;
use buramu_gen::Megastructure;
use buramu_world::{CellId, Sdf};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, probe_world)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Placeholder ground plane: real smooth-voxel mesher plugs in here.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.12, 0.12, 0.14))),
    ));
    commands.spawn((
        DirectionalLight { shadows_enabled: true, ..default() },
        Transform::from_xyz(10.0, 20.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 6.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(WorldGen(Megastructure { seed: 7, stratum_height_cells: 256 }));
}

#[derive(Resource)]
struct WorldGen(Megastructure);

/// Temporary probe: log the SDF at a few cells so `cargo run`
/// proves the crates are wired before the mesher exists.
fn probe_world(gen: Res<WorldGen>, time: Res<Time>) {
    if (time.elapsed_secs() as u64) % 5 == 0 && time.elapsed_secs() < 0.1 {
        for x in [0, 1, 64] {
            let d = gen.0.distance(CellId([x, 0, 0]));
            info!("sdf({x},0,0) = {d}");
        }
    }
}
