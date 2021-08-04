use std::{collections::HashMap, sync::Arc};

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

#[derive(Debug, Default, Clone)]
pub struct AnnotationLabelSet {
    // pub name: String,
    pub annot_file_name: String,
    pub column: String,
    pub path_name: String,
    pub show: bool,

    path_id: PathId,
    labels: FxHashMap<NodeId, Vec<String>>,
}

impl AnnotationLabelSet {
    pub fn new<C, R, K>(
        annotations: &C,
        path_id: PathId,
        path_name: &[u8],
        column: &K,
        labels: FxHashMap<NodeId, Vec<String>>,
    ) -> Self
    where
        C: AnnotationCollection<ColumnKey = K, Record = R>,
        R: AnnotationRecord<ColumnKey = K>,
        K: ColumnKey,
    {
        let annot_file_name = annotations.file_name().to_string();
        let column = column.to_string();
        let path_name = path_name.to_str().unwrap().to_string();

        let show = false;

        Self {
            annot_file_name,
            column,
            path_name,
            show,

            path_id,
            labels,
        }
    }

    pub fn labels(&self) -> &FxHashMap<NodeId, Vec<String>> {
        &self.labels
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnnotationFileType {
    Gff3,
    // Bed,
}

#[derive(Default, Clone)]
pub struct Annotations {
    annot_names: Vec<(String, AnnotationFileType)>,
    // gff3_annot_names: Vec<(String>,
    gff3_annotations: HashMap<String, Arc<Gff3Records>>,
    // bed_annotations: HashMap<String, BedRecords>,
    label_sets: HashMap<String, AnnotationLabelSet>,
}

impl Annotations {
    pub fn annot_names(&self) -> &[(String, AnnotationFileType)] {
        &self.annot_names
    }

    pub fn insert_gff3(&mut self, name: &str, records: Gff3Records) {
        let records = Arc::new(records);
        self.gff3_annotations.insert(name.to_string(), records);
        self.annot_names
            .push((name.to_string(), AnnotationFileType::Gff3));
    }

    pub fn remove_gff3(&mut self, name: &str) {
        self.gff3_annotations.remove(name);
        self.annot_names.retain(|(n, _)| n != name);
    }

    pub fn get_gff3(&self, name: &str) -> Option<&Arc<Gff3Records>> {
        self.gff3_annotations.get(name)
    }

    pub fn insert_label_set(
        &mut self,
        name: &str,
        label_set: AnnotationLabelSet,
    ) {
        self.label_sets.insert(name.to_string(), label_set);
    }

    pub fn get_label_set(&mut self, name: &str) -> Option<&AnnotationLabelSet> {
        self.label_sets.get(name)
    }
}

pub trait ColumnKey:
    Clone + Eq + Ord + std::hash::Hash + std::fmt::Display
{
}

impl<T> ColumnKey for T where
    T: Clone + Eq + Ord + std::hash::Hash + std::fmt::Display
{
}

pub trait AnnotationRecord {
    type ColumnKey: ColumnKey;

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

    fn is_column_optional(key: &Self::ColumnKey) -> bool;
}

pub trait AnnotationCollection {
    type ColumnKey: ColumnKey;
    type Record: AnnotationRecord<ColumnKey = Self::ColumnKey>;

    fn file_name(&self) -> &str;

    fn len(&self) -> usize;

    fn all_columns(&self) -> Vec<Self::ColumnKey>;

    fn mandatory_columns(&self) -> Vec<Self::ColumnKey>;

    fn optional_columns(&self) -> Vec<Self::ColumnKey>;

    fn records(&self) -> &[Self::Record];

    fn is_column_optional(key: &Self::ColumnKey) -> bool {
        Self::Record::is_column_optional(key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
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
