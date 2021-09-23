use std::{collections::HashMap, sync::Arc};

use anyhow::Result;

use crossbeam::atomic::AtomicCell;
use handlegraph::{
    handle::{Handle, NodeId},
    pathhandlegraph::*,
};

use handlegraph::packedgraph::paths::StepPtr;

use bstr::ByteSlice;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::gui::text::LabelPos;
use crate::{geometry::*, universe::Node, view::*};

use nalgebra_glm as glm;

pub mod bed;
pub mod gff;

pub use bed::*;
pub use gff::*;

#[derive(Debug, Default, Clone)]
pub struct LabelSet {
    positions: Vec<LabelPos>,
    // positions: Vec<(LabelPos, Vec<usize>)>,
    label_strings: Vec<String>,
}

impl LabelSet {
    pub fn add_at_world_point(
        &mut self,
        point: Point,
        text: &str,
        offset: Option<Point>,
    ) {
        let pos = LabelPos::World { point, offset };

        self.positions.push(pos);
        self.label_strings.push(text.to_string());
    }

    pub fn add_at_handle(&mut self, handle: Handle, text: &str) {
        let pos = LabelPos::Handle {
            handle,
            offset: None,
        };

        self.positions.push(pos);
        self.label_strings.push(text.to_string());
    }

    pub fn add_at_node(&mut self, node: NodeId, text: &str) {
        let handle = Handle::pack(node, false);
        self.add_at_handle(handle, text);
    }

