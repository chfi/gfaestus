use gfa::gfa::Orientation;
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

use crossbeam::{
    atomic::AtomicCell,
    channel::{Receiver, Sender},
};
use std::sync::Arc;

use bstr::{BStr, ByteSlice};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::graph_query::{GraphQuery, GraphQueryRequest, GraphQueryResp, GraphQueryWorker};
use crate::view::View;
use crate::{
    app::{AppMsg, Select},
    geometry::*,
};

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
    NextPage,
    PrevPage,
    SetPage(usize),
}

#[derive(Debug, Clone)]
pub struct PathListSlot {
    path_id: Arc<AtomicCell<Option<PathId>>>,
    path_name: Vec<u8>,
    fetched_path: Option<PathId>,

    head: StepPtr,
    tail: StepPtr,

    step_count: usize,
    base_count: usize,
}

impl std::default::Default for PathListSlot {
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

impl PathListSlot {
    pub fn path_id_cell(&self) -> &Arc<AtomicCell<Option<PathId>>> {
        &self.path_id
    }

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
        let path_id = self.path_id.load();
        if self.fetched_path == path_id || path_id.is_none() {
            return Some(());
        }

        self.fetch_path_id(graph_query, path_id.unwrap())
    }
}

pub struct PathDetails {
    pub(crate) path_details: PathListSlot,

    pub(crate) step_list: StepList,
}

impl std::default::Default for PathDetails {
    fn default() -> Self {
        Self {
            path_details: Default::default(),

            step_list: StepList::new(15),
        }
    }
}

impl PathDetails {
    const ID: &'static str = "path_details_window";

