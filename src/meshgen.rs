use std::num::NonZero;
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf, time::Instant};

use glam::{I16Vec3, Vec2, Vec3};
use luanti_core::{ContentId, MapBlockNodes, MapBlockPos, MapNode, MapNodePos};
use luanti_protocol::types::DrawType;
use tokio::sync::mpsc;
use wgpu::util::DeviceExt;

use crate::luanti_client::ClientToMainEvent;
use crate::map::{LuantiMap, MeshgenMapData, NEIGHBOR_DIRS};
use crate::node_def::NodeDefManager;
use crate::texture::Texture;

pub type MediaPathMap = HashMap<String, PathBuf>;

pub type TextureVec = Vec<Texture>;
// contains indices into a TextureVec
pub type TextureMap = HashMap<String, usize>;

pub struct Meshgen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    main_tx: mpsc::UnboundedSender<ClientToMainEvent>,
    pool: rayon::ThreadPool,

    node_def: Arc<NodeDefManager>,
    texture_vec: TextureVec,
    texture_map: Arc<TextureMap>,
}

/// A thread pool for generating mapblock meshes and uploading them to the GPU.
impl Meshgen {
    /// Creates the meshgen, setting up the thread pool.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        main_tx: mpsc::UnboundedSender<ClientToMainEvent>,
        mut node_def: NodeDefManager,
        media_paths: MediaPathMap,
    ) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(0)
            .thread_name(|index| format!("Meshgen #{}", index))
            .build()
            .unwrap();

        let mut texture_vec: TextureVec = Vec::new();
        let mut texture_map: TextureMap = HashMap::new();

        for (_, def) in &mut node_def.map {
            for tile in &mut def.tiledef {
                if tile.name.is_empty() {
                    continue;
                }

                // strip texture modifiers
                let name_simple = tile.name.split('^').next().unwrap();
                tile.name = String::from(name_simple);

                if texture_map.contains_key(&tile.name) {
                    continue;
                }

                let path = media_paths.get(&tile.name);
                let Some(path) = path else {
                    println!("Missing texture {} for node {}", tile.name, def.name);
                    tile.name = String::from("");
                    continue;
                };
                let Ok(texture) = Texture::load(&device, &queue, &tile.name, &path) else {
                    println!("Failed to load texture {} from {:?}", tile.name, path);
                    tile.name = String::from("");
                    continue;
                };
                texture_vec.push(texture);
                texture_map.insert(tile.name.clone(), texture_vec.len() - 1);
            }
        }

        let mut texture_view_vec: Vec<&wgpu::TextureView> = Vec::with_capacity(texture_vec.len());
        for texture in &texture_vec {
            texture_view_vec.push(&texture.view);
        }

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Node texture sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });

        // TODO: check if we are within limits (but we almost definitely are if
        // the bindless features are available)
        let count = NonZero::new(texture_vec.len() as u32).unwrap();

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Node texture bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: Some(count),
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Node texture bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&texture_view_vec),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        main_tx
            .send(ClientToMainEvent::MapblockTextureData(
                MapblockTextureData {
                    bind_group_layout,
                    bind_group,
                },
            ))
            .unwrap();

        println!("Loaded {} textures", texture_vec.len());

        Self {
            device,
            queue,
            main_tx,
            pool,
            node_def: Arc::new(node_def),
            texture_vec,
            texture_map: Arc::new(texture_map),
        }
    }

    /// Submits a mapblock for mesh generation.
    /// The finished MapblockMesh is returned using the UnboundedSender given to Meshgen::new.
    pub fn submit(&self, map: &LuantiMap, blockpos: MapBlockPos, block: &MapBlockNodes) {
        MeshgenTask::spawn(
            self.device.clone(),
            self.main_tx.clone(),
            self.node_def.clone(),
            self.texture_map.clone(),
            &self.pool,
            map,
            blockpos,
            block,
        );
    }
}

/// The representation of a vertex, used by the CPU-side mesh representation,
/// and byte-serializable for uploading to GPU buffers.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    position: Vec3,
    uv: Vec2,
    normal: Vec3,
    texture_index: u32,
}

