use handlegraph::packedgraph::{iter::EdgeListHandleIter, nodes::IndexMapIter};
use rhai::plugin::*;

use anyhow::Result;

use rayon::prelude::*;

use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use handlegraph::packedgraph::{paths::StepPtr, PackedGraph};

use std::sync::Arc;

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

use handlegraph::packedgraph::occurrences::OccurrencesIter;

#[derive(Clone)]
pub struct OccursIter {
    graph: Arc<PackedGraph>,
    iter: OccurrencesIter<'static>,
}

impl OccursIter {
    pub fn new(graph: Arc<PackedGraph>, handle: Handle) -> Option<Self> {
        let iter_ = graph.steps_on_handle(handle)?;

        let iter = unsafe {
            std::mem::transmute::<OccurrencesIter<'_>, OccurrencesIter<'static>>(
                iter_,
            )
        };

        Some(Self { graph, iter })
    }
}

impl Iterator for OccursIter {
    type Item = (PathId, StepPtr);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

#[derive(Clone)]
pub struct NeighborsIter {
    graph: Arc<PackedGraph>,
    iter: EdgeListHandleIter<'static>,
}

impl NeighborsIter {
    pub fn new(
        graph: Arc<PackedGraph>,
        handle: Handle,
        dir: Direction,
    ) -> Self {
        let iter_ = graph.neighbors(handle, dir);

        let iter = unsafe {
            std::mem::transmute::<
                EdgeListHandleIter<'_>,
                EdgeListHandleIter<'static>,
            >(iter_)
        };

        Self { graph, iter }
    }
}

impl Iterator for NeighborsIter {
    type Item = Handle;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
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

pub type Step = (PathId, StepPtr);

#[export_module]
pub mod graph_iters {
    #[rhai_fn(pure)]
    pub fn handles(graph: &mut Arc<PackedGraph>) -> HandlesIter {
        let graph_arc: Arc<PackedGraph> = graph.clone();
        HandlesIter::new(graph_arc)
    }

    #[rhai_fn(pure)]
    pub fn steps_on_handle(
        graph: &mut Arc<PackedGraph>,
        handle: Handle,
    ) -> OccursIter {
        let graph_arc: Arc<PackedGraph> = graph.clone();
        OccursIter::new(graph_arc, handle).unwrap()
    }

    #[rhai_fn(pure)]
    pub fn neighbors_forward(
        graph: &mut Arc<PackedGraph>,
        handle: Handle,
    ) -> NeighborsIter {
        let graph_arc: Arc<PackedGraph> = graph.clone();
        NeighborsIter::new(graph_arc, handle, Direction::Right)
    }

    #[rhai_fn(pure)]
    pub fn neighbors_backward(
        graph: &mut Arc<PackedGraph>,
        handle: Handle,
    ) -> NeighborsIter {
        let graph_arc: Arc<PackedGraph> = graph.clone();
        NeighborsIter::new(graph_arc, handle, Direction::Left)
    }

    #[rhai_fn(pure, get = "path_id")]
    pub fn occur_path_id(occur: &mut Step) -> PathId {
        let path = occur.0;
        path
    }

    #[rhai_fn(pure, get = "step_ix")]
    pub fn occur_step_ix(occur: &mut Step) -> StepPtr {
        occur.1
    }

    pub fn unwrap_path_id(path: PathId) -> i64 {
        path.0 as i64
    }
}

#[export_module]
pub mod paths_plugin {
    #[rhai_fn(pure, name = "path_len")]
    pub fn path_len(graph: &mut Arc<PackedGraph>, path: PathId) -> usize {
        graph.path_len(path).unwrap_or(0)
    }

    #[rhai_fn(pure, name = "path_len")]
    pub fn path_len_i32(graph: &mut Arc<PackedGraph>, path: i32) -> usize {
        path_len(graph, PathId(path as u64))
    }

    #[rhai_fn(pure, name = "path_len")]
    pub fn path_len_i64(graph: &mut Arc<PackedGraph>, path: i64) -> usize {
        path_len(graph, PathId(path as u64))
    }

    #[rhai_fn(pure)]
    pub fn path_handle_at_step(
        graph: &mut Arc<PackedGraph>,
        path: PathId,
        step: StepPtr,
    ) -> Handle {
        graph.path_handle_at_step(path, step).unwrap()
    }

    #[rhai_fn(pure)]
    pub fn has_next_step(
        graph: &mut Arc<PackedGraph>,
        path: PathId,
        step: StepPtr,
    ) -> bool {
        graph.path_next_step(path, step).is_some()
    }

    #[rhai_fn(pure)]
    pub fn has_prev_step(
        graph: &mut Arc<PackedGraph>,
        path: PathId,
        step: StepPtr,
    ) -> bool {
        graph.path_prev_step(path, step).is_some()
    }

    #[rhai_fn(pure)]
    pub fn next_step(
        graph: &mut Arc<PackedGraph>,
        path: PathId,
        step: StepPtr,
    ) -> StepPtr {
        graph.path_next_step(path, step).unwrap()
    }

    #[rhai_fn(pure)]
    pub fn prev_step(
        graph: &mut Arc<PackedGraph>,
        path: PathId,
        step: StepPtr,
    ) -> StepPtr {
        graph.path_prev_step(path, step).unwrap()
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

    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> rgb::RGBA<f32> {
        rgb::RGBA::new(r, g, b, a)
    }
}
