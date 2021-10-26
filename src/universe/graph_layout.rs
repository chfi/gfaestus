#![allow(dead_code)]
#![allow(unused_variables)]

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

use crate::geometry::*;

#[allow(unused_imports)]
use anyhow::Result;

pub struct LayoutQuadtree {
    // data: Vec<u32>,
    // node_offsets: Vec<u32>,
    data: Vec<Point>, // use u32 instead of NodeId to ease mapping to GPU
    tree_node_offsets: Vec<u32>, // same here w/ u32 vs usize

    elements: usize,
    leaf_capacity: usize,
    depth: usize,

    polynomial_t: usize,
}

impl LayoutQuadtree {
    pub fn truncated(nodes: &[Point], leaf_capacity: usize) -> Self {
        /*
        let depth = ((nodes.len() / leaf_capacity) as f64).log2().floor();
        let depth = (depth as usize).max(1);

        let elements = nodes.len();

        // no idea if this is even close to correct; should probably
        // take the node count & initial layout size into account here
        let polynomial_t = 1_000_000;

        let mut data: Vec<u32> = Vec::with_capacity(elements);
        let mut tree_node_offsets: Vec<u32> = Vec::with_capacity(elements);
        */

        unimplemented!();
    }

    fn map_coordinate(
        point: Point,
        min_p: Point,
        max_p: Point,
        poly_t: usize,
        node_count: usize,
    ) -> (usize, usize) {
        let offset_point = point - min_p;

        let x_f = offset_point.x / (max_p.x - min_p.x);
        let y_f = offset_point.y / (max_p.y - min_p.y);

        let coef = poly_t * node_count * node_count;

        let x = (x_f * (coef as f32)) as usize;
        let y = (y_f * (coef as f32)) as usize;

        (x, y)
    }
}