    pub fn ui(
        &mut self,
        open_path_details: &mut bool,
        graph_query: &GraphQuery,
        graph_query_worker: &GraphQueryWorker,
        ctx: &egui::CtxRef,
        node_details_id_cell: &AtomicCell<Option<NodeId>>,
        open_node_details: &mut bool,
        app_msg_tx: &Sender<AppMsg>,
    ) -> Option<egui::Response> {
        self.path_details.fetch(graph_query)?;

        if let Some(path) = self.path_details.path_id.load() {
            if self.step_list.fetched_path_id != Some(path) && self.step_list.step_query.is_none() {
                self.step_list.steps.clear();
                self.step_list.async_path_update(graph_query_worker, path);
            }
        }

        egui::Window::new("Path details")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(600.0, 200.0))
            .open(open_path_details)
            .show(ctx, |mut ui| {
                if let Some(path_id) = self.path_details.path_id.load() {
                    ui.set_min_height(200.0);
                    ui.set_max_width(300.0);

                    ui.label(format!(
                        "Path name: {}",
                        self.path_details.path_name.as_bstr()
                    ));

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label(format!("Step count: {}", self.path_details.step_count));

                        ui.separator();

                        ui.label(format!("Base count: {}", self.path_details.base_count));
                    });

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "First step: {}",
                            self.path_details.head.to_vector_value()
                        ));

                        ui.separator();

                        ui.label(format!(
                            "Last step: {}",
                            self.path_details.tail.to_vector_value()
                        ));
                    });

                    self.step_list.ui(
                        ui,
                        app_msg_tx,
                        graph_query,
                        node_details_id_cell,
                        open_node_details,
                    );

                    ui.shrink_width_to_current();
                } else {
                    ui.label("Examine a path by picking it from the path list");
                }
            })
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
        let paths = &self.all_paths;

        self.page_count = paths.len() / self.page_size;
        if paths.len() == self.page_size {
            self.page_count -= 1;
        }

        if self.update_slots {
            self.update_slots(graph_query, false);

            self.update_slots = false;
        }

        egui::Window::new("Paths")
            // .enabled(*show)
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(400.0, 200.0))
            .show(ctx, |mut ui| {
                ui.set_min_height(300.0);
                ui.set_max_width(200.0);

                if ui
                    .selectable_label(*open_path_details, "Path Details")
                    .clicked()
                {
                    *open_path_details = !*open_path_details;
                }

                let page = &mut self.page;
                let page_count = self.page_count;
                let update_slots = &mut self.update_slots;

                /*
                if ui.selectable_label(filter, "Filter by node selection").clicked() {
                    apply_filter.store(!filter);
                    *update_slots = true;
                }
                */

                ui.label(format!("Page {}/{}", *page + 1, page_count + 1));

                ui.horizontal(|ui| {
                    if ui.button("First").clicked() {
                        *page = 0;
                    }

                    if ui.button("Prev").clicked() {
                        if *page > 0 {
                            *page -= 1;
                            *update_slots = true;
                        }
                    }

                    if ui.button("Next").clicked() {
                        if *page < page_count {
                            *page += 1;
                            *update_slots = true;
                        }
                    }

                    if ui.button("Last").clicked() {
                        *page = page_count;
                    }
                });

                let path_id_cell = &self.path_details_id;

                egui::ScrollArea::auto_sized().show(&mut ui, |mut ui| {
                    egui::Grid::new("path_list_grid")
                        .striped(true)
                        .show(&mut ui, |ui| {
                            ui.label("Path");
                            ui.label("Step count");
                            ui.label("Base count");
                            ui.end_row();

                            for (ix, slot) in self.slots.iter().enumerate() {
                                // let slot = &slot.path_details;

                                if let Some(path_id) = slot.path_id.load() {
                                    let mut row = ui.label(format!("{}", slot.path_name.as_bstr()));

                                    row = row.union(ui.label(format!("{}", slot.step_count)));

                                    row = row.union(ui.label(format!("{}", slot.base_count)));

                                    let row_interact = ui.interact(
                                        row.rect,
                                        egui::Id::new(ui.id().with(ix)),
                                        egui::Sense::click(),
                                    );

                                    if row_interact.clicked() {
                                        path_id_cell.store(Some(path_id));

                                        *open_path_details = true;
                                    }

                                    ui.end_row();
                                }
                            }
                        });
                });

                ui.shrink_width_to_current();
            })
    }

    pub fn new(
        graph_query: &GraphQuery,
        page_size: usize,
        path_details_id: Arc<AtomicCell<Option<PathId>>>,
    ) -> Self {
        let graph = graph_query.graph();
        let path_count = graph.path_count();

        let page_size = page_size.min(path_count);

        let mut all_paths = graph.path_ids().collect::<Vec<_>>();
        all_paths.sort();

        let page = 0;
        let page_count = path_count / page_size;

        let mut slots: Vec<PathListSlot> = Vec::with_capacity(page_size);

        for &path in all_paths[0..page_size].iter() {
            slots.push(PathListSlot::default());
        }

        let update_slots = true;

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
            slot.path_id.store(None);
        }

        for (slot, path) in self.slots.iter_mut().zip(&paths[page_start..page_end]) {
            slot.fetch_path_id(graph_query, *path).unwrap();
        }

        self.update_slots = false;
    }
}

pub struct StepList {
    fetched_path_id: Option<PathId>,

    steps: Vec<(Handle, StepPtr, usize)>,

    page: usize,
    page_size: usize,
    page_count: usize,

    step_query: Option<Receiver<(PathId, Vec<(Handle, StepPtr, usize)>)>>,
}

impl StepList {
    fn new(page_size: usize) -> Self {
        Self {
            fetched_path_id: None,

            steps: Vec::new(),

            page: 0,
            page_size,
            page_count: 0,

            step_query: None,
        }
    }

    fn async_path_update(&mut self, graph_query_worker: &GraphQueryWorker, path: PathId) {
        if self.fetched_path_id == Some(path) {
            return;
        }

        let result_recv = graph_query_worker.run_query(
            move |graph_query| -> (PathId, Vec<(Handle, StepPtr, usize)>) {
                let graph = graph_query.graph();
                let path_pos = graph_query.path_positions();

                if let Some(steps) = graph.path_steps(path) {
                    let steps_vec = steps
                        .filter_map(|step| {
                            let handle = step.handle();
                            let (step_ptr, _) = step;
                            let base = path_pos.path_step_position(path, step_ptr)?;
                            Some((handle, step_ptr, base))
                        })
                        .collect::<Vec<_>>();

                    (path, steps_vec)
                } else {
                    return (path, Vec::new());
                }
            },
        );

        self.step_query = Some(result_recv);
    }

