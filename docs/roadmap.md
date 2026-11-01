# Roadmap

## M0 — scaffold (this commit)

Workspace, crates, docs, placeholder Bevy scene. `cargo test` green.

## M1 — walkable stratum

Floor collider from SDF query. Third-person capsule. Fog and emissive strips for mood.

## M2 — GBE hole

Shootable beam. Boolean subtract on field. Remesh one chunk. Save diff (seed + edits).

## M3 — builder regrowth

Background actor adds material back on a timer. Distant simulation stays `i64`-based.

## M4 — first boss

One scripted arena boss. Rigged mesh + weak-point sockets + foot IK via `distance_at`.
