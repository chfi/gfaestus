pub mod gui;
pub mod lines;
pub mod nodes;
pub mod post;
pub mod shapes;

pub use gui::GuiDrawSystem;
pub use lines::LineDrawSystem;
pub use nodes::NodeDrawSystem;
pub use post::PostDrawSystem;
pub use shapes::ShapeDrawSystem;

use std::sync::Arc;

use vulkano::{
    descriptor::{
        descriptor_set::{
            FixedSizeDescriptorSetsPool, PersistentDescriptorSet,
        },
        DescriptorSet,
    },
    device::{Device, Queue},
    format::Format,
    framebuffer::{
        Framebuffer, FramebufferAbstract, RenderPassAbstract, RenderPassDesc,
        Subpass,
    },
    image::{AttachmentImage, ImageAccess, ImageUsage, ImageViewAccess},
    instance::PhysicalDevice,
};

use vulkano::sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode};

use anyhow::Result;

pub type PoolChunk<T> = vulkano::buffer::cpu_pool::CpuBufferPoolChunk<
    T,
    std::sync::Arc<vulkano::memory::pool::StdMemoryPool>,
>;
pub type SubPoolChunk<T> = vulkano::buffer::cpu_pool::CpuBufferPoolSubbuffer<
    T,
    std::sync::Arc<vulkano::memory::pool::StdMemoryPool>,
>;

use crate::util::*;

#[derive(Default, Debug, Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 2],
}

vulkano::impl_vertex!(Vertex, position);

#[derive(Default, Debug, Clone, Copy)]
pub struct Color {
    pub color: [f32; 3],
}

vulkano::impl_vertex!(Color, color);

pub struct RenderPipeline {
    gfx_queue: Arc<Queue>,

    final_format: Format,
    samples: u32,

    offscreen_msaa_depth_mask_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    offscreen_pass: Arc<dyn RenderPassAbstract + Send + Sync>,

    final_pass: Arc<dyn RenderPassAbstract + Send + Sync>,

    nodes_color: OffscreenImage,
    nodes_mask: OffscreenImage,

    selection_edge_color: OffscreenImage,
}

impl RenderPipeline {
    pub fn nodes_color(&self) -> &OffscreenImage {
        &self.nodes_color
    }

    pub fn nodes_mask(&self) -> &OffscreenImage {
        &self.nodes_mask
    }

    pub fn selection_edge_color(&self) -> &OffscreenImage {
        &self.selection_edge_color
    }

    pub fn nodes_pass(&self) -> &Arc<dyn RenderPassAbstract + Send + Sync> {
        &self.offscreen_msaa_depth_mask_pass
    }

    pub fn post_processing_pass(
        &self,
    ) -> &Arc<dyn RenderPassAbstract + Send + Sync> {
        &self.offscreen_pass
    }

    pub fn final_pass(&self) -> &Arc<dyn RenderPassAbstract + Send + Sync> {
        &self.final_pass
    }

