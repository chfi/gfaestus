use crate::geometry::*;
use crate::view::*;

pub mod config;
pub mod grid;
pub mod physics;

pub use config::*;

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

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Universe {
    bp_per_world_unit: f32,
    grid: grid::Grid<NodeId>,
    // node_ids: Vec<NodeId>,
    offset: Point,
    angle: f32,
    physics_config: PhysicsConfig,
    layout_config: LayoutConfig,
    view_config: ViewConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Node {
    pub p0: Point,
    pub p1: Point,
}

fn rotate(p: Point, angle: f32) -> Point {
    let x = p.x * angle.cos() - p.y * angle.sin();
    let y = p.x * angle.sin() + p.y * angle.cos();

    Point { x, y }
}

impl Node {
    pub fn center(&self) -> Point {
        let diff = self.p1 - self.p0;
        self.p0 + (diff / 2.0)
    }

    pub fn vertices(&self) -> [Vertex; 6] {
        self.vertices_width(100.0)
    }

    pub fn vertices_width(&self, width: f32) -> [Vertex; 6] {
        let diff = self.p0 - self.p1;

        let pos0_to_pos1_norm = diff / diff.length();

        let pos0_orthogonal = rotate(pos0_to_pos1_norm, 3.14159265 / 2.0);

        let p0 = self.p0 + pos0_orthogonal * (width / 2.0);
        let p1 = self.p0 + pos0_orthogonal * (-width / 2.0);

        let p2 = self.p1 + pos0_orthogonal * (width / 2.0);
        let p3 = self.p1 + pos0_orthogonal * (-width / 2.0);

        [
            p0.vertex(),
            p2.vertex(),
            p1.vertex(),
            p3.vertex(),
            p2.vertex(),
            p1.vertex(),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Edge {
    pub from: usize,
    pub from_end: Direction,
    pub to: usize,
    pub to_end: Direction,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Spine {
    pub offset: Point,
    pub angle: f32,
    pub node_ids: Vec<NodeId>,
    pub nodes: Vec<Node>,
    // pub edges: Vec<Edge>,
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

            // let x = x / 10.0;
            // let y = y / 10.0;

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

        // let mut layout_vec: Vec<(usize, f32, f32)> = Vec::with_capacity(self.node_ids.len());

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

pub fn test_spines() -> Vec<Spine> {
    let graph = test_graph_with_paths();

    let mut spines = Vec::new();

    let path0 = graph.get_path_id(b"path0").unwrap();
    let path1 = graph.get_path_id(b"path1").unwrap();
    let path2 = graph.get_path_id(b"path2").unwrap();
    let path3 = graph.get_path_id(b"path3").unwrap();

    let mut spine0 = Spine::from_path(&graph, path0).unwrap();
    spine0.offset.y -= 40.0;

    let mut spine1 = Spine::from_path(&graph, path1).unwrap();
    spine1.offset.y -= 15.0;

    let mut spine2 = Spine::from_path(&graph, path2).unwrap();
    spine2.offset.y += 15.0;

    let mut spine3 = Spine::from_path(&graph, path3).unwrap();
    spine3.offset.y += 40.0;

    spines.push(spine0);
    spines.push(spine1);
    spines.push(spine2);
    spines.push(spine3);

    spines
}

fn path_vertices(segs: &[Node]) -> Vec<Vertex> {
    let mut res = Vec::with_capacity(segs.len() * 6);

    for seg in segs {
        res.extend(seg.vertices().iter());
    }

    res
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

    pub fn vertices_(&self, vxs: &mut Vec<Vertex>) {
        vxs.clear();
        for seg in self.nodes.iter() {
            vxs.extend(seg.vertices().iter());
        }
    }

    pub fn vertices_into_with_width(&self, width: f32, vxs: &mut Vec<Vertex>) {
        vxs.clear();

        for seg in self.nodes.iter() {
            vxs.extend(seg.vertices_width(width).iter());
        }
    }

    pub fn vertices_into_color(&self, vxs: &mut Vec<Vertex>, cols: &mut Vec<Color>) {
        vxs.clear();
        cols.clear();

        for seg in self.nodes.iter() {
            vxs.extend(seg.vertices().iter());
        }

        let color_period = [
            [1.0, 0.0, 0.0],
            [1.0, 0.65, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 0.5, 0.0],
            [0.0, 0.0, 1.0],
            [0.3, 0.0, 0.51],
            [0.93, 0.51, 0.93],
        ];

        cols.extend(vxs.iter().enumerate().map(|(ix, _)| {
            let ix_ = (ix / 6) % color_period.len();
            Color {
                color: color_period[ix_],
            }
        }));
    }

    pub fn vertices(&self) -> (Vec<Vertex>, Vec<Color>) {
        let color_period = [
            [1.0, 0.0, 0.0],
            [1.0, 0.65, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 0.5, 0.0],
            [0.0, 0.0, 1.0],
            [0.3, 0.0, 0.51],
            [0.93, 0.51, 0.93],
        ];

        let vxs = path_vertices(&self.nodes);

        let colors: Vec<Color> = vxs
            .iter()
            .enumerate()
            .map(|(ix, _)| {
                let ix_ = (ix / 6) % color_period.len();
                Color {
                    color: color_period[ix_],
                }
            })
            .collect();

        (vxs, colors)
    }

    pub fn from_path(graph: &PackedGraph, path_id: PathId) -> Option<Self> {
        let path_len = graph.path_len(path_id)?;
        let path_steps = graph.path_steps(path_id)?;

        let mut node_ids: Vec<NodeId> = Vec::with_capacity(path_len);
        let mut nodes: Vec<Node> = Vec::with_capacity(path_len);
        // let mut edges: Vec<Edge> = Vec::with_capacity(path_len);

        let mut x_offset = 0.0;

        let base_len = 30.0;
        let pad = 10.0;

        eprintln!("adding path of length {}", path_len);
        for step in path_steps {
            // for step in path_steps.take(2000) {
            node_ids.push(step.handle().id());

            let seq_len = graph.node_len(step.handle());
            let len = base_len * seq_len as f32;

            let p0 = Point {
                x: x_offset,
                y: 0.0,
            };
            let p1 = Point {
                x: x_offset + len,
                y: 0.0,
            };

            nodes.push(Node { p0, p1 });

            x_offset += len + pad;
        }

        let offset = Point { x: 0.0, y: 0.0 };
        let angle = 0.0;

        Some(Spine {
            offset,
            angle,
            node_ids,
            nodes,
            // edges,
        })
    }
}

fn hnd(x: u64) -> Handle {
    Handle::pack(x, false)
}

fn vec_hnd(v: Vec<u64>) -> Vec<Handle> {
    v.into_iter().map(hnd).collect::<Vec<_>>()
}

fn edge(l: u64, r: u64) -> handlegraph::handle::Edge {
    handlegraph::handle::Edge(hnd(l), hnd(r))
}

fn test_graph_no_paths() -> PackedGraph {
    let mut graph = PackedGraph::new();

    let mut seqs: Vec<&[u8]> = Vec::new();

    seqs.push(b"GTCA"); //  1
    seqs.push(b"AAGTGCTAGT"); //  2
    seqs.push(b"ATA"); //  3
    seqs.push(b"AGTA"); //  4
    seqs.push(b"GTCCA"); //  5
    seqs.push(b"GGGT"); //  6
    seqs.push(b"AACT"); //  7
    seqs.push(b"AACAT"); //  8
    seqs.push(b"AGCC"); //  9

    /*
    1 ----- 8 --- 4 -----
      \   /   \     \     \
        2      \     \      6
      /   \     \     \   /
    5 ----- 7 --- 3 --- 9
    */

    let _handles = seqs
        .iter()
        .map(|seq| graph.append_handle(seq))
        .collect::<Vec<_>>();

    macro_rules! insert_edges {
            ($graph:ident, [$(($from:literal, $to:literal)),*]) => {
                $(
                    $graph.create_edge(edge($from, $to));
                )*
            };
        }

    insert_edges!(
        graph,
        [
            (1, 2),
            (1, 8),
            (5, 2),
            (5, 7),
            (2, 8),
            (2, 7),
            (7, 3),
            (8, 3),
            (8, 4),
            (3, 9),
            (4, 9),
            (4, 6),
            (9, 6)
        ]
    );

    graph
}

pub fn test_graph_with_paths() -> PackedGraph {
    let mut graph = test_graph_no_paths();
    /* Paths
            path_1: 1 8 4 6
            path_2: 5 2 8 4 6
            path_3: 1 2 8 4 9 6
            path_4: 5 7 3 9 6
    */

    let prep_path = |graph: &mut PackedGraph, name: &[u8], steps: Vec<u64>| {
        let path = graph.create_path(name, false);
        let hnds = vec_hnd(steps);
        (path, hnds)
    };

    let (path_0, p_0_steps) = prep_path(&mut graph, b"path0", vec![1, 8, 4, 6]);

    let (path_1, p_1_steps) = prep_path(&mut graph, b"path1", vec![5, 2, 8, 4, 6]);

    let (path_2, p_2_steps) = prep_path(&mut graph, b"path2", vec![1, 2, 8, 4, 9, 6]);

    let (path_3, p_3_steps) = prep_path(&mut graph, b"path3", vec![5, 7, 3, 9, 6]);

    for &step in p_0_steps.iter() {
        graph.path_append_step(path_0.unwrap(), step);
    }

    for &step in p_1_steps.iter() {
        graph.path_append_step(path_1.unwrap(), step);
    }

    for &step in p_2_steps.iter() {
        graph.path_append_step(path_2.unwrap(), step);
    }

    for &step in p_3_steps.iter() {
        graph.path_append_step(path_3.unwrap(), step);
    }

    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spines_from_paths() {
        let graph = test_graph_with_paths();
    }
}
