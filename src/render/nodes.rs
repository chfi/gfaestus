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
}

impl std::default::Default for NodeDrawCache {
    fn default() -> Self {
        Self {
            cached_vertex_buffer: None,
            node_id_color_buffer: None,
        }
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

    pub fn draw_primary<'a, VI>(
        &self,
        builder: &'a mut AutoCommandBufferBuilder,
        dynamic_state: &DynamicState,
        vertices: Option<VI>,
        view: View,
        offset: Point,
        node_width: f32,
        use_lines: bool,
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

        let builder = builder.build()?;

        Ok(builder)
    }
}

/*

mod p_vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        path: "shaders/nodes_post/vertex.vert",
    }
}

mod p_gs {
    vulkano_shaders::shader! {
        ty: "geometry",
        path: "shaders/nodes_post/geometry.geom",
    }
}

mod p_fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "shaders/nodes_post/fragment.frag",
    }
}

mod post_vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        path: "shaders/nodes_post/post.vert",
    }
}

mod post_fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "shaders/nodes_post/post.frag",
    }
}

pub struct NodeDrawSystemPost {
    gfx_queue: Arc<Queue>,
    vertex_buffer_pool: CpuBufferPool<Vertex>,
    pipeline_pass_1: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    pipeline_pass_2: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    caches: Mutex<NodeDrawCache>,
    post_processing_vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
}

impl NodeDrawSystemPost {
    pub fn new<R>(
        gfx_queue: Arc<Queue>,
        first_pass: Subpass<R>,
        second_pass: Subpass<R>,
    ) -> NodeDrawSystemPost
    where
        R: RenderPassAbstract + Clone + Send + Sync + 'static,
    {
        let _ = include_str!("../../shaders/nodes_post/vertex.vert");
        let _ = include_str!("../../shaders/nodes_post/geometry.geom");
        let _ = include_str!("../../shaders/nodes_post/fragment.frag");

        let _ = include_str!("../../shaders/nodes_post/post.vert");
        let _ = include_str!("../../shaders/nodes_post/post.frag");

        let vs = p_vs::Shader::load(gfx_queue.device().clone()).unwrap();
        let fs = p_fs::Shader::load(gfx_queue.device().clone()).unwrap();
        let gs = p_gs::Shader::load(gfx_queue.device().clone()).unwrap();
        let post_vs =
            post_vs::Shader::load(gfx_queue.device().clone()).unwrap();
        let post_fs =
            post_fs::Shader::load(gfx_queue.device().clone()).unwrap();

        let vertex_buffer_pool: CpuBufferPool<Vertex> =
            CpuBufferPool::vertex_buffer(gfx_queue.device().clone());

        let pipeline_pass_1 = {
            Arc::new(
                GraphicsPipeline::start()
                    .vertex_input_single_buffer::<Vertex>()
                    .vertex_shader(vs.main_entry_point(), ())
                    .line_list()
                    .geometry_shader(gs.main_entry_point(), ())
                    .viewports_dynamic_scissors_irrelevant(1)
                    .fragment_shader(fs.main_entry_point(), ())
                    .render_pass(first_pass)
                    .blend_alpha_blending()
                    .build(gfx_queue.device().clone())
                    .unwrap(),
            ) as Arc<_>
        };

        let pipeline_pass_2 = {
            Arc::new(
                GraphicsPipeline::start()
                    .vertex_input_single_buffer::<Vertex>()
                    .vertex_shader(post_vs.main_entry_point(), ())
                    .triangle_list()
                    .viewports_dynamic_scissors_irrelevant(1)
                    .fragment_shader(post_fs.main_entry_point(), ())
                    .render_pass(second_pass)
                    .blend_alpha_blending()
                    .build(gfx_queue.device().clone())
                    .unwrap(),
            ) as Arc<_>
        };

        let vertex_buffer = {
            CpuAccessibleBuffer::from_iter(
                gfx_queue.device().clone(),
                BufferUsage::all(),
                false,
                [
                    Vertex {
                        position: [-0.5, -0.25],
                    },
                    Vertex {
                        position: [0.0, 0.5],
                    },
                    Vertex {
                        position: [0.25, -0.1],
                    },
                ]
                .iter()
                .cloned(),
            )
            .expect("failed to create buffer")
        };

        Self {
            gfx_queue,
            pipeline_pass_1,
            pipeline_pass_2,
            vertex_buffer_pool,
            caches: Mutex::new(Default::default()),
            post_processing_vertex_buffer: vertex_buffer,
        }
    }

    pub fn draw_second_pass<C, M>(
        &self,
        color_input: C,
        mask_input: M,
        dynamic_state: &DynamicState,
    ) -> Result<AutoCommandBuffer>
    where
        C: ImageViewAccess + Send + Sync + 'static,
        M: ImageViewAccess + Send + Sync + 'static,
    {
        let mut builder: AutoCommandBufferBuilder =
            AutoCommandBufferBuilder::secondary_graphics(
                self.gfx_queue.device().clone(),
                self.gfx_queue.family(),
                self.pipeline_pass_1.clone().subpass(),
            )?;

        let layout = self.pipeline_pass_2.descriptor_set_layout(0).unwrap();
        let set = {
            let set = PersistentDescriptorSet::start(layout.clone())
                .add_image(color_input)?
                .add_image(mask_input)?;
            let set = set.build()?;
            Arc::new(set)
        };

        builder.draw(
            self.pipeline_pass_2.clone(),
            &dynamic_state,
            vec![self.post_processing_vertex_buffer.clone()],
            set.clone(),
            (),
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }

    pub fn draw_first_pass<VI>(
        &self,
        dynamic_state: &DynamicState,
        vertices: Option<VI>,
        view: View,
        offset: Point,
        node_width: f32,
    ) -> Result<AutoCommandBuffer>
    where
        VI: IntoIterator<Item = Vertex>,
        VI::IntoIter: ExactSizeIterator,
    {
        let mut builder: AutoCommandBufferBuilder =
            AutoCommandBufferBuilder::secondary_graphics(
                self.gfx_queue.device().clone(),
                self.gfx_queue.family(),
                self.pipeline_pass_1.clone().subpass(),
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

        let layout = self.pipeline_pass_1.descriptor_set_layout(0).unwrap();
        let set = {
            let set = PersistentDescriptorSet::start(layout.clone())
                .add_buffer(data_buffer.clone())?;
            let set = set.build()?;
            Arc::new(set)
        };

        builder.draw(
            self.pipeline_pass_1.clone(),
            &dynamic_state,
            vec![vertex_buffer],
            set.clone(),
            view_pc,
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }
}
*/
