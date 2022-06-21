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

use bstr::ByteSlice;

use rustc_hash::FxHashSet;

use crate::{
    context::ContextMgr,
    gui::util::{grid_row_label, ColumnWidths},
    reactor::{Host, Outbox, Reactor},
};

use crate::gui::util as gui_util;

use crate::graph_query::GraphQuery;
use crate::{
    app::{AppMsg, Select},
    geometry::*,
};

pub struct PathList {
    all_paths: Vec<PathId>,

    path_details_id: Arc<AtomicCell<Option<PathId>>>,

    col_widths: ColumnWidths<3>,
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
            path_id: Arc::new(None.into()),
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

    fn fetch_path_id(
        &mut self,
        graph_query: &GraphQuery,
        path: PathId,
    ) -> Option<()> {
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

impl PathDetails {
    const ID: &'static str = "path_details_window";

    pub fn new(reactor: &Reactor) -> Self {
        Self {
            path_details: Default::default(),
            step_list: StepList::new(reactor, 15),
        }
    }

    pub fn ui(
        &mut self,
        open_path_details: &mut bool,
        graph_query: &GraphQuery,
        ctx: &egui::CtxRef,
        node_details_id_cell: &AtomicCell<Option<NodeId>>,
        open_node_details: &mut bool,
        app_msg_tx: &Sender<AppMsg>,
        ctx_mgr: &ContextMgr,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        self.path_details.fetch(graph_query)?;

        if let Some(path) = self.path_details.path_id.load() {
            if self.step_list.fetched_path_id != Some(path) {
                self.step_list.steps_host.call(path).unwrap();
                self.step_list.fetched_path_id = Some(path);
                self.step_list.update_filter = true;
            }
        }

        egui::Window::new("Path details")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(600.0, 200.0))
            .open(open_path_details)
            .show(ctx, |ui| {
                if let Some(_path_id) = self.path_details.path_id.load() {
                    ui.label(format!(
                        "Path name: {}",
                        self.path_details.path_name.as_bstr()
                    ));

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Step count: {}",
                            self.path_details.step_count
                        ));

                        ui.separator();

                        ui.label(format!(
                            "Base count: {}",
                            self.path_details.base_count
                        ));
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
                        ctx_mgr,
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
        _app_msg_tx: &Sender<AppMsg>,
        open_path_details: &mut bool,
        graph_query: &GraphQuery,
        ctx_mgr: &ContextMgr,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        let paths = &self.all_paths;

        egui::Window::new("Paths")
            .id(egui::Id::new(Self::ID))
            .show(ctx, |ui| {
                if ui
                    .selectable_label(*open_path_details, "Path Details")
                    .clicked()
                {
                    *open_path_details = !*open_path_details;
                }

                let scroll_align = gui_util::add_scroll_buttons(ui);

                let path_id_cell = &self.path_details_id;

                let num_rows = paths.len();
                let text_style = egui::TextStyle::Body;
                let row_height = ui.fonts()[text_style].row_height();

                let [w0, w1, w2] = self.col_widths.get();

                let header =
                    egui::Grid::new("path_list_grid_header").show(ui, |ui| {
                        let inner = grid_row_label(
                            ui,
                            egui::Id::new("path_list_grid_header__"),
                            &["Path", "Step count", "Base count"],
                            false,
                            Some(&[w0, w1, w2]),
                        );
                        self.col_widths.set_hdr(&inner.inner);
                    });

                gui_util::scrolled_area(ui, num_rows, scroll_align).show_rows(
                    ui,
                    row_height,
                    num_rows,
                    |ui, range| {
                        ui.set_min_width(header.response.rect.width());

                        let graph = graph_query.graph();

                        egui::Grid::new("path_list_grid").striped(true).show(
                            ui,
                            |ui| {
                                let take_n =
                                    range.start.max(range.end) - range.start;

                                for (ix, &path_id) in paths
                                    .iter()
                                    .enumerate()
                                    .skip(range.start)
                                    .take(take_n)
                                {
                                    // let slot = &slot.path_details;

                                    let path_name = graph
                                        .get_path_name_vec(path_id)
                                        .unwrap();

                                    let path_name =
                                        format!("{}", path_name.as_bstr());

                                    let step_count = format!(
                                        "{}",
                                        graph
                                            .path_len(path_id)
                                            .unwrap_or_default()
                                    );

                                    let base_count = format!(
                                        "{}",
                                        graph_query
                                            .path_positions
                                            .path_base_len(path_id)
                                            .unwrap_or_default()
                                    );

                                    let fields: [&str; 3] =
                                        [&path_name, &step_count, &base_count];

                                    let inner = grid_row_label(
                                        ui,
                                        egui::Id::new(ui.id().with(ix)),
                                        &fields,
                                        false,
                                        Some(&[w0, w1, w2]),
                                    );

                                    self.col_widths.set(&inner.inner);

                                    let row = inner.response;

                                    if row.clicked() {
                                        path_id_cell.store(Some(path_id));
                                        *open_path_details = true;
                                    }

                                    if row.hovered() {
                                        ctx_mgr.produce_context(|| path_id);
                                    }

                                    // if row.clicked_by(
                                    //     egui::PointerButton::Secondary,
                                    // ) {
                                    //     ctx_tx
                                    //         .send(ContextEntry::Path(path_id))
                                    //         .unwrap();
                                    // }
                                }
                            },
                        );
                    },
                );

                ui.shrink_width_to_current();
            })
    }

    pub fn new(
        graph_query: &GraphQuery,
        path_details_id: Arc<AtomicCell<Option<PathId>>>,
    ) -> Self {
        let graph = graph_query.graph();

        let mut all_paths = graph.path_ids().collect::<Vec<_>>();
        all_paths.sort();

        Self {
            all_paths,

            path_details_id,

            col_widths: Default::default(),
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct StepRange {
    from_ix: usize,
    to_ix: usize,

    from_pos: usize,
    to_pos: usize,

    path_base_len: usize,
}

impl StepRange {
    fn from_steps(
        path_base_len: usize,
        steps: &[(Handle, StepPtr, usize)],
    ) -> Self {
        let from_ix = 0;
        let to_ix = steps.len();

        let from_pos = 0;
        let to_pos = path_base_len;

        Self {
            from_ix,
            to_ix,

            from_pos,
            to_pos,

            path_base_len,
        }
    }
}

type StepsResult =
    std::result::Result<(PathId, usize, Vec<(Handle, StepPtr, usize)>), String>;

pub struct StepList {
    fetched_path_id: Option<PathId>,

    steps_host: Host<PathId, StepsResult>,
    latest_result: Option<StepsResult>,

    range_filter: StepRange,

    update_filter: bool,

    col_widths: ColumnWidths<3>,
}

impl StepList {
    fn new(reactor: &Reactor, page_size: usize) -> Self {
        let graph_query = reactor.graph_query.clone();

        let steps_host = reactor.create_host(
            move |_outbox: &Outbox<StepsResult>, path: PathId| {
                println!("in steps_host");
                dbg!();
                let graph = graph_query.graph();
                let path_pos = graph_query.path_positions();

                if let Some(steps) = graph.path_steps(path) {
                    dbg!();
                    let base_len = path_pos.path_base_len(path).unwrap();

                    let steps_vec = steps
                        .filter_map(|step| {
                            let handle = step.handle();
                            let (step_ptr, _) = step;
                            let base =
                                path_pos.path_step_position(path, step_ptr)?;
                            Some((handle, step_ptr, base))
                        })
                        .collect::<Vec<_>>();

                    Ok((path, base_len, steps_vec))
                } else {
                    dbg!();
                    Err("Path not found".to_string())
                }
            },
        );

        Self {
            fetched_path_id: None,

            steps_host,
            latest_result: None,

            range_filter: StepRange::default(),

            update_filter: false,

            col_widths: Default::default(),
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        app_msg_tx: &Sender<AppMsg>,
        _graph_query: &GraphQuery,
        node_details_id_cell: &AtomicCell<Option<NodeId>>,
        open_node_details: &mut bool,
        ctx_mgr: &ContextMgr,
    ) -> egui::InnerResponse<()> {
        if let Some(result) = self.steps_host.take() {
            if let Ok((_path, path_base_len, steps)) = &result {
                if self.update_filter {
                    self.range_filter =
                        StepRange::from_steps(*path_base_len, steps);

                    self.update_filter = false;
                }
            }

            self.latest_result = Some(result);
        }

        let steps = if let Some(Ok((_, len, steps))) = &self.latest_result {
            if self.update_filter {
                self.range_filter = StepRange::from_steps(*len, steps);

                self.update_filter = false;
            }
            steps.as_slice()
        } else {
            self.range_filter = StepRange::default();
            &[]
        };

        let scroll_align = gui_util::add_scroll_buttons(ui);

        let range_filter = &mut self.range_filter;

        ui.vertical(|ui| {
            let path_base_len = range_filter.path_base_len;

            let from_pos = &mut range_filter.from_pos;
            let to_pos = &mut range_filter.to_pos;

            let from_range = 0..=*to_pos;
            let to_range = *from_pos..=path_base_len;

            let from_drag =
                egui::DragValue::new::<usize>(from_pos).clamp_range(from_range);
            let to_drag =
                egui::DragValue::new::<usize>(to_pos).clamp_range(to_range);

            ui.horizontal(|ui| {
                ui.label("Filter by base pos");
                let _from_ui = ui.add(from_drag);
                let _to_ui = ui.add(to_drag);
            });

            let buttons = ui.horizontal(|ui| {
                let apply_btn = ui.button("Apply filter");
                let reset_btn = ui.button("Reset filter");

                (apply_btn, reset_btn)
            });

            let (apply_btn, reset_btn) = buttons.inner;

            if apply_btn.clicked() {
                range_filter.from_ix = match steps
                    .binary_search_by_key(from_pos, |(_, _, p)| *p)
                {
                    Ok(x) => x,
                    Err(x) => x,
                };

                range_filter.to_ix =
                    match steps.binary_search_by_key(to_pos, |(_, _, p)| *p) {
                        Ok(x) => x,
                        Err(x) => x,
                    };
            }

            if reset_btn.clicked() {
                *from_pos = 0;
                *to_pos = path_base_len;

                range_filter.from_ix = 0;
                range_filter.to_ix = steps.len();
            }
        });

        let steps = {
            let from = self.range_filter.from_ix;
            let to = self.range_filter.to_ix;

            let from = from.min(to);
            let to = to.min(steps.len());

            &steps[from..to]
        };

        let select_path = ui.button("Select nodes in path");

        if select_path.clicked() {
            let nodes = steps
                .iter()
                .map(|(h, _, _)| h.id())
                .collect::<FxHashSet<_>>();
            let selection = AppMsg::Selection(Select::Many {
                nodes,
                clear: false,
            });
            app_msg_tx.send(selection).unwrap();
        }

        let num_rows = steps.len();
        let text_style = egui::TextStyle::Body;
        let row_height = ui.fonts()[text_style].row_height();

        let [w0, w1, w2] = self.col_widths.get();

        let header =
            egui::Grid::new("path_details_step_list_header").show(ui, |ui| {
                let inner = grid_row_label(
                    ui,
                    egui::Id::new("path_details_step_list_header__"),
                    &["Path", "Step", "Base pos"],
                    false,
                    Some(&[w0, w1, w2]),
                );
                self.col_widths.set_hdr(&inner.inner);
            });

        gui_util::scrolled_area(ui, num_rows, scroll_align).show_rows(
            ui,
            row_height,
            num_rows,
            |ui, range| {
                ui.set_min_width(header.response.rect.width());

                egui::Grid::new("path_details_step_list")
                    .spacing(Point { x: 10.0, y: 5.0 })
                    .striped(true)
                    .show(ui, |ui| {
                        let take_n = range.start.max(range.end) - range.start;

                        for (slot_ix, (handle, step_ptr, pos)) in steps
                            .iter()
                            .enumerate()
                            .skip(range.start)
                            .take(take_n)
                        {
                            let node_id = handle.id();

                            let handle_str = if handle.is_reverse() {
                                format!("{}-", node_id.0)
                            } else {
                                format!("{}+", node_id.0)
                            };

                            let step_ptr_str =
                                format!("{}", step_ptr.to_vector_value());

                            let pos_str = format!("{}", pos);

                            let fields: [&str; 3] =
                                [&handle_str, &step_ptr_str, &pos_str];

                            let inner = grid_row_label(
                                ui,
                                egui::Id::new(ui.id().with(slot_ix)),
                                &fields,
                                false,
                                Some(&[w0, w1, w2]),
                            );

                            self.col_widths.set(&inner.inner);

                            let row = inner.response;

                            if row.clicked() {
                                node_details_id_cell.store(Some(handle.id()));
                                *open_node_details = true;
                            }

                            if row.hovered() {
                                ctx_mgr.produce_context(|| handle.id())
                            }

                            // if row.clicked_by(egui::PointerButton::Secondary) {
                            //     ctx_tx
                            //         .send(ContextEntry::Node(handle.id()))
                            //         .unwrap();
                            // }
                        }
                    })
            },
        )
    }
}
