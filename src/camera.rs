use wgpu::util::DeviceExt;

#[derive(Debug)]
pub struct CameraParams {
    pub pos: glam::Vec3,
    pub dir: glam::Vec3,
    pub fov_y: f32,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub fog_color: glam::Vec3,
    pub z_near: f32,
    pub z_far: f32,
}

impl CameraParams {
    pub const WORLD_UP: glam::Vec3 = glam::Vec3::Y;
    pub const WORLD_FORWARD: glam::Vec3 = glam::Vec3::Z;

    fn build_view_matrix(&self) -> glam::Mat4 {
        // TODO: proper up vector
        glam::Mat4::look_to_lh(self.pos, self.dir, Self::WORLD_UP)
    }

    fn build_proj_matrix(&self) -> glam::Mat4 {
        glam::Mat4::perspective_lh(
            self.fov_y,
            self.size.width as f32 / self.size.height as f32,
            self.z_near,
            self.z_far,
        )
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view: [f32; 16],
    view_proj: [f32; 16],
    fog_color: [f32; 3],
    z_far: f32,
}

impl CameraUniform {
    fn from_params(params: &CameraParams) -> Self {
        let view = params.build_view_matrix();
        let proj = params.build_proj_matrix();
        CameraUniform {
            view: view.to_cols_array(),
            view_proj: (proj * view).to_cols_array(),
            fog_color: params.fog_color.to_array(),
            z_far: params.z_far,
        }
    }
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
        let uniform = CameraUniform::from_params(&params);
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
        self.uniform = CameraUniform::from_params(&self.params);
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
