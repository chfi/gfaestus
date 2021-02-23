pub mod gui;
pub mod lines;
pub mod nodes;
pub mod shapes;

pub use gui::GuiDrawSystem;
pub use lines::LineDrawSystem;
pub use nodes::NodeDrawSystem;
pub use shapes::ShapeDrawSystem;

use std::sync::Arc;

use vulkano::{
    device::{Device, Queue},
    format::Format,
    framebuffer::{
        Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass,
    },
    image::{AttachmentImage, ImageAccess, ImageUsage, ImageViewAccess},
    instance::PhysicalDevice,
};

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
    samples: u32,
    format: Format,
}

impl SinglePass {
    pub fn new(
        gfx_queue: Arc<Queue>,
        samples: Option<u32>,
        output_format: Format,
    ) -> Result<Self> {
        let supported_samples =
            supported_sample_counts(&gfx_queue.device().physical_device());

        let min_support = 1;
        let max_support =
            supported_samples.last().copied().unwrap_or(min_support);

        let samples = if let Some(samples) = samples {
            supported_samples
                .into_iter()
                .find(|&s| s >= samples)
                .unwrap_or(max_support)
        } else {
            max_support
        };

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