    pub fn add_many_at<'a, 'b>(
        &'a mut self,
        pos: LabelPos,
        strings: impl Iterator<Item = &'b str>,
    ) {
        for text in strings {
            self.positions.push(pos);
            self.label_strings.push(text.to_string());
        }
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

pub struct QuadTree<T: Clone> {
    boundary: Rect,

    points: Vec<Point>,
    data: Vec<T>,

    north_west: Option<Box<QuadTree<T>>>,
    north_east: Option<Box<QuadTree<T>>>,

    south_west: Option<Box<QuadTree<T>>>,
    south_east: Option<Box<QuadTree<T>>>,
}

impl<T: Clone> QuadTree<T> {
    pub const NODE_CAPACITY: usize = 4;

    pub fn new(boundary: Rect) -> Self {
        Self {
            boundary,

            points: Vec::new(),
            data: Vec::new(),

            north_west: None,
            north_east: None,

            south_west: None,
            south_east: None,
        }
    }

    pub fn node_len(&self) -> usize {
        self.points.len()
    }

    pub fn is_leaf(&self) -> bool {
        self.north_west.is_none()
    }

    fn insert_child(
        child: Option<&mut Self>,
        point: Point,
        data: T,
    ) -> std::result::Result<(), T> {
        if let Some(child) = child {
            return child.insert(point, data);
        }

        Err(data)
    }

    fn children(&self) -> Option<[&QuadTree<T>; 4]> {
        let nw = self.north_west.as_deref()?;
        let ne = self.north_east.as_deref()?;
        let sw = self.south_west.as_deref()?;
        let se = self.south_east.as_deref()?;

        let children = [nw, ne, sw, se];

        Some(children)
    }

    fn children_mut(&mut self) -> Option<[&mut QuadTree<T>; 4]> {
        let nw = self.north_west.as_deref_mut()?;
        let ne = self.north_east.as_deref_mut()?;
        let sw = self.south_west.as_deref_mut()?;
        let se = self.south_east.as_deref_mut()?;

        let children = [nw, ne, sw, se];

        Some(children)
    }

    pub fn insert(
        &mut self,
        point: Point,
        data: T,
    ) -> std::result::Result<(), T> {
        if !self.boundary.contains(point) {
            return Err(data);
        }

        if self.node_len() < Self::NODE_CAPACITY && self.is_leaf() {
            self.points.push(point);
            self.data.push(data);
            return Ok(());
        }

        if self.is_leaf() {
            self.subdivide();
        }

        let children = [
            self.north_west.as_deref_mut(),
            self.north_east.as_deref_mut(),
            self.south_west.as_deref_mut(),
            self.south_east.as_deref_mut(),
        ];

        let mut data = data;
        for child in children {
            match Self::insert_child(child, point, data) {
                Ok(_) => return Ok(()),
                Err(d) => data = d,
            }
        }

        Err(data)
    }

    fn subdivide(&mut self) {
        let min = self.boundary.min();
        let max = self.boundary.max();

        let pt = |x, y| Point::new(x, y);

        let mid_x = min.x + ((max.x - min.x) / 2.0);
        let mid_y = min.y + ((max.y - min.y) / 2.0);

        // calculate the boundary rectangles for the children
        let top_left_bnd = Rect::new(pt(min.x, min.y), pt(mid_x, mid_y));
        let top_right_bnd = Rect::new(pt(mid_x, min.y), pt(max.x, mid_y));

        let btm_left_bnd = Rect::new(pt(min.x, mid_y), pt(mid_x, max.y));
        let btm_right_bnd = Rect::new(pt(mid_x, mid_y), pt(max.x, max.y));

        let mut top_left = Self::new(top_left_bnd);
        let mut top_right = Self::new(top_right_bnd);
        let mut btm_left = Self::new(btm_left_bnd);
        let mut btm_right = Self::new(btm_right_bnd);

        let move_to_child = |child: &mut Self| {
            let bnd = child.boundary;

            // TODO this should be done without cloning, but the data
            // will probably be Copy in most cases so it doesn't
            // matter for now
            for (point, data) in self.query_range(bnd) {
                if let Err(_) = child.insert(point, data.clone()) {
                    panic!("unexpected error when subdividing quadtree");
                }
            }
        };

        move_to_child(&mut top_left);
        move_to_child(&mut top_right);
        move_to_child(&mut btm_left);
        move_to_child(&mut btm_right);

        self.north_west = Some(Box::new(top_left));
        self.north_east = Some(Box::new(top_right));

        self.south_west = Some(Box::new(btm_left));
        self.south_east = Some(Box::new(btm_right));
    }

    pub fn query_range(&self, range: Rect) -> Vec<(Point, &T)> {
        let mut results = Vec::new();

        if !self.boundary.intersects(range) {
            return results;
        }

        for (&point, data) in self.points.iter().zip(self.data.iter()) {
            if range.contains(point) {
                results.push((point, data));
            }
        }

        if let Some(children) = self.children() {
            for child in children {
                results.extend(Self::child_range(child, range));
            }
        }

        results
    }

    fn child_range<'a>(
        child: &'a Self,
        range: Rect,
    ) -> impl Iterator<Item = (Point, &'a T)> {
        child.points.iter().zip(child.data.iter()).filter_map(
            move |(&point, data)| {
                if range.contains(point) {
                    Some((point, data))
                } else {
                    None
                }
            },
        )
    }
}

/*
pub struct QuadTree<const N: usize> {
    boundary: Rect,

    points: [Point; N],

    north_west: Option<Box<QuadTree<N>>>,
    north_east: Option<Box<QuadTree<N>>>,

    south_west: Option<Box<QuadTree<N>>>,
    south_east: Option<Box<QuadTree<N>>>,
}

impl<const N: usize> QuadTree<N> {
    pub fn new() -> Self {
        Self {
            boundary: Rect::default(),
            points: [
        }
    }
}
*/

// pub enum ClusterTreeNode {
//     Leaf {
// }

// pub struct ClusterTree {

// }

impl LabelSet {
    pub fn cluster(&self) -> Vec<LabelCluster> {
        // TODO this should be done using a spatial data structure/querying

        let mut clusters = Vec::new();

        // for

        clusters
    }
}

#[derive(Debug, Clone)]
pub struct LabelSets {
    pub label_sets: HashMap<String, Arc<LabelSet>>,
    // names: Vec<String>,
    // label_sets: Vec<LabelSet>,
    // counter: usize,
}

#[derive(Debug, Clone)]
pub struct LabelCluster {
    pub anchor_world: Point,
    pub offset: Option<Point>,

    pub text_indices: Vec<usize>,
}

pub struct LabelClusterCache {
    pub label_set: Arc<LabelSet>,

    pub clusters: Vec<LabelCluster>,

    pub view_scale: f32,
    pub radius: f32,
}
pub struct SuperCache {
    pub sub_caches: Vec<LabelClusterCache>,
    pub clusters: Vec<LabelCluster>,
}

impl SuperCache {
    pub fn merge_clusters(&mut self) {
        todo!();
    }
}

// pub struct LabelSet

#[derive(Debug, Clone)]
pub struct AnnotationLabelSet {
    pub annotation_name: String,
    pub label_set_name: String,
    pub column_str: String,
    pub column: AnnotationColumn,
    pub path_id: PathId,
    pub path_name: String,

    show: Arc<AtomicCell<bool>>,

    label_strings: Vec<String>,
    labels: FxHashMap<NodeId, Vec<usize>>,
}

impl AnnotationLabelSet {
    pub fn new<C, R, K>(
        annotations: &C,
        path_id: PathId,
        path_name: &[u8],
        column: &K,
        label_set_name: &str,
        label_strings: Vec<String>,
        labels: FxHashMap<NodeId, Vec<usize>>,
    ) -> Self
    where
        C: AnnotationCollection<ColumnKey = K, Record = R>,
        R: AnnotationRecord<ColumnKey = K>,
        K: ColumnKey,
    {
        let annotation_name = annotations.file_name().to_string();
        let column_str = column.to_string();
        let path_name = path_name.to_str().unwrap().to_string();

        let show = Arc::new(true.into());

        let column = C::wrap_column(column.to_owned());

        let label_set_name = label_set_name.to_owned();

        Self {
            annotation_name,
            label_set_name,

            column_str,
            column,

            path_name,
            show,

            path_id,
            label_strings,
            labels,
        }
    }