    fn update_for_path(&mut self, graph_query: &GraphQuery, path: PathId) -> Option<()> {
        if self.fetched_path_id == Some(path) {
            return Some(());
        }

        let graph = graph_query.graph();
        let steps = graph.path_steps(path)?;

        self.steps.clear();
        self.steps.extend(steps.filter_map(|step| {
            let handle = step.handle();
            let (step_ptr, _) = step;
            let base = graph.path_step_base_offset(path, step_ptr)?;
            Some((handle, step_ptr, base))
        }));

        self.page = 0;

        self.page_count = self.steps.len() / self.page_size;

        self.fetched_path_id = Some(path);

        Some(())
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        app_msg_tx: &Sender<AppMsg>,
        graph_query: &GraphQuery,
        node_details_id_cell: &AtomicCell<Option<NodeId>>,
        open_node_details: &mut bool,
    ) -> egui::InnerResponse<()> {
        {
            let mut query_complete = false;
            if let Some(query) = self.step_query.as_ref() {
                // println!("has query");
                if let Ok((path, result)) = query.try_recv() {
                    // println!("receiving query result");
                    query_complete = true;

                    self.steps = result;
                    self.fetched_path_id = Some(path);
                }
            }

            if query_complete {
                self.step_query = None;
            }
        }

        let steps = &self.steps;

        let page = &mut self.page;
        let page_count = self.page_count;

        ui.label(format!("Page {}/{}", *page + 1, page_count + 1));

        ui.horizontal(|ui| {
            if ui.button("First").clicked() {
                *page = 0;
            }

            if ui.button("Prev").clicked() {
                if *page > 0 {
                    *page -= 1;
                }
            }

            if ui.button("Next").clicked() {
                if *page < page_count {
                    *page += 1;
                }
            }

            if ui.button("Last").clicked() {
                *page = page_count;
            }
        });

        let select_path = ui.button("Select nodes in path");

        if select_path.clicked() {
            let nodes = self
                .steps
                .iter()
                .map(|(h, _, _)| h.id())
                .collect::<FxHashSet<_>>();
            let selection = AppMsg::Selection(Select::Many {
                nodes,
                clear: false,
            });
            app_msg_tx.send(selection).unwrap();
        }

        let page_start = (*page * self.page_size).min(steps.len() - (steps.len() % self.page_size));
        let page_end = (page_start + self.page_size).min(steps.len());

        let separator = || egui::Separator::default().spacing(1.0);

        egui::ScrollArea::auto_sized().show(ui, |mut ui| {
            egui::Grid::new("path_details_step_list")
                .spacing(Point { x: 10.0, y: 5.0 })
                .striped(true)
                .show(&mut ui, |ui| {
                    ui.label("Node");
                    ui.add(separator());

                    ui.label("Step");
                    ui.add(separator());

                    ui.label("Base pos");
                    ui.end_row();

                    for (slot_ix, (handle, step_ptr, pos)) in
                        steps[page_start..page_end].iter().enumerate()
                    {
                        let node_id = handle.id();

                        let mut row = if handle.is_reverse() {
                            ui.label(format!("{}-", node_id.0))
                        } else {
                            ui.label(format!("{}+", node_id.0))
                        };
                        row = row.union(ui.add(separator()));

                        row = row.union(ui.label(format!("{}", step_ptr.to_vector_value())));
                        row = row.union(ui.add(separator()));

                        row = row.union(ui.label(format!("{}", pos)));
                        ui.end_row();

                        let row_interact = ui.interact(
                            row.rect,
                            egui::Id::new(ui.id().with(slot_ix)),
                            egui::Sense::click(),
                        );

                        if row_interact.clicked() {
                            node_details_id_cell.store(Some(handle.id()));
                            *open_node_details = true;
                        }
                    }
                })
        })
    }
}