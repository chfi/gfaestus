use crate::geometry::{Point, Rect};

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;

use crate::app::node_flags::SelectionBuffer;

use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

use super::{ComputeManager, ComputePipeline};

pub struct ComputeBuffer {
    pub(super) buffer: vk::Buffer,
    pub(super) memory: vk::DeviceMemory,
    pub(super) size: vk::DeviceSize,

    pub(super) element_count: usize,
}

impl ComputeBuffer {
    pub fn new<T>(app: &GfaestusVk, element_count: usize) -> Result<Self> {
        let size = ((element_count * std::mem::size_of::<T>()) as u32)
            as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::TRANSFER_SRC
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::DEVICE_LOCAL;

        let (buffer, memory, size) =
            app.create_buffer(size, usage, mem_props)?;

        // let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
        //     | vk::MemoryPropertyFlags::HOST_CACHED
        //     | vk::MemoryPropertyFlags::HOST_COHERENT;

        Ok(Self {
            buffer,
            memory,
            size,

            element_count,
        })
    }
}

pub struct BinBuffers {
    node_bins: ComputeBuffer,
    node_bin_offsets: ComputeBuffer,
    bin_offsets: ComputeBuffer,
    bins: ComputeBuffer,
}

impl BinBuffers {
    fn new(
        app: &GfaestusVk,
        node_count: usize,
        bin_count: usize,
    ) -> Result<Self> {
        // node_bins maps node ends to bin ID, i.e. index in `bins`
        let node_bins = ComputeBuffer::new::<u32>(app, node_count * 2)?;

        // node_bin_offsets maps node ends to offset in bin, in `bins`
        let node_bin_offsets = ComputeBuffer::new::<u32>(app, node_count * 2)?;

        // bin_offsets has the start index and length of each bin in `bins`
        let bin_offsets = ComputeBuffer::new::<u32>(app, bin_count)?;

        // bins has node end index for each bin, in order
        let bins = ComputeBuffer::new::<u32>(app, node_count * 2)?;

        Ok(Self {
            node_bins,
            node_bin_offsets,
            bin_offsets,
            bins,
        })
    }
}

// pub struct ScreenBins {

// }
