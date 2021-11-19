use crate::geometry::{Point, Rect};
use crate::overlays::OverlayKind;
use crate::reactor::Reactor;
use crate::vulkan::texture::Texture;
use crate::vulkan::GpuTask;

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;

use crossbeam::atomic::AtomicCell;
use futures::future::RemoteHandle;
// use futures::lock::Mutex;
use handlegraph::handle::{Handle, NodeId};
use handlegraph::pathhandlegraph::PathId;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use parking_lot::Mutex;
use std::sync::Arc;

use crate::app::selection::SelectionBuffer;

use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

use super::{ComputeManager, ComputePipeline};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadState {
    Idle,
    Loading,
    ShouldReload,
}

#[allow(dead_code)]
pub struct PathViewRenderer {
    rgb_pipeline: ComputePipeline,
    val_pipeline: ComputePipeline,
    descriptor_set_layout: vk::DescriptorSetLayout,

    descriptor_pool: vk::DescriptorPool,
    buffer_desc_set: vk::DescriptorSet,

    pub width: usize,
    pub height: usize,

    left: AtomicCell<f32>,
    right: AtomicCell<f32>,
    state: Arc<AtomicCell<LoadState>>,
    reload: AtomicCell<bool>,
    should_rerender: Arc<AtomicCell<bool>>,

    // path_data: Vec<u32>,
    path_data: Arc<Mutex<Vec<u32>>>,
    path_count: Arc<AtomicCell<usize>>,

    path_buffer: vk::Buffer,
    path_allocation: vk_mem::Allocation,
    path_allocation_info: vk_mem::AllocationInfo,

    pub output_image: Texture,

    fence_id: AtomicCell<Option<usize>>,
}

impl PathViewRenderer {
    pub fn fence_id(&self) -> Option<usize> {
        self.fence_id.load()
    }

    pub fn block_on_fence(&self, comp_manager: &mut ComputeManager) {
        if let Some(fid) = self.fence_id.load() {
            comp_manager.block_on_fence(fid).unwrap();
            comp_manager.free_fence(fid, false).unwrap();
            self.fence_id.store(None);
        }
    }

    pub fn new(
        app: &GfaestusVk,
        rgb_overlay_desc_layout: vk::DescriptorSetLayout,
        val_overlay_desc_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let width = 2048;
        let height = 64;
        let size = width * height;

        let device = app.vk_context().device();

        let (path_buffer, path_allocation, path_allocation_info) = {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST;
            // | vk::BufferUsageFlags::TRANSFER_SRC;
            let memory_usage = vk_mem::MemoryUsage::CpuToGpu;

            let data = vec![0u32; size];

            let (buffer, allocation, allocation_info) =
                app.create_buffer_with_data(usage, memory_usage, true, &data)?;

            app.set_debug_object_name(
                buffer,
                "Path View Renderer (Path Buffer)",
            )?;

            (buffer, allocation, allocation_info)
        };

        dbg!();

        let output_image = {
            let format = vk::Format::R8G8B8A8_UNORM;

            let texture = Texture::allocate(
                app,
                app.transient_command_pool,
                app.graphics_queue,
                width,
                height,
                format,
                vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED,
            )?;

            texture
        };

        dbg!();

        let descriptor_pool = {
            let buffer_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                // ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 1,
            };

            let image_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                // ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 1,
            };

            let pool_sizes = [buffer_size, image_size];

            let pool_info = vk::DescriptorPoolCreateInfo::builder()
                .pool_sizes(&pool_sizes)
                .max_sets(2)
                .build();

