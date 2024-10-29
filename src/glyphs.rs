use crate::atlas::{Atlas, AtlasContent};
use cosmic_text::{SubpixelBin, SwashContent, SwashImage};
use rect_packer::Rect;
use std::collections::HashMap;

#[derive(Copy, Clone, Debug)]
pub struct AtlasInfo {
    pub rect: Option<Rect>,
    pub left: i32,
    pub top: i32,
    pub colored: bool,
}

pub enum PixelFormat {
    //TODO: add Rgb(currently we assume Rgba everywhere)
    Rgba,
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub pixel_format: PixelFormat,
}

pub struct GlyphCache {
    pub size: u32,
    pub mask_atlas: Atlas,
    pub color_atlas: Atlas,
    glyph_infos: HashMap<
        (
            cosmic_text::fontdb::ID,
            u16,
            u32,
            (SubpixelBin, SubpixelBin),
        ),
        AtlasInfo,
    >,
    svg_infos: HashMap<Vec<u8>, HashMap<(u32, u32), AtlasInfo>>,
    img_infos: HashMap<Vec<u8>, AtlasInfo>,
}

impl GlyphCache {
    pub fn new(device: &wgpu::Device) -> Self {
        let size = 4096;
        Self {
            size,
            mask_atlas: Atlas::new(device, AtlasContent::Mask, size, size),
            color_atlas: Atlas::new(device, AtlasContent::Color, size, size),
            glyph_infos: HashMap::new(),
            img_infos: HashMap::new(),
            svg_infos: HashMap::new(),
        }
    }

    pub fn get_image_mask(&mut self, hash: &[u8], image_fn: impl FnOnce() -> Image) -> AtlasInfo {
        if let Some(info) = self.img_infos.get(hash) {
            // println!("Found cached image info: {:?}", info);
            return *info;
        }

        let image = image_fn();
        // println!("Adding new image: {}x{}", image.width, image.height);

        let rect = self
            .color_atlas
            .add_region(&image.data, image.width, image.height);
        let info = AtlasInfo {
            rect,
            left: 0,
            top: 0,
            colored: true,
        };
        // println!("Got atlas info: {:?}", info);
        self.img_infos.insert(hash.to_vec(), info);

        info
    }

    pub fn get_svg_mask(
        &mut self,
        hash: &[u8],
        width: u32,
        height: u32,
        image: impl FnOnce() -> Vec<u8>,
    ) -> AtlasInfo {
        if !self.svg_infos.contains_key(hash) {
            self.svg_infos.insert(hash.to_vec(), HashMap::new());
        }

        {
            let svg_infos = self.svg_infos.get(hash).unwrap();
            if let Some(info) = svg_infos.get(&(width, height)) {
                return *info;
            }
        }

        let data = image();
        let rect = self.color_atlas.add_region(&data, width, height);
        let info = AtlasInfo {
            rect,
            left: 0,
            top: 0,
            colored: true,
        };

        let svg_infos = self.svg_infos.get_mut(hash).unwrap();
        svg_infos.insert((width, height), info);

        info
    }

    pub fn get_glyph_mask(
        &mut self,
        font_id: cosmic_text::fontdb::ID,
        glyph_id: u16,
        size: u32,
        subpx: (SubpixelBin, SubpixelBin),
        image: impl FnOnce() -> SwashImage,
    ) -> AtlasInfo {
        let key = (font_id, glyph_id, size, subpx);
        if let Some(rect) = self.glyph_infos.get(&key) {
            return *rect;
        }

        let image = image();
        let rect = match image.content {
            SwashContent::Mask => self.mask_atlas.add_region(
                &image.data,
                image.placement.width,
                image.placement.height,
            ),
            SwashContent::SubpixelMask | SwashContent::Color => self.color_atlas.add_region(
                &image.data,
                image.placement.width,
                image.placement.height,
            ),
        };
        let info = AtlasInfo {
            rect,
            left: image.placement.left,
            top: image.placement.top,
            colored: image.content != SwashContent::Mask,
        };
        self.glyph_infos.insert(key, info);
        info
    }

    pub fn update(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        self.mask_atlas.update(device, encoder);
        self.color_atlas.update(device, encoder);
    }

    pub fn check_usage(&mut self, device: &wgpu::Device) -> bool {
        let max_seen = (self.mask_atlas.max_seen as f32 * 2.0)
            .max(self.color_atlas.max_seen as f32 * 2.0) as u32;
        if max_seen > self.size {
            self.size = max_seen;
            self.mask_atlas.resize(device, self.size, self.size);
            self.color_atlas.resize(device, self.size, self.size);
            self.clear();
            true
        } else if self.mask_atlas.usage() > 0.7 || self.color_atlas.usage() > 0.7 {
            self.clear();
            false
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.mask_atlas.clear();
        self.color_atlas.clear();
        self.glyph_infos.clear();
        self.svg_infos.clear();
        self.img_infos.clear();
    }
}
