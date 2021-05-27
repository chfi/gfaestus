use crate::geometry::{Point, Rect};

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;

use crate::app::node_flags::SelectionBuffer;

use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

use super::{ComputeManager, ComputePipeline};

pub struct EdgeRenderer {}

pub struct EdgeBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,

    edge_count: usize,
}

impl EdgeBuffer {
    pub fn new(app: &GfaestusVk, edge_count: usize) -> Result<Self> {
        let size = ((edge_count * 2 * std::mem::size_of::<u32>()) as u32)
            as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::TRANSFER_SRC
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_CACHED
            | vk::MemoryPropertyFlags::HOST_COHERENT;

        let (buffer, memory, size) =
            app.create_buffer(size, usage, mem_props)?;

        // let latest_selection = FxHashSet::default();

        Ok(Self {
            // latest_selection,
            // node_count,
            buffer,
            memory,
            size,

            edge_count,
        })
    }
}
