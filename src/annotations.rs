use anyhow::Result;

use handlegraph::{
    handle::{Handle, NodeId},
    pathhandlegraph::*,
};

use handlegraph::packedgraph::paths::StepPtr;

use bstr::ByteSlice;

use rustc_hash::FxHashMap;

use crate::{
    geometry::*, gluon::GraphHandle, graph_query::GraphQuery, universe::Node,
    view::*,
};

use nalgebra as na;
use nalgebra_glm as glm;

pub mod gff;

pub use gff::*;

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