            unsafe { device.create_descriptor_pool(&pool_info, None) }
        }?;

        dbg!();

        let descriptor_set_layout = Self::create_descriptor_set_layout(device)?;

        let descriptor_sets = {
            let layouts = vec![descriptor_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        dbg!();

        let buffer_desc_set = descriptor_sets[0];

        {
            let path_buf_info = vk::DescriptorBufferInfo::builder()
                .buffer(path_buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)
                .build();

            let path_buf_infos = [path_buf_info];

            let path_write = vk::WriteDescriptorSet::builder()
                .dst_set(buffer_desc_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&path_buf_infos)
                .build();

            let output_img_info = vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::GENERAL)
                .image_view(output_image.view)
                // .sampler(sampler)
                .build();
            let image_infos = [output_img_info];

            let output_write = vk::WriteDescriptorSet::builder()
                .dst_set(buffer_desc_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&image_infos)
                .build();

            let desc_writes = [path_write, output_write];

            unsafe { device.update_descriptor_sets(&desc_writes, &[]) };
        }

        dbg!();

        let pipeline_layout = {
            use vk::ShaderStageFlags as Flags;

            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(Flags::COMPUTE)
                .offset(0)
                .size(16)
                .build();

            let pc_ranges = [pc_range];
            // let pc_ranges = [];

            let layouts = [descriptor_set_layout, rgb_overlay_desc_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe { device.create_pipeline_layout(&layout_info, None) }
        }?;

        dbg!();

        let rgb_pipeline = ComputePipeline::new(
            device,
            descriptor_set_layout,
            pipeline_layout,
            crate::include_shader!("compute/path_view.comp.spv"),
        )?;

        let pipeline_layout = {
            use vk::ShaderStageFlags as Flags;

            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(Flags::COMPUTE)
                .offset(0)
                .size(16)
                .build();

            let pc_ranges = [pc_range];
            // let pc_ranges = [];

            let layouts = [descriptor_set_layout, val_overlay_desc_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe { device.create_pipeline_layout(&layout_info, None) }
        }?;

        dbg!();

        let val_pipeline = ComputePipeline::new(
            device,
            descriptor_set_layout,
            pipeline_layout,
            crate::include_shader!("compute/path_view_val.comp.spv"),
        )?;

        dbg!();

        Ok(Self {
            rgb_pipeline,
            val_pipeline,
            descriptor_set_layout,

            descriptor_pool,
            buffer_desc_set,

            width,
            height,

            left: AtomicCell::new(0.0),
            right: AtomicCell::new(1.0),
            state: Arc::new(AtomicCell::new(LoadState::Idle)),
            reload: AtomicCell::new(false),
            should_rerender: Arc::new(AtomicCell::new(false)),

            path_data: Arc::new(Mutex::new(Vec::with_capacity(width * height))),
            path_count: Arc::new(AtomicCell::new(0)),

            path_buffer,
            path_allocation,
            path_allocation_info,

            output_image,
            // output_buffer,
            // output_allocation,
            // output_allocation_info,
            fence_id: AtomicCell::new(None),
        })
    }

    pub fn reset_zoom(&self) {
        self.set_visible_range(0.0, 1.0);
    }

    pub fn set_visible_range(&self, left: f32, right: f32) {
        let l = left.min(right).clamp(0.0, 1.0);
        let r = left.max(right).clamp(0.0, 1.0);

        self.left.store(l);
        self.right.store(r);
    }

    pub fn pan(&self, pixel_delta: f32) {
        let l = self.left.load();
        let r = self.right.load();

        let len = r - l;

        let norm_delta = pixel_delta / (self.width as f32);

        log::warn!("norm_delta: {}", norm_delta);

        if norm_delta < 0.0 {
            let l_ = (l - norm_delta).clamp(0.0, 1.0);
            let r_ = (l_ + len).clamp(0.0, 1.0);

            self.left.store(l_);
            self.right.store(r_);
            self.reload.store(true);
            self.state.store(LoadState::ShouldReload);
        } else {
            let r_ = (r + norm_delta).clamp(0.0, 1.0);
            let l_ = (r_ - len).clamp(0.0, 1.0);

            self.left.store(l_);
            self.right.store(r_);
            self.reload.store(true);
            self.state.store(LoadState::ShouldReload);
        }
    }

    pub fn zoom(&self, delta: f32) {
        let delta = delta.clamp(-1.0, 1.0);

        let l = self.left.load();
        let r = self.right.load();

        let len = r - l;
        let mid = l + (len / 2.0);

        let len_ = len * delta;
        let rad = len_ / 2.0;

        let l_ = (mid - rad).clamp(0.0, 1.0);
        let r_ = (mid + rad).clamp(0.0, 1.0);

        if l_ != l || r_ != r {
            self.left.store(l_);
            self.right.store(r_);
            self.reload.store(true);
            self.state.store(LoadState::ShouldReload);
        }

        log::warn!("new zoom: {} - {}", l_, r_);
    }

    pub fn should_rerender(&self) -> bool {
        self.should_rerender.load()
    }

    pub fn state_should_reload(&self) -> bool {
        matches!(self.state.load(), LoadState::ShouldReload)
    }

    pub fn state_idle(&self) -> bool {
        matches!(self.state.load(), LoadState::Idle)
    }

    pub fn state_loading(&self) -> bool {
        matches!(self.state.load(), LoadState::Loading)
    }

    pub fn should_reload(&self) -> bool {
        self.reload.load()
    }

    pub fn load_paths_async(
        &self,
        app: &GfaestusVk,
        reactor: &mut Reactor,
        paths: impl IntoIterator<Item = PathId> + Send + Sync + 'static,
    ) -> Result<()> {
        // if self.load_paths_handle.is_some() {
        //     return Ok(());
        // }
        let left = self.left.load();
        let right = self.right.load();

        let graph = reactor.graph_query.clone();

        let width = self.width;
        let height = self.height;

        // let
        let gpu_tasks = reactor.gpu_tasks.clone();

        let buffer = self.path_buffer;

        let state_cell = self.state.clone();
        let should_rerender = self.should_rerender.clone();

        let path_data = self.path_data.clone();
        let path_count = self.path_count.clone();

        let fut = async move {
            //

            let mut path_data_local = Vec::with_capacity(width * height);

            let mut num_paths = 0;

            for path in paths.into_iter().take(64) {
                let steps = graph.path_pos_steps(path).unwrap();
                let (_, _, path_len) = steps.last().unwrap();

                num_paths += 1;

                let len = *path_len as f32;
                let start = left * len;
                let end = start + (right - left) * len;

                let s = start as usize;
                // let e = end as usize;

                for x in 0..width {
                    let n = (x as f64) / (width as f64);
                    let p_ = ((n as f32) * len) as usize;

                    let p = s + p_;

                    // let p = (n * (*path_len as f64)) as usize;

                    let ix =
                        match steps.binary_search_by_key(&p, |(_, _, p)| *p) {
                            Ok(i) => i,
                            Err(i) => i,
                        };

                    let ix = ix.min(steps.len() - 1);

                    let (handle, _step, _pos) = steps[ix];

                    path_data_local.push(handle.id().0 as u32);

                    // self.path_data.push(handle.id().0 as u32);
                }
            }

            {
                let mut lock = path_data.lock();
                *lock = path_data_local.clone();
                path_count.store(num_paths);
            }

            state_cell.store(LoadState::Loading);

            let data = Arc::new(path_data_local);
            let dst = buffer;
            let task = GpuTask::CopyDataToBuffer { data, dst };

            let copy_complete = gpu_tasks.queue_task(task);

            if let Ok(complete) = copy_complete {
                let _ = complete.await;
                // the path buffer has been updated here
                state_cell.store(LoadState::Idle);
                should_rerender.store(true);
            } else {
                log::warn!("error queing GPU task in load_paths");
                state_cell.store(LoadState::Idle);
            }
            // gpu_tasks.queue_task(
            // std::mem::swap(&mut lock, &mut path_data_local);
            // path_data_local
        };

        reactor.spawn_forget(fut)?;
        // let handle = reactor.spawn(fut)?;
        // self.load_paths_handle = Some(handle);

        // TODO for now hardcoded to max 64 paths
        /*
         */

        /*
        self.reload.store(false);
        if !self.path_data.is_empty() {
            app.copy_data_to_buffer::<u32, u32>(
                &self.path_data,
                self.path_buffer,
            )?;
        }
        */

        Ok(())
    }

    /*
    pub fn load_paths(
        &mut self,
        app: &GfaestusVk,
        reactor: &mut Reactor,
        paths: impl IntoIterator<Item = PathId>,
    ) -> Result<()> {
        self.path_data.clear();

        let left = self.left.load();
        let right = self.right.load();

        // TODO for now hardcoded to max 64 paths
        for path in paths.into_iter().take(64) {
            let steps = reactor.graph_query.path_pos_steps(path).unwrap();
            let (_, _, path_len) = steps.last().unwrap();

            let len = *path_len as f32;
            let start = left * len;
            let end = start + (right - left) * len;

            let s = start as usize;
            // let e = end as usize;

            for x in 0..self.width {
                let n = (x as f64) / (self.width as f64);
                let p_ = ((n as f32) * len) as usize;

                let p = s + p_;

                // let p = (n * (*path_len as f64)) as usize;

                let ix = match steps.binary_search_by_key(&p, |(_, _, p)| *p) {
                    Ok(i) => i,
                    Err(i) => i,
                };

                let ix = ix.min(steps.len() - 1);

                let (handle, _step, _pos) = steps[ix];

                self.path_data.push(handle.id().0 as u32);
            }
        }

        self.reload.store(false);
        if !self.path_data.is_empty() {
            app.copy_data_to_buffer::<u32, u32>(
                &self.path_data,
                self.path_buffer,
            )?;
        }

        Ok(())
    }
    */

    pub fn get_node_at(&self, x: usize, y: usize) -> Option<NodeId> {
        let ix = y * self.width + x;

        let raw = self.path_data.try_lock().and_then(|l| l.get(x).copied())?;

        if raw == 0 {
            return None;
        }

        let id = raw + 1;
        let node = NodeId::from(id as u64);

        Some(node)
    }

    pub fn running(&self, comp_manager: &mut ComputeManager) -> Result<bool> {
        if let Some(fid) = self.fence_id.load() {
            let is_ready = comp_manager.is_fence_ready(fid)?;
            Ok(!is_ready)
        } else {
            Ok(false)
        }
    }

    pub fn dispatch_complete(
        &self,
        comp_manager: &mut ComputeManager,
    ) -> Result<bool> {
        dbg!();
        if let Some(fid) = self.fence_id.load() {
            dbg!();
            if comp_manager.is_fence_ready(fid)? {
                dbg!();
                comp_manager.block_on_fence(fid).unwrap();
                comp_manager.free_fence(fid, false).unwrap();
                self.fence_id.store(None);

                Ok(true)
            } else {
                dbg!();
                Ok(false)
            }
        } else {
            dbg!();
            Ok(false)
        }
    }

    pub fn dispatch_managed(
        &mut self,
        comp_manager: &mut ComputeManager,
        app: &GfaestusVk,
        rgb_overlay_desc: vk::DescriptorSet,
        val_overlay_desc: vk::DescriptorSet,
        overlay_kind: OverlayKind,
    ) -> Result<()> {
        if !self.state_idle() {
            return Ok(());
        }

        if let Some(fid) = self.fence_id.load() {
            dbg!();
            // handle this, but how
        } else {
            let path_count = self.path_count.load();
            dbg!();
            self.should_rerender.store(false);
            let fence_id = comp_manager.dispatch_with(|device, cmd_buf| {
                let (barrier, src_stage, dst_stage) =
                    GfaestusVk::image_transition_barrier(
                        self.output_image.image,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        vk::ImageLayout::GENERAL,
                    );

                unsafe {
                    device.cmd_pipeline_barrier(
                        cmd_buf,
                        src_stage,
                        dst_stage,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[barrier],
                    )
                };

                match overlay_kind {
                    OverlayKind::RGB => {
                        self.dispatch_cmd_rgb(
                            cmd_buf,
                            app,
                            rgb_overlay_desc,
                            path_count,
                        )
                        .unwrap();
                    }
                    OverlayKind::Value => {
                        self.dispatch_cmd_val(
                            cmd_buf,
                            app,
                            val_overlay_desc,
                            path_count,
                        )
                        .unwrap();
                    }
                }

                let (barrier, src_stage, dst_stage) =
                    GfaestusVk::image_transition_barrier(
                        self.output_image.image,
                        vk::ImageLayout::GENERAL,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    );

                unsafe {
                    device.cmd_pipeline_barrier(
                        cmd_buf,
                        src_stage,
                        dst_stage,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[barrier],
                    )
                };
            })?;

            self.fence_id.store(Some(fence_id));
        }

        Ok(())
    }

    pub fn dispatch_cmd_val(
        &self,
        cmd_buf: vk::CommandBuffer,
        app: &GfaestusVk,
        val_overlay_desc: vk::DescriptorSet,
        path_count: usize,
    ) -> Result<()> {
        log::warn!("in dispatch()");
        let device = app.vk_context().device();

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.val_pipeline.pipeline,
            );

            let desc_sets = [self.buffer_desc_set, val_overlay_desc];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.val_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=1],
                &null,
            );

            let push_constants = [
                path_count as u32,
                self.width as u32,
                self.height as u32,
                0u32,
            ];

            let pc_bytes = bytemuck::cast_slice(&push_constants);

            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.val_pipeline.pipeline_layout,
                Flags::COMPUTE,
                0,
                pc_bytes,
            )
        };

        let x_group_count = self.width / 256;
        // let y_group_count = path_count;
        let y_group_count = 64;
        let z_group_count = 1;

        unsafe {
            device.cmd_dispatch(
                cmd_buf,
                x_group_count as u32,
                y_group_count as u32,
                z_group_count as u32,
            )
        };

        Ok(())
    }

    pub fn dispatch_cmd_rgb(
        &self,
        cmd_buf: vk::CommandBuffer,
        app: &GfaestusVk,
        rgb_overlay_desc: vk::DescriptorSet,
        path_count: usize,
    ) -> Result<()> {
        log::warn!("in dispatch()");
        let device = app.vk_context().device();

        unsafe {
            device.cmd_bind_pipeline(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.rgb_pipeline.pipeline,
            );

            let desc_sets = [self.buffer_desc_set, rgb_overlay_desc];

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::COMPUTE,
                self.rgb_pipeline.pipeline_layout,
                0,
                &desc_sets[0..=1],
                &null,
            );

            let push_constants = [
                path_count as u32,
                self.width as u32,
                self.height as u32,
                0u32,
            ];

            let pc_bytes = bytemuck::cast_slice(&push_constants);

            use vk::ShaderStageFlags as Flags;
            device.cmd_push_constants(
                cmd_buf,
                self.rgb_pipeline.pipeline_layout,
                Flags::COMPUTE,
                0,
                pc_bytes,
            )
        };

        let x_group_count = self.width / 256;
        // let y_group_count = path_count;
        let y_group_count = 64;
        let z_group_count = 1;

        unsafe {
            device.cmd_dispatch(
                cmd_buf,
                x_group_count as u32,
                y_group_count as u32,
                z_group_count as u32,
            )
        };

        Ok(())
    }

    fn layout_binding() -> [vk::DescriptorSetLayoutBinding; 2] {
        use vk::ShaderStageFlags as Stages;

        let path_buffer = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build();

        let output_image = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build();

        // let output_buffer = vk::DescriptorSetLayoutBinding::builder()
        //     .binding(1)
        //     .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        //     .descriptor_count(1)
        //     .stage_flags(Stages::COMPUTE)
        //     .build();

        // let overlay_sampler = vk::DescriptorSetLayoutBinding::builder()
        //     .binding(2)
        //     .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        //     .descriptor_count(1)
        //     .stage_flags(Stages::COMPUTE)
        //     .build();

        // [path_buffer, output_buffer, overlay_sampler]
        [path_buffer, output_image]
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let bindings = Self::layout_binding();

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout =
            unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        Ok(layout)
    }
}
