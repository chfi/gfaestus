use handlegraph::handle::NodeId;

use rustc_hash::{FxHashMap, FxHashSet};

use vulkano::buffer::cpu_access::{
    CpuAccessibleBuffer, ReadLock, ReadLockError, WriteLock, WriteLockError,
};
//     BufferUsage, CpuAccessibleBuffer, CpuBufferPool, ImmutableBuffer,
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

#[derive(Debug, Default, Clone, PartialEq)]
pub struct LayoutFlags {
    // latest_flags: Vec<(NodeId, NodeFlag)>,
    latest_selection: FxHashSet<NodeId>,
    // selection_buffer: CpuAccessibleBuffer,
}

/// Instruction for updating the flags of a single node
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlagUpdate {
    node: NodeId,
    add: NodeFlags,
    remove: NodeFlags,
}

impl LayoutFlags {
    pub fn update_selection(
        &mut self,
        new_selection: &FxHashSet<NodeId>,
        buffer: &CpuAccessibleBuffer<[u8]>,
    ) -> Result<(), WriteLockError> {
        let removed = self.latest_selection.difference(new_selection);
        let added = new_selection.difference(&self.latest_selection);

        {
            let mut buf = buffer.write()?;

            for &node in removed {
                let ix = node.0 as usize;
                buf[ix] = 0;
            }

            for &node in added {
                let ix = node.0 as usize;
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
