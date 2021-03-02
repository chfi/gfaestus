use vulkano::{
    device::Queue,
    format::{Format, R8G8B8A8Unorm},
    framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract},
    image::{
        AttachmentImage, ImageAccess, ImageUsage, ImageViewAccess,
        ImmutableImage,
    },
    instance::PhysicalDevice,
};

use vulkano::sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode};

use rgb::*;

use anyhow::Result;

use std::sync::Arc;

pub struct ThemeDef {
    background: RGB<f32>,
    node_colors: Vec<RGB<f32>>,
}

pub struct AppThemes {
    active: ThemeData,

    base: ThemeDef,
    available: Vec<ThemeDef>,

    sampler: Arc<Sampler>,
}

pub struct ThemeData {
    background: [f32; 4],
    node_colors: Arc<ImmutableImage<R8G8B8A8Unorm>>,
}
