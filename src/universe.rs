#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

#[allow(unused_imports)]
use handlegraph::packedgraph::PackedGraph;

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use anyhow::Result;

use crate::vulkan::{draw_system::Vertex, GfaestusVk};
use crate::{geometry::*, vulkan::draw_system::nodes::NodeVertices};

pub mod config;
pub mod graph_layout;
pub mod grid;
pub mod physics;
pub mod selection;

pub use config::*;
pub use graph_layout::*;
pub use selection::*;

// Trait abstracting over Grid and FlatLayout -- this definition only
// supports FlatLayout, though, and should be changed to use iterators
// to support more solutions
//
// note: `node_ids` and `nodes` must be in the same order
pub trait GraphLayout {
    fn node_ids(&self) -> &[NodeId];

    fn nodes(&self) -> &[Node];

    fn bounding_box(&self) -> (Point, Point);

    // `vertices` must contain the vertices for nodes in the same
    // order as returned by the `node_ids` and `nodes` methods
    #[inline]
    fn node_line_vertices(&self, vertices: &mut Vec<Vertex>) {
        vertices.clear();

        for node in self.nodes().iter() {
            let v0 = Vertex {
                position: [node.p0.x, node.p0.y],
            };
            let v1 = Vertex {
                position: [node.p1.x, node.p1.y],
            };
            vertices.push(v0);
            vertices.push(v1);
        }
    }
}

#[derive(Debug, Clone)]
pub struct Universe<G: GraphLayout> {
    // TODO bp_per_world_unit isn't used yet; and it should probably
    // be a 2D vector allowing nonuniform scaling
    bp_per_world_unit: f32,
    // grid: grid::Grid<NodeId>,
    graph_layout: G,
    // node_ids: Vec<NodeId>,
    pub offset: Point,
    pub angle: f32,
    // physics_config: PhysicsConfig,
    // layout_config: LayoutConfig,
    // view_config: ViewConfig,
}

impl<G: GraphLayout> Universe<G> {
    pub fn layout(&self) -> &G {
        &self.graph_layout
    }

    pub fn layout_mut(&mut self) -> &mut G {
        &mut self.graph_layout
    }
}

impl Universe<FlatLayout> {
    pub fn from_laid_out_graph(
        graph: &PackedGraph,
        layout_path: &str,
    ) -> Result<Self> {
        let bp_per_world_unit = 1.0;
        let offset = Point::new(0.0, 0.0);
        let angle = 0.0;

        let graph_layout = FlatLayout::from_laid_out_graph(graph, layout_path)?;

        Ok(Self {
            bp_per_world_unit,
            graph_layout,
            offset,
            angle,
        })
    }

    pub fn update_positions_from_gpu(
        &mut self,
        app: &GfaestusVk,
        vertices: &NodeVertices,
    ) -> Result<()> {
        let node_count = self.graph_layout.nodes.len();

        vertices.download_vertices(
            app,
            node_count,
            &mut self.graph_layout.nodes,
        )
    }

    /*
    pub fn update_positions_from_gpu(&mut self,
                                     device: &Device,
                                     vertices: &NodeVertices) -> Result<()> {

        let node_count = self.graph_layout.nodes.len();

        unsafe {
            let data_ptr = device.map_memory(
                vertices.memory,
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
    }
    */

