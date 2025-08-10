use std::f32::consts::PI;
use wgpu::util::DeviceExt;

#[derive(Debug)]
pub struct CameraParams {
    pub pos: glam::Vec3,
    pub dir: glam::Vec3,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub fog_color: glam::Vec3,
    pub view_distance: f32,
}

impl CameraParams {
    fn build_view_matrix(&self) -> glam::Mat4 {
        glam::Mat4::look_to_lh(self.pos, self.dir, glam::Vec3::Y)
    }

    fn build_view_proj_matrix(&self) -> glam::Mat4 {
        let view = self.build_view_matrix();
        let proj = glam::Mat4::perspective_lh(
            PI * 0.4,
            self.size.width as f32 / self.size.height as f32,
            0.1,
            self.view_distance,
        );
        proj * view
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view: [f32; 16],
    view_proj: [f32; 16],
    fog_color: [f32; 3],
    view_distance: f32,
}

#[derive(Debug)]
pub struct Camera {
    pub params: CameraParams,
    uniform: CameraUniform,
    uniform_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl Camera {
    pub fn new(device: &wgpu::Device, params: CameraParams) -> Camera {
        let uniform = CameraUniform {
            view: params.build_view_matrix().to_cols_array(),
            view_proj: params.build_view_proj_matrix().to_cols_array(),
            fog_color: params.fog_color.to_array(),
            view_distance: params.view_distance,
        };

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Camera {
            params,
            uniform,
            uniform_buffer,
            bind_group_layout,
            bind_group,
        }
    }

    pub fn update(&mut self, queue: &wgpu::Queue) {
        self.uniform.view = self.params.build_view_matrix().to_cols_array();
        self.uniform.view_proj = self.params.build_view_proj_matrix().to_cols_array();
        self.uniform.fog_color = self.params.fog_color.to_array();
        self.uniform.view_distance = self.params.view_distance;

        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniform]),
        );
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
