use vulkano::format::Format;
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::instance::debug::{DebugCallback, MessageSeverity, MessageType};
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::{
    buffer::cpu_pool::CpuBufferPoolChunk,
    device::{Device, DeviceExtensions, RawDeviceExtensions},
    memory::pool::StdMemoryPool,
};
use vulkano::{
    buffer::{
        BufferAccess, BufferUsage, CpuAccessibleBuffer, CpuBufferPool, ImmutableBuffer,
        TypedBufferAccess,
    },
    image::{AttachmentImage, Dimensions},
};
use vulkano::{
    command_buffer::{
        AutoCommandBuffer, AutoCommandBufferBuilder, CommandBufferExecFuture, DynamicState,
        SubpassContents,
    },
    pipeline::vertex::TwoBuffersDefinition,
};
use vulkano::{
    descriptor::{descriptor_set::PersistentDescriptorSet, PipelineLayoutAbstract},
    device::Queue,
};

use vulkano::pipeline::{viewport::Viewport, GraphicsPipeline, GraphicsPipelineAbstract};

use vulkano::swapchain::{
    self, AcquireError, ColorSpace, FullscreenExclusive, PresentMode, SurfaceTransform, Swapchain,
    SwapchainCreationError,
};
use vulkano::sync::{self, FlushError, GpuFuture};

use vulkano_win::VkSurfaceBuild;

use std::sync::Arc;

use crossbeam::channel;

use anyhow::{Context, Result};

use nalgebra_glm as glm;

use rgb::*;

use crate::geometry::*;
use crate::gfa::*;
use crate::ui::events::{keyboard_input, mouse_wheel_input};
use crate::ui::{UICmd, UIState, UIThread};
use crate::view;
use crate::view::View;

use crate::input::*;

use crate::layout::physics;
use crate::layout::*;

use super::{PoolChunk, SubPoolChunk};

#[derive(Default, Debug, Clone, Copy)]
pub struct LineVertex {
    pub position: [f32; 2],
    pub color: [f32; 3],
}

vulkano::impl_vertex!(LineVertex, position, color);

mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        path: "shaders/grid/vertex.vert",
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "shaders/grid/fragment.frag",
    }
}

pub struct LineDrawSystem {
    gfx_queue: Arc<Queue>,
    vertex_buffer_pool: CpuBufferPool<LineVertex>,
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
}

fn line_vertices(lines: &[(Point, Point)], color: RGB<f32>) -> Vec<LineVertex> {
    let mut res = Vec::with_capacity(lines.len() * 2);

    let pv = |p: Point| LineVertex {
        position: [p.x, p.y],
        color: [color.r, color.g, color.b],
    };

    for &(p0, p1) in lines.iter() {
        res.push(pv(p0));
        res.push(pv(p1));
    }
    res
}

impl LineDrawSystem {
    pub fn new<R>(gfx_queue: Arc<Queue>, subpass: Subpass<R>) -> LineDrawSystem
    where
        R: RenderPassAbstract + Send + Sync + 'static,
    {
        let _ = include_str!("../../shaders/grid/fragment.frag");
        let _ = include_str!("../../shaders/grid/vertex.vert");

        let vs = vs::Shader::load(gfx_queue.device().clone()).unwrap();
        let fs = fs::Shader::load(gfx_queue.device().clone()).unwrap();

        let vertex_buffer_pool: CpuBufferPool<LineVertex> =
            CpuBufferPool::vertex_buffer(gfx_queue.device().clone());

        let pipeline = {
            Arc::new(
                GraphicsPipeline::start()
                    .vertex_input_single_buffer::<LineVertex>()
                    .vertex_shader(vs.main_entry_point(), ())
                    .line_list()
                    .viewports_dynamic_scissors_irrelevant(1)
                    .fragment_shader(fs.main_entry_point(), ())
                    .render_pass(subpass)
                    .blend_alpha_blending()
                    .cull_mode_disabled()
                    .build(gfx_queue.device().clone())
                    .unwrap(),
            ) as Arc<_>
        };

        LineDrawSystem {
            gfx_queue,
            pipeline,
            vertex_buffer_pool,
        }
    }

    pub fn draw_lines(
        &self,
        dynamic_state: &DynamicState,
        lines: &[(Point, Point)],
        color: RGB<f32>,
        view: View,
    ) -> Result<AutoCommandBuffer> {
        let mut builder: AutoCommandBufferBuilder = AutoCommandBufferBuilder::secondary_graphics(
            self.gfx_queue.device().clone(),
            self.gfx_queue.family(),
            self.pipeline.clone().subpass(),
        )?;

        let viewport_dims = {
            let viewport = dynamic_state
                .viewports
                .as_ref()
                .and_then(|v| v.get(0))
                .unwrap();
            viewport.dimensions
        };

        #[rustfmt::skip]
        let push_constants = {
            let view_mat = view.to_scaled_matrix();

            let width = viewport_dims[0];
            let height = viewport_dims[1];

            let viewport_mat = view::viewport_scale(width, height);

            let matrix = viewport_mat * view_mat;

            let view_data = view::mat4_to_array(&matrix);

            vs::ty::View {
                view: view_data
            }

        };

        let vertices = line_vertices(lines, color);

        let vertex_buffer = self.vertex_buffer_pool.chunk(vertices)?;

        builder.draw(
            self.pipeline.clone(),
            dynamic_state,
            vec![Arc::new(vertex_buffer)],
            (),
            push_constants,
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }
}
