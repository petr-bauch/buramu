# Roadmap

## M0 — scaffold (this commit)

Workspace, crates, docs, placeholder Bevy scene. `cargo test` green.

## M1 — walkable stratum (done)

Kinematic capsule vs SDF (`world::collision`), third-person controller,
fog and emissive strips. Placeholder planes stand in for the mesher.

## M2 — GBE hole

Shootable beam. Boolean subtract on field. Remesh one chunk. Save diff (seed + edits).

## M3 — builder regrowth

Background actor adds material back on a timer. Distant simulation stays `i64`-based.

## M4 — first boss

One scripted arena boss. Rigged mesh + weak-point sockets + foot IK via `distance_at`.
