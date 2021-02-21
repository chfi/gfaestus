#[allow(unused_imports)]
use vulkano::buffer::{
    BufferUsage, CpuAccessibleBuffer, CpuBufferPool, ImmutableBuffer,
};
use vulkano::command_buffer::{
    AutoCommandBuffer, AutoCommandBufferBuilder, DynamicState,
};
use vulkano::device::Queue;
use vulkano::{
    descriptor::descriptor_set::PersistentDescriptorSet,
    framebuffer::{RenderPassAbstract, Subpass},
};

use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};

use std::sync::Arc;

use anyhow::Result;

use nalgebra_glm as glm;

use crate::geometry::*;
use crate::view;
use crate::view::{ScreenDims, View};

use super::Vertex;

mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        path: "shaders/nodes/vertex.vert",
    }
}

mod gs {
    vulkano_shaders::shader! {
        ty: "geometry",
        path: "shaders/nodes/geometry.geom",
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "shaders/nodes/fragment.frag",
    }
}

pub struct NodeDrawSystem {
    gfx_queue: Arc<Queue>,
    vertex_buffer_pool: CpuBufferPool<Vertex>,
    cached_vertex_buffer: Option<Arc<super::PoolChunk<Vertex>>>,
    rect_pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    line_pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    node_id_color_buffer: Option<Arc<CpuAccessibleBuffer<[u32]>>>,
}

impl NodeDrawSystem {
    pub fn new<R>(gfx_queue: Arc<Queue>, subpass: Subpass<R>) -> NodeDrawSystem
    where
        R: RenderPassAbstract + Clone + Send + Sync + 'static,
    {
        let _ = include_str!("../../shaders/nodes/vertex.vert");
        let _ = include_str!("../../shaders/nodes/geometry.geom");
        let _ = include_str!("../../shaders/nodes/fragment.frag");

        let vs = vs::Shader::load(gfx_queue.device().clone()).unwrap();
        let fs = fs::Shader::load(gfx_queue.device().clone()).unwrap();
        let gs = gs::Shader::load(gfx_queue.device().clone()).unwrap();

        let vertex_buffer_pool: CpuBufferPool<Vertex> =
            CpuBufferPool::vertex_buffer(gfx_queue.device().clone());

        let rect_pipeline = {
            Arc::new(
                GraphicsPipeline::start()
                    .vertex_input_single_buffer::<Vertex>()
                    .vertex_shader(vs.main_entry_point(), ())
                    .line_list()
                    .geometry_shader(gs.main_entry_point(), ())
                    .viewports_dynamic_scissors_irrelevant(1)
                    .fragment_shader(fs.main_entry_point(), ())
                    .render_pass(subpass.clone())
                    .blend_alpha_blending()
                    .build(gfx_queue.device().clone())
                    .unwrap(),
            ) as Arc<_>
        };

        let line_pipeline = {
            Arc::new(
                GraphicsPipeline::start()
                    .vertex_input_single_buffer::<Vertex>()
                    .vertex_shader(vs.main_entry_point(), ())
                    .line_list()
                    .viewports_dynamic_scissors_irrelevant(1)
                    .line_width_dynamic()
                    .fragment_shader(fs.main_entry_point(), ())
                    .render_pass(subpass)
                    .blend_alpha_blending()
                    .build(gfx_queue.device().clone())
                    .unwrap(),
            ) as Arc<_>
        };

        NodeDrawSystem {
            gfx_queue,
            // pipeline,
            vertex_buffer_pool,
            cached_vertex_buffer: None,
            node_id_color_buffer: None,
            rect_pipeline,
            line_pipeline,
        }
    }

    pub fn read_node_id_at<Dims: Into<ScreenDims>>(
        &self,
        screen_dims: Dims,
        point: Point,
    ) -> Option<u32> {
        let screen = screen_dims.into();
        let screen_width = screen.width as u32;
        let screen_height = screen.height as u32;

        let xu = point.x as u32;
        let yu = point.y as u32;
        if xu >= screen_width as u32 || yu >= screen_height as u32 {
            return None;
        }
        let ix = yu * screen_width + xu;
        let buffer = self.node_id_color_buffer.as_ref()?;
        let value = buffer.read().unwrap().get(ix as usize).copied()?;

        if value == 0 {
            None
        } else {
            Some(value)
        }
    }