    pub fn new(
        gfx_queue: Arc<Queue>,
        samples: Option<u32>,
        final_format: Format,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let samples = pick_supported_sample_count(
            &gfx_queue.device().physical_device(),
            samples,
        );

        use vulkano::image::ImageLayout;

        let offscreen_msaa_depth_mask_pass = {
            let render_pass = vulkano::single_pass_renderpass!(
            gfx_queue.device().clone(),
            attachments: {
                intermediary: {
                    load: Clear,
                    store: DontCare,
                    format: Format::R8G8B8A8Unorm,
                    samples: samples,
                    initial_layout: ImageLayout::Undefined,
                    final_layout: ImageLayout::ColorAttachmentOptimal,
                },
                mask_intermediary: {
                    load: Clear,
                    store: DontCare,
                    format: Format::R8G8B8A8Unorm,
                    samples: samples,
                    initial_layout: ImageLayout::Undefined,
                    final_layout: ImageLayout::ColorAttachmentOptimal,
                },
                color: {
                    load: Clear,
                    store: Store,
                    format: Format::R8G8B8A8Unorm,
                    samples: 1,
                },
                depth: {
                    load: Clear,
                    store: DontCare,
                    format: Format::D16Unorm,
                    samples: samples,
                },
                mask: {
                    load: Clear,
                    store: Store,
                    format: Format::R8G8B8A8Unorm,
                    samples: 1,
                }
            },
            pass: {
                color: [intermediary, mask_intermediary],
                depth_stencil: {depth},
                resolve: [color, mask],
            }
            )?;

            Arc::new(render_pass)
        };

        let offscreen_pass = {
            let render_pass = vulkano::single_pass_renderpass!(
                gfx_queue.device().clone(),
                attachments: {
                    color: {
                        load: Clear,
                        store: Store,
                        format: Format::R8G8B8A8Unorm,
                        samples: 1,
                    }
                },
                pass: {
                    color: [color],
                    depth_stencil: {}
                    resolve: [],
                }
            )?;

            Arc::new(render_pass)
        };

        let final_pass = {
            let render_pass = vulkano::single_pass_renderpass!(
            gfx_queue.device().clone(),
            attachments: {
                intermediary: {
                    load: DontCare,
                    store: DontCare,
                    format: final_format,
                    samples: samples,
                },
                color: {
                    load: DontCare,
                    store: Store,
                    format: final_format,
                    samples: 1,
                }
            },
            pass: {
                color: [intermediary],
                depth_stencil: {}
                resolve: [color],
            }
                )?;

            Arc::new(render_pass)
        };

        let nodes_color =
            OffscreenImage::new(gfx_queue.clone(), width, height)?;
        let nodes_mask = OffscreenImage::new(gfx_queue.clone(), width, height)?;

        let selection_edge_color =
            OffscreenImage::new(gfx_queue.clone(), width, height)?;

        Ok(Self {
            gfx_queue,
            final_format,
            offscreen_msaa_depth_mask_pass,
            offscreen_pass,
            final_pass,
            nodes_color,
            nodes_mask,
            selection_edge_color,
            samples,
        })
    }

    pub fn recreate_offscreen(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.nodes_color.recreate(width, height)?;
        self.nodes_mask.recreate(width, height)?;
        self.selection_edge_color.recreate(width, height)?;

        Ok(())
    }

    pub fn nodes_framebuffer(
        &self,
    ) -> Result<Arc<dyn FramebufferAbstract + Send + Sync>> {
        let img_dims =
            ImageAccess::dimensions(self.nodes_color.image()).width_height();

        let intermediary = AttachmentImage::transient_multisampled(
            self.gfx_queue.device().clone(),
            img_dims,
            self.samples,
            Format::R8G8B8A8Unorm,
        )?;

        let intermediary_mask = AttachmentImage::transient_multisampled(
            self.gfx_queue.device().clone(),
            img_dims,
            self.samples,
            Format::R8G8B8A8Unorm,
        )?;

        let depth = AttachmentImage::transient_multisampled(
            self.gfx_queue.device().clone(),
            img_dims,
            self.samples,
            Format::D16Unorm,
        )?;

        let framebuffer =
            Framebuffer::start(self.offscreen_msaa_depth_mask_pass.clone())
                .add(intermediary.clone())?
                .add(intermediary_mask.clone())?
                .add(self.nodes_color.image().clone())?
                .add(depth.clone())?
                .add(self.nodes_mask.image().clone())?
                .build()?;

        Ok(Arc::new(framebuffer) as Arc<dyn FramebufferAbstract + Send + Sync>)
    }

    /*
    pub fn nodes_color_framebuffer(
        &self,
    ) -> Result<Arc<dyn FramebufferAbstract + Send + Sync>> {
        let img_dims = ImageAccess::dimensions(self.nodes_color.image())
            .width_height();

        let intermediary = AttachmentImage::transient_multisampled(
            self.gfx_queue.device().clone(),
            img_dims,
            self.samples,
            Format::R8G8B8A8Unorm,
        )?;

        let framebuffer = Framebuffer::start(self.offscreen_msaa_pass.clone())
            .add(intermediary.clone())?
            .add(self.nodes_color.image().clone())?
            .build()?;

        Ok(Arc::new(framebuffer) as Arc<dyn FramebufferAbstract + Send + Sync>)
    }
    */

