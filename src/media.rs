use std::{collections::HashMap, num::NonZero, path::PathBuf};

use base64::{Engine as _, engine::DecodePaddingMode};

use crate::texture::MyTexture;

pub enum MediaSource {
    Path(PathBuf),
    Bytes(&'static [u8]),
}

pub struct MediaManager {
    base64: base64::engine::GeneralPurpose,
    cache_dir: PathBuf,
    /// File name -> path or bytes
    map: HashMap<String, MediaSource>,
}

/// A media manager. Media is identified by file name. To use a file, it must be
/// "added" to the media manager first. Then it can be "gotten" by file name.
impl MediaManager {
    /// A fallback texture that is guaranteed to always be available.
    pub const FALLBACK_TEXTURE: &str = "no_texture.png";

    pub fn new() -> Self {
        let base64 = base64::engine::GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            base64::engine::GeneralPurposeConfig::new()
                // Luanti encodes without padding (currently)
                .with_decode_padding_mode(DecodePaddingMode::Indifferent),
        );

        let mut cache_dir = std::env::home_dir().unwrap();
        cache_dir.push(".minetest/cache/media");

        let mut map = HashMap::new();
        map.insert(
            String::from(Self::FALLBACK_TEXTURE),
            MediaSource::Bytes(include_bytes!("no_texture.png")),
        );

        Self {
            base64,
            cache_dir,
            map,
        }
    }

    /// Tries to find a file with the given sha1 in the existing Luanti media
    /// cache, and adds it to the media manager as `name`.
    /// Returns Ok(true) on success.
    /// Returns Ok(false) if there is no such file in the cache.
    /// Returns Err(err) for unexpected errors (bad base64, IO error).
    pub fn try_add_from_cache(&mut self, name: &str, sha1_base64: &str) -> anyhow::Result<bool> {
        // The encoding choices made here are very curious
        let sha1_raw = self.base64.decode(&sha1_base64)?;
        let sha1_hex = hex::encode(sha1_raw);

        let path = self.cache_dir.join(sha1_hex);
        let exists = path.try_exists()?;
        if exists {
            self.map.insert(String::from(name), MediaSource::Path(path));
        }
        Ok(exists)
    }

    /// Gets a file from the media manager.
    /// Returns None if the file name is unknown.
    pub fn get(&self, name: &str) -> Option<&MediaSource> {
        self.map.get(name)
    }
}

pub struct NodeTextureData {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

/// A node texture manager using bindless textures (yay!)
pub struct NodeTextureManager {
    texture_vec: Vec<MyTexture>,
    // contains indices into texture_vec
    texture_map: HashMap<String, usize>,

    finished: bool,
}

impl NodeTextureManager {
    pub fn new() -> Self {
        Self {
            texture_vec: Vec::new(),
            texture_map: HashMap::new(),
            finished: false,
        }
    }

    /// Adds the texture with the given file name if it hasn't been added already,
    /// allocating an index for it.
    /// Returns Ok(true) on success.
    /// Returns Ok(false) if the file name is unknown.
    /// Returns Err(err) for texture loading errors.
    ///
    /// `finish` must not have been called yet.
    pub fn add_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        media: &MediaManager,
        name: &str,
    ) -> anyhow::Result<bool> {
        assert!(!self.finished);

        if self.texture_map.contains_key(name) {
            return Ok(true);
        }

        let Some(source) = media.get(name) else {
            return Ok(false);
        };
        let texture = match source {
            MediaSource::Path(path) => MyTexture::from_path(device, queue, name, path),
            MediaSource::Bytes(bytes) => MyTexture::from_bytes(device, queue, name, bytes),
        }?;
        self.texture_vec.push(texture);
        let index = self.texture_vec.len() - 1;
        self.texture_map.insert(String::from(name), index);
        Ok(true)
    }

    /// Returns the index allocated for the texture with the given file name.
    /// Returns None if the file name is unknown.
    ///
    /// `finish` must have been called.
    pub fn get_texture_index(&self, name: &str) -> Option<usize> {
        assert!(self.finished);

        self.texture_map.get(name).copied()
    }

    /// Finishes the NodeTextureManager, preventing further modification.
    /// Creates the bind group (layout) so the textures can be used for
    /// rendering.
    pub fn finish(&mut self, device: &wgpu::Device) -> NodeTextureData {
        assert!(!self.finished);
        self.finished = true;

        let texture_view_vec: Vec<&wgpu::TextureView> = self
            .texture_vec
            .iter()
            .map(|texture| &texture.view)
            .collect();

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
        let count = NonZero::new(self.texture_vec.len() as u32).unwrap();

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

        NodeTextureData {
            bind_group_layout,
            bind_group,
        }
    }
}
