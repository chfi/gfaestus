use handlegraph::handle::NodeId;
use rustc_hash::FxHashSet;

use crate::geometry::*;

use super::Node;

pub struct Selection {
    bounding_box: Rect,
    nodes: FxHashSet<NodeId>,
}

impl Selection {
    pub fn singleton(node_positions: &[Node], node: NodeId) -> Self {
        let ix = (node.0 - 1) as usize;

        let node_pos = node_positions[ix];
        let bounding_box = Rect::new(node_pos.p0, node_pos.p1);

        let mut nodes = FxHashSet::default();
        nodes.insert(node);

        Self {
            bounding_box,
            nodes,
        }
    }

    pub fn from_iter<I>(node_positions: &[Node], nodes_iter: I) -> Self
    where
        I: Iterator<Item = NodeId>,
    {
        let mut bounding_box = Rect::nowhere();
        let mut nodes = FxHashSet::default();

        for node in nodes_iter {
            let ix = (node.0 - 1) as usize;
            let node_pos = node_positions[ix];

            let rect = Rect::new(node_pos.p0, node_pos.p1);
            bounding_box = bounding_box.union(rect);

            nodes.insert(node);
        }

        Self {
            bounding_box,
            nodes,
        }
    }

    pub fn union(self, other: Self) -> Self {
        let bounding_box = self.bounding_box.union(other.bounding_box);

        let nodes = self
            .nodes
            .union(&other.nodes)
            .copied()
            .collect::<FxHashSet<_>>();

        Self {
            bounding_box,
            nodes,
        }
    }

    pub fn union_from(&mut self, other: &Self) {
        self.bounding_box = self.bounding_box.union(other.bounding_box);
        self.nodes.extend(other.nodes.iter().copied());
    }
}
