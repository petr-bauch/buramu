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

Controls: click to lock the pointer, WASD + mouse to move, Space to
jump, LMB fires the GBE (carves the world), Delete resets all edits,
Esc releases the pointer. Carved edits autosave to `buramu-save.json`
in the working directory and reload on the next start.

If the shared cargo cache is read-only in your shell, point cargo at a
writable home first (sandboxed shells hit this):

```sh
export CARGO_HOME=/tmp/buramu-cargo-home; mkdir -p $CARGO_HOME
```

The renderer uses wgpu's primary backends only (Vulkan / DX12 / Metal),
never OpenGL — the GL backend loses the device at startup on Mesa / WSLg.
On Linux install Vulkan drivers (e.g. `mesa-vulkan-drivers` + `vulkan-tools`,
check with `vulkaninfo`); in WSL2 prefer running the Windows build natively.
`WGPU_BACKEND=vulkan|dx12|metal|gl` overrides the backend for debugging.

Note: `buramu_app` enables Bevy without `bevy_audio` until sound lands,
so no ALSA system lib is required yet.

Read `docs/design-principles.md` first, then `docs/tech-decisions.md`.
