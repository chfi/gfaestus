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
    path_name: Vec<u8>,
    fetched_path: Option<PathId>,

    head: StepPtr,
    tail: StepPtr,

    step_count: usize,
    base_count: usize,
}

impl PathDetails {
    fn fetch_path_id(&mut self, graph_query: &GraphQuery, path: PathId) -> Option<()> {
        self.path_name.clear();
        let path_name = graph_query.graph().get_path_name(path)?;
        self.path_name.extend(path_name);

        self.head = graph_query.graph().path_first_step(path)?;
        self.tail = graph_query.graph().path_last_step(path)?;

        self.step_count = graph_query.graph().path_len(path)?;
        self.base_count = graph_query.graph().path_bases_len(path)?;

        self.path_id.store(Some(path));
        self.fetched_path = Some(path);

        Some(())
    }

    fn fetch(&mut self, graph_query: &GraphQuery) -> Option<()> {
        let path_id = self.path_id.load()?;
        if self.fetched_path == Some(path_id) {
            return Some(());
        }

        self.fetch_path_id(graph_query, path_id)
    }
}

impl std::default::Default for PathDetails {
    fn default() -> Self {
        Self {
            path_id: Arc::new(AtomicCell::new(None)),
            path_name: Vec::new(),
            fetched_path: None,

            head: StepPtr::null(),
            tail: StepPtr::null(),

            step_count: 0,
            base_count: 0,
        }
    }
}

impl PathList {
    const ID: &'static str = "path_list_window";

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        app_msg_tx: &Sender<AppMsg>,
        open_path_details: &mut bool,
        graph_query: &GraphQuery,
    ) -> Option<egui::Response> {
        unimplemented!();
    }

    pub fn new(
        graph_query: &GraphQuery,
        page_size: usize,
        path_details_id: Arc<AtomicCell<Option<PathId>>>,
    ) -> Self {
        let graph = graph_query.graph();
        let path_count = graph.path_count();

        let mut all_paths = graph.path_ids().collect::<Vec<_>>();
        all_paths.sort();

        let page = 0;
        let page_count = path_count / page_size;

        let mut slots: Vec<PathListSlot> = Vec::with_capacity(page_size);

        for &path in all_paths[0..page_size].iter() {
            slots.push(PathListSlot::default());
        }

        let update_slots = false;

        Self {
            all_paths,

            page,
            page_size,
            page_count,

            slots,

            update_slots,

            path_details_id,
        }
    }

    pub fn apply_msg(&mut self, msg: PathListMsg) {
        match msg {
            PathListMsg::NextPage => {
                if self.page < self.page_count {
                    self.page += 1;
                    self.update_slots = true;
                }
            }
            PathListMsg::PrevPage => {
                if self.page > 0 {
                    self.page -= 1;
                    self.update_slots = true;
                }
            }
            PathListMsg::SetPage(page) => {
                let page = page.clamp(0, self.page_count);
                if self.page != page {
                    self.page = page;
                    self.update_slots = true;
                }
            }
        }
    }

    fn update_slots(&mut self, graph_query: &GraphQuery, force_update: bool) {
        if !self.update_slots && !force_update {
            return;
        }

        let paths = &self.all_paths;

        let page_start =
            (self.page * self.page_size).min(paths.len() - (paths.len() % self.page_size));
        let page_end = (page_start + self.page_size).min(paths.len());

        for slot in self.slots.iter_mut() {
            slot.path_details.path_id.store(None);
        }

        for (slot, path) in self.slots.iter_mut().zip(&paths[page_start..page_end]) {
            let slot = &mut slot.path_details;
            slot.fetch_path_id(graph_query, *path).unwrap();
        }

        self.update_slots = false;
    }
}