    pub fn name(&self) -> &str {
        &self.label_set_name
    }

    pub fn label_strings(&self) -> &[String] {
        &self.label_strings
    }

    // pub fn labels(&self) -> &FxHashMap<NodeId, Vec<String>> {
    pub fn labels(&self) -> &FxHashMap<NodeId, Vec<usize>> {
        &self.labels
    }

    pub fn is_visible(&self) -> bool {
        self.show.load()
    }

    pub fn set_visibility(&self, to: bool) {
        self.show.store(to);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnnotationFileType {
    Gff3,
    Bed,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum AnnotationColumn {
    Gff3(Gff3Column),
    Bed(BedColumn),
}

#[derive(Default, Clone)]
pub struct Annotations {
    annot_names: Vec<(String, AnnotationFileType)>,
    // gff3_annot_names: Vec<(String>,
    gff3_annotations: HashMap<String, Arc<Gff3Records>>,
    bed_annotations: HashMap<String, Arc<BedRecords>>,

    label_sets: HashMap<String, Arc<AnnotationLabelSet>>,
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

    pub fn insert_bed(&mut self, name: &str, records: BedRecords) {
        let records = Arc::new(records);
        self.bed_annotations.insert(name.to_string(), records);
        self.annot_names
            .push((name.to_string(), AnnotationFileType::Bed));
    }

    pub fn remove_bed(&mut self, name: &str) {
        self.bed_annotations.remove(name);
        self.annot_names.retain(|(n, _)| n != name);
    }

    pub fn get_bed(&self, name: &str) -> Option<&Arc<BedRecords>> {
        self.bed_annotations.get(name)
    }

    pub fn insert_label_set(
        &mut self,
        name: &str,
        label_set: AnnotationLabelSet,
    ) {
        self.label_sets
            .insert(name.to_string(), Arc::new(label_set));
    }

    pub fn get_label_set(
        &mut self,
        name: &str,
    ) -> Option<&Arc<AnnotationLabelSet>> {
        self.label_sets.get(name)
    }

    pub fn visible_label_sets(
        &self,
    ) -> impl Iterator<Item = &'_ Arc<AnnotationLabelSet>> + '_ {
        self.label_sets.values().filter(|ls| ls.is_visible())
    }

    pub fn label_sets(&self) -> &HashMap<String, Arc<AnnotationLabelSet>> {
        &self.label_sets
    }
}

pub trait ColumnKey:
    Clone + Eq + Ord + std::hash::Hash + std::fmt::Display + Send + Sync
{
    fn is_column_optional(key: &Self) -> bool;

    fn seq_id() -> Self;

    fn start() -> Self;

    fn end() -> Self;
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

    fn wrap_column(column: Self::ColumnKey) -> AnnotationColumn;
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
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClusterIndices {
    pub label_indices: Vec<usize>,
    pub offset_ix: usize,
}

pub struct ClusterCache {
    pub label_set: Arc<AnnotationLabelSet>,
    pub cluster_offsets: Vec<Point>,

    pub node_labels: FxHashMap<NodeId, ClusterIndices>,

    pub view_scale: f32,
    pub radius: f32,
}

impl ClusterCache {
    /*
    pub fn clusters(
        &self,
    ) -> impl Iterator<Item = (NodeId, Point, &'_ [usize])> + '_ {
        self.node_labels.iter().map(|(node, cluster_indices)| {
            (
                *node,
                self.cluster_offsets[cluster_indices.offset_ix],
                cluster_indices.label_indices.as_slice(),
            )
        })
    }
    */

    pub fn new_cluster(
        steps: &[(Handle, StepPtr, usize)],
        nodes: &[Node],
        label_set: &Arc<AnnotationLabelSet>,
        view: View,
        radius: f32,
    ) -> Self {
        let mut node_label_indices: FxHashMap<NodeId, ClusterIndices> =
            FxHashMap::default();
        let mut cluster_offsets: Vec<Point> = Vec::new();

        let mut cluster_range_ix: Option<(usize, usize)> = None;
        let mut cluster_start_pos: Option<Point> = None;
        let mut current_cluster: Vec<usize> = Vec::new();

        let mut clusters: FxHashMap<(usize, usize), Vec<usize>> =
            FxHashMap::default();

        let view_matrix = view.to_scaled_matrix();
        let to_screen = |p: Point| {
            let v = glm::vec4(p.x, p.y, 0.0, 1.0);
            let v_ = view_matrix * v;
            Point::new(v_[0], v_[1])
        };

        for (ix, (handle, _, _)) in steps.iter().enumerate() {
            let node = handle.id();

            if let Some(label_indices) = label_set.labels.get(&node) {
                let node_ix = (node.0 - 1) as usize;
                let node_pos = to_screen(nodes[node_ix].center());

                if let Some(start_pos) = cluster_start_pos {
                    if node_pos.dist(start_pos) <= radius {
                        cluster_range_ix.as_mut().map(|(_, end)| *end = ix);
                        current_cluster.extend_from_slice(label_indices);
                    } else {
                        clusters.insert(
                            cluster_range_ix.unwrap(),
                            current_cluster.clone(),
                        );
                        current_cluster.clear();

                        cluster_start_pos = Some(node_pos);
                        cluster_range_ix = Some((ix, ix));

                        current_cluster.extend_from_slice(label_indices);
                    }
                } else {
                    cluster_start_pos = Some(node_pos);
                    cluster_range_ix = Some((ix, ix));

                    current_cluster.extend_from_slice(label_indices);
                }
            }
        }

        for ((start, end), cluster_label_indices) in clusters {
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

            let cluster_indices = ClusterIndices {
                label_indices: cluster_label_indices,
                offset_ix: cluster_offsets.len(),
            };

            node_label_indices.insert(mid_handle.id(), cluster_indices);
            cluster_offsets.push(offset);
        }

        Self {
            label_set: label_set.clone(),
            cluster_offsets,
            node_labels: node_label_indices,

            view_scale: view.scale,
            radius,
        }
    }

    pub fn rebuild_cluster(
        &mut self,
        steps: &[(Handle, StepPtr, usize)],
        nodes: &[Node],
        view: View,
        radius: f32,
    ) -> bool {
        if (view.scale - self.view_scale).abs() < 0.0001
            && radius == self.radius
        {
            return false;
        }

        self.view_scale = view.scale;
        self.radius = radius;

        self.cluster_offsets.clear();
        self.node_labels.clear();

        let mut cluster_range_ix: Option<(usize, usize)> = None;
        let mut cluster_start_pos: Option<Point> = None;
        let mut current_cluster: Vec<usize> = Vec::new();

        let label_set = &self.label_set;

        let mut clusters: FxHashMap<(usize, usize), Vec<usize>> =
            FxHashMap::default();

        let view_matrix = view.to_scaled_matrix();
        let to_screen = |p: Point| {
            let v = glm::vec4(p.x, p.y, 0.0, 1.0);
            let v_ = view_matrix * v;
            Point::new(v_[0], v_[1])
        };

        for (ix, (handle, _, _)) in steps.iter().enumerate() {
            let node = handle.id();

            if let Some(label_indices) = label_set.labels.get(&node) {
                let node_ix = (node.0 - 1) as usize;
                let node_pos = to_screen(nodes[node_ix].center());

                if let Some(start_pos) = cluster_start_pos {
                    if node_pos.dist(start_pos) <= radius {
                        cluster_range_ix.as_mut().map(|(_, end)| *end = ix);
                        current_cluster.extend_from_slice(label_indices);
                    } else {
                        clusters.insert(
                            cluster_range_ix.unwrap(),
                            current_cluster.clone(),
                        );
                        current_cluster.clear();

                        cluster_start_pos = Some(node_pos);
                        cluster_range_ix = Some((ix, ix));

                        current_cluster.extend_from_slice(label_indices);
                    }
                } else {
                    cluster_start_pos = Some(node_pos);
                    cluster_range_ix = Some((ix, ix));

                    current_cluster.extend_from_slice(label_indices);
                }
            }
        }

        for ((start, end), cluster_label_indices) in clusters {
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

            let cluster_indices = ClusterIndices {
                label_indices: cluster_label_indices,
                offset_ix: self.cluster_offsets.len(),
            };

            self.node_labels.insert(mid_handle.id(), cluster_indices);
            self.cluster_offsets.push(offset);
        }

        true
    }
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

pub fn record_column_hash_color<R, K>(
    record: &R,
    column: &K,
) -> Option<rgb::RGBA<f32>>
where
    R: AnnotationRecord<ColumnKey = K>,
    K: ColumnKey,
{
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::default();

    if column == &K::start() {
        record.start().hash(&mut hasher);
    } else if column == &K::end() {
        record.end().hash(&mut hasher);
    } else {
        record.get_all(column).hash(&mut hasher);
    }

    let (r, g, b) = crate::overlays::hash_node_color(hasher.finish());

    Some(rgb::RGBA::new(r, g, b, 1.0))
}
