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
        Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass,
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

pub struct SinglePass {
    gfx_queue: Arc<Queue>,
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    format: Format,
}

impl SinglePass {
    pub fn new(gfx_queue: Arc<Queue>, output_format: Format) -> Result<Self> {
        let render_pass = vulkano::single_pass_renderpass!(
        gfx_queue.device().clone(),
        attachments: {
            color: {
                load: Clear,
                store: Store,
                format: output_format,
                samples: 1,
            }
        },
        pass: {
            color: [color],
            depth_stencil: {}
            resolve: [],
        }
        )?;

        let render_pass = Arc::new(render_pass);

        Ok(Self {
            gfx_queue,
            render_pass,
            format: output_format,
        })
    }

    pub fn render_pass(&self) -> Arc<dyn RenderPassAbstract + Send + Sync> {
        self.render_pass.clone()
    }

    pub fn subpass(
        &self,
    ) -> Subpass<Arc<dyn RenderPassAbstract + Send + Sync>> {
        Subpass::from(self.render_pass.clone(), 0).unwrap()
    }

    pub fn queue(&self) -> Arc<Queue> {
        self.gfx_queue.clone()
    }

    pub fn framebuffer<I>(
        &self,
        image: I,
    ) -> Result<Arc<dyn FramebufferAbstract + Send + Sync>>
    where
        I: ImageAccess + ImageViewAccess + Clone + Send + Sync + 'static,
    {
        let framebuffer = Framebuffer::start(self.render_pass())
            .add(image.clone())?
            .build()?;

        Ok(Arc::new(framebuffer) as Arc<dyn FramebufferAbstract + Send + Sync>)
    }
}

pub struct SinglePassMSAA {
    gfx_queue: Arc<Queue>,
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    samples: u32,
    format: Format,
}

impl SinglePassMSAA {
    pub fn new(
        gfx_queue: Arc<Queue>,
        samples: Option<u32>,
        output_format: Format,
    ) -> Result<Self> {
        let samples = pick_supported_sample_count(
            &gfx_queue.device().physical_device(),
            samples,
        );

        let render_pass = vulkano::single_pass_renderpass!(
        gfx_queue.device().clone(),
        attachments: {
            intermediary: {
                load: Clear,
                store: DontCare,
                format: output_format,
                samples: samples,
            },
            color: {
                load: Clear,
                store: Store,
                format: output_format,
                samples: 1,
            }
        },
        pass: {
            color: [intermediary],
            depth_stencil: {}
            resolve: [color],
        }
        )?;

        let render_pass = Arc::new(render_pass);

        Ok(Self {
            gfx_queue,
            render_pass,
            samples,
            format: output_format,
        })
    }

    pub fn render_pass(&self) -> Arc<dyn RenderPassAbstract + Send + Sync> {
        self.render_pass.clone()
    }

    pub fn subpass(
        &self,
    ) -> Subpass<Arc<dyn RenderPassAbstract + Send + Sync>> {
        Subpass::from(self.render_pass.clone(), 0).unwrap()
    }

    pub fn queue(&self) -> Arc<Queue> {
        self.gfx_queue.clone()
    }

    pub fn framebuffer<I>(
        &self,
        image: I,
    ) -> Result<Arc<dyn FramebufferAbstract + Send + Sync>>
    where
        I: ImageAccess + ImageViewAccess + Clone + Send + Sync + 'static,
    {
        let img_dims = ImageAccess::dimensions(&image).width_height();

        let intermediary = AttachmentImage::transient_multisampled(
            self.gfx_queue.device().clone(),
            img_dims,
            self.samples,
            self.format,
        )?;

        let framebuffer: Framebuffer<
            Arc<dyn RenderPassAbstract + Send + Sync>,
            (((), Arc<AttachmentImage>), I),
        > = Framebuffer::start(self.render_pass())
            .add(intermediary.clone())?
            .add(image.clone())?
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
    pub fn new(gfx_queue: Arc<Queue>, width: u32, height: u32) -> Result<Self> {
        let color = AttachmentImage::with_usage(
            gfx_queue.device().clone(),
            [width, height],
            Format::R8G8B8A8Unorm,
            ImageUsage {
                color_attachment: true,
                sampled: true,
                transfer_destination: true,
                ..ImageUsage::none()
            },
        )?;

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

        let color = AttachmentImage::with_usage(
            self.gfx_queue.device().clone(),
            [width, height],
            Format::R8G8B8A8Unorm,
            ImageUsage {
                color_attachment: true,
                sampled: true,
                ..ImageUsage::none()
            },
        )?;

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
