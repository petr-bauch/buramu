# Tech decisions

## Language: Rust

Bevy for gameplay iteration. Engine-agnostic core crates so the renderer can be replaced later.

- `crates/world`: canonical storage. `i64` lattice truth, SDF trait, floating origin.
- `crates/gen`: deterministic procedural base signal. Pure functions of coordinates.
- `crates/app`: Bevy shell. Wiring, camera, placeholder scene.

## Coordinates

- Truth: `i64` cell / chunk ids. Used for saves, seeds, edits.
- Logic: `f64` on CPU for large ranges.
- Render and physics: `f32` local to origin or chunk. Physics engines and GPUs expect `f32`.
- Boundary function `Origin::to_local` converts. Rebase when the viewer drifts past threshold.

## Destruction without bricks

No Minecraft blocks. Options in order: smooth voxels (Dual Contouring / Surface Nets with Hermite data), SDF boolean subtract for GBE, sparse storage (brickmap / DAG) when memory matters.

GBE = subtract capsule along beam from field. Builders = add material back. Characters query `distance_at`.

## Meshing and streaming (planned)

Chunks of 32^3 cells. Async meshing. Pin boss arenas resident. Coarse colliders regenerated on edit, never per-voxel colliders.
