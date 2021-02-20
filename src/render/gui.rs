use vulkano::framebuffer::{RenderPassAbstract, Subpass};
use vulkano::sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode};

#[allow(unused_imports)]
use vulkano::{
    buffer::{
        BufferUsage, CpuAccessibleBuffer, CpuBufferPool, ImmutableBuffer,
    },
    image::{
        AttachmentImage, Dimensions, ImageUsage, ImmutableImage, StorageImage,
        SwapchainImage,
    },
};
use vulkano::{
    command_buffer::{
        AutoCommandBuffer, AutoCommandBufferBuilder, DynamicState,
    },
    sync::GpuFuture,
};
use vulkano::{
    descriptor::descriptor_set::PersistentDescriptorSet, device::Queue,
};

use vulkano::format::R8Unorm;

use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};

use std::sync::Arc;

use anyhow::Result;

use rustc_hash::FxHashMap;

mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        path: "shaders/gui/vertex.vert",
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "shaders/gui/fragment.frag",
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GuiVertex {
    /// Logical pixel coordinates (points).
    /// (0,0) is the top left corner of the screen.
    pub pos: [f32; 2], // 64 bit

    /// Normalized texture coordinates.
    /// (0, 0) is the top left corner of the texture.
    /// (1, 1) is the bottom right corner of the texture.
    pub uv: [f32; 2], // 64 bit

    /// sRGBA with premultiplied alpha
    pub color: [f32; 4], // 32 bit
}

vulkano::impl_vertex!(GuiVertex, pos, uv, color);

struct GuiTexture {
    version: u64,
    texture: Arc<ImmutableImage<R8Unorm>>,
}

pub struct GuiDrawSystem {
    gfx_queue: Arc<Queue>,
    // texture_cache: FxHashMap<u64, Arc<ImmutableImage<R8Unorm>>>,
    sampler: Arc<Sampler>,
    cached_texture: Option<GuiTexture>,
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    vertex_buffer_pool: CpuBufferPool<GuiVertex>,
    index_buffer_pool: CpuBufferPool<u32>,
}

impl GuiDrawSystem {
    pub fn new<R>(gfx_queue: Arc<Queue>, subpass: Subpass<R>) -> GuiDrawSystem
    where
        R: RenderPassAbstract + Send + Sync + 'static,
    {
        let _ = include_str!("../../shaders/gui/fragment.frag");
        let _ = include_str!("../../shaders/gui/vertex.vert");

        let vs = vs::Shader::load(gfx_queue.device().clone()).unwrap();
        let fs = fs::Shader::load(gfx_queue.device().clone()).unwrap();

        let vertex_buffer_pool: CpuBufferPool<GuiVertex> =
            CpuBufferPool::vertex_buffer(gfx_queue.device().clone());
        let index_buffer_pool: CpuBufferPool<u32> = CpuBufferPool::new(
            gfx_queue.device().clone(),
            BufferUsage::index_buffer(),
        );

        use vulkano::pipeline::blend::{AttachmentBlend, BlendFactor, BlendOp};

        let mut at_blend = AttachmentBlend::pass_through();
        at_blend.enabled = true;

        at_blend.color_op = BlendOp::Add;
        at_blend.color_source = BlendFactor::One;
        at_blend.color_destination = BlendFactor::OneMinusSrcAlpha;

        at_blend.alpha_op = BlendOp::Add;
        at_blend.alpha_source = BlendFactor::OneMinusDstAlpha;
        at_blend.alpha_destination = BlendFactor::One;

        let sampler = Sampler::new(
            gfx_queue.device().clone(),
            Filter::Linear,
            Filter::Linear,
            // Filter::Nearest,
            // Filter::Nearest,
            MipmapMode::Nearest,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            // SamplerAddressMode::Repeat,
            // SamplerAddressMode::Repeat,
            // SamplerAddressMode::Repeat,
            0.0,
            1.0,
            0.0,
            0.0,
        )
        .unwrap();

        let pipeline = {
            Arc::new(
                GraphicsPipeline::start()
                    .vertex_input_single_buffer::<GuiVertex>()
                    .vertex_shader(vs.main_entry_point(), ())
                    .triangle_list()
                    .viewports_dynamic_scissors_irrelevant(1)
                    .fragment_shader(fs.main_entry_point(), ())
                    .render_pass(subpass)
                    .cull_mode_disabled()
                    .blend_alpha_blending()
                    // .blend_collective(at_blend)
                    .build(gfx_queue.device().clone())
                    .unwrap(),
            ) as Arc<_>
        };

        GuiDrawSystem {
            gfx_queue,
            pipeline,
            vertex_buffer_pool,
            index_buffer_pool,
            cached_texture: None,
            sampler,
        }
    }

    pub fn texture_version(&self) -> Option<u64> {
        self.cached_texture.as_ref().map(|gt| gt.version)
    }

    fn force_upload_texture(
        &mut self,
        texture: &egui::Texture,
    ) -> Result<Box<dyn GpuFuture>> {
        let (img, tex_future) = ImmutableImage::from_iter(
            texture.pixels.iter().cloned(),
            Dimensions::Dim2d {
                width: texture.width as u32,
                height: texture.height as u32,
            },
            vulkano::image::MipmapsCount::One,
            R8Unorm,
            self.gfx_queue.clone(),
        )?;

        self.cached_texture = Some(GuiTexture {
            version: texture.version,
            texture: img,
        });

        Ok(tex_future.boxed())
    }

    pub fn upload_texture(
        &mut self,
        texture: &egui::Texture,
    ) -> Option<Result<Box<dyn GpuFuture>>> {
        let cached_version = self.texture_version();
        if Some(texture.version) == cached_version {
            return None;
        }

        let future = self.force_upload_texture(texture);

        Some(future)
    }

    pub fn draw_egui_ctx(
        &self,
        dynamic_state: &DynamicState,
        clipped_meshes: &[egui::ClippedMesh],
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

        let screen_size_pc = vs::ty::ScreenSize {
            width: viewport_dims[0],
            height: viewport_dims[1],
        };

        let texture = self.cached_texture.as_ref().unwrap().texture.clone();

        let layout = self.pipeline.descriptor_set_layout(0).unwrap();
        let set = Arc::new(
            PersistentDescriptorSet::start(layout.clone())
                .add_sampled_image(texture.clone(), self.sampler.clone())
                .unwrap()
                .build()
                .unwrap(),
        );

        let mut vertices: Vec<GuiVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        for clipped in clipped_meshes.into_iter() {
            vertices.clear();
            indices.clear();

            let rect = &clipped.0;
            let mesh = &clipped.1;

            indices.extend(mesh.indices.iter().copied());

            vertices.extend(mesh.vertices.iter().map(|v| {
                let pos = [v.pos.x, v.pos.y];
                let uv = [v.uv.x, v.uv.y];
                let (r, g, b, a) = v.color.to_tuple();
                let color = [
                    (r as f32) / 255.0,
                    (g as f32) / 255.0,
                    (b as f32) / 255.0,
                    (a as f32) / 255.0,
                ];
                GuiVertex { pos, uv, color }
            }));

            let vertex_buffer = self
                .vertex_buffer_pool
                .chunk(vertices.iter().copied())
                .unwrap();
            let index_buffer = self
                .index_buffer_pool
                .chunk(indices.iter().copied())
                .unwrap();

            builder.draw_indexed(
                self.pipeline.clone(),
                &dynamic_state,
                vec![Arc::new(vertex_buffer.clone())],
                index_buffer.clone(),
                set.clone(),
                screen_size_pc,
            )?;
        }

        let builder = builder.build()?;

        Ok(builder)
    }
}
