use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use glam::{I16Vec3, Vec3};
use tokio::sync::mpsc;
use wgpu::{FeaturesWGPU, FeaturesWebGPU, SurfaceError};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use luanti_client::LuantiClientRunner;

use crate::luanti_client::{ClientToMainEvent, MainToClientEvent};
use crate::media::NodeTextureData;
use crate::meshgen::MapblockMesh;
use crate::texture::MyTexture;

mod camera;
mod camera_controller;
mod luanti_client;
mod map;
mod media;
mod meshgen;
mod node_def;
mod texture;

struct State {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,

    surface: wgpu::Surface<'static>,
    size: winit::dpi::PhysicalSize<u32>,
    surface_format: wgpu::TextureFormat,

    depth_texture: MyTexture,

    camera: camera::Camera,
    camera_controller: camera_controller::CameraController,

    last_frame: Instant,
    last_send: Instant,

    client_tx: mpsc::UnboundedSender<MainToClientEvent>,
    client_rx: mpsc::UnboundedReceiver<ClientToMainEvent>,

    mapblock_texture_data: Option<NodeTextureData>,
    render_pipeline: Option<wgpu::RenderPipeline>,

    remesh_counter_total: u32,
    remesh_counter: HashMap<I16Vec3, u32>,
    mapblock_meshes: HashMap<I16Vec3, MapblockMesh>,
}

impl State {
    const BG_COLOR: Vec3 = Vec3::new(0.262250658, 0.491020850, 0.955973353);
    const VIEW_DISTANCE: f32 = 200.0;

    async fn new(window: Arc<Window>) -> State {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..wgpu::RequestAdapterOptions::default()
            })
            .await
            .unwrap();

        let avail_features = adapter.features().features_wgpu;
        let avail_limits = adapter.limits();

        let bindless_features = FeaturesWGPU::TEXTURE_BINDING_ARRAY
            | FeaturesWGPU::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING;
        if !avail_features.contains(bindless_features) {
            panic!(
                "Missing wgpu features for bindless textures: {:?}",
                bindless_features.difference(avail_features)
            );
        }

        let mut limits = wgpu::Limits::defaults();
        let the_limit = avail_limits.max_binding_array_elements_per_shader_stage;
        limits.max_binding_array_elements_per_shader_stage = the_limit;
        println!(
            "max_binding_array_elements_per_shader_stage = {}",
            the_limit
        );

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features {
                    features_wgpu: bindless_features,
                    features_webgpu: FeaturesWebGPU::empty(),
                },
                required_limits: limits,
                ..wgpu::DeviceDescriptor::default()
            })
            .await
            .unwrap();

        let size = window.inner_size();
        let cap = surface.get_capabilities(&adapter);
        let surface_format = cap.formats[0];

        let camera = camera::Camera::new(
            &device,
            camera::CameraParams {
                // These will be overwritten by the CameraController anyway
                pos: Vec3::ZERO,
                dir: Vec3::ZERO,
                size,
                fog_color: Self::BG_COLOR,
                view_distance: Self::VIEW_DISTANCE,
            },
        );
        let camera_controller = camera_controller::CameraController::new();

        let depth_texture = MyTexture::new_depth(&device, size);

        let (client_tx, main_rx) = mpsc::unbounded_channel();
        let (main_tx, client_rx) = mpsc::unbounded_channel();
        LuantiClientRunner::spawn(device.clone(), queue.clone(), main_tx, main_rx).await;

        let state = State {
            window,
            device,
            queue,

            surface,
            size,
            surface_format,

            depth_texture,

            camera,
            camera_controller,

            last_frame: Instant::now(),
            last_send: Instant::now(),

            client_tx,
            client_rx,

            mapblock_texture_data: None,
            render_pipeline: None,

            remesh_counter_total: 0,
            remesh_counter: HashMap::new(),
            mapblock_meshes: HashMap::new(),
        };
        state.configure_surface();
        state
    }

    fn configure_surface(&self) {
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.surface_format,
                view_formats: vec![self.surface_format.add_srgb_suffix()],
                width: self.size.width,
                height: self.size.height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                desired_maximum_frame_latency: 2,
            },
        );
        println!(
            "Surface configured, size: {:?}, format: {:?}",
            self.size, self.surface_format
        );
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        self.configure_surface();

        self.depth_texture = MyTexture::new_depth(&self.device, new_size);

        self.camera.params.size = new_size;
        // camera update will happen before rendering either way
    }

    fn render(&mut self) {
        let now = Instant::now();
        let dtime = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;

        let send_dtime = (now - self.last_send).as_secs_f32();
        if send_dtime >= 0.1 {
            let pos = self.camera_controller.get_pos();
            self.client_tx
                .send(MainToClientEvent::PlayerPos(pos.clone()))
                .unwrap();
            self.last_send = now;
        }

        self.camera_controller.step(dtime, &mut self.camera.params);
        self.camera.update(&self.queue);

        let mut output = self.surface.get_current_texture();
        // Fixes a crash when pressing F11 (toggle fullscreen) on one of my systems with Wayland
        // TODO: this shouldn't be necessary, winit bug?
        if let Err(err) = &output
            && *err == SurfaceError::Outdated
        {
            self.resize(self.window.inner_size());
            output = self.surface.get_current_texture();
        }
        let output = output.unwrap();

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Surface texture view"),
            format: Some(self.surface_format.add_srgb_suffix()),
            ..wgpu::TextureViewDescriptor::default()
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: Self::BG_COLOR.x as f64,
                        g: Self::BG_COLOR.y as f64,
                        b: Self::BG_COLOR.z as f64,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_texture.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..wgpu::RenderPassDescriptor::default()
        });

        if self.render_pipeline.is_some() {
            let render_pipeline = self.render_pipeline.as_ref().unwrap();
            let mapblock_texture_data = self.mapblock_texture_data.as_ref().unwrap();

            pass.set_pipeline(render_pipeline);
            pass.set_bind_group(0, self.camera.bind_group(), &[]);
            pass.set_bind_group(1, &mapblock_texture_data.bind_group, &[]);

            let mut drawlist = Vec::new();

            let camera_pos = self.camera.params.pos;

            for (_, mesh) in &self.mapblock_meshes {
                if mesh.num_indices == 0 {
                    continue;
                }

                let sphere = mesh.bounding_sphere.as_ref().unwrap();
                let distance_sq = camera_pos.distance_squared(sphere.center);
                let max_distance = Self::VIEW_DISTANCE + sphere.radius;
                if distance_sq > max_distance * max_distance {
                    continue;
                }

                drawlist.push(mesh);
            }

            for mesh in drawlist {
                let index_buffer = mesh.index_buffer.as_ref().unwrap();
                let vertex_buffer = mesh.vertex_buffer.as_ref().unwrap();

                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
            }
        }

        drop(pass);

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        output.present();
    }

    fn setup_mapblock_rendering(&mut self, data: NodeTextureData) {
        assert!(self.mapblock_texture_data.is_none());
        assert!(self.render_pipeline.is_none());

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Mapblock pipeline layout"),
                bind_group_layouts: &[&self.camera.bind_group_layout(), &data.bind_group_layout],
                push_constant_ranges: &[],
            });

        let shader = self
            .device
            .create_shader_module(wgpu::include_wgsl!("mapblock_shader.wgsl"));

        let render_pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Mapblock render pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[meshgen::Vertex::layout()],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    // Irrlicht's fault
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: Some(wgpu::Face::Back),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    ..wgpu::PrimitiveState::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: MyTexture::DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            });

        self.mapblock_texture_data = Some(data);
        self.render_pipeline = Some(render_pipeline);
    }

    fn insert_mapblock_mesh(&mut self, mesh: MapblockMesh) {
        assert!(self.mapblock_texture_data.is_some());
        assert!(self.render_pipeline.is_some());

        self.remesh_counter_total += 1;

        let counter = self.remesh_counter.entry(mesh.blockpos.vec()).or_insert(0);
        *counter += 1;

        let prev_mesh = self.mapblock_meshes.get_mut(&mesh.blockpos.vec());

        if let Some(prev_mesh) = prev_mesh {
            // A meshgen task for the same mapblock might have started
            // later, but finished earlier than this one.
            // Don't replace the new data with our outdated data in that case.
            if mesh.timestamp_task_spawned > prev_mesh.timestamp_task_spawned {
                /*
                println!(
                    "Received mapblock mesh for {} [UPDATED] [#{}]",
                    mesh.blockpos.vec(),
                    counter,
                );
                */
                *prev_mesh = mesh;
            }
            /* else {
                println!(
                    "Received mapblock mesh for {} [UPDATED, OBSOLETE] [#{}]",
                    mesh.blockpos.vec(),
                    counter,
                );
            }
            */
        } else {
            /*
            println!(
                "Received mapblock mesh for {} [NEW] [#{}]",
                mesh.blockpos.vec(),
                counter
            );
            */
            self.mapblock_meshes.insert(mesh.blockpos.vec(), mesh);
        }
    }
}

