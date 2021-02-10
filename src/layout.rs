use crate::geometry::*;
use crate::view::*;

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
    pub fn vertices(&self) -> [Vertex; 6] {
        let diff = self.p0 - self.p1;

        let pos0_to_pos1_norm = diff / diff.length();

        let pos0_orthogonal = rotate(pos0_to_pos1_norm, 3.14159265 / 2.0);

        let width = 25.0;

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

pub struct Edge {
    pub from: usize,
    pub from_end: Direction,
    pub to: usize,
    pub to_end: Direction,
}

pub struct Spine {
    pub offset: Point,
    pub angle: f32,
    pub node_ids: Vec<NodeId>,
    pub nodes: Vec<Node>,
    // pub edges: Vec<Edge>,
}

pub fn test_spines() -> Vec<Spine> {
    let graph = test_graph_with_paths();

    let mut spines = Vec::new();

    let path0 = graph.get_path_id(b"path0").unwrap();
    let path1 = graph.get_path_id(b"path1").unwrap();
    let path2 = graph.get_path_id(b"path2").unwrap();
    let path3 = graph.get_path_id(b"path3").unwrap();

    let mut spine0 = Spine::from_path(&graph, path0).unwrap();
    spine0.offset.y -= 150.0;

    let mut spine1 = Spine::from_path(&graph, path1).unwrap();
    spine1.offset.y -= 50.0;

    let mut spine2 = Spine::from_path(&graph, path2).unwrap();
    spine2.offset.y += 50.0;

    let mut spine3 = Spine::from_path(&graph, path3).unwrap();
    spine3.offset.y += 150.0;

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

        let base_len = 15.0;
        let pad = 15.0;

        for step in path_steps {
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
