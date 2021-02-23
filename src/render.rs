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
    device::Queue,
    format::Format,
    framebuffer::{RenderPassAbstract, Subpass},
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

pub struct SinglePassMSAA {
    gfx_queue: Arc<Queue>,
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
}

impl SinglePassMSAA {
    pub fn new(
        gfx_queue: Arc<Queue>,
        samples: u32,
        output_format: Format,
    ) -> Result<Self> {
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
}