impl Vertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBS: [wgpu::VertexAttribute; 4] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3, 3 => Uint32];

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
struct Mesh {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

/// A finished mapblock mesh that has been uploaded to the GPU.
pub struct MapblockMesh {
    pub blockpos: MapBlockPos,
    pub num_indices: u32,
    /// None if num_indices == 0
    pub index_buffer: Option<wgpu::Buffer>,
    /// None if num_indices == 0
    pub vertex_buffer: Option<wgpu::Buffer>,
    pub timestamp_task_spawned: Instant,
}

pub struct MapblockTextureData {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

/// A task for generating a single mapblock mesh and uploading it to the GPU.
struct MeshgenTask {
    device: wgpu::Device,
    main_tx: mpsc::UnboundedSender<ClientToMainEvent>,
    node_def: Arc<NodeDefManager>,
    texture_map: Arc<TextureMap>,
    data: MeshgenMapData,
    timestamp_task_spawned: Instant,
}

impl MeshgenTask {
    /// Spawns the meshgen task on the thread pool.
    fn spawn(
        device: wgpu::Device,
        main_tx: mpsc::UnboundedSender<ClientToMainEvent>,
        node_def: Arc<NodeDefManager>,
        texture_map: Arc<TextureMap>,
        pool: &rayon::ThreadPool,
        map: &LuantiMap,
        blockpos: MapBlockPos,
        block: &MapBlockNodes,
    ) {
        let t = Instant::now();

        let mut empty = true;
        for node in &block.0 {
            // Quick check, not exhaustive (other nodes can have DrawType::Airlike as well).
            if node.content_id != ContentId::AIR {
                empty = false;
            }
        }

        // If the mapblock is empty, we can skip cloning 7 mapblocks and spawning
        // the task.
        if empty {
            // println!("Skipped spawning meshgen task for empty {}", blockpos.vec());

            main_tx
                .send(ClientToMainEvent::MapblockMesh(MapblockMesh {
                    blockpos: blockpos,
                    num_indices: 0,
                    index_buffer: None,
                    vertex_buffer: None,
                    timestamp_task_spawned: t,
                }))
                .unwrap();
        } else {
            // println!("Spawning meshgen task for {}", blockpos.vec());

            let data = MeshgenMapData::new(map, blockpos, block);

            pool.install(move || {
                MeshgenTask {
                    device,
                    node_def,
                    texture_map,
                    main_tx,
                    data,
                    timestamp_task_spawned: t,
                }
                .generate();
            });
        }
    }

