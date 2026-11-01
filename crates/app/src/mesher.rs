//! Naive Surface Nets meshing for one 32^3 chunk.
//!
//! The corner grid is sampled one cell beyond the chunk on every side,
//! so quads along chunk borders find all four neighbouring cells and
//! neighbouring chunks generate coincident vertices instead of holes.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use buramu_world::{CellId, ChunkId, Sdf, CHUNK_SIZE};

const N: i64 = CHUNK_SIZE; // cells per chunk edge (32)
const G: i64 = N + 3; // sampled corners per edge: -1 ..= N+1

fn grid_index(x: i64, y: i64, z: i64) -> usize {
    ((x + 1) * G * G + (y + 1) * G + (z + 1)) as usize
}

/// World-space lattice point of grid corner (x, y, z) in a chunk.
fn corner(chunk: ChunkId, x: i64, y: i64, z: i64) -> [i64; 3] {
    [
        chunk.0[0] * N + x,
        chunk.0[1] * N + y,
        chunk.0[2] * N + z,
    ]
}

fn gradient(field: &dyn Sdf, p: [f64; 3]) -> Vec3 {
    let h = 0.25;
    let dx = field.sample([p[0] + h, p[1], p[2]]) - field.sample([p[0] - h, p[1], p[2]]);
    let dy = field.sample([p[0], p[1] + h, p[2]]) - field.sample([p[0], p[1] - h, p[2]]);
    let dz = field.sample([p[0], p[1], p[2] + h]) - field.sample([p[0], p[1], p[2] - h]);
    let v = Vec3::new(dx, dy, dz);
    if v.length_squared() < 1e-8 {
        Vec3::Y
    } else {
        v.normalize()
    }
}

