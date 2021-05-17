use ash::Device;
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

use anyhow::Result;

use crate::vulkan::draw_system::Vertex;
use crate::{geometry::*, vulkan::draw_system::nodes::NodeVertices};

pub mod config;
pub mod grid;
pub mod physics;
pub mod selection;

pub use config::*;
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
        device: &Device,
        vertices: &NodeVertices,
    ) -> Result<()> {
        let node_count = self.graph_layout.nodes.len();

        vertices.download_vertices(
            device,
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

    pub fn new_vertices(&self) -> Vec<Vertex> {
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
    fn from_laid_out_graph(
        graph: &PackedGraph,
        layout_path: &str,
    ) -> Result<Self> {
        use std::fs::File;
        use std::io::prelude::*;
        use std::io::BufReader;

        use rustc_hash::FxHashMap;

        eprintln!("loading layout");
        let layout_file = File::open(layout_path)?;
        let reader = BufReader::new(layout_file);

        let mut lines = reader.lines();
        // throw away header
        lines.next().unwrap()?;

        let mut layout_map: FxHashMap<NodeId, (Point, Point)> =
            FxHashMap::default();

        let mut prev_point = None;

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

            let _component = if let Some(c) = fields.next() {
                Some(c.parse::<usize>()?)
            } else {
                None
            };

            let this_p = Point { x, y };

            let node_ix = (ix / 2) + 1;
            let node_id = NodeId::from(node_ix);

            if let Some(prev_p) = prev_point {
                layout_map.insert(node_id, (prev_p, this_p));
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
            top_left,
            bottom_right,
        })
    }

    pub fn apply_layout_tsv(&mut self, path: &str) -> Result<()> {
        use std::fs::File;
        use std::io::prelude::*;
        use std::io::BufReader;

        let layout_file = File::open(path)?;
        let reader = BufReader::new(layout_file);

        let mut lines = reader.lines();
        // throw away header
        lines.next().unwrap()?;

        let mut min_x = std::f32::MAX;
        let mut max_x = std::f32::MIN;
        let mut min_y = std::f32::MAX;
        let mut max_y = std::f32::MIN;

        for line in lines {
            let line: String = line?;

            println!("reading line: {}", line);

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut fields = trimmed.split_whitespace();

            let ix = fields.next().unwrap();
            let ix = ix.parse::<usize>()?;

            let x = fields.next().unwrap();
            let x = x.parse::<f32>()?;

            let y = fields.next().unwrap();
            let y = y.parse::<f32>()?;

            min_x = min_x.min(x);
            max_x = max_x.max(x);

            min_y = min_y.min(y);
            max_y = max_y.max(y);

            let new_p = Point { x, y };

            let node_ix = (ix / 2) + 1;

            let node = self.nodes.get_mut(node_ix).unwrap();

            if ix % 2 == 0 {
                // node start
                node.p0 = new_p;
            } else {
                // node end
                node.p1 = new_p;
            }
        }

        self.top_left = Point::new(min_x, min_y);
        self.bottom_right = Point::new(max_x, max_y);

        Ok(())
    }
}
