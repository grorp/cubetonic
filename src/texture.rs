use std::path::Path;

use image::{GenericImageView, ImageReader};
use wgpu::util::DeviceExt;

pub struct MyTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl MyTexture {
    pub fn from_bytes(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        name: &str,
        bytes: &[u8],
    ) -> anyhow::Result<Self> {
        let img = image::load_from_memory(bytes)?;
        Self::from_image(device, queue, name, &img)
    }

    pub fn from_path(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        name: &str,
        path: &Path,
    ) -> anyhow::Result<Self> {
        let img = ImageReader::open(path)?.with_guessed_format()?.decode()?;
        Self::from_image(device, queue, name, &img)
    }

    pub fn from_image(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        name: &str,
        img: &image::DynamicImage,
    ) -> anyhow::Result<Self> {
        let dimensions = img.dimensions();

        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some(name),
                size: wgpu::Extent3d {
                    width: dimensions.0,
                    height: dimensions.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &img.to_rgba8(),
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(name),
            ..wgpu::TextureViewDescriptor::default()
        });

        Ok(Self { texture, view })
    }

    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn new_depth(device: &wgpu::Device, size: winit::dpi::PhysicalSize<u32>) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("depth texture view"),
            ..wgpu::TextureViewDescriptor::default()
        });

        Self { texture, view }
    }
}
