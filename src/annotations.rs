use anyhow::Result;

use handlegraph::{
    handle::{Handle, NodeId},
    pathhandlegraph::*,
};

use handlegraph::packedgraph::paths::StepPtr;

use bstr::ByteSlice;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    geometry::*, gluon::GraphHandle, graph_query::GraphQuery, universe::Node,
    view::*,
};

use nalgebra as na;
use nalgebra_glm as glm;

pub mod gff;

pub use gff::*;

pub trait AnnotationRecord {
    type ColumnKey: Clone + std::fmt::Display;

    fn columns(&self) -> Vec<Self::ColumnKey>;

    fn seq_id(&self) -> &[u8];

    fn start(&self) -> usize;

    fn end(&self) -> usize;

    fn range(&self) -> (usize, usize) {
        (self.start(), self.end())
    }

    fn score(&self) -> Option<f64>;

    /// Get the value of one of the columns, other than those
    /// corresponding to the range or the score
    ///
    /// If the column has multiple entries, return the first
    fn get_first(&self, key: &Self::ColumnKey) -> Option<&[u8]>;

    fn get_all(&self, key: &Self::ColumnKey) -> Vec<&[u8]>;
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum Strand {
    Pos,
    Neg,
    None,
}

impl std::str::FromStr for Strand {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        if s == "+" {
            Ok(Strand::Pos)
        } else if s == "-" {
            Ok(Strand::Neg)
        } else if s == "." {
            Ok(Strand::None)
        } else {
            Err(())
        }
    }
}

pub struct PathCoordinateSystem {
    path: PathId,

    step_list: Vec<(Handle, StepPtr, usize)>,

    // node_steps: FxHashMap<NodeId, Vec<StepPtr>>,
    node_indices: FxHashMap<NodeId, Vec<usize>>,

    points: Vec<Point>,
}

impl PathCoordinateSystem {
    pub fn new(
        graph: &GraphQuery,
        nodes: &[Node],
        path: PathId,
    ) -> Option<Self> {
        let step_list = graph.path_pos_steps(path)?;

        let mut node_indices: FxHashMap<NodeId, Vec<usize>> =
            FxHashMap::default();

        let mut points: Vec<Point> = Vec::with_capacity(step_list.len());

        for (ix, (handle, _, _)) in step_list.iter().enumerate() {
            let id = handle.id();
            node_indices.entry(id).or_default().push(ix);

            let ix = (id.0 - 1) as usize;

            let node = nodes.get(ix)?;

            points.push(node.center());
        }

        Some(Self {
            path,
            step_list,
            node_indices,
            points,
        })
    }

    pub fn update_points(&mut self, nodes: &[Node]) {
        self.points.clear();

        for (handle, _, _) in self.step_list.iter() {
            let id = handle.id();
            let ix = (id.0 - 1) as usize;

            let node = nodes.get(ix).unwrap();
            self.points.push(node.center());
        }
    }

    pub fn node_step_indices(&self, node: NodeId) -> Option<&[usize]> {
        let ixs = self.node_indices.get(&node)?;
        Some(ixs.as_slice())
    }

    pub fn unit_normal(&self, ix: usize) -> Option<na::Vector2<f32>> {
        // TODO handle 1st and last steps
        let prev = *self.points.get(ix - 1)?;
        // let mid = self.points.get(ix)?;
        let next = *self.points.get(ix + 1)?;

        let p = na::Vector2::new(prev.x, prev.y);
        let n = na::Vector2::new(next.x, next.y);

        let delta = n - p;

        let mid = p + (delta * 0.5);

        let rot = na::Rotation2::new(std::f32::consts::PI / 2.0);
        let normal = rot * mid;

        Some(normal.normalize())
    }

    pub fn rect_on_perp(
        &self,
        ix: usize,
        offset: f32,
        width: f32,
        height: f32,
    ) -> Option<Rect> {
        let node = self.points.get(ix)?;
        let node = na::Vector2::new(node.x, node.y);
        let norm = self.unit_normal(ix)?;

        let center = node + (norm * offset);

        let rw = width / 2.0;
        let rh = height / 2.0;

        let center = Point::new(center[0], center[1]);
        let diag = Point::new(rw, rh);

        Some(Rect::new(center - diag, center + diag))
    }
}

// NB: this assumes that the path name is of the form
// "path_name#seq_id:start-end", where seq_id is a string, and start
// and end are unsigned integers
pub fn path_name_chr_range(path_name: &[u8]) -> Option<(&[u8], usize, usize)> {
    let pos_start_ix = path_name.find_byte(b'#')?;

    if pos_start_ix + 1 >= path_name.len() {
        return None;
    }

    let pos_str = &path_name[pos_start_ix + 1..];

    let seq_id_end = pos_str.find_byte(b':')?;
    let range_mid = pos_str.find_byte(b'-')?;

    if range_mid + 1 >= pos_str.len() {
        return None;
    }

    let chr = &pos_str[..seq_id_end];

    let start_str = pos_str[seq_id_end + 1..range_mid].to_str().ok()?;
    let start: usize = start_str.parse().ok()?;

    let end_str = pos_str[range_mid + 1..].to_str().ok()?;
    let end: usize = end_str.parse().ok()?;

    Some((chr, start, end))
}

pub fn path_name_range(path_name: &[u8]) -> Option<(usize, usize)> {
    let mut range_split = path_name.split_str(":");
    let _name = range_split.next()?;
    let range = range_split.next()?;

    let mut start_end = range.split_str("-");

    let start = start_end.next()?;
    let start_str = start.to_str().ok()?;
    let start = start_str.parse().ok()?;

    let end = start_end.next()?;
    let end_str = end.to_str().ok()?;
    let end = end_str.parse().ok()?;

    Some((start, end))
}

