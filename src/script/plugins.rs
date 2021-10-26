use handlegraph::packedgraph::{iter::EdgeListHandleIter, nodes::IndexMapIter};
use rhai::plugin::*;

use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
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

    #[rhai_fn(pure, get = "id")]
    pub fn id(handle: &mut Handle) -> NodeId {
        handle.id()
    }

    #[rhai_fn(pure, get = "is_reverse")]
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

    #[rhai_fn(pure, return_raw)]
    pub fn path_first_step(
        graph: &mut Arc<PackedGraph>,
        path: PathId,
    ) -> std::result::Result<StepPtr, Box<EvalAltResult>> {
        graph.path_first_step(path).ok_or("Path not found".into())
    }

    #[rhai_fn(pure, return_raw)]
    pub fn path_last_step(
        graph: &mut Arc<PackedGraph>,
        path: PathId,
    ) -> std::result::Result<StepPtr, Box<EvalAltResult>> {
        graph.path_last_step(path).ok_or("Path not found".into())
    }

    #[rhai_fn(pure, return_raw)]
    pub fn path_handle_at_step(
        graph: &mut Arc<PackedGraph>,
        path: PathId,
        step: StepPtr,
    ) -> std::result::Result<Handle, Box<EvalAltResult>> {
        graph
            .path_handle_at_step(path, step)
            .ok_or("Path or step not found".into())
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

    #[rhai_fn(pure, return_raw)]
    pub fn next_step(
        graph: &mut Arc<PackedGraph>,
        path: PathId,
        step: StepPtr,
    ) -> std::result::Result<StepPtr, Box<EvalAltResult>> {
        graph
            .path_next_step(path, step)
            .ok_or("Step not found".into())
    }

    #[rhai_fn(pure, return_raw)]
    pub fn prev_step(
        graph: &mut Arc<PackedGraph>,
        path: PathId,
        step: StepPtr,
    ) -> std::result::Result<StepPtr, Box<EvalAltResult>> {
        graph
            .path_prev_step(path, step)
            .ok_or("Step not found".into())
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

    // `PathId` can't (and shouldn't be able to) be created in
    // isolation by the console, meaning all instances of `path` here
    // must be valid path identifiers in a graph, and because we only
    // have one graph, they must always refer to valid paths in the
    // provided graph
    #[rhai_fn(pure)]
    pub fn get_path_name(graph: &mut Arc<PackedGraph>, path: PathId) -> String {
        use bstr::ByteSlice;
        let name_vec = graph.get_path_name_vec(path).unwrap();
        format!("{}", name_vec.as_bstr())
    }

    #[rhai_fn(pure, return_raw)]
    pub fn get_path_id(
        graph: &mut Arc<PackedGraph>,
        path_name: &str,
    ) -> std::result::Result<PathId, Box<EvalAltResult>> {
        graph
            .get_path_id(path_name.as_bytes())
            .ok_or("Path not found".into())
    }

    #[rhai_fn(pure)]
    pub fn get_path_ids_by_prefix(
        graph: &mut Arc<PackedGraph>,
        path_name_prefix: &str,
    ) -> Vec<rhai::Dynamic> {
        use bstr::ByteSlice;

        let graph: &PackedGraph = graph.as_ref();

        let mut result: Vec<rhai::Dynamic> = Vec::new();

        let path_name_prefix = path_name_prefix.as_bytes();
        let mut path_name_buf: Vec<u8> = Vec::new();

        for path_id in graph.path_ids() {
            if let Some(path_name) = graph.get_path_name(path_id) {
                path_name_buf.clear();
                path_name_buf.extend(path_name);

                if path_name_buf.starts_with(path_name_prefix) {
                    result.push(rhai::Dynamic::from(path_id));
                }
            }
        }

        result
    }

    #[rhai_fn(pure)]
    pub fn get_path_names_by_prefix(
        graph: &mut Arc<PackedGraph>,
        path_name_prefix: &str,
    ) -> Vec<rhai::Dynamic> {
        use bstr::ByteSlice;

        let graph: &PackedGraph = graph.as_ref();

        let mut result: Vec<rhai::Dynamic> = Vec::new();

        let path_name_prefix = path_name_prefix.as_bytes();
        let mut path_name_buf: Vec<u8> = Vec::new();

        for path_id in graph.path_ids() {
            if let Some(path_name) = graph.get_path_name(path_id) {
                path_name_buf.clear();
                path_name_buf.extend(path_name);

                if path_name_buf.starts_with(path_name_prefix) {
                    result.push(rhai::Dynamic::from(format!(
                        "{}",
                        path_name_buf.as_bstr()
                    )));
                }
            }
        }

        result
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

    #[rhai_fn(pure, get = "r")]
    pub fn rgba_r(color: &mut rgb::RGBA<f32>) -> f32 {
        color.r
    }

    #[rhai_fn(pure, get = "g")]
    pub fn rgba_g(color: &mut rgb::RGBA<f32>) -> f32 {
        color.g
    }

    #[rhai_fn(pure, get = "b")]
    pub fn rgba_b(color: &mut rgb::RGBA<f32>) -> f32 {
        color.b
    }

    #[rhai_fn(pure, get = "a")]
    pub fn rgba_a(color: &mut rgb::RGBA<f32>) -> f32 {
        color.a
    }

    #[rhai_fn(set = "r")]
    pub fn rgba_r_set(color: &mut rgb::RGBA<f32>, v: f32) {
        color.r = v;
    }

    #[rhai_fn(set = "g")]
    pub fn rgba_g_set(color: &mut rgb::RGBA<f32>, v: f32) {
        color.g = v;
    }

    #[rhai_fn(set = "b")]
    pub fn rgba_b_set(color: &mut rgb::RGBA<f32>, v: f32) {
        color.b = v;
    }

    #[rhai_fn(set = "a")]
    pub fn rgba_a_set(color: &mut rgb::RGBA<f32>, v: f32) {
        color.a = v;
    }

    pub fn rgba_as_tuple(color: &mut rgb::RGBA<f32>) -> (f32, f32, f32, f32) {
        (color.r, color.g, color.b, color.a)
    }

    pub fn rgb(r: f32, g: f32, b: f32) -> rgb::RGB<f32> {
        rgb::RGB::new(r, g, b)
    }

    #[rhai_fn(pure, get = "r")]
    pub fn rgb_r(color: &mut rgb::RGB<f32>) -> f32 {
        color.r
    }

    #[rhai_fn(pure, get = "g")]
    pub fn rgb_g(color: &mut rgb::RGB<f32>) -> f32 {
        color.g
    }

    #[rhai_fn(pure, get = "b")]
    pub fn rgb_b(color: &mut rgb::RGB<f32>) -> f32 {
        color.b
    }

    #[rhai_fn(set = "r")]
    pub fn rgb_r_set(color: &mut rgb::RGB<f32>, v: f32) {
        color.r = v;
    }

    #[rhai_fn(set = "g")]
    pub fn rgb_g_set(color: &mut rgb::RGB<f32>, v: f32) {
        color.g = v;
    }

    #[rhai_fn(set = "b")]
    pub fn rgb_b_set(color: &mut rgb::RGB<f32>, v: f32) {
        color.b = v;
    }

    pub fn rgb_as_tuple(color: &mut rgb::RGB<f32>) -> (f32, f32, f32) {
        (color.r, color.g, color.b)
    }
}

#[export_module]
pub mod selection {
    use crate::app::selection::NodeSelection;

    #[rhai_fn(pure)]
    pub fn union(
        first: &mut NodeSelection,
        other: NodeSelection,
    ) -> NodeSelection {
        first.union(&other)
    }

    #[rhai_fn(pure)]
    pub fn intersection(
        first: &mut NodeSelection,
        other: NodeSelection,
    ) -> NodeSelection {
        first.intersection(&other)
    }

    #[rhai_fn(pure)]
    pub fn difference(
        first: &mut NodeSelection,
        other: NodeSelection,
    ) -> NodeSelection {
        first.difference(&other)
    }

    pub fn add_one(sel: &mut NodeSelection, node: NodeId) {
        sel.add_one(false, node);
    }

    pub fn add_array(sel: &mut NodeSelection, nodes: Vec<NodeId>) {
        sel.add_slice(false, &nodes);
    }

    #[rhai_fn(pure)]
    pub fn len(sel: &mut NodeSelection) -> i64 {
        sel.nodes.len() as i64
    }
}
