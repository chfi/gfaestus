#[allow(unused_imports)]
use vulkano::buffer::{
    BufferUsage, CpuAccessibleBuffer, CpuBufferPool, ImmutableBuffer,
};
use vulkano::device::Queue;
use vulkano::{
    command_buffer::{
        AutoCommandBuffer, AutoCommandBufferBuilder, DynamicState,
    },
    image::ImageViewAccess,
};
use vulkano::{
    descriptor::descriptor_set::PersistentDescriptorSet,
    framebuffer::{RenderPassAbstract, Subpass},
};

use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};

use parking_lot::Mutex;
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

struct NodeDrawCache {
    cached_vertex_buffer: Option<Arc<super::PoolChunk<Vertex>>>,
    node_id_color_buffer: Option<Arc<CpuAccessibleBuffer<[u32]>>>,
    node_selection_buffer: Option<Arc<CpuAccessibleBuffer<[u8]>>>,
}

impl std::default::Default for NodeDrawCache {
    fn default() -> Self {
        Self {
            cached_vertex_buffer: None,
            node_id_color_buffer: None,
            node_selection_buffer: None,
        }
    }
}

impl NodeDrawCache {
    fn allocate_selection_buffer(
        &mut self,
        queue: &Queue,
        node_count: usize,
    ) -> Result<()> {
        let buffer_usage = BufferUsage {
            transfer_source: false,
            transfer_destination: false,
            uniform_texel_buffer: false,
            storage_texel_buffer: false,
            uniform_buffer: true,
            storage_buffer: false,
            index_buffer: false,
            vertex_buffer: false,
            indirect_buffer: false,
            device_address: false,
        };

        let data_iter = (0..node_count).map(|_| 0u8);

        let buffer = CpuAccessibleBuffer::from_iter(
            queue.device().clone(),
            buffer_usage,
            false,
            data_iter,
        )?;

        self.node_selection_buffer = Some(buffer);

        Ok(())
    }
}

pub struct NodeDrawSystem {
    gfx_queue: Arc<Queue>,
    vertex_buffer_pool: CpuBufferPool<Vertex>,
    rect_pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    line_pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,

    caches: Mutex<NodeDrawCache>,
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

        use vulkano::pipeline::depth_stencil::{
            Compare, DepthBounds, DepthStencil, Stencil,
        };

        let depth_stencil = DepthStencil {
            depth_compare: Compare::Less,
            depth_write: true,
            depth_bounds_test: DepthBounds::Disabled,
            stencil_front: Stencil::default(),
            stencil_back: Stencil::default(),
        };

        let rect_pipeline = {
            Arc::new(
                GraphicsPipeline::start()
                    .vertex_input_single_buffer::<Vertex>()
                    .vertex_shader(vs.main_entry_point(), ())
                    .line_list()
                    .geometry_shader(gs.main_entry_point(), ())
                    .viewports_dynamic_scissors_irrelevant(1)
                    .fragment_shader(fs.main_entry_point(), ())
                    .depth_stencil(depth_stencil.clone())
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
                    .depth_stencil(depth_stencil.clone())
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
            rect_pipeline,
            line_pipeline,
            caches: Mutex::new(Default::default()),
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
        let value = {
            let cache_lock = self.caches.lock();
            let buffer = cache_lock.node_id_color_buffer.as_ref()?;
            let value = buffer.read().unwrap().get(ix as usize).copied()?;
            value
        };

        if value == 0 {
            None
        } else {
            Some(value)
        }
    }

    pub fn has_cached_vertices(&self) -> bool {
        let cache_lock = self.caches.lock();
        cache_lock.cached_vertex_buffer.is_some()
    }

    pub fn allocate_node_selection_buffer(
        &self,
        node_count: usize,
    ) -> Result<()> {
        let mut cache_lock = self.caches.lock();
        cache_lock.allocate_selection_buffer(&self.gfx_queue, node_count)
    }

    pub fn is_node_selection_buffer_alloc(
        &self,
        node_count: usize,
    ) -> Result<bool> {
        let cache_lock = self.caches.lock();

        if let Some(buffer) = cache_lock.node_selection_buffer.as_ref() {
            let buf = buffer.read()?;
            if buf.len() == node_count {
                return Ok(true);
            } else {
                return Ok(false);
            }
        } else {
            return Ok(false);
        }
    }

    // pub fn update_node_selection(&self, node_count: usize,

    pub fn update_node_selection<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(&CpuAccessibleBuffer<[u8]>) -> Result<()>,
    {
        let cache_lock = self.caches.lock();
        let buffer = cache_lock.node_selection_buffer.as_ref().unwrap();

        f(buffer)?;

        Ok(())
    }

    pub fn draw_primary<'a, VI>(
        &self,
        builder: &'a mut AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
        vertices: Option<VI>,
        view: View,
        offset: Point,
        node_width: f32,
        use_lines: bool,
        selected_node: i32,
    ) -> Result<&'a mut AutoCommandBufferBuilder>
    where
        VI: IntoIterator<Item = Vertex>,
        VI::IntoIter: ExactSizeIterator,
    {
        let min_node_width = 2.0;
        let use_rect_pipeline = !use_lines
            || (use_lines && view.scale < (node_width / min_node_width));

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
                selected_node,
            }
        };

        let data_buffer = {
            let buffer_usage = BufferUsage {
                storage_buffer: true,
                ..BufferUsage::none()
            };

            let data_iter = (0..((viewport_dims[0] as u32)
                * (viewport_dims[1] as u32)))
                .map(|_| 0u32);
            CpuAccessibleBuffer::from_iter(
                self.gfx_queue.device().clone(),
                buffer_usage,
                false,
                data_iter,
            )?
        };

        let vertex_buffer = {
            let mut cache_lock = self.caches.lock();

            cache_lock.node_id_color_buffer = Some(data_buffer.clone());

            let inner_buf = if let Some(vertices) = vertices {
                println!("replacing vertex cache");
                let chunk = self.vertex_buffer_pool.chunk(vertices)?;
                let arc_chunk = Arc::new(chunk);
                cache_lock.cached_vertex_buffer = Some(arc_chunk.clone());
                arc_chunk
            } else {
                cache_lock.cached_vertex_buffer.as_ref().unwrap().clone()
            };

            inner_buf
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

        Ok(builder)
    }

    pub fn draw<VI>(
        &self,
        dynamic_state: &DynamicState,
        vertices: Option<VI>,
        view: View,
        offset: Point,
        node_width: f32,
        use_lines: bool,
        selected_node: i32,
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

        self.draw_primary(
            &mut builder,
            dynamic_state,
            vertices,
            view,
            offset,
            node_width,
            use_lines,
            selected_node,
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }
}
