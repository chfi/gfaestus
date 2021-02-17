use crate::geometry::*;
use crate::render::Vertex;

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

use nalgebra_glm as glm;

use anyhow::Result;

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
pub struct Spine {
    pub offset: Point,
    pub angle: f32,
    pub node_ids: Vec<NodeId>,
    pub nodes: Vec<Node>,
}

impl Spine {
    pub fn from_laid_out_graph(graph: &PackedGraph, layout_path: &str) -> Result<Self> {
        use std::fs::File;
        use std::io::prelude::*;
        use std::io::BufReader;

        use rustc_hash::FxHashMap;

        let layout_file = File::open(layout_path)?;
        let reader = BufReader::new(layout_file);

        let mut lines = reader.lines();
        // throw away header
        lines.next().unwrap()?;

        let mut layout_map: FxHashMap<NodeId, (Point, Point)> = FxHashMap::default();

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

        for handle in graph.handles() {
            let id = handle.id();

            let (p0, p1) = *layout_map.get(&id).unwrap();

            node_ids.push(id);
            nodes.push(Node { p0, p1 });
        }

        Ok(Spine {
            offset: Point { x: 0.0, y: 0.0 },
            angle: 0.0,
            node_ids,
            nodes,
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

        Ok(())
    }
}

impl Spine {
    #[rustfmt::skip]
    pub fn model_matrix(&self) -> glm::Mat4 {

        let cos_t = self.angle.cos();
        let sin_t = self.angle.sin();

        let rotation = glm::mat4(
            cos_t, -sin_t, 0.0, 0.0,
            sin_t,  cos_t, 0.0, 0.0,
            0.0,    0.0,   1.0, 0.0,
            0.0,    0.0,   0.0, 1.0,
        );

        let translation = glm::mat4(
            1.0, 0.0, 0.0, self.offset.x,
            0.0, 1.0, 0.0, self.offset.y,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0
        );

        translation * rotation
    }

    pub fn vertices_into_lines(&self, vxs: &mut Vec<Vertex>) {
        vxs.clear();

        for node in self.nodes.iter() {
            let v0 = Vertex {
                position: [node.p0.x, node.p0.y],
            };
            let v1 = Vertex {
                position: [node.p1.x, node.p1.y],
            };
            vxs.push(v0);
            vxs.push(v1);
        }
    }
}
