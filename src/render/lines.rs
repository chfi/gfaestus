use vulkano::buffer::{BufferUsage, CpuBufferPool, ImmutableBuffer};
use vulkano::command_buffer::{
    AutoCommandBuffer, AutoCommandBufferBuilder, DynamicState,
};
use vulkano::device::Queue;
use vulkano::framebuffer::{RenderPassAbstract, Subpass};

use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};

use vulkano::sync::GpuFuture;

use std::sync::Arc;

use anyhow::Result;

use rgb::*;

use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;

use crate::geometry::*;
use crate::view;
use crate::view::View;

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
    vertex_buffers: Mutex<Vec<(usize, Arc<ImmutableBuffer<[LineVertex]>>)>>,
    next_buffer: AtomicCell<usize>,
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

        let vertex_buffers = Vec::new();

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
            vertex_buffers: Mutex::new(vertex_buffers),
            next_buffer: AtomicCell::new(0),
        }
    }

    /// Add a set of lines to be rendered by this `LineDrawSystem`
    ///
    /// Returns the index of the vertex buffer in the
    /// `LineDrawSystem`, and a `GpuFuture` representing the upload of
    /// the resulting vertices to the GPU
    pub fn add_lines(
        &self,
        lines: &[(Point, Point)],
        color: RGB<f32>,
    ) -> Result<(usize, Box<dyn GpuFuture>)> {
        let vertices = line_vertices(lines, color);

        let (vbuf, buf_future) = ImmutableBuffer::from_iter(
            vertices.into_iter(),
            BufferUsage::vertex_buffer(),
            self.gfx_queue.clone(),
        )?;

        let index = self.next_buffer.fetch_add(1);
        {
            let mut buf_lock = self.vertex_buffers.lock();
            buf_lock.push((index, vbuf));
        }

        Ok((index, buf_future.boxed()))
    }

    pub fn draw_stored(
        &self,
        dynamic_state: &DynamicState,
        view: View,
    ) -> Result<AutoCommandBuffer> {
        let mut builder: AutoCommandBufferBuilder =
            AutoCommandBufferBuilder::secondary_graphics(
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

        let vertex_buffers = {
            let buf_lock = self.vertex_buffers.lock();
            buf_lock
                .iter()
                .map(|(_, b)| (b.clone()) as Arc<_>)
                .collect::<Vec<_>>()
        };

        builder.draw(
            self.pipeline.clone(),
            dynamic_state,
            vertex_buffers,
            (),
            push_constants,
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }

    pub fn draw_dynamic(
        &self,
        dynamic_state: &DynamicState,
        lines: &[(Point, Point)],
        color: RGB<f32>,
        view: View,
    ) -> Result<AutoCommandBuffer> {
        let mut builder: AutoCommandBufferBuilder =
            AutoCommandBufferBuilder::secondary_graphics(
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
