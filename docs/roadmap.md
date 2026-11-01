# Roadmap

## M0 — scaffold (this commit)

Workspace, crates, docs, placeholder Bevy scene. `cargo test` green.

## M1 — walkable stratum (done)

Kinematic capsule vs SDF (`world::collision`), third-person controller,
fog and emissive strips. Placeholder planes stand in for the mesher.

## M2 — GBE hole (done)

Shootable beam. Boolean subtract on field. Remesh one chunk. Save diff (seed + edits).

LMB (pointer locked) fires a capsule carve through the edited field
(`buramu_world::carve_capsule` over a sparse `EditStore` layered on the
generator). Surface Nets chunk meshes around spawn replace the M1
placeholder planes; touched chunks remesh in place. Edits autosave to
`buramu-save.json` and reload on startup; Delete resets the world.

## M3 — builder regrowth (done)

Background actor adds material back on a timer. Distant simulation stays `i64`-based.

A hovering builder drone reverts GBE damage rim-first via
`buramu_world::regrow` (pure lattice logic, plus pruning of redundant
overrides), remeshing touched chunks and autosaving on a throttle.
Fresh damage gets a 5 s grace period so the player can out-carve it.

## M4 — first boss

One scripted arena boss. Rigged mesh + weak-point sockets + foot IK via `distance_at`.
