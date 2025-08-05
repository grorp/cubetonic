use std::{sync::Arc, time::Instant};

use glam::{I16Vec3, Vec3};
use luanti_core::{ContentId, MapBlockPos, MapNode, MapNodePos};
use wgpu::util::DeviceExt;

use crate::map::{MeshgenMapData, NEIGHBOR_DIRS};

/// The representation of a vertex, used by the CPU-side mesh representation,
/// and byte-serializable for uploading to GPU buffers.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    position: Vec3,
    normal: Vec3,
}

impl Vertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBS,
        }
    }
}

/// The CPU-side representation of a mesh. Usually dropped after uploading
/// the data to GPU buffers.
#[derive(Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// A finished mapblock mesh uploaded to the GPU, returned by a MeshgenTask.
pub struct MapblockMesh {
    blockpos: MapBlockPos,
    index_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    timestamp_task_spawned: Instant,
}

/// Generates mapblock meshes and uploads them to the GPU.
pub struct MeshgenTask {
    device: Arc<wgpu::Device>,
    data: MeshgenMapData,
    result_sender: tokio::sync::mpsc::UnboundedSender<MapblockMesh>,
    timestamp_task_spawned: Instant,
}

impl MeshgenTask {
    /// Spawns a blocking meshgen task on the current Tokio runtime.
    /// The finished MapblockMesh is returned using the provided UnboundedSender.
    pub fn spawn(
        device: Arc<wgpu::Device>,
        data: MeshgenMapData,
        result_sender: tokio::sync::mpsc::UnboundedSender<MapblockMesh>,
    ) {
        let t = Instant::now();
        tokio::task::spawn_blocking(move || {
            MeshgenTask {
                device,
                data,
                result_sender,
                timestamp_task_spawned: t,
            }
            .generate();
        });
    }

    /// Generates the mapblock mesh and uploads it to GPU buffers.
    fn generate(&self) {
        let mut mesh = Mesh::default();

        let block = self.data.get_block();
        let mut index: usize = 0;
        for z in 0..MapBlockPos::SIZE as i16 {
            for y in 0..MapBlockPos::SIZE as i16 {
                for x in 0..MapBlockPos::SIZE as i16 {
                    self.generate_single(&mut mesh, I16Vec3::new(x, y, z), block.0[index]);
                    index += 1;
                }
            }
        }

        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        self.result_sender
            .send(MapblockMesh {
                blockpos: self.data.get_blockpos(),
                index_buffer,
                vertex_buffer,
                timestamp_task_spawned: self.timestamp_task_spawned,
            })
            .unwrap();
    }
}

// Compare to Luanti, content_mapblock.cpp, setupCuboidVertices
// Note: Face order is expected to match NEIGHBOR_DIRS order
#[cfg_attr(rustfmt, rustfmt_skip)]
const CUBE_VERTICES: &[Vertex] = &[
    // Top
    Vertex { position: Vec3::new(-0.5, 0.5, 0.5), normal: Vec3::new(0.0, 1.0, 0.0) },
    Vertex { position: Vec3::new(0.5, 0.5, 0.5), normal: Vec3::new(0.0, 1.0, 0.0) },
    Vertex { position: Vec3::new(0.5, 0.5, -0.5), normal: Vec3::new(0.0, 1.0, 0.0) },
    Vertex { position: Vec3::new(-0.5, 0.5, -0.5), normal: Vec3::new(0.0, 1.0, 0.0) },
    // Bottom
    Vertex { position: Vec3::new(-0.5, -0.5, -0.5), normal: Vec3::new(0.0, -1.0, 0.0) },
    Vertex { position: Vec3::new(0.5, -0.5, -0.5), normal: Vec3::new(0.0, -1.0, 0.0) },
    Vertex { position: Vec3::new(0.5, -0.5, 0.5), normal: Vec3::new(0.0, -1.0, 0.0) },
    Vertex { position: Vec3::new(-0.5, -0.5, 0.5), normal: Vec3::new(0.0, -1.0, 0.0) },
    // Right
    Vertex { position: Vec3::new(0.5, 0.5, -0.5), normal: Vec3::new(1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(0.5, 0.5, 0.5), normal: Vec3::new(1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(0.5, -0.5, 0.5), normal: Vec3::new(1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(0.5, -0.5, -0.5), normal: Vec3::new(1.0, 0.0, 0.0) },
    // Left
    Vertex { position: Vec3::new(-0.5, 0.5, 0.5), normal: Vec3::new(-1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(-0.5, 0.5, -0.5), normal: Vec3::new(-1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(-0.5, -0.5, -0.5), normal: Vec3::new(-1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(-0.5, -0.5, 0.5), normal: Vec3::new(-1.0, 0.0, 0.0) },
    // Back
    Vertex { position: Vec3::new(0.5, 0.5, 0.5), normal: Vec3::new(0.0, 0.0, 1.0) },
    Vertex { position: Vec3::new(-0.5, 0.5, 0.5), normal: Vec3::new(0.0, 0.0, 1.0) },
    Vertex { position: Vec3::new(-0.5, -0.5, 0.5), normal: Vec3::new(0.0, 0.0, 1.0) },
    Vertex { position: Vec3::new(0.5, -0.5, 0.5), normal: Vec3::new(0.0, 0.0, 1.0) },
    // Front
    Vertex { position: Vec3::new(-0.5, 0.5, -0.5), normal: Vec3::new(0.0, 0.0, -1.0) },
    Vertex { position: Vec3::new(0.5, 0.5, -0.5), normal: Vec3::new(0.0, 0.0, -1.0) },
    Vertex { position: Vec3::new(0.5, -0.5, -0.5), normal: Vec3::new(0.0, 0.0, -1.0) },
    Vertex { position: Vec3::new(-0.5, -0.5, -0.5), normal: Vec3::new(0.0, 0.0, -1.0) },
];

// Compare to Luanti, content_mapblock.cpp, quad_indices
// Note: Winding order is clockwise
const QUAD_INDICES: &[u32] = &[0, 1, 2, 2, 3, 0];

impl MeshgenTask {
    /// Determines if a node is rendered or not (for now ;).
    fn is_solid(node: MapNode) -> bool {
        node.content_id != ContentId::IGNORE && node.content_id != ContentId::AIR
    }

    /// Generates the mesh for a single node within the mapblock.
    fn generate_single(&self, mesh: &mut Mesh, pos: I16Vec3, node: MapNode) {
        if !Self::is_solid(node) {
            return;
        }

        for (face_index, dir) in NEIGHBOR_DIRS.iter().enumerate() {
            let n_pos = pos + dir;

            // Faces to non-existent mapblocks are not generated, as we don't know if the
            // node is solid or not. The mesh will be re-generated once the neighboring
            // mapblock arrives.
            if let Some(n_node) = self.data.get_node(MapNodePos(n_pos))
                && !Self::is_solid(n_node)
            {
                let index_offset = mesh.vertices.len() as u32;
                let vertex_offset =
                    MapNodePos::from(self.data.get_blockpos()).0.as_vec3() + pos.as_vec3();

                let from_vertex = face_index * 4;
                let to_vertex = from_vertex + 4;
                let vertices = CUBE_VERTICES[from_vertex..to_vertex]
                    .iter()
                    .map(|vertex| Vertex {
                        position: vertex_offset + vertex.position,
                        ..*vertex
                    });
                mesh.vertices.extend(vertices);

                let indices = QUAD_INDICES.iter().map(|index| index_offset + index);
                mesh.indices.extend(indices);
            }
        }
    }
}
