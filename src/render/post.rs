#[allow(unused_imports)]
use vulkano::buffer::{
    BufferUsage, CpuAccessibleBuffer, CpuBufferPool, ImmutableBuffer,
};
use vulkano::{
    command_buffer::{
        AutoCommandBuffer, AutoCommandBufferBuilder, DynamicState,
    },
    image::{ImageAccess, ImageViewAccess},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
};
use vulkano::{
    descriptor::descriptor_set::PersistentDescriptorSet,
    framebuffer::{RenderPassAbstract, Subpass},
};
use vulkano::{device::Queue, image::AttachmentImage};

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
        path: "shaders/post/vertex.vert",
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "shaders/post/fragment.frag",
    }
}

pub struct PostDrawSystem {
    gfx_queue: Arc<Queue>,
    vertex_buffer: Arc<CpuAccessibleBuffer<[Vertex]>>,
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
}

impl PostDrawSystem {
    pub fn new<R>(gfx_queue: Arc<Queue>, subpass: Subpass<R>) -> Self
    where
        R: RenderPassAbstract + Clone + Send + Sync + 'static,
    {
        let _ = include_str!("../../shaders/post/vertex.vert");
        let _ = include_str!("../../shaders/post/fragment.frag");

        let vs = vs::Shader::load(gfx_queue.device().clone()).unwrap();
        let fs = fs::Shader::load(gfx_queue.device().clone()).unwrap();

        let pipeline = {
            Arc::new(
                GraphicsPipeline::start()
                    .vertex_input_single_buffer::<Vertex>()
                    .vertex_shader(vs.main_entry_point(), ())
                    .triangle_list()
                    .viewports_dynamic_scissors_irrelevant(1)
                    .fragment_shader(fs.main_entry_point(), ())
                    .render_pass(subpass)
                    .blend_alpha_blending()
                    .build(gfx_queue.device().clone())
                    .unwrap(),
            ) as Arc<_>
        };

        let vertex_buffer = {
            CpuAccessibleBuffer::from_iter(
                gfx_queue.device().clone(),
                BufferUsage::vertex_buffer(),
                false,
                [
                    Vertex {
                        position: [-1.0, -1.0],
                    },
                    Vertex {
                        position: [3.0, -1.0],
                    },
                    Vertex {
                        position: [-1.0, 3.0],
                    },
                ]
                .iter()
                .cloned(),
            )
            .expect("failed to create buffer")
        };

        Self {
            gfx_queue,
            vertex_buffer,
            pipeline,
        }
    }

    pub fn draw<C>(
        &self,
        color_input: C,
        sampler: Arc<Sampler>,
        dynamic_state: &DynamicState,
    ) -> Result<AutoCommandBuffer>
    where
        C: ImageViewAccess + Send + Sync + 'static,
    {
        let mut builder: AutoCommandBufferBuilder =
            AutoCommandBufferBuilder::secondary_graphics(
                self.gfx_queue.device().clone(),
                self.gfx_queue.family(),
                self.pipeline.clone().subpass(),
            )?;

        let layout = self.pipeline.descriptor_set_layout(0).unwrap();

        let set = {
            let set = PersistentDescriptorSet::start(layout.clone())
                .add_sampled_image(color_input, sampler)?;
            let set = set.build()?;
            Arc::new(set)
        };

        let viewport_dims = {
            let viewport = dynamic_state
                .viewports
                .as_ref()
                .and_then(|v| v.get(0))
                .unwrap();
            viewport.dimensions
        };

        let pc = vs::ty::Dims {
            width: viewport_dims[0],
            height: viewport_dims[1],
        };

        builder.draw(
            self.pipeline.clone(),
            &dynamic_state,
            vec![self.vertex_buffer.clone()],
            set.clone(),
            pc,
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }

    /*
    pub fn draw_blur<C, M>(
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
                self.pipeline.clone().subpass(),
            )?;

        let layout = self.pipeline.descriptor_set_layout(0).unwrap();

        // let set = {
        //     let set = PersistentDescriptorSet::start(layout.clone())
        //         .add_image(color_input)?
        //         .add_image(mask_input)?;
        //     let set = set.build()?;
        //     Arc::new(set)
        // };

        let set = {
            let set = PersistentDescriptorSet::start(layout.clone())
                .add_sampled_image(color_input, self.sampler.clone())?
                .add_sampled_image(mask_input, self.sampler.clone())?;
            let set = set.build()?;
            Arc::new(set)
        };

        builder.draw(
            self.pipeline.clone(),
            &dynamic_state,
            vec![self.vertex_buffer.clone()],
            set.clone(),
            (),
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }
    */
}
