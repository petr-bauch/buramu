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

## M3 — builder regrowth

Background actor adds material back on a timer. Distant simulation stays `i64`-based.

## M4 — first boss

One scripted arena boss. Rigged mesh + weak-point sockets + foot IK via `distance_at`.