pub fn path_name_offset(path_name: &[u8]) -> Option<usize> {
    path_name_range(path_name).map(|(s, _)| s)
    /*
    let mut range_split = path_name.split_str(":");
    let _name = range_split.next()?;
    let range = range_split.next()?;

    let mut start_end = range.split_str("-");
    let start = start_end.next()?;

    let start_str = start.to_str().ok()?;
    start_str.parse().ok()
    */
}

pub fn path_step_range(
    steps: &[(Handle, StepPtr, usize)],
    offset: Option<usize>,
    start: usize,
    end: usize,
) -> Option<&[(Handle, StepPtr, usize)]> {
    let offset = offset.unwrap_or(0);

    let len = end - start;

    let start = start.checked_sub(offset).unwrap_or(0);
    let end = end.checked_sub(offset).unwrap_or(start + len);

    let (start, end) = {
        let start = steps.binary_search_by_key(&start, |(_, _, p)| *p);

        let end = steps.binary_search_by_key(&end, |(_, _, p)| *p);

        let (start, end) = match (start, end) {
            (Ok(s), Ok(e)) => (s, e),
            (Ok(s), Err(e)) => (s, e),
            (Err(s), Ok(e)) => (s, e),
            (Err(s), Err(e)) => (s, e),
        };

        let end = end.min(steps.len());

        Some((start, end))
    }?;

    Some(&steps[start..end])
}

pub fn path_step_radius(
    steps: &[(Handle, StepPtr, usize)],
    nodes: &[Node],
    step_ix: usize,
    radius: f32,
) -> FxHashSet<NodeId> {
    let (handle, _, _) = steps[step_ix];
    let node = handle.id();
    let node_ix = (node.0 as usize) - 1;

    let origin = nodes[node_ix].center();

    let rad_sqr = radius * radius;

    steps
        .iter()
        .filter_map(|(handle, _, _)| {
            let ix = (handle.id().0 - 1) as usize;
            let pos = nodes.get(ix)?.center();

            if pos.dist_sqr(origin) <= rad_sqr {
                let id = NodeId::from((ix + 1) as u64);
                Some(id)
            } else {
                None
            }
        })
        .collect()

    /*
    nodes
        .iter()
        .enumerate()
        .filter_map(|(ix, node_loc)| {
            let pos = node_loc.center();
            if pos.dist_sqr(origin) <= rad_sqr {
                let id = NodeId::from((ix + 1) as u64);
                Some(id)
            } else {
                None
            }
        })
        .collect()
        */
}

pub fn cluster_annotations(
    steps: &[(Handle, StepPtr, usize)],
    nodes: &[Node],
    view: View,
    node_labels: &FxHashMap<NodeId, Vec<String>>,
    radius: f32,
) -> FxHashMap<NodeId, (Point, Vec<String>)> {
    let mut cluster_range_ix: Option<(usize, usize)> = None;
    let mut cluster_start_pos: Option<Point> = None;
    let mut current_cluster: Vec<String> = Vec::new();

    let mut clusters: FxHashMap<(usize, usize), Vec<String>> =
        FxHashMap::default();

    let view_matrix = view.to_scaled_matrix();
    let to_screen = |p: Point| {
        let v = glm::vec4(p.x, p.y, 0.0, 1.0);
        let v_ = view_matrix * v;
        Point::new(v_[0], v_[1])
    };

    for (ix, (handle, _, _)) in steps.iter().enumerate() {
        let node = handle.id();

        if let Some(labels) = node_labels.get(&node) {
            let node_ix = (node.0 - 1) as usize;
            let node_pos = to_screen(nodes[node_ix].center());

            if let Some(start_pos) = cluster_start_pos {
                if node_pos.dist(start_pos) <= radius {
                    cluster_range_ix.as_mut().map(|(_, end)| *end = ix);
                    current_cluster.extend_from_slice(labels);
                } else {
                    clusters.insert(
                        cluster_range_ix.unwrap(),
                        current_cluster.clone(),
                    );
                    current_cluster.clear();

                    cluster_start_pos = Some(node_pos);
                    cluster_range_ix = Some((ix, ix));

                    current_cluster.extend_from_slice(labels);
                }
            } else {
                cluster_start_pos = Some(node_pos);
                cluster_range_ix = Some((ix, ix));

                current_cluster.extend_from_slice(labels);
            }
        }
    }

    // let mut res: FxHashMap<NodeId, Vec<String>> = FxHashMap::default();

    clusters
        .into_iter()
        .map(|((start, end), labels)| {
            let slice = &steps[start..=end];
            let (mid_handle, _, _) = slice[slice.len() / 2];

            let (start_h, _, _) = steps[start];
            let (end_h, _, _) = steps[end];

            let s_ix = (start_h.id().0 - 1) as usize;
            let e_ix = (end_h.id().0 - 1) as usize;

            let start_p = nodes[s_ix].p0;
            let end_p = nodes[e_ix].p1;

            let start_v = glm::vec2(start_p.x, start_p.y);
            let end_v = glm::vec2(end_p.x, end_p.y);

            let del = end_v - start_v;
            let rot_del = glm::rotate_vec2(&del, std::f32::consts::PI / 2.0);

            let rot_del_norm = rot_del.normalize();

            let offset = Point::new(rot_del_norm[0], rot_del_norm[1]);

            (mid_handle.id(), (offset, labels))
        })
        .collect()
}
