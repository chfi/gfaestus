#[allow(unused_imports)]
use vulkano::buffer::{
    BufferUsage, CpuAccessibleBuffer, CpuBufferPool, ImmutableBuffer,
};
use vulkano::{
    buffer::BufferView,
    command_buffer::{
        AutoCommandBuffer, AutoCommandBufferBuilder, DynamicState,
    },
    descriptor::descriptor_set::PersistentDescriptorSetBufView,
    format::R8G8B8A8Unorm,
    image::{Dimensions, ImmutableImage, MipmapsCount},
    sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode},
    sync::GpuFuture,
};
use vulkano::{
    descriptor::descriptor_set::UnsafeDescriptorSetLayout, device::Queue,
};
use vulkano::{
    descriptor::descriptor_set::{
        PersistentDescriptorSet, PersistentDescriptorSetBuf,
        PersistentDescriptorSetImg, PersistentDescriptorSetSampler,
    },
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

use crate::app::theme::*;

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

type SelectionDataDescSet = PersistentDescriptorSet<(
    (
        (),
        PersistentDescriptorSetBuf<Arc<CpuAccessibleBuffer<[u32]>>>,
    ),
    PersistentDescriptorSetBuf<Arc<CpuAccessibleBuffer<[u32]>>>,
)>;

struct NodeDrawCache {
    cached_vertex_buffer: Option<Arc<super::PoolChunk<Vertex>>>,

    node_id_color_buffer: Option<Arc<CpuAccessibleBuffer<[u32]>>>,
    node_selection_buffer: Option<Arc<CpuAccessibleBuffer<[u32]>>>,

    descriptor_set: Option<Arc<SelectionDataDescSet>>,
}

impl std::default::Default for NodeDrawCache {
    fn default() -> Self {
        Self {
            cached_vertex_buffer: None,

            node_id_color_buffer: None,
            node_selection_buffer: None,

            descriptor_set: None,
        }
    }
}

impl NodeDrawCache {
    fn build_descriptor_set(
        &mut self,
        layout: &Arc<UnsafeDescriptorSetLayout>,
    ) -> Result<&Arc<SelectionDataDescSet>> {
        let node_id_buf = self.node_id_color_buffer.as_ref().unwrap().clone();
        let select_buf = self.node_selection_buffer.as_ref().unwrap().clone();

        let set = PersistentDescriptorSet::start(layout.clone())
            .add_buffer(node_id_buf)?
            .add_buffer(select_buf)?;

        let set = set.build()?;

        self.descriptor_set = Some(Arc::new(set));

        Ok(&self.descriptor_set.as_ref().unwrap())
    }

    fn descriptor_set(&self) -> Option<&Arc<SelectionDataDescSet>> {
        self.descriptor_set.as_ref()
    }

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
            uniform_buffer: false,
            storage_buffer: true,
            index_buffer: false,
            vertex_buffer: false,
            indirect_buffer: false,
            device_address: false,
        };

        let data_iter = (0..node_count).map(|_| 0u32);

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

type ThemeDescSet = PersistentDescriptorSet<(
    (
        (
            (),
            PersistentDescriptorSetBuf<Arc<CpuAccessibleBuffer<i32>>>,
        ),
        PersistentDescriptorSetImg<Arc<ImmutableImage<R8G8B8A8Unorm>>>,
    ),
    PersistentDescriptorSetSampler,
)>;

type OverlayDescSet = PersistentDescriptorSet<(
    (),
    PersistentDescriptorSetBufView<
        Arc<BufferView<R8G8B8A8Unorm, Arc<ImmutableBuffer<[[u8; 4]]>>>>,
    >,
)>;

struct CachedTheme {
    color_hash: u64,
    _width_buf: Arc<CpuAccessibleBuffer<i32>>,
    descriptor_set: Arc<ThemeDescSet>,
}

impl CachedTheme {
    fn build_descriptor_set(
        queue: &Arc<Queue>,
        layout: &Arc<UnsafeDescriptorSetLayout>,
        sampler: Arc<Sampler>,
        theme: &Theme,
    ) -> Result<Self> {
        let width = theme.width();

        let width_buf = CpuAccessibleBuffer::from_data(
            queue.device().clone(),
            BufferUsage::uniform_buffer(),
            false,
            width as i32,
        )?;

        let set: ThemeDescSet = PersistentDescriptorSet::start(layout.clone())
            .add_buffer(width_buf.clone())?
            .add_sampled_image(theme.texture().clone(), sampler)?
            .build()?;

        let color_hash = theme.color_hash();

        Ok(Self {
            color_hash,
            _width_buf: width_buf,
            descriptor_set: Arc::new(set),
        })
    }

    fn get_set(&self) -> &Arc<ThemeDescSet> {
        &self.descriptor_set
    }
}

#[derive(Default)]
struct ThemeCache {
    primary: Option<CachedTheme>,
    secondary: Option<CachedTheme>,
}

impl ThemeCache {
    fn get_theme_set(&self, id: ThemeId) -> Option<&Arc<ThemeDescSet>> {
        match id {
            ThemeId::Primary => self.primary.as_ref().map(|t| t.get_set()),
            ThemeId::Secondary => self.secondary.as_ref().map(|t| t.get_set()),
        }
    }

    fn theme_hash(&self, id: ThemeId) -> Option<u64> {
        let theme = match id {
            ThemeId::Primary => self.primary.as_ref(),
            ThemeId::Secondary => self.secondary.as_ref(),
        };

        theme.map(|t| t.color_hash)
    }

    fn set_theme(
        &mut self,
        queue: &Arc<Queue>,
        layout: &Arc<UnsafeDescriptorSetLayout>,
        sampler: &Arc<Sampler>,
        theme_id: ThemeId,
        theme: &Theme,
    ) -> Result<()> {
        let theme = CachedTheme::build_descriptor_set(
            queue,
            layout,
            sampler.clone(),
            theme,
        )?;

        match theme_id {
            ThemeId::Primary => self.primary = Some(theme),
            ThemeId::Secondary => self.secondary = Some(theme),
        }

        Ok(())
    }

    fn fill(
        &mut self,
        queue: &Arc<Queue>,
        layout: &Arc<UnsafeDescriptorSetLayout>,
        sampler: &Arc<Sampler>,
        primary: &Theme,
        secondary: &Theme,
    ) -> Result<()> {
        let primary = CachedTheme::build_descriptor_set(
            queue,
            layout,
            sampler.clone(),
            primary,
        )?;
        let secondary = CachedTheme::build_descriptor_set(
            queue,
            layout,
            sampler.clone(),
            secondary,
        )?;

        self.primary = Some(primary);
        self.secondary = Some(secondary);

        Ok(())
    }
}

pub struct OverlayCache {
    overlay_buf: Arc<ImmutableBuffer<[[u8; 4]]>>,
    overlay_buf_view:
        Arc<BufferView<R8G8B8A8Unorm, Arc<ImmutableBuffer<[[u8; 4]]>>>>,
    descriptor_set: Arc<OverlayDescSet>,
}

impl OverlayCache {
    pub fn from_node_colors<C>(
        queue: &Arc<Queue>,
        layout: &Arc<UnsafeDescriptorSetLayout>,
        colors: C,
    ) -> Result<(Self, Box<dyn GpuFuture>)>
    where
        C: Iterator<Item = rgb::RGB<f32>>,
    {
        let mut img_colors = Vec::new();

        for color in colors {
            let r = (255.0 * color.r).floor() as u8;
            let g = (255.0 * color.g).floor() as u8;
            let b = (255.0 * color.b).floor() as u8;
            let a = 255u8;

            img_colors.push([r, g, b, a]);
        }

        let buffer_usage = BufferUsage {
            transfer_source: false,
            transfer_destination: false,
            uniform_texel_buffer: true,
            storage_texel_buffer: false,
            uniform_buffer: false,
            storage_buffer: false,
            index_buffer: false,
            vertex_buffer: false,
            indirect_buffer: false,
            device_address: false,
        };

        let (overlay_buf, future) = ImmutableBuffer::from_iter(
            img_colors.into_iter(),
            buffer_usage,
            queue.clone(),
        )?;

        let overlay_buf_view =
            Arc::new(BufferView::new(overlay_buf.clone(), R8G8B8A8Unorm)?);

        let set: OverlayDescSet =
            PersistentDescriptorSet::start(layout.clone())
                .add_buffer_view(overlay_buf_view.clone())?
                .build()?;

        Ok((
            Self {
                overlay_buf,
                overlay_buf_view,
                descriptor_set: Arc::new(set),
            },
            future.boxed(),
        ))
    }
}

pub struct NodeDrawSystem {
    gfx_queue: Arc<Queue>,
    vertex_buffer_pool: CpuBufferPool<Vertex>,
    rect_pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    caches: Mutex<NodeDrawCache>,

    theme_cache: Mutex<ThemeCache>,
    empty_overlay: OverlayCache,
}

impl NodeDrawSystem {
    pub fn new<R>(
        gfx_queue: Arc<Queue>,
        subpass: Subpass<R>,
    ) -> Result<(NodeDrawSystem, Box<dyn GpuFuture>)>
    where
        R: RenderPassAbstract + Clone + Send + Sync + 'static,
    {
        let _ = include_str!("../../shaders/nodes/vertex.vert");
        let _ = include_str!("../../shaders/nodes/geometry.geom");
        let _ = include_str!("../../shaders/nodes/fragment.frag");

        let vs = vs::Shader::load(gfx_queue.device().clone())?;
        let fs = fs::Shader::load(gfx_queue.device().clone())?;
        let gs = gs::Shader::load(gfx_queue.device().clone())?;

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
                    .build(gfx_queue.device().clone())?,
            ) as Arc<dyn GraphicsPipelineAbstract + Send + Sync>
        };

        let layout = rect_pipeline.descriptor_set_layout(2).unwrap();

        let (empty_overlay, future) = OverlayCache::from_node_colors(
            &gfx_queue,
            layout,
            std::iter::empty(),
        )?;

        Ok((
            NodeDrawSystem {
                gfx_queue,
                vertex_buffer_pool,
                rect_pipeline,
                caches: Mutex::new(Default::default()),
                theme_cache: Mutex::new(Default::default()),
                empty_overlay,
            },
            future,
        ))
    }

    pub fn prepare_themes(
        &self,
        sampler: &Arc<Sampler>,
        primary: &Theme,
        secondary: &Theme,
    ) -> Result<()> {
        let mut theme_cache = self.theme_cache.lock();

        let layout = self.rect_pipeline.descriptor_set_layout(0).unwrap();

        theme_cache.fill(
            &self.gfx_queue,
            &layout,
            sampler,
            primary,
            secondary,
        )?;

        Ok(())
    }

    pub fn cached_theme_hash(&self, theme_id: ThemeId) -> Option<u64> {
        let theme_cache = self.theme_cache.lock();
        theme_cache.theme_hash(theme_id)
    }

    pub fn set_theme(
        &self,
        sampler: &Arc<Sampler>,
        theme_id: ThemeId,
        theme: &Theme,
    ) -> Result<()> {
        let mut theme_cache = self.theme_cache.lock();

        let layout = self.rect_pipeline.descriptor_set_layout(0).unwrap();

        theme_cache.set_theme(
            &self.gfx_queue,
            &layout,
            sampler,
            theme_id,
            theme,
        )?;

        Ok(())
    }

    pub fn build_overlay_cache<I>(
        &self,
        colors: I,
    ) -> Result<(OverlayCache, Box<dyn GpuFuture>)>
    where
        I: Iterator<Item = rgb::RGB<f32>>,
    {
        let layout = self.rect_pipeline.descriptor_set_layout(2).unwrap();

        OverlayCache::from_node_colors(&self.gfx_queue, &layout, colors)
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

    pub fn update_node_selection<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(&CpuAccessibleBuffer<[u32]>) -> Result<()>,
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
        theme: ThemeId,
        overlay: Option<&OverlayCache>,
    ) -> Result<&'a mut AutoCommandBufferBuilder>
    where
        VI: IntoIterator<Item = Vertex>,
        VI::IntoIter: ExactSizeIterator,
    {
        let viewport_dims = {
            let viewport = dynamic_state
                .viewports
                .as_ref()
                .and_then(|v| v.get(0))
                .unwrap();
            viewport.dimensions
        };

        let flags = if overlay.is_some() { 1u32 } else { 0u32 };

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
                flags,
            }
        };

        let mut recreate_desc_set = false;

        {
            let mut cache_lock = self.caches.lock();

            let cache_buf_len = if let Some(buffer) =
                cache_lock.node_id_color_buffer.as_ref()
            {
                let buf = buffer.read()?;
                buf.len()
            } else {
                0
            };

            let mut cleared = false;

            if cache_buf_len
                != (viewport_dims[0] as usize) * (viewport_dims[1] as usize)
            {
                let buffer_usage = BufferUsage {
                    storage_buffer: true,
                    ..BufferUsage::none()
                };

                let data_iter = (0..((viewport_dims[0] as u32)
                    * (viewport_dims[1] as u32)))
                    .map(|_| 0u32);

                let buffer = CpuAccessibleBuffer::from_iter(
                    self.gfx_queue.device().clone(),
                    buffer_usage,
                    false,
                    data_iter,
                )?;

                recreate_desc_set = true;
                cache_lock.node_id_color_buffer = Some(buffer.clone());

                cleared = true;
            }

            let buffer = cache_lock.node_id_color_buffer.as_ref().unwrap();

            if !cleared {
                let mut buf = buffer.write()?;

                for ix in 0..buf.len() {
                    buf[ix] = 0;
                }
            }
        };

        let vertex_buffer = {
            let mut cache_lock = self.caches.lock();

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

        let layout = self.rect_pipeline.descriptor_set_layout(1).unwrap();

        let set_0 = {
            let theme_cache = self.theme_cache.lock();

            if let Some(set) = theme_cache.get_theme_set(theme) {
                set.clone()
            } else {
                panic!("Tried to draw nodes using unavailable theme");
            }
        };

        let set_1 = {
            let mut cache_lock = self.caches.lock();
            if recreate_desc_set {
                cache_lock.build_descriptor_set(layout)?;
            }

            cache_lock.descriptor_set().unwrap().clone()
        };

        let set_2 = if let Some(overlay_cache) = overlay {
            overlay_cache.descriptor_set.clone()
        } else {
            self.empty_overlay.descriptor_set.clone()
        };

        builder.draw(
            self.rect_pipeline.clone(),
            &dynamic_state,
            vec![vertex_buffer],
            (set_0.clone(), set_1.clone(), set_2.clone()),
            view_pc,
        )?;

        Ok(builder)
    }

    pub fn draw<VI>(
        &self,
        dynamic_state: &DynamicState,
        vertices: Option<VI>,
        view: View,
        offset: Point,
        node_width: f32,
        theme: ThemeId,
        overlay: Option<&OverlayCache>,
        // use_lines: bool,
    ) -> Result<AutoCommandBuffer>
    where
        VI: IntoIterator<Item = Vertex>,
        VI::IntoIter: ExactSizeIterator,
    {
        let mut builder: AutoCommandBufferBuilder =
            AutoCommandBufferBuilder::secondary_graphics(
                self.gfx_queue.device().clone(),
                self.gfx_queue.family(),
                self.rect_pipeline.clone().subpass(),
            )?;

        self.draw_primary(
            &mut builder,
            dynamic_state,
            vertices,
            view,
            offset,
            node_width,
            theme, // use_lines,
            overlay,
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }
}