/// Build the render mesh for `chunk` from `field`. Vertex positions are
/// local to the chunk origin (place the entity at the chunk's world
/// lattice origin). Returns an empty mesh for a featureless chunk.
pub fn mesh_chunk(field: &dyn Sdf, chunk: ChunkId) -> Mesh {
    // Sample the extended corner grid once.
    let mut vals = vec![0.0f32; (G * G * G) as usize];
    for x in -1..=N + 1 {
        for y in -1..=N + 1 {
            for z in -1..=N + 1 {
                let c = corner(chunk, x, y, z);
                vals[grid_index(x, y, z)] = field.distance(CellId(c));
            }
        }
    }
    let at = |x: i64, y: i64, z: i64| vals[grid_index(x, y, z)];

    // One vertex per sign-changing cell, at the average edge crossing.
    // Cells range over -1..=N so border quads can use neighbours.
    let mut vertex_of: std::collections::HashMap<[i64; 3], u32> =
        std::collections::HashMap::new();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();

    let cell_vertex = |cell: [i64; 3],
                       vertex_of: &mut std::collections::HashMap<[i64; 3], u32>,
                       positions: &mut Vec<[f32; 3]>,
                       normals: &mut Vec<[f32; 3]>|
     -> Option<u32> {
        if let Some(v) = vertex_of.get(&cell) {
            return Some(*v);
        }
        let [cx, cy, cz] = cell;
        let mut signs = 0u8;
        for dx in 0..2 {
            for dy in 0..2 {
                for dz in 0..2 {
                    if at(cx + dx, cy + dy, cz + dz) < 0.0 {
                        signs += 1;
                    }
                }
            }
        }
        if signs == 0 || signs == 8 {
            return None;
        }
        // Average the crossings on the cell's 12 edges.
        let mut sum = Vec3::ZERO;
        let mut count = 0.0;
        let edges: [([i64; 3], [i64; 3]); 12] = [
            ([0, 0, 0], [1, 0, 0]),
            ([0, 1, 0], [1, 1, 0]),
            ([0, 0, 1], [1, 0, 1]),
            ([0, 1, 1], [1, 1, 1]),
            ([0, 0, 0], [0, 1, 0]),
            ([1, 0, 0], [1, 1, 0]),
            ([0, 0, 1], [0, 1, 1]),
            ([1, 0, 1], [1, 1, 1]),
            ([0, 0, 0], [0, 0, 1]),
            ([1, 0, 0], [1, 0, 1]),
            ([0, 1, 0], [0, 1, 1]),
            ([1, 1, 0], [1, 1, 1]),
        ];
        for (a, b) in edges {
            let va = at(cx + a[0], cy + a[1], cz + a[2]);
            let vb = at(cx + b[0], cy + b[1], cz + b[2]);
            if (va < 0.0) != (vb < 0.0) {
                let t = va / (va - vb);
                sum += Vec3::new(
                    cx as f32 + a[0] as f32 + (b[0] - a[0]) as f32 * t,
                    cy as f32 + a[1] as f32 + (b[1] - a[1]) as f32 * t,
                    cz as f32 + a[2] as f32 + (b[2] - a[2]) as f32 * t,
                );
                count += 1.0;
            }
        }
        if count == 0.0 {
            return None;
        }
        let local = sum / count;
        let world = [
            (chunk.0[0] * N) as f64 + local.x as f64,
            (chunk.0[1] * N) as f64 + local.y as f64,
            (chunk.0[2] * N) as f64 + local.z as f64,
        ];
        let idx = positions.len() as u32;
        positions.push(local.to_array());
        normals.push(gradient(field, world).to_array());
        vertex_of.insert(cell, idx);
        Some(idx)
    };

    // A quad around every grid edge (with endpoints inside the chunk)
    // whose sign changes, joining the cells that share that edge.
    let mut indices: Vec<u32> = Vec::new();
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for i in 0..N {
            for j in 0..N {
                for k in 0..N {
                    let mut p = [0i64; 3];
                    p[axis] = i;
                    p[u] = j;
                    p[v] = k;
                    let mut q = p;
                    q[axis] += 1;
                    let va = at(p[0], p[1], p[2]);
                    let vb = at(q[0], q[1], q[2]);
                    if (va < 0.0) == (vb < 0.0) {
                        continue;
                    }
                    // The four cells sharing this edge.
                    let mut quad = [0u32; 4];
                    let mut ok = true;
                    for (n, (du, dv)) in
                        [(0, 0), (0, -1), (-1, -1), (-1, 0)].iter().enumerate()
                    {
                        let mut cell = p;
                        cell[u] += du;
                        cell[v] += dv;
                        match cell_vertex(
                            cell,
                            &mut vertex_of,
                            &mut positions,
                            &mut normals,
                        ) {
                            Some(idx) => quad[n] = idx,
                            None => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok {
                        continue;
                    }
                    // Wind so the face points from solid toward empty.
                    if va < 0.0 {
                        indices.extend([quad[0], quad[1], quad[2], quad[0], quad[2], quad[3]]);
                    } else {
                        indices.extend([quad[0], quad[2], quad[1], quad[0], quad[3], quad[2]]);
                    }
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use buramu_gen::Megastructure;
    use buramu_world::{carve_capsule, LayeredField};

    fn vertex_count(mesh: &Mesh) -> usize {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(values) => values.len(),
            None => 0,
        }
    }

    #[test]
    fn spawn_chunk_has_surface() {
        let gen = Megastructure { seed: 7, stratum_height_cells: 256 };
        let mesh = mesh_chunk(&gen, ChunkId([0, 0, 0]));
        assert!(vertex_count(&mesh) > 100, "gallery chunk is meshed");
        assert!(mesh.indices().map(|i| i.len()).unwrap_or(0) > 0);
        let normals = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap();
        assert_eq!(normals.len(), vertex_count(&mesh));
    }

    #[test]
    fn carving_changes_the_mesh() {
        let gen = Megastructure { seed: 7, stratum_height_cells: 256 };
        let before = mesh_chunk(&gen, ChunkId([0, -1, 0]));
        let mut field = LayeredField::new(gen);
        carve_capsule(&mut field.edits, [2.0, -1.0, 8.0], [10.0, -1.0, 8.0], 1.2);
        let after = mesh_chunk(&field, ChunkId([0, -1, 0]));
        assert_ne!(
            vertex_count(&before),
            vertex_count(&after),
            "carved cavity adds surface"
        );
    }
}
