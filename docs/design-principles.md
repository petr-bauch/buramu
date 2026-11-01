# Design principles

Inspired by BLAME! and Shadow of the Colossus.

## Pillars

1. Vast empty Megastructure. Traversal is the game. Silence, scale, fog.
2. Sporadic unique bosses. Few fights, each memorable. Scripted first, procedural only where contact happens (feet, grip, weak points).
3. Destroyable and rebuildable world. Gravitational beam emitter carves. Builders regrow. World is never static.
4. Brutalist proceduralism. Concrete, shafts, slabs, galleries. Repetition is a feature. It hides procedural limits and fits BLAME!.

## Characters

Mesh-based rigged entities (`glTF`), not voxels. Killy, Cibo, Safeguard, bosses, builders are standard skinned meshes with textures. They live in the same scene as the meshed voxel world and query it for collision (`distance_at`, `is_solid`).

Builders write to the voxel store. Their bodies are still meshes.

## What we do not build yet

No quests, no crowds, no multiplayer. One stratum, one boss arena, one GBE hole is the first win.