    pub fn node_vertices(&self) -> Vec<Vertex> {
        let mut vertices = Vec::new();

        for node in self.graph_layout.nodes().iter() {
            let v0 = Vertex {
                position: [node.p0.x, node.p0.y],
            };
            let v1 = Vertex {
                position: [node.p1.x, node.p1.y],
            };
            vertices.push(v0);
            vertices.push(v1);
        }

        vertices
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Node {
    pub p0: Point,
    pub p1: Point,
}

impl Node {
    pub fn center(&self) -> Point {
        let diff = self.p1 - self.p0;
        self.p0 + (diff / 2.0)
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct FlatLayout {
    node_ids: Vec<NodeId>,
    nodes: Vec<Node>,
    // pub components: Vec<(usize, usize)>,
    pub component_offsets: Vec<usize>,
    top_left: Point,
    bottom_right: Point,
}

impl GraphLayout for FlatLayout {
    fn node_ids(&self) -> &[NodeId] {
        &self.node_ids
    }

    fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    fn bounding_box(&self) -> (Point, Point) {
        (self.top_left, self.bottom_right)
    }
}

impl FlatLayout {
    pub fn node_component(&self, node_id: NodeId) -> usize {
        let offset =
            self.component_offsets.iter().enumerate().find(|(_, o)| {
                let id = (node_id.0) as usize;
                id >= **o
            });

        if let Some((ix, _)) = offset {
            ix
        } else {
            self.component_offsets.len()
        }
    }

    fn from_laid_out_graph(
        graph: &PackedGraph,
        layout_path: &str,
    ) -> Result<Self> {
        use std::fs::File;
        use std::io::prelude::*;
        use std::io::BufReader;

        use rustc_hash::FxHashMap;

        info!("loading layout");
        let layout_file = File::open(layout_path)?;
        let reader = BufReader::new(layout_file);

        let mut lines = reader.lines();
        // throw away header
        lines.next().unwrap()?;

        let mut layout_map: FxHashMap<NodeId, (Point, Point)> =
            FxHashMap::default();

        let mut component_map: FxHashMap<NodeId, usize> = FxHashMap::default();

        let mut components: Vec<usize> = Vec::new();

        let mut cur_comp = 0;

        let mut prev_point = None;

        let mut line_count = 0;

        for line in lines {
            let line: String = line?;

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut fields = trimmed.split_whitespace();

            let ix = fields.next().unwrap().parse::<usize>()?;
            let x = fields.next().unwrap().parse::<f32>()?;
            let y = fields.next().unwrap().parse::<f32>()?;

            let component = if let Some(c) = fields.next() {
                let val = c.parse::<usize>()?;
                if val != cur_comp {
                    let id = (line_count / 2) + 1;
                    components.push(id);
                    cur_comp = val;
                }

                Some(c.parse::<usize>()?)
            } else {
                None
            };

            let this_p = Point { x, y };

            let node_ix = (ix / 2) + 1;
            let node_id = NodeId::from(node_ix);

            line_count += 1;

            if let Some(prev_p) = prev_point {
                layout_map.insert(node_id, (prev_p, this_p));
                if let Some(comp) = component {
                    component_map.insert(node_id, comp);
                }
                prev_point = None;
            } else {
                prev_point = Some(this_p);
            }
        }

        let mut node_ids = Vec::with_capacity(graph.node_count());
        let mut nodes = Vec::with_capacity(graph.node_count());

        // make sure the nodes are stored in ascending NodeId order so
        // that the vertex index in the NodeDrawSystem render pipeline
        // is correctly mapped to node ID
        let mut handles = graph.handles().collect::<Vec<_>>();
        handles.sort();

        let mut min_x = std::f32::MAX;
        let mut max_x = std::f32::MIN;

        let mut min_y = std::f32::MAX;
        let mut max_y = std::f32::MIN;

        for handle in handles {
            let id = handle.id();

            let (p0, p1) = *layout_map.get(&id).unwrap();

            let comp = component_map.get(&id).copied().unwrap_or(0);

            let delta = Point::new(0.0, (comp as f32) * 10_000.0);
            // let delta = Point::new(0.0, 0.0);

            let p0 = p0 + delta;
            let p1 = p1 + delta;

            min_x = min_x.min(p0.x).min(p1.x);
            max_x = max_x.max(p0.x).max(p1.x);

            min_y = min_y.min(p0.y).min(p1.y);
            max_y = max_y.max(p0.y).max(p1.y);

            node_ids.push(id);
            nodes.push(Node { p0, p1 });
        }

        let top_left = Point::new(min_x, min_y);
        let bottom_right = Point::new(max_x, max_y);

        Ok(FlatLayout {
            node_ids,
            nodes,
            component_offsets: components,
            top_left,
            bottom_right,
        })
    }
}
