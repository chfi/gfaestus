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

pub struct SelectionBuffer {
    latest_selection: FxHashSet<NodeId>,
    node_count: usize,

    pub buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    pub size: vk::DeviceSize,
}

impl SelectionBuffer {
    pub fn new(app: &GfaestusVk, node_count: usize) -> Result<Self> {
        let size = ((node_count * std::mem::size_of::<u32>()) as u32)
            as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::TRANSFER_SRC
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_CACHED
            | vk::MemoryPropertyFlags::HOST_COHERENT;

        let (buffer, memory, size) =
            app.create_buffer(size, usage, mem_props)?;

        let latest_selection = FxHashSet::default();

        Ok(Self {
            latest_selection,
            node_count,

            buffer,
            memory,
            size,
        })
    }

    pub fn selection_set(&self) -> &FxHashSet<NodeId> {
        &self.latest_selection
    }

    /// fill `latest_selection` by reading from the buffer
    pub fn fill_selection_set(&mut self, device: &Device) -> Result<()> {
        let node_count = (self.size / 4) as usize;
        println!("selection buffer node_count {}", node_count);

        self.latest_selection.clear();
        self.latest_selection.reserve(node_count);

        unsafe {
            let data_ptr = device.map_memory(
                self.memory,
                0,
                self.size,
                vk::MemoryMapFlags::empty(),
            )?;

            let val_ptr = data_ptr as *const u32;
            let sel_slice = std::slice::from_raw_parts(val_ptr, node_count);

            self.latest_selection.extend(
                sel_slice.iter().enumerate().filter_map(|(ix, &val)| {
                    let node_id = NodeId::from((ix + 1) as u64);
                    if val == 1 {
                        Some(node_id)
                    } else {
                        None
                    }
                }),
            );

            device.unmap_memory(self.memory);
        }

        self.latest_selection.shrink_to_fit();

        Ok(())
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

    pub fn clear_buffer(&mut self, device: &Device) -> Result<()> {
        unsafe {
            let data_ptr = device.map_memory(
                self.memory,
                0,
                self.size,
                vk::MemoryMapFlags::empty(),
            )?;

            let val_ptr = data_ptr as *mut u32;
            std::ptr::write_bytes(val_ptr, 0u8, (self.size / 4) as usize);

            device.unmap_memory(self.memory);
        }

        Ok(())
    }

    pub fn add_select_one(
        &mut self,
        device: &Device,
        node: NodeId,
    ) -> Result<()> {
        if self.latest_selection.insert(node) {
            unsafe {
                let data_ptr = device.map_memory(
                    self.memory,
                    0,
                    self.size,
                    vk::MemoryMapFlags::empty(),
                )?;

                let val_ptr = data_ptr as *mut u32;
                let ix = (node.0 - 1) as usize;

                if ix >= (self.size / 4) as usize {
                    panic!("attempted to select a node that does not exist");
                }

                let val_ptr = val_ptr.add(ix);
                // let val_ptr = val_ptr.add(2);
                val_ptr.write(1);

                device.unmap_memory(self.memory);
            }
        }

        Ok(())
    }

    pub fn write_latest_buffer(&mut self, device: &Device) -> Result<()> {
        unsafe {
            let data_ptr = device.map_memory(
                self.memory,
                0,
                self.size,
                vk::MemoryMapFlags::empty(),
            )?;

            let val_ptr = data_ptr as *mut u32;

            for ix in 0..self.size {
                let node = NodeId::from((ix + 1) as u64);

                let val_ptr = val_ptr.add(1);

                if self.latest_selection.contains(&node) {
                    val_ptr.write(1);
                } else {
                    val_ptr.write(0);
                }
            }

            device.unmap_memory(self.memory);
        }

        Ok(())
    }

    pub fn update_selection(
        &mut self,
        device: &Device,
        new_selection: &FxHashSet<NodeId>,
    ) -> Result<()> {
        let removed = self.latest_selection.difference(new_selection);
        let added = new_selection.difference(&self.latest_selection);

        unsafe {
            let data_ptr = device.map_memory(
                self.memory,
                0,
                self.size,
                vk::MemoryMapFlags::empty(),
            )?;

            for &node in removed {
                let val_ptr = data_ptr as *mut u32;
                let ix = (node.0 - 1) as usize;

                if ix >= (self.size / 4) as usize {
                    panic!("attempted to deselect a node that does not exist");
                }

                let val_ptr = val_ptr.add(ix);
                val_ptr.write(0);
            }

            for &node in added {
                let val_ptr = data_ptr as *mut u32;
                let ix = (node.0 - 1) as usize;

                if ix >= (self.size / 4) as usize {
                    panic!("attempted to select a node that does not exist");
                }

                let val_ptr = val_ptr.add(ix);
                val_ptr.write(1);
            }

            device.unmap_memory(self.memory);
        }

        self.latest_selection.clone_from(new_selection);

        Ok(())
    }
}
