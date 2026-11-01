# buramu

A BLAME!-inspired Colossus-like: an effectively infinite procedural Megastructure, sporadic unique bosses, destroyable and rebuildable world.

Rust workspace. Bevy shell, engine-agnostic core.

## Layout

- `crates/world` — canonical `i64` lattice, SDF trait, floating origin.
- `crates/gen` — deterministic procedural base signal.
- `crates/app` — Bevy binary (`buramu`).
- `docs/` — design principles, tech decisions, roadmap.

## Run

```sh
cargo test
cargo run -p buramu_app
```

If the shared cargo cache is read-only in your shell, point cargo at a
writable home first (sandboxed shells hit this):

```sh
export CARGO_HOME=/tmp/buramu-cargo-home; mkdir -p $CARGO_HOME
```

Note: `buramu_app` enables Bevy without `bevy_audio` until sound lands,
so no ALSA system lib is required yet.

Read `docs/design-principles.md` first, then `docs/tech-decisions.md`.