struct App {
    rt: tokio::runtime::Runtime,
    state: Option<State>,
}

impl App {
    fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        App { rt, state: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attr = Window::default_attributes().with_title("Cubetonic");
        let window = Arc::new(event_loop.create_window(attr).unwrap());

        let state = self.rt.block_on(State::new(window.clone()));
        self.state = Some(state);

        window.set_cursor_visible(false);
        if let Err(err) = window.set_cursor_grab(CursorGrabMode::Locked) {
            println!("Could not lock cursor: {:?}", err);
        }

        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let state = self.state.as_mut().unwrap();

        if state.camera_controller.process_window_event(&event) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                state.render();
                state.window.request_redraw();
            }
            WindowEvent::Resized(new_size) => {
                state.resize(new_size);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: key_state,
                        physical_key: PhysicalKey::Code(keycode),
                        ..
                    },
                ..
            } => match keycode {
                KeyCode::Escape => event_loop.exit(),
                KeyCode::F11 => {
                    if key_state == ElementState::Pressed {
                        state
                            .window
                            .set_fullscreen(if state.window.fullscreen().is_none() {
                                Some(Fullscreen::Borderless(None))
                            } else {
                                None
                            })
                    }
                }
                _ => (),
            },

            _ => (),
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let state = self.state.as_mut().unwrap();

        state.camera_controller.process_device_event(&event);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let state = self.state.as_mut().unwrap();

        while let Ok(event) = state.client_rx.try_recv() {
            match event {
                ClientToMainEvent::PlayerPos(pos) => state.camera_controller.set_pos(pos),
                ClientToMainEvent::MapblockTextureData(data) => {
                    state.setup_mapblock_rendering(data)
                }
                ClientToMainEvent::MapblockMesh(mesh) => state.insert_mapblock_mesh(mesh),
            }
        }
    }
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
