use handlegraph::packedgraph::paths::StepPtr;
#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    packedgraph::index::OneBasedIndex,
    packedgraph::*,
    path_position::*,
    pathhandlegraph::*,
};

use crossbeam::{atomic::AtomicCell, channel::Sender};
use std::sync::Arc;

use bstr::{BStr, ByteSlice};

use crate::graph_query::{GraphQuery, GraphQueryRequest, GraphQueryResp};
use crate::view::View;
use crate::{app::AppMsg, geometry::*};

pub struct PathList {
    all_paths: Vec<PathId>,

    page: usize,
    page_size: usize,
    page_count: usize,

    slots: Vec<PathListSlot>,

    update_slots: bool,

    // apply_filter: AtomicCell<bool>,
    path_details_id: Arc<AtomicCell<Option<PathId>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathListMsg {
    // ApplyFilter(Option<bool>),
    NextPage,
    PrevPage,
    SetPage(usize),
    // SetFiltered(Vec<NodeId>),
}

#[derive(Debug, Default, Clone)]
pub struct PathListSlot {
    path_details: PathDetails,
}

#[derive(Debug, Clone)]
pub struct PathDetails {
    path_id: Arc<AtomicCell<Option<PathId>>>,
    path_name: String,
    fetched_path: Option<PathId>,

    head: StepPtr,
    tail: StepPtr,

    step_count: usize,
    base_count: usize,
}

impl std::default::Default for PathDetails {
    fn default() -> Self {
        Self {
            path_id: Arc::new(AtomicCell::new(None)),
            path_name: String::new(),
            fetched_path: None,

            head: StepPtr::null(),
            tail: StepPtr::null(),

            step_count: 0,
            base_count: 0,
        }
    }
}
