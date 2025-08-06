use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use glam::I16Vec3;
use tokio::sync::mpsc;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use luanti_client::LuantiClientRunner;

use crate::luanti_client::MainToClientEvent;
use crate::meshgen::MapblockMesh;

mod camera;
mod camera_controller;
mod luanti_client;
mod map;
mod meshgen;
mod texture;

struct State {
    window: Arc<Window>,
    device: Arc<wgpu::Device>,
    queue: wgpu::Queue,

    surface: wgpu::Surface<'static>,
    size: winit::dpi::PhysicalSize<u32>,
    surface_format: wgpu::TextureFormat,

    render_pipeline: wgpu::RenderPipeline,
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,

    camera: camera::Camera,
    camera_controller: camera_controller::CameraController,

    last_frame: Instant,
    last_send: Instant,

    rt: Arc<tokio::runtime::Runtime>,
    client_tx: mpsc::UnboundedSender<MainToClientEvent>,
    meshgen_rx: mpsc::UnboundedReceiver<MapblockMesh>,

    remesh_counter: HashMap<I16Vec3, usize>,
    mapblock_meshes: HashMap<I16Vec3, MapblockMesh>,
}

impl State {
    async fn new(window: Arc<Window>, rt: Arc<tokio::runtime::Runtime>) -> State {
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

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let device = Arc::new(device);

        let size = window.inner_size();
        let cap = surface.get_capabilities(&adapter);
        let surface_format = cap.formats[0];

        let camera = camera::Camera::new(
            &device,
            camera::CameraParams {
                pos: (0.0, 0.0, -5.0).into(),
                dir: (0.0, 0.0, 1.0).into(),
                size,
            },
        );
        let camera_controller = camera_controller::CameraController::new();

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&camera.bind_group_layout()],
                push_constant_ranges: &[],
            });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&render_pipeline_layout),
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
                format: texture::DEPTH_FORMAT,
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
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let (depth_texture, depth_texture_view) = texture::create_depth_texture(&device, size);

        let (client_tx, client_rx) = mpsc::unbounded_channel();
        let (meshgen_tx, meshgen_rx) = mpsc::unbounded_channel();
        LuantiClientRunner::spawn(device.clone(), client_rx, meshgen_tx).await;

        let state = State {
            window,
            device,
            queue,

            surface,
            size,
            surface_format,

            render_pipeline,
            depth_texture,
            depth_texture_view,

            camera,
            camera_controller,

            last_frame: Instant::now(),
            last_send: Instant::now(),

            rt,
            client_tx,
            meshgen_rx,

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
                view_formats: vec![],
                width: self.size.width,
                height: self.size.height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                desired_maximum_frame_latency: 2,
            },
        );
        println!(
            "Surface configured: size {:?}, format {:?}",
            self.size, self.surface_format
        );
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        self.configure_surface();

        (self.depth_texture, self.depth_texture_view) =
            texture::create_depth_texture(&self.device, new_size);

        self.camera.params.size = new_size;
        // camera update will happen before rendering either way
    }

    fn render(&mut self) {
        let now = Instant::now();
        let dtime = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;

        let send_dtime = (now - self.last_send).as_secs_f32();
        if send_dtime >= 0.1 {
            self.client_tx
                .send(MainToClientEvent::PlayerPos {
                    pos: self.camera.params.pos,
                    yaw: self.camera_controller.yaw,
                    pitch: self.camera_controller.pitch,
                })
                .unwrap();
            self.last_send = now;
        }

        self.camera_controller
            .update_camera(&mut self.camera.params, dtime);
        self.camera.update(&self.queue);

        let output = self.surface.get_current_texture().unwrap();

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_texture_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..wgpu::RenderPassDescriptor::default()
        });

        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, self.camera.bind_group(), &[]);

        let mut num: u32 = 0;
        let mut num_empty: u32 = 0;

        for (_, mesh) in self.mapblock_meshes.iter() {
            if mesh.num_indices == 0 {
                num_empty += 1;
                continue;
            }
            pass.set_index_buffer(
                mesh.index_buffer.as_ref().unwrap().slice(..),
                wgpu::IndexFormat::Uint32,
            );
            pass.set_vertex_buffer(0, mesh.vertex_buffer.as_ref().unwrap().slice(..));
            pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
            num += 1;
        }

        println!("dtime: {:.4}; Meshes: {} + {} empty", dtime, num, num_empty);

        drop(pass);

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        output.present();
    }
}

#[derive(Default)]
struct App {
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attr = Window::default_attributes().with_title("Cubetonic");
        let window = Arc::new(event_loop.create_window(attr).unwrap());

        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
        );

        let state = rt.block_on(State::new(window.clone(), rt.clone()));
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

        while let Ok(mesh) = state.meshgen_rx.try_recv() {
            let counter = state.remesh_counter.entry(mesh.blockpos.vec()).or_insert(0);
            *counter += 1;

            let prev_mesh = state.mapblock_meshes.get_mut(&mesh.blockpos.vec());

            if let Some(prev_mesh) = prev_mesh {
                // A meshgen task for the same mapblock might have started
                // later, but finished earlier than this one.
                // Don't replace the new data with our outdated data in that case.
                if mesh.timestamp_task_spawned > prev_mesh.timestamp_task_spawned {
                    println!(
                        "Received mapblock mesh for {} [UPDATED] [#{}]",
                        mesh.blockpos.vec(),
                        counter,
                    );
                    *prev_mesh = mesh;
                } else {
                    println!(
                        "Received mapblock mesh for {} [UPDATED, OBSOLETE] [#{}]",
                        mesh.blockpos.vec(),
                        counter,
                    );
                }
            } else {
                println!(
                    "Received mapblock mesh for {} [NEW] [#{}]",
                    mesh.blockpos.vec(),
                    counter
                );
                state.mapblock_meshes.insert(mesh.blockpos.vec(), mesh);
            }
        }
    }
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
