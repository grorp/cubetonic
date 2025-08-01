use glam::{I16Vec3, Vec3};
use wgpu::util::DeviceExt;

pub const CHUNK_SIZE: usize = 32;

pub struct Chunk {
    // To be indexed as data[z][y][x]
    pub data: [[[bool; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
}

impl Chunk {
    pub fn check_pos(pos: &I16Vec3) -> bool {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        return
            pos.x >= 0 && pos.x < (CHUNK_SIZE as i16) &&
            pos.y >= 0 && pos.y < (CHUNK_SIZE as i16) &&
            pos.z >= 0 && pos.z < (CHUNK_SIZE as i16);
    }

    pub fn get(&self, pos: &I16Vec3) -> Option<bool> {
        if Self::check_pos(pos) {
            Some(self.data[pos.z as usize][pos.y as usize][pos.x as usize])
        } else {
            None
        }
    }
}

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

#[derive(Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

// compare Luanti, content_mapblock.cpp, setupCuboidVertices
#[cfg_attr(rustfmt, rustfmt_skip)]
const CUBE_VERTICES: &[Vertex] = &[
    // top
    Vertex { position: Vec3::new(-0.5, 0.5, 0.5), normal: Vec3::new(0.0, 1.0, 0.0) },
    Vertex { position: Vec3::new(0.5, 0.5, 0.5), normal: Vec3::new(0.0, 1.0, 0.0) },
    Vertex { position: Vec3::new(0.5, 0.5, -0.5), normal: Vec3::new(0.0, 1.0, 0.0) },
    Vertex { position: Vec3::new(-0.5, 0.5, -0.5), normal: Vec3::new(0.0, 1.0, 0.0) },
    // bottom
    Vertex { position: Vec3::new(-0.5, -0.5, -0.5), normal: Vec3::new(0.0, -1.0, 0.0) },
    Vertex { position: Vec3::new(0.5, -0.5, -0.5), normal: Vec3::new(0.0, -1.0, 0.0) },
    Vertex { position: Vec3::new(0.5, -0.5, 0.5), normal: Vec3::new(0.0, -1.0, 0.0) },
    Vertex { position: Vec3::new(-0.5, -0.5, 0.5), normal: Vec3::new(0.0, -1.0, 0.0) },
    // right
    Vertex { position: Vec3::new(0.5, 0.5, -0.5), normal: Vec3::new(1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(0.5, 0.5, 0.5), normal: Vec3::new(1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(0.5, -0.5, 0.5), normal: Vec3::new(1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(0.5, -0.5, -0.5), normal: Vec3::new(1.0, 0.0, 0.0) },
    // left
    Vertex { position: Vec3::new(-0.5, 0.5, 0.5), normal: Vec3::new(-1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(-0.5, 0.5, -0.5), normal: Vec3::new(-1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(-0.5, -0.5, -0.5), normal: Vec3::new(-1.0, 0.0, 0.0) },
    Vertex { position: Vec3::new(-0.5, -0.5, 0.5), normal: Vec3::new(-1.0, 0.0, 0.0) },
    // back
    Vertex { position: Vec3::new(0.5, 0.5, 0.5), normal: Vec3::new(0.0, 0.0, 1.0) },
    Vertex { position: Vec3::new(-0.5, 0.5, 0.5), normal: Vec3::new(0.0, 0.0, 1.0) },
    Vertex { position: Vec3::new(-0.5, -0.5, 0.5), normal: Vec3::new(0.0, 0.0, 1.0) },
    Vertex { position: Vec3::new(0.5, -0.5, 0.5), normal: Vec3::new(0.0, 0.0, 1.0) },
    // front
    Vertex { position: Vec3::new(-0.5, 0.5, -0.5), normal: Vec3::new(0.0, 0.0, -1.0) },
    Vertex { position: Vec3::new(0.5, 0.5, -0.5), normal: Vec3::new(0.0, 0.0, -1.0) },
    Vertex { position: Vec3::new(0.5, -0.5, -0.5), normal: Vec3::new(0.0, 0.0, -1.0) },
    Vertex { position: Vec3::new(-0.5, -0.5, -0.5), normal: Vec3::new(0.0, 0.0, -1.0) },
];

const CUBE_FACE_DIRS: &[I16Vec3] = &[
    I16Vec3::new(0, 1, 0),
    I16Vec3::new(0, -1, 0),
    I16Vec3::new(1, 0, 0),
    I16Vec3::new(-1, 0, 0),
    I16Vec3::new(0, 0, 1),
    I16Vec3::new(0, 0, -1),
];

// compare Luanti, content_mapblock.cpp, quad_indices
const QUAD_INDICES: &[u32] = &[0, 1, 2, 2, 3, 0];

fn generate_mesh(chunk: &Chunk) -> Mesh {
    let mut mesh = Mesh::default();

    for (z, z_slice) in chunk.data.iter().enumerate() {
        for (y, y_slice) in z_slice.iter().enumerate() {
            for (x, voxel) in y_slice.iter().enumerate() {
                if *voxel {
                    generate_mesh_single(
                        chunk,
                        &mut mesh,
                        I16Vec3::new(x as i16, y as i16, z as i16),
                    );
                }
            }
        }
    }

    mesh
}

fn generate_mesh_single(chunk: &Chunk, mesh: &mut Mesh, pos: I16Vec3) {
    for (face_index, dir) in CUBE_FACE_DIRS.iter().enumerate() {
        let n_pos = pos + dir;
        let n_voxel = chunk.get(&n_pos).unwrap_or(false);
        if !n_voxel {
            let index_offset = mesh.vertices.len() as u32;

            let from_vertex = face_index * 4;
            let to_vertex = from_vertex + 4;
            let vertices = CUBE_VERTICES[from_vertex..to_vertex]
                .iter()
                .map(|vertex| Vertex {
                    position: pos.as_vec3() + vertex.position,
                    ..*vertex
                });
            mesh.vertices.extend(vertices);

            let indices = QUAD_INDICES.iter().map(|index| index_offset + index);
            mesh.indices.extend(indices);
        }
    }
}

pub struct MeshChunk {
    chunk: Chunk,

    pub mesh: Mesh,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
}

impl MeshChunk {
    pub fn new(device: &wgpu::Device, chunk: Chunk) -> MeshChunk {
        let mesh = generate_mesh(&chunk);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        MeshChunk {
            chunk,
            mesh,
            vertex_buffer,
            index_buffer,
        }
    }
}