    pub fn selection_edge_color_framebuffer(
        &self,
    ) -> Result<Arc<dyn FramebufferAbstract + Send + Sync>> {
        // let img_dims =
        //     ImageAccess::dimensions(self.selection_edge_color.image())
        //         .width_height();

        // let intermediary = AttachmentImage::transient_multisampled(
        //     self.gfx_queue.device().clone(),
        //     img_dims,
        //     self.samples,
        //     Format::R8G8B8A8Unorm,
        // )?;

        let framebuffer = Framebuffer::start(self.offscreen_pass.clone())
            // .add(intermediary.clone())?
            .add(self.selection_edge_color.image().clone())?
            .build()?;

        Ok(Arc::new(framebuffer) as Arc<dyn FramebufferAbstract + Send + Sync>)
    }

    pub fn final_framebuffer<I>(
        &self,
        target: I,
    ) -> Result<Arc<dyn FramebufferAbstract + Send + Sync>>
    where
        I: ImageAccess + ImageViewAccess + Clone + Send + Sync + 'static,
    {
        let img_dims =
            ImageAccess::dimensions(self.selection_edge_color.image())
                .width_height();

        let intermediary = AttachmentImage::transient_multisampled(
            self.gfx_queue.device().clone(),
            img_dims,
            self.samples,
            self.final_format,
        )?;

        let framebuffer = Framebuffer::start(self.final_pass.clone())
            .add(intermediary)?
            .add(target.clone())?
            .build()?;

        Ok(Arc::new(framebuffer) as Arc<dyn FramebufferAbstract + Send + Sync>)
    }
}

pub struct OffscreenImage {
    gfx_queue: Arc<Queue>,
    color: Arc<AttachmentImage>,
    dims: [u32; 2],
    sampler: Arc<Sampler>,
}

impl OffscreenImage {
    fn create_image(
        gfx_queue: &Queue,
        width: u32,
        height: u32,
    ) -> Result<Arc<AttachmentImage>> {
        let usage = ImageUsage {
            sampled: true,
            transfer_destination: true,
            ..ImageUsage::none()
        };

        AttachmentImage::with_usage(
            gfx_queue.device().clone(),
            [width, height],
            Format::R8G8B8A8Unorm,
            usage,
        )
        .map_err(|e| e.into())
    }

    pub fn new(gfx_queue: Arc<Queue>, width: u32, height: u32) -> Result<Self> {
        let color = Self::create_image(&gfx_queue, width, height)?;

        let sampler = Sampler::new(
            gfx_queue.device().clone(),
            Filter::Linear,
            Filter::Linear,
            MipmapMode::Linear,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.0,
            1.0,
            0.0,
            1.0,
        )?;

        Ok(Self {
            gfx_queue,
            color,
            dims: [width, height],
            sampler,
        })
    }

    pub fn recreate(&mut self, width: u32, height: u32) -> Result<bool> {
        if self.dims == [width, height] {
            return Ok(false);
        }

        let color = Self::create_image(&self.gfx_queue, width, height)?;

        self.color = color;
        self.dims = [width, height];

        Ok(true)
    }

    pub fn image(&self) -> &Arc<AttachmentImage> {
        &self.color
    }

    pub fn sampler(&self) -> &Arc<Sampler> {
        &self.sampler
    }
}

fn pick_supported_sample_count(
    device: &PhysicalDevice,
    samples: Option<u32>,
) -> u32 {
    let supported_samples = supported_sample_counts(device);

    let min_support = 1;
    let max_support = supported_samples.last().copied().unwrap_or(min_support);

    if let Some(samples) = samples {
        supported_samples
            .into_iter()
            .find(|&s| s >= samples)
            .unwrap_or(max_support)
    } else {
        max_support
    }
}

fn supported_sample_counts(device: &PhysicalDevice) -> Vec<u32> {
    let limits = device.limits();

    let color_sample_counts = limits.framebuffer_color_sample_counts();
    let depth_sample_counts = limits.framebuffer_depth_sample_counts();

    let counts = color_sample_counts & depth_sample_counts;

    let mut res = Vec::new();

    for i in 0..32 {
        if (counts >> i) & 1 == 1 {
            res.push(1 << i);
        }
    }

    res
}
