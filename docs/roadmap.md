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

## M4 — first boss (done)

One scripted arena boss. Rigged mesh + weak-point sockets + foot IK via `distance_at`.

A colossus waits in a scripted arena chamber north of spawn (carved
by `buramu_gen`). Articulated primitive build in `crates/app/src/boss.rs`
until a rigged glTF exists: four feet planted/grounded through the SDF,
diagonal gait, stomp knockback, three emissive weak-point sockets that
take GBE hits (3 each). Destroying all sockets downs the boss. The
builder keeps repairing the arena while you fight.

## M5 — scale: chunk streaming + floating origin (done)

Stream chunks around the player (boss arena pinned) with a per-frame
meshing budget; wire `buramu_world::Origin` into the app so logic stays
`f64` logical while rendering is origin-local. De-risk first: a 100 km
teleport test and mesher throughput numbers. Async meshing only if the
budgeted synchronous path cannot hold frame time.

Done as planned: budgeted synchronous streaming (8 chunks/frame)
holds frame time in the headless encounter recording, so async
meshing stays deferred. The teleport test also caught the streaming
centre needing the body chunk, not the feet chunk (feet at y=-0.5
sit one chunk below the gallery ceiling).

## M6 — procedural grammar v2 + stratification

Replace gallery-bands-and-noise with a deterministic region grammar
(decks, slab fields, shaft wells with landings, tunnels, chambers) on
the same lattice SDF. Make strata real: per-stratum type/parameters
from `hash(seed, stratum)`, shafts connecting levels. Connectivity and
determinism tests; the boss arena must stay reachable.

## M7 — visual identity (the manga look)

Mesher vertex colours (per-face tint + cavity darkening so carved
holes read), then world-space UVs with a procedurally generated
concrete panel atlas, then motif dressing (emissive strips, glyph
panels, cable runs). No custom shaders until this stack is judged.

## M8 — collision & traversal hardening

Tests first against M6's thinner features: tunnelling at fall speed,
chunk-seam sprinting, step-up, head bumps, boss feet on edges. Fix
generically with velocity-aware substepping in `world::collision`.
Camera boom collision (march the field, shorten the boom) lands here.
