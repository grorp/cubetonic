use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use crate::example_chunk::EXAMPLE_CHUNK;

mod camera;
mod camera_controller;
mod example_chunk;
mod texture;
mod voxels;

struct State {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,

    surface: wgpu::Surface<'static>,
    size: winit::dpi::PhysicalSize<u32>,
    surface_format: wgpu::TextureFormat,

    render_pipeline: wgpu::RenderPipeline,
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,
    mesh_chunk: voxels::MeshChunk,

    camera: camera::Camera,
    camera_controller: camera_controller::CameraController,

    last_frame: Instant,
}

impl State {
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

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();

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
                buffers: &[voxels::Vertex::layout()],
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

        let mesh_chunk = voxels::MeshChunk::new(&device, EXAMPLE_CHUNK);

        let (depth_texture, depth_texture_view) = texture::create_depth_texture(&device, size);

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
            mesh_chunk,

            camera,
            camera_controller,

            last_frame: Instant::now(),
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
        pass.set_index_buffer(
            self.mesh_chunk.index_buffer.slice(..),
            wgpu::IndexFormat::Uint32,
        );
        pass.set_vertex_buffer(0, self.mesh_chunk.vertex_buffer.slice(..));
        pass.draw_indexed(0..(self.mesh_chunk.mesh.indices.len() as u32), 0, 0..1);

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

        let state = pollster::block_on(State::new(window.clone()));
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
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