    /// Generates the mapblock mesh and uploads it to GPU buffers.
    fn generate(&self) {
        // let begin = Instant::now();

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

        if mesh.indices.len() == 0 {
            // This can still happen even though we attempt to skip empty mapblocks
            // earlier: A mapblock may be non-empty, but not render any faces due to
            // culling depending on its neighbors (imagine a fully solid mapblock).
            /*
            println!(
                "Late empty mesh detected for {}",
                self.data.get_blockpos().vec()
            );
            */

            self.main_tx
                .send(ClientToMainEvent::MapblockMesh(MapblockMesh {
                    blockpos: self.data.get_blockpos(),
                    num_indices: 0,
                    index_buffer: None,
                    vertex_buffer: None,
                    timestamp_task_spawned: self.timestamp_task_spawned,
                }))
                .unwrap();
            return;
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

        self.main_tx
            .send(ClientToMainEvent::MapblockMesh(MapblockMesh {
                blockpos: self.data.get_blockpos(),
                num_indices: mesh.indices.len() as u32,
                index_buffer: Some(index_buffer),
                vertex_buffer: Some(vertex_buffer),
                timestamp_task_spawned: self.timestamp_task_spawned,
            }))
            .unwrap();

        // println!("Meshgen took: {}", begin.elapsed().as_millis());
    }
}

// Compare to Luanti, content_mapblock.cpp, setupCuboidVertices
// Note: Face order is expected to match NEIGHBOR_DIRS order,
// and also tiledef order in luanti-protocol
#[cfg_attr(rustfmt, rustfmt_skip)]
const CUBE_VERTICES: &[Vertex] = &[
    // Top
    Vertex { position: Vec3::new(-0.5, 0.5, 0.5), uv: Vec2::new(0.0, 0.0), normal: Vec3::new(0.0, 1.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(0.5, 0.5, 0.5), uv: Vec2::new(1.0, 0.0), normal: Vec3::new(0.0, 1.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(0.5, 0.5, -0.5), uv: Vec2::new(1.0, 1.0), normal: Vec3::new(0.0, 1.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(-0.5, 0.5, -0.5), uv: Vec2::new(0.0, 1.0), normal: Vec3::new(0.0, 1.0, 0.0), texture_index: 0 },
    // Bottom
    Vertex { position: Vec3::new(-0.5, -0.5, -0.5), uv: Vec2::new(0.0, 0.0), normal: Vec3::new(0.0, -1.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(0.5, -0.5, -0.5), uv: Vec2::new(1.0, 0.0), normal: Vec3::new(0.0, -1.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(0.5, -0.5, 0.5), uv: Vec2::new(1.0, 1.0), normal: Vec3::new(0.0, -1.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(-0.5, -0.5, 0.5), uv: Vec2::new(0.0, 1.0), normal: Vec3::new(0.0, -1.0, 0.0), texture_index: 0 },
    // Right
    Vertex { position: Vec3::new(0.5, 0.5, -0.5), uv: Vec2::new(0.0, 0.0), normal: Vec3::new(1.0, 0.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(0.5, 0.5, 0.5), uv: Vec2::new(1.0, 0.0), normal: Vec3::new(1.0, 0.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(0.5, -0.5, 0.5), uv: Vec2::new(1.0, 1.0), normal: Vec3::new(1.0, 0.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(0.5, -0.5, -0.5), uv: Vec2::new(0.0, 1.0), normal: Vec3::new(1.0, 0.0, 0.0), texture_index: 0 },
    // Left
    Vertex { position: Vec3::new(-0.5, 0.5, 0.5), uv: Vec2::new(0.0, 0.0), normal: Vec3::new(-1.0, 0.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(-0.5, 0.5, -0.5), uv: Vec2::new(1.0, 0.0), normal: Vec3::new(-1.0, 0.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(-0.5, -0.5, -0.5), uv: Vec2::new(1.0, 1.0), normal: Vec3::new(-1.0, 0.0, 0.0), texture_index: 0 },
    Vertex { position: Vec3::new(-0.5, -0.5, 0.5), uv: Vec2::new(0.0, 1.0), normal: Vec3::new(-1.0, 0.0, 0.0), texture_index: 0 },
    // Back
    Vertex { position: Vec3::new(0.5, 0.5, 0.5), uv: Vec2::new(0.0, 0.0), normal: Vec3::new(0.0, 0.0, 1.0), texture_index: 0 },
    Vertex { position: Vec3::new(-0.5, 0.5, 0.5), uv: Vec2::new(1.0, 0.0), normal: Vec3::new(0.0, 0.0, 1.0), texture_index: 0 },
    Vertex { position: Vec3::new(-0.5, -0.5, 0.5), uv: Vec2::new(1.0, 1.0), normal: Vec3::new(0.0, 0.0, 1.0), texture_index: 0 },
    Vertex { position: Vec3::new(0.5, -0.5, 0.5), uv: Vec2::new(0.0, 1.0), normal: Vec3::new(0.0, 0.0, 1.0), texture_index: 0 },
    // Front
    Vertex { position: Vec3::new(-0.5, 0.5, -0.5), uv: Vec2::new(0.0, 0.0), normal: Vec3::new(0.0, 0.0, -1.0), texture_index: 0 },
    Vertex { position: Vec3::new(0.5, 0.5, -0.5), uv: Vec2::new(1.0, 0.0), normal: Vec3::new(0.0, 0.0, -1.0), texture_index: 0 },
    Vertex { position: Vec3::new(0.5, -0.5, -0.5), uv: Vec2::new(1.0, 1.0), normal: Vec3::new(0.0, 0.0, -1.0), texture_index: 0 },
    Vertex { position: Vec3::new(-0.5, -0.5, -0.5), uv: Vec2::new(0.0, 1.0), normal: Vec3::new(0.0, 0.0, -1.0), texture_index: 0 },
];

// Compare to Luanti, content_mapblock.cpp, quad_indices
// Note: Winding order is clockwise
const QUAD_INDICES: &[u32] = &[0, 1, 2, 2, 3, 0];

impl MeshgenTask {
    /// Generates the mesh for a single node within the mapblock.
    fn generate_single(&self, mesh: &mut Mesh, pos: I16Vec3, node: MapNode) {
        let def = self.node_def.get_with_fallback(node.content_id);
        if def.drawtype == DrawType::AirLike {
            return;
        }

        for (face_index, dir) in NEIGHBOR_DIRS.iter().enumerate() {
            let n_pos = pos + dir;

            // Faces to non-existent mapblocks are not generated, as we don't know if the
            // node is solid or not. The mesh will be re-generated once the neighboring
            // mapblock arrives.
            if let Some(n_node) = self.data.get_node(MapNodePos(n_pos))
                && let n_def = self.node_def.get_with_fallback(n_node.content_id)
                && n_def.drawtype != DrawType::Normal
            {
                let texture_name = &def.tiledef[face_index].name;
                // TODO: get a proper fallback texture
                let texture_index = *self.texture_map.get(texture_name).unwrap_or(&0) as u32;

                /*
                println!(
                    "Texture id {} for node {} face {}",
                    texture,
                    def.and_then(|def| Some(def.name.as_str()))
                        .unwrap_or("<unknown>"),
                    face_index
                );
                */

                let index_offset = mesh.vertices.len() as u32;
                let vertex_offset =
                    MapNodePos::from(self.data.get_blockpos()).0.as_vec3() + pos.as_vec3();

                let from_vertex = face_index * 4;
                let to_vertex = from_vertex + 4;
                let vertices = CUBE_VERTICES[from_vertex..to_vertex]
                    .iter()
                    .map(|vertex| Vertex {
                        position: vertex_offset + vertex.position,
                        texture_index,
                        ..*vertex
                    });
                mesh.vertices.extend(vertices);

                let indices = QUAD_INDICES.iter().map(|index| index_offset + index);
                mesh.indices.extend(indices);
            }
        }
    }
}
