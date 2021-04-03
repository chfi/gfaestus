use handlegraph::handle::NodeId;

use rustc_hash::{FxHashMap, FxHashSet};

use ash::{
    extensions::{
        ext::DebugReport,
        khr::{Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
};
use ash::{vk, Device, Entry, Instance};

use anyhow::Result;

use crate::vulkan::GfaestusVk;

// use vulkano::buffer::cpu_access::{
//     CpuAccessibleBuffer, ReadLock, ReadLockError, WriteLock, WriteLockError,
// };

/// Bitflags for controlling display options on a per-node basis
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum NodeFlag {
    None = 0b0,
    Selected = 0b1,
    // SeqHash = 0b10,
    // Coverage = 0b100,
    // Highlight = 0b1000,
}

/// A collection of [`NodeFlag`] bitflags for a single node
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(transparent)]
pub struct NodeFlags(u8);

pub struct SelectionBuffer {
    latest_selection: FxHashSet<NodeId>,
    node_count: usize,

    pub buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
}

impl SelectionBuffer {
    pub fn new(app: &GfaestusVk, node_count: usize) -> Result<Self> {
        let size =
            ((node_count * std::mem::size_of::<u8>()) as u32) as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_COHERENT;

        let (buffer, memory, size) =
            app.create_buffer(size, usage, mem_props)?;

        let latest_selection = FxHashSet::default();

        let device = app.vk_context().device().clone();

        Ok(Self {
            latest_selection,
            node_count,

            buffer,
            memory,
            size,

            device,
        })
    }

    pub fn destroy(&mut self, device: &Device) {
        unsafe {
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }

        self.latest_selection.clear();
        self.buffer = vk::Buffer::null();
        self.memory = vk::DeviceMemory::null();
        self.size = 0 as vk::DeviceSize;
    }

    pub fn clear(&mut self) {
        self.latest_selection.clear();
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct LayoutFlags {
    // latest_flags: Vec<(NodeId, NodeFlag)>,
    latest_selection: FxHashSet<NodeId>,
    // selection_buffer: CpuAccessibleBuffer,

    // latest_flags: FxHashMap<NodeId, NodeFlag>,
}

/// Instruction for updating the flags of a single node
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlagUpdate {
    node: NodeId,
    add: NodeFlags,
    remove: NodeFlags,
}

impl LayoutFlags {
    /*
    pub fn update_flags(
        &mut self,
        new_flags: &FxHashMap<NodeId, NodeFlag>,
        buffer: &CpuAccessibleBuffer<[u32]>,
    ) -> Result<(), WriteLockError> {
        let latest_keys = self.latest_flags.keys().collect::<FxHashSet<_>>();
        let new_keys = new_flags.keys().collect::<FxHashSet<_>>();
        let removed = latest_keys.difference(&new_keys);
        let added = new_keys.difference(&latest_keys);
        {
            let mut buf = buffer.write()?;
            for &node in removed {
                let ix = node.0 as usize;
                buf[ix] = 0;
            }
            for &node in added {
                let ix = node.0 as usize;
                let value = *new_flags.get(&node).unwrap() as u32;
                buf[ix] = value;
            }
        }
        self.latest_flags.clone_from(new_flags);
        Ok(())
    }
    */

    pub fn clear(&mut self) {
        self.latest_selection.clear()
    }

    pub fn clear_buffer(
        &mut self,
        buffer: &CpuAccessibleBuffer<[u32]>,
    ) -> Result<(), WriteLockError> {
        let mut buf = buffer.write()?;

        for ix in 0..buf.len() {
            buf[ix] = 0;
        }

        Ok(())
    }

    pub fn add_select_one(
        &mut self,
        node: NodeId,
        buffer: &CpuAccessibleBuffer<[u32]>,
    ) -> Result<(), WriteLockError> {
        if self.latest_selection.insert(node) {
            let mut buf = buffer.write()?;
            let ix = (node.0 - 1) as usize;
            buf[ix] = 1;
        }
        Ok(())
    }

    pub fn write_latest_buffer(
        &self,
        buffer: &CpuAccessibleBuffer<[u32]>,
    ) -> Result<(), WriteLockError> {
        let mut buf = buffer.write()?;

        for ix in 0..buf.len() {
            let node = NodeId::from((ix + 1) as u64);
            if self.latest_selection.contains(&node) {
                buf[ix] = 1;
            } else {
                buf[ix] = 0;
            }
        }

        Ok(())
    }

    pub fn update_selection(
        &mut self,
        new_selection: &FxHashSet<NodeId>,
        buffer: &CpuAccessibleBuffer<[u32]>,
    ) -> Result<(), WriteLockError> {
        let removed = self.latest_selection.difference(new_selection);
        let added = new_selection.difference(&self.latest_selection);

        {
            let mut buf = buffer.write()?;

            for &node in removed {
                let ix = (node.0 - 1) as usize;
                buf[ix] = 0;
            }

            for &node in added {
                let ix = (node.0 - 1) as usize;
                buf[ix] = 1;
            }
        }

        self.latest_selection.clone_from(new_selection);

        Ok(())
    }
}

impl std::default::Default for NodeFlag {
    fn default() -> Self {
        Self::None
    }
}

impl From<NodeFlag> for NodeFlags {
    fn from(flag: NodeFlag) -> Self {
        NodeFlags(flag as u8)
    }
}

impl std::ops::BitOr<NodeFlag> for NodeFlags {
    type Output = Self;

    fn bitor(self, rhs: NodeFlag) -> Self::Output {
        NodeFlags(self.0 | rhs as u8)
    }
}

impl std::ops::BitAnd<NodeFlag> for NodeFlags {
    type Output = Self;

    fn bitand(self, rhs: NodeFlag) -> Self::Output {
        NodeFlags(self.0 & rhs as u8)
    }
}

impl std::ops::BitOrAssign<NodeFlag> for NodeFlags {
    fn bitor_assign(&mut self, rhs: NodeFlag) {
        self.0 |= rhs as u8;
    }
}

impl std::ops::BitAndAssign<NodeFlag> for NodeFlags {
    fn bitand_assign(&mut self, rhs: NodeFlag) {
        self.0 &= rhs as u8;
    }
}