    pub fn has_cached_vertices(&self) -> bool {
        self.cached_vertex_buffer.is_some()
    }

    pub fn draw<VI>(
        &mut self,
        dynamic_state: &DynamicState,
        vertices: Option<VI>,
        view: View,
        offset: Point,
        node_width: f32,
        use_lines: bool,
    ) -> Result<AutoCommandBuffer>
    where
        VI: IntoIterator<Item = Vertex>,
        VI::IntoIter: ExactSizeIterator,
    {
        let min_node_width = 2.0;
        let use_rect_pipeline = !use_lines
            || (use_lines && view.scale < (node_width / min_node_width));

        let mut builder: AutoCommandBufferBuilder = if use_rect_pipeline {
            AutoCommandBufferBuilder::secondary_graphics(
                self.gfx_queue.device().clone(),
                self.gfx_queue.family(),
                self.rect_pipeline.clone().subpass(),
            )
        } else {
            AutoCommandBufferBuilder::secondary_graphics(
                self.gfx_queue.device().clone(),
                self.gfx_queue.family(),
                self.line_pipeline.clone().subpass(),
            )
        }?;

        let viewport_dims = {
            let viewport = dynamic_state
                .viewports
                .as_ref()
                .and_then(|v| v.get(0))
                .unwrap();
            viewport.dimensions
        };

        #[rustfmt::skip]
        let view_pc = {
            // is this correct?
            let model_mat = glm::mat4(
                1.0, 0.0, 0.0, offset.x,
                0.0, 1.0, 0.0, offset.y,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0
            );

            let view_mat = view.to_scaled_matrix();

            let width = viewport_dims[0];
            let height = viewport_dims[1];

            let viewport_mat = view::viewport_scale(width, height);

            let matrix = viewport_mat * view_mat * model_mat;

            let view_data = view::mat4_to_array(&matrix);

            vs::ty::View {
                node_width,
                viewport_dims,
                view: view_data,
                scale: view.scale,

            }
        };

        let data_buffer = {
            let data_iter = (0..((viewport_dims[0] as u32)
                * (viewport_dims[1] as u32)))
                .map(|_| 0u32);
            CpuAccessibleBuffer::from_iter(
                self.gfx_queue.device().clone(),
                BufferUsage::all(),
                false,
                data_iter,
            )?
        };

        self.node_id_color_buffer = Some(data_buffer.clone());

        let vertex_buffer = if let Some(vertices) = vertices {
            println!("replacing vertex cache");
            let chunk = self.vertex_buffer_pool.chunk(vertices)?;
            let arc_chunk = Arc::new(chunk);
            self.cached_vertex_buffer = Some(arc_chunk.clone());
            arc_chunk
        } else {
            self.cached_vertex_buffer.as_ref().unwrap().clone()
        };

        if use_rect_pipeline {
            let layout = self.rect_pipeline.descriptor_set_layout(0).unwrap();
            let set = {
                let set = PersistentDescriptorSet::start(layout.clone())
                    .add_buffer(data_buffer.clone())?;
                let set = set.build()?;
                Arc::new(set)
            };

            builder.draw(
                self.rect_pipeline.clone(),
                &dynamic_state,
                vec![vertex_buffer],
                set.clone(),
                view_pc,
            )?;
        } else {
            let layout = self.line_pipeline.descriptor_set_layout(0).unwrap();
            let set = {
                let set = PersistentDescriptorSet::start(layout.clone())
                    .add_buffer(data_buffer.clone())?;
                let set = set.build()?;
                Arc::new(set)
            };

            // let line_width = (50.0 / view.scale).max(2.0);
            let line_width = (50.0 / view.scale).max(min_node_width);
            let mut dynamic_state = dynamic_state.clone();
            dynamic_state.line_width = Some(line_width);

            builder.draw(
                self.line_pipeline.clone(),
                &dynamic_state,
                vec![vertex_buffer],
                set.clone(),
                view_pc,
            )?;
        }

        let builder = builder.build()?;

        Ok(builder)
    }
}
