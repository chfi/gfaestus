use handlegraph::packedgraph::nodes::IndexMapIter;
use rhai::plugin::*;

use rhai::{Engine, EvalAltResult, INT};

use anyhow::Result;

use rayon::prelude::*;

use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    packedgraph::index::OneBasedIndex,
    pathhandlegraph::*,
};

use handlegraph::{
    packedgraph::{paths::StepPtr, PackedGraph},
    path_position::PathPositionMap,
};
use rustc_hash::FxHashMap;

use std::{path::Path, sync::Arc};

use bstr::ByteVec;
use futures::Future;

use bytemuck::{Contiguous, Pod, Zeroable};

use crate::vulkan::draw_system::nodes::overlay::NodeOverlay;

use crate::graph_query::GraphQuery;
use crate::overlays::{OverlayData, OverlayKind};

#[derive(Clone)]
pub struct HandlesIter {
    graph: Arc<PackedGraph>,
    iter: NodeIdHandles<IndexMapIter<'static>>,
}

impl HandlesIter {
    pub fn new(graph: Arc<PackedGraph>) -> Self {
        let iter: NodeIdHandles<IndexMapIter<'_>> = graph.handles();

        let ridiculous = unsafe {
            std::mem::transmute::<
                NodeIdHandles<IndexMapIter<'_>>,
                NodeIdHandles<IndexMapIter<'static>>,
            >(iter)
        };

        Self {
            graph,
            iter: ridiculous,
        }
    }
}

impl Iterator for HandlesIter {
    type Item = Handle;

    #[inline]
    fn next(&mut self) -> Option<Handle> {
        self.iter.next()
    }
}

#[export_module]
pub mod handle_plugin {
    #[rhai_fn(name = "handle")]
    pub fn handle_from_i64(id: i64, reverse: bool) -> Handle {
        Handle::pack(id as u64, reverse)
    }

    #[rhai_fn(name = "handle")]
    pub fn handle_from_node_id(id: NodeId, reverse: bool) -> Handle {
        Handle::pack(id.0, reverse)
    }

    #[rhai_fn(pure)]
    pub fn id(handle: &mut Handle) -> NodeId {
        handle.id()
    }

    #[rhai_fn(pure)]
    pub fn is_reverse(handle: &mut Handle) -> bool {
        handle.is_reverse()
    }

    #[rhai_fn(pure)]
    pub fn flip(handle: &mut Handle) -> Handle {
        handle.flip()
    }

    #[rhai_fn(pure)]
    pub fn forward(handle: &mut Handle) -> Handle {
        handle.forward()
    }
}

#[export_module]
pub mod graph_plugin {
    #[rhai_fn(pure)]
    pub fn node_count(graph: &mut Arc<PackedGraph>) -> usize {
        graph.node_count()
    }

    #[rhai_fn(pure)]
    pub fn edge_count(graph: &mut Arc<PackedGraph>) -> usize {
        graph.edge_count()
    }

    #[rhai_fn(pure)]
    pub fn path_count(graph: &mut Arc<PackedGraph>) -> usize {
        graph.path_count()
    }

    #[rhai_fn(pure)]
    pub fn total_length(graph: &mut Arc<PackedGraph>) -> usize {
        graph.total_length()
    }

    #[rhai_fn(pure)]
    pub fn min_node_id(graph: &mut Arc<PackedGraph>) -> NodeId {
        graph.min_node_id()
    }

    #[rhai_fn(pure)]
    pub fn max_node_id(graph: &mut Arc<PackedGraph>) -> NodeId {
        graph.max_node_id()
    }

    #[rhai_fn(pure)]
    pub fn sequence(graph: &mut Arc<PackedGraph>, handle: Handle) -> Vec<u8> {
        graph.sequence_vec(handle)
    }

    #[rhai_fn(pure)]
    pub fn handles(graph: &mut Arc<PackedGraph>) -> HandlesIter {
        let graph_arc: Arc<PackedGraph> = graph.clone();
        HandlesIter::new(graph_arc)
    }

    // pub fn handles(graph: &mut Arc<PackedGraph>) ->

    #[rhai_fn(pure)]
    pub fn get_path_id(
        graph: &mut Arc<PackedGraph>,
        path_name: &str,
    ) -> Option<PathId> {
        graph.get_path_id(path_name.as_bytes())
    }
}

#[export_module]
pub mod colors {
    #[rhai_fn(pure)]
    pub fn hash_bytes(bytes: &mut Vec<u8>) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::default();
        bytes.hash(&mut hasher);
        let hash = hasher.finish();
        bytemuck::cast(hash)
    }

    pub fn hash_color(hash: u64) -> rgb::RGBA<f32> {
        let r_u16 = ((hash >> 32) & 0xFFFFFFFF) as u16;
        let g_u16 = ((hash >> 16) & 0xFFFFFFFF) as u16;
        let b_u16 = (hash & 0xFFFFFFFF) as u16;

        let max = r_u16.max(g_u16).max(b_u16) as f32;
        let r = (r_u16 as f32) / max;
        let g = (g_u16 as f32) / max;
        let b = (b_u16 as f32) / max;
        rgb::RGBA::new(r, g, b, 1.0)
    }
}
