use futures::executor::ThreadPool;
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

use crossbeam::{atomic::AtomicCell, channel::Sender};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bstr::ByteSlice;

use rustc_hash::FxHashSet;

use anyhow::Result;
use egui::emath::Numeric;

use crate::{
    annotations::Gff3Column,
    app::AppMsg,
    asynchronous::AsyncResult,
    geometry::Point,
    graph_query::{GraphQuery, GraphQueryWorker},
    gui::{windows::overlays::OverlayCreatorMsg, GuiMsg},
    overlays::OverlayData,
};

use crate::annotations::{Gff3Record, Gff3Records};

use crate::gui::windows::{
    file::FilePicker, filters::*, graph_picker::PathPicker,
};

pub struct Gff3RecordList {
    filtered_records: Vec<usize>,

    offset: usize,
    slot_count: usize,

    filter_open: bool,
    filter: Gff3Filter,

    column_picker_open: bool,
    enabled_columns: EnabledColumns,

    path_picker_open: bool,
    path_picker: PathPicker,
    active_path: Option<(PathId, String)>,

    file_picker: FilePicker,
    file_picker_open: bool,

    gff3_load_result: Option<AsyncResult<Result<Gff3Records>>>,

    overlay_creator: Gff3OverlayCreator,
    overlay_creator_open: bool,
}

struct EnabledColumns {
    source: bool,
    type_: bool,

    score: bool,
    frame: bool,

    attributes: HashMap<Vec<u8>, bool>,
}

impl Gff3RecordList {
    pub const ID: &'static str = "gff_record_list_window";

    pub fn new(
        path_picker: PathPicker,
        new_overlay_tx: Sender<OverlayCreatorMsg>,
    ) -> Self {
        let filtered_records = Vec::new();

        let filter = Gff3Filter::default();
        let enabled_columns = EnabledColumns::default();

        let pwd = std::fs::canonicalize("./").unwrap();
        let file_picker = FilePicker::new(
            egui::Id::with(egui::Id::new(Self::ID), "file_picker"),
            pwd,
        )
        .unwrap();

        let overlay_creator = Gff3OverlayCreator::new(new_overlay_tx);

        Self {
            // records,
            filtered_records,

            offset: 0,
            slot_count: 20,

            filter_open: false,
            filter,

            column_picker_open: false,
            enabled_columns,

            active_path: None,

            path_picker_open: false,
            path_picker,

            file_picker_open: false,
            file_picker,

            gff3_load_result: None,

            overlay_creator_open: false,
            overlay_creator,
        }
    }

    pub fn update_records(&mut self, records: &Gff3Records) {
        self.filter = Gff3Filter::new(records);
        self.enabled_columns = EnabledColumns::new(records);
        self.overlay_creator
            .column_picker
            .update_attributes(records);
    }

    fn ui_row(
        &self,
        records: &Gff3Records,
        record: &Gff3Record,
        ui: &mut egui::Ui,
    ) -> egui::Response {
        let mut resp = ui.label(format!("{}", record.seq_id().as_bstr()));

        if self.enabled_columns.source {
            resp =
                resp.union(ui.label(format!("{}", record.source().as_bstr())));
        }

        if self.enabled_columns.type_ {
            resp =
                resp.union(ui.label(format!("{}", record.type_().as_bstr())));
        }
        resp = resp.union(ui.label(format!("{}", record.start())));
        resp = resp.union(ui.label(format!("{}", record.end())));

        if self.enabled_columns.frame {
            resp =
                resp.union(ui.label(format!("{}", record.frame().as_bstr())));
        }

        let mut keys = records.attribute_keys.iter().collect::<Vec<_>>();
        keys.sort_by(|k1, k2| k1.cmp(k2));

        let attrs = record.attributes();

        for key in keys {
            if self.enabled_columns.attributes.get(key) == Some(&true) {
                let label = if let Some(values) = attrs.get(key) {
                    let mut contents = String::new();
                    for (ix, val) in values.into_iter().enumerate() {
                        if ix != 0 {
                            contents.push_str("; ");
                        }
                        contents.push_str(&format!("{}", val.as_bstr()));
                    }
                    contents
                } else {
                    "".to_string()
                };
                resp = resp.union(ui.label(label));
            }
        }

        ui.end_row();

        resp
    }

    fn select_record(
        &self,
        app_msg_tx: &crossbeam::channel::Sender<AppMsg>,
        graph_query: &GraphQuery,
        record: &Gff3Record,
    ) {
        let active_path_id =
            self.path_picker.active_path().map(|(id, _name)| id);

        if let Some(path_id) = active_path_id {
            if let Some(range) = graph_query.path_basepair_range(
                path_id,
                record.start(),
                record.end(),
            ) {
                let nodes = range
                    .into_iter()
                    .map(|(handle, _, _)| handle.id())
                    .collect::<FxHashSet<_>>();

                use crate::app::Select;

                let select = Select::Many { nodes, clear: true };
                let msg = AppMsg::Selection(select);
                app_msg_tx.send(msg).unwrap();
            }
        }
    }

    fn apply_filter(&mut self, records: &Gff3Records) {
        self.filtered_records.clear();

        eprintln!("applying filter");
        let total = records.records.len();

        let records = &records.records;
        let filter = &self.filter;
        let filtered_records = &mut self.filtered_records;

        filtered_records.extend(records.iter().enumerate().filter_map(
            |(ix, rec)| {
                if filter.filter_record(rec) {
                    Some(ix)
                } else {
                    None
                }
            },
        ));
        let filtered = self.filtered_records.len();
        eprintln!(
            "filter complete, showing {} out of {} records",
            filtered, total
        );

        self.offset = 0;
    }

    fn clear_filter(&mut self) {
        self.filtered_records.clear();
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        thread_pool: &ThreadPool,
        graph_query: &GraphQueryWorker,
        gui_msg_tx: &crossbeam::channel::Sender<GuiMsg>,
        app_msg_tx: &crossbeam::channel::Sender<AppMsg>,

        records: Option<&Arc<Gff3Records>>,
        // records: Option<&Gff3Records>,
        open: &mut bool,
    ) -> Option<egui::Response> {
        if let Some(query) = self.gff3_load_result.as_mut() {
            query.move_result_if_ready();
            self.file_picker.reset();
        }

        if let Some(gff3_result) = self
            .gff3_load_result
            .as_mut()
            .and_then(|r| r.take_result_if_ready())
        {
            match gff3_result {
                Ok(records) => {
                    let records = records.to_owned();
                    gui_msg_tx
                        .send(GuiMsg::Gff3RecordsLoaded(records))
                        .unwrap();
                }
                Err(err) => {
                    eprintln!("error loading GFF3 file: {}", err);
                }
            }
        }

        if let Some(records) = records {
            self.list_ui(ctx, open, graph_query, app_msg_tx, records)
        } else {
            self.load_ui(ctx, thread_pool, open)
        }
    }

    fn load_id() -> egui::Id {
        egui::Id::with(egui::Id::new(Self::ID), "load_records")
    }

    fn list_id() -> egui::Id {
        egui::Id::with(egui::Id::new(Self::ID), "record_list")
    }

    fn load_ui(
        &mut self,
        ctx: &egui::CtxRef,
        thread_pool: &ThreadPool,
        open: &mut bool,
    ) -> Option<egui::Response> {
        if self.file_picker.selected_path().is_some() {
            self.file_picker_open = false;
        }

        self.file_picker.ui(ctx, &mut self.file_picker_open);

        let resp = egui::Window::new("GFF3")
            .id(Self::load_id())
            .default_pos(egui::Pos2::new(600.0, 200.0))
            .collapsible(false)
            .open(open)
            .show(ctx, |mut ui| {
                if self.gff3_load_result.is_none() {
                    if ui.button("Choose GFF3 file").clicked() {
                        self.file_picker_open = true;
                    }

                    let label = if let Some(path) = self
                        .file_picker
                        .selected_path()
                        .and_then(|p| p.to_str())
                    {
                        ui.label(path)
                    } else {
                        ui.label("No file selected")
                    };

                    if ui.button("Load").clicked() {
                        if let Some(path) = self.file_picker.selected_path() {
                            let path_str = path.to_str();
                            eprintln!("Loading GFF3 file {:?}", path_str);
                            let path = path.to_owned();
                            let query =
                                AsyncResult::new(thread_pool, async move {
                                    println!("parsing gff3 file");
                                    let records =
                                        Gff3Records::parse_gff3_file(path);
                                    println!("parsing complete");
                                    records
                                });
                            self.gff3_load_result = Some(query);
                        }
                    }
                } else {
                    ui.label("Loading file");
                }
            });

        resp
    }

    fn list_ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        graph_query: &GraphQueryWorker,
        // gui_msg_tx: &crossbeam::channel::Sender<GuiMsg>,
        app_msg_tx: &crossbeam::channel::Sender<AppMsg>,
        records: &Arc<Gff3Records>,
    ) -> Option<egui::Response> {
        let active_path_name = self
            .path_picker
            .active_path()
            .map(|(_id, name)| name.to_owned());

        self.filter.ui(ctx, &mut self.filter_open);

        self.path_picker.ui(ctx, &mut self.path_picker_open);

        if let Some(path) = self.path_picker.active_path().map(|(p, _)| p) {
            self.overlay_creator.ui(
                ctx,
                graph_query,
                &mut self.overlay_creator_open,
                path,
                records.clone(),
                &self.filtered_records,
            );
        }

        let resp = egui::Window::new("GFF3")
            .id(Self::list_id())
            .default_pos(egui::Pos2::new(600.0, 200.0))
            .collapsible(true)
            .open(open)
            // .resizable(true)
            .show(ctx, |mut ui| {
                ui.set_min_height(200.0);
                ui.set_max_height(ui.input().screen_rect.height() - 100.0);

                ui.horizontal(|ui| {
                    let filter_config_open = self.filter_open;
                    if ui
                        .selectable_label(
                            filter_config_open,
                            "Configure filter",
                        )
                        .clicked()
                    {
                        self.filter_open = !self.filter_open;
                    }

                    let column_picker_open = self.column_picker_open;

                    if ui
                        .selectable_label(column_picker_open, "Enabled columns")
                        .clicked()
                    {
                        self.column_picker_open = !self.column_picker_open;
                    }
                });

                ui.horizontal(|ui| {
                    if ui.button("Apply filter").clicked() {
                        self.apply_filter(records);
                    }

                    if ui.button("Clear filter").clicked() {
                        self.clear_filter();
                    }
                });

                ui.horizontal(|ui| {
                    let path_picker_btn = {
                        let label = if let Some(name) = &active_path_name {
                            format!("Path: {}", name)
                        } else {
                            "Select a path".to_string()
                        };

                        ui.button(label)
                    };

                    if path_picker_btn.clicked() {
                        self.path_picker_open = !self.path_picker_open;
                    }

                    let overlay_creator_btn = ui.button("Overlay creator");

                    if overlay_creator_btn.clicked() {
                        self.overlay_creator_open = !self.overlay_creator_open;
                    }
                });

                let grid = egui::Grid::new("gff3_record_list_grid")
                    .striped(true)
                    .show(&mut ui, |ui| {
                        ui.label("seq_id");
                        if self.enabled_columns.source {
                            ui.label("source");
                        }
                        if self.enabled_columns.type_ {
                            ui.label("type");
                        }
                        ui.label("start");
                        ui.label("end");
                        if self.enabled_columns.frame {
                            ui.label("frame");
                        }

                        let mut keys =
                            records.attribute_keys.iter().collect::<Vec<_>>();
                        keys.sort_by(|k1, k2| k1.cmp(k2));

                        for key in keys {
                            if self.enabled_columns.attributes.get(key)
                                == Some(&true)
                            {
                                ui.label(format!("{}", key.as_bstr()));
                            }
                        }

                        ui.end_row();

                        for i in 0..self.slot_count {
                            let row_record = if self.filtered_records.is_empty()
                            {
                                records.records.get(self.offset + i).map(
                                    |record| {
                                        (
                                            self.ui_row(records, record, ui),
                                            record,
                                        )
                                    },
                                )
                            } else {
                                self.filtered_records
                                    .get(self.offset + i)
                                    .and_then(|&ix| {
                                        let record = records.records.get(ix)?;
                                        let row =
                                            self.ui_row(records, record, ui);
                                        Some((row, record))
                                    })
                            };

                            if let Some((row, record)) = row_record {
                                let row_interact = ui.interact(
                                    row.rect,
                                    egui::Id::new(ui.id().with(i)),
                                    egui::Sense::click(),
                                );

                                if row_interact.clicked() {
                                    self.select_record(
                                        app_msg_tx,
                                        graph_query.graph(),
                                        record,
                                    );
                                }
                                if row_interact.double_clicked() {
                                    app_msg_tx
                                        .send(AppMsg::GotoSelection)
                                        .unwrap();
                                }
                            }
                        }
                    });

                if grid.response.hover_pos().is_some() {
                    let scroll = ctx.input().scroll_delta;
                    if scroll.y.abs() >= 4.0 {
                        let sig = (scroll.y.signum() as isize) * -1;
                        let delta = sig * ((scroll.y.abs() as isize) / 4);

                        let mut offset = self.offset as isize;

                        offset += delta;

                        offset = offset.clamp(
                            0,
                            (records.records.len() - self.slot_count) as isize,
                        );
                        self.offset = offset as usize;
                    }
                }
            });

        if let Some(resp) = &resp {
            let pos = resp.rect.right_top();
            self.enabled_columns
                .ui(ctx, pos, &mut self.column_picker_open);
        }

        resp
    }
}

impl std::default::Default for EnabledColumns {
    fn default() -> Self {
        Self {
            source: true,
            type_: true,
            score: true,
            frame: true,
            attributes: Default::default(),
        }
    }
}

impl EnabledColumns {
    pub const ID: &'static str = "gff_enabled_columns_window";

    fn new(records: &Gff3Records) -> Self {
        let attributes = records
            .attribute_keys
            .iter()
            .map(|k| (k.to_owned(), false))
            .collect::<HashMap<_, _>>();

        Self {
            source: true,
            type_: true,
            score: true,
            frame: true,
            attributes,
        }
    }

    fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        pos: impl Into<egui::Pos2>,
        open: &mut bool,
    ) -> Option<egui::Response> {
        macro_rules! bool_label {
            ($ui:ident, $field:ident, $label:expr) => {
                if $ui.selectable_label(self.$field, $label).clicked() {
                    self.$field = !self.$field;
                }
            };
        }

        egui::Window::new("GFF3 Columns")
            .id(egui::Id::new(Self::ID))
            .fixed_pos(pos)
            .collapsible(false)
            .open(open)
            .show(ctx, |ui| {
                ui.set_max_height(ui.input().screen_rect.height() - 250.0);

                ui.label("Mandatory fields");
                ui.horizontal(|ui| {
                    bool_label!(ui, source, "Source");
                    bool_label!(ui, type_, "Type");

                    // bool_label!(ui, score, "Score");
                    bool_label!(ui, frame, "Frame");
                });

                ui.collapsing("Attributes", |mut ui| {
                    egui::ScrollArea::from_max_height(
                        ui.input().screen_rect.height() - 250.0,
                    )
                    .show(&mut ui, |ui| {
                        let mut enabled_attrs =
                            self.attributes.iter_mut().collect::<Vec<_>>();

                        enabled_attrs.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

                        for (_count, (key, enabled)) in
                            enabled_attrs.into_iter().enumerate()
                        {
                            if ui
                                .selectable_label(
                                    *enabled,
                                    key.to_str().unwrap(),
                                )
                                .clicked()
                            {
                                *enabled = !*enabled;
                            }
                        }
                    });
                });
            })
    }
}

#[derive(Debug)]
pub struct Gff3Filter {
    seq_id: FilterString,
    source: FilterString,
    type_: FilterString,

    start: FilterNum<usize>,
    end: FilterNum<usize>,

    score: FilterNum<f64>,

    frame: FilterString,

    attributes: HashMap<Vec<u8>, FilterString>,
    // attributes: ??
}

impl std::default::Default for Gff3Filter {
    fn default() -> Self {
        Self {
            seq_id: FilterString::default(),
            source: FilterString::default(),
            type_: FilterString::default(),

            start: FilterNum::default(),
            end: FilterNum::default(),

            score: FilterNum::default(),
            frame: FilterString::default(),

            attributes: Default::default(),
        }
    }
}

impl Gff3Filter {
    pub const ID: &'static str = "gff_filter_window";

    fn new(records: &Gff3Records) -> Self {
        let attributes = records
            .attribute_keys
            .iter()
            .map(|k| (k.to_owned(), FilterString::default()))
            .collect::<HashMap<_, _>>();

        Self {
            attributes,
            ..Gff3Filter::default()
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
    ) -> Option<egui::Response> {
        egui::Window::new("GFF3 Filter")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(600.0, 200.0))
            .open(open)
            .show(ctx, |ui| {
                ui.set_max_width(400.0);

                ui.collapsing("Mandatory fields", |ui| {
                    ui.label("seq_id");
                    self.seq_id.ui(ui);
                    ui.separator();

                    ui.label("source");
                    self.source.ui(ui);
                    ui.separator();

                    ui.label("type");
                    self.type_.ui(ui);
                    ui.separator();

                    ui.label("start");
                    self.start.ui(ui);
                    ui.separator();

                    ui.label("end");
                    self.end.ui(ui);
                    ui.separator();

                    ui.label("frame");
                    self.frame.ui(ui);
                    ui.separator();
                });

                ui.collapsing("Attributes", |mut ui| {
                    egui::ScrollArea::from_max_height(
                        ui.input().screen_rect.height() - 250.0,
                    )
                    // egui::ScrollArea::auto_sized()
                    .show(&mut ui, |ui| {
                        // ui.set_max_height(800.0);
                        let mut attr_filters =
                            self.attributes.iter_mut().collect::<Vec<_>>();

                        attr_filters.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

                        for (_count, (key, filter)) in
                            attr_filters.into_iter().enumerate()
                        {
                            ui.label(key.to_str().unwrap());
                            filter.ui(ui);
                            ui.separator();
                            // if count % 5 == 0 {
                            //     ui.end_row()
                            // }
                        }
                    });
                });

                if ui.button("debug print").clicked() {
                    eprintln!("seq_id: {:?}", self.seq_id);
                    eprintln!("source: {:?}", self.source);
                    eprintln!("type:   {:?}", self.type_);

                    eprintln!("start: {:?}", self.start);
                    eprintln!("end: {:?}", self.end);
                }
            })
    }

    fn attr_filter(&self, record: &Gff3Record) -> bool {
        // let active_filters = self.attributes.iter().filter(|(_, filter)| !matches!(filter, FilterStringOp::None))
        self.attributes.iter().all(|(key, filter)| {
            if matches!(filter.op, FilterStringOp::None) {
                return true;
            }

            if let Some(values) = record.attributes().get(key) {
                values.iter().any(|v| filter.filter_bytes(v))
            } else {
                false
            }
        })
    }

    fn filter_record(&self, record: &Gff3Record) -> bool {
        self.seq_id.filter_bytes(record.seq_id())
            && self.source.filter_bytes(record.source())
            && self.type_.filter_bytes(record.type_())
            && self.start.filter(record.start())
            && self.end.filter(record.end())
            // && self.score.filter(record.score())
            && self.frame.filter_bytes(record.frame())
            && self.attr_filter(record)
    }
}

#[derive(Default)]
pub struct Gff3ColumnPicker {
    columns: Vec<Gff3Column>,

    chosen_column: Option<usize>,
}

impl Gff3ColumnPicker {
    pub const ID: &'static str = "gff3_column_picker_window";

    pub fn update_attributes(&mut self, records: &Gff3Records) {
        self.chosen_column = None;

        self.columns.clear();

        self.columns.push(Gff3Column::SeqId);
        self.columns.push(Gff3Column::Source);
        self.columns.push(Gff3Column::Type);
        self.columns.push(Gff3Column::Start);
        self.columns.push(Gff3Column::End);
        self.columns.push(Gff3Column::Score);
        self.columns.push(Gff3Column::Strand);
        self.columns.push(Gff3Column::Frame);

        self.columns.extend(
            records
                .attribute_keys
                .iter()
                .cloned()
                .map(Gff3Column::Attribute),
        );

        self.columns.sort();
    }

    pub fn chosen_column(&self) -> Option<&Gff3Column> {
        let ix = self.chosen_column?;
        self.columns.get(ix)
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
    ) -> Option<egui::Response> {
        egui::Window::new("GFF3 Columns")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |mut ui| {
                egui::ScrollArea::from_max_height(
                    ui.input().screen_rect.height() - 250.0,
                )
                .show(&mut ui, |ui| {
                    let chosen_column = self.chosen_column;

                    for (ix, col) in self.columns.iter().enumerate() {
                        let active = chosen_column == Some(ix);
                        if ui
                            .selectable_label(active, col.to_string())
                            .clicked()
                        {
                            if active {
                                self.chosen_column = None;
                            } else {
                                self.chosen_column = Some(ix);
                            }
                        }
                    }
                });
            })
    }
}

pub struct Gff3OverlayCreator {
    overlay_name: String,

    new_overlay_tx: Sender<OverlayCreatorMsg>,
    overlay_query: Option<AsyncResult<OverlayData>>,

    column_picker: Gff3ColumnPicker,
    column_picker_open: bool,
}

fn gff3_column_hash_color(
    record: &Gff3Record,
    column: &Gff3Column,
) -> rgb::RGB<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::default();

    match column {
        Gff3Column::SeqId => record.seq_id().hash(&mut hasher),
        Gff3Column::Source => record.source().hash(&mut hasher),
        Gff3Column::Type => record.type_().hash(&mut hasher),
        Gff3Column::Start => record.start().hash(&mut hasher),
        Gff3Column::End => record.end().hash(&mut hasher),
        Gff3Column::Score => {
            // todo really gotta fix this
            let v = record.score().unwrap_or(0.0) as usize;
            v.hash(&mut hasher);
        }
        Gff3Column::Strand => record.strand().hash(&mut hasher),
        Gff3Column::Frame => record.frame().hash(&mut hasher),
        Gff3Column::Attribute(attr) => {
            if let Some(val) = record.attributes().get(attr) {
                val.hash(&mut hasher);
            }
        }
    }

    let (r, g, b) = crate::gluon::hash_node_color(hasher.finish());

    rgb::RGB::new(r, g, b)
}

impl Gff3OverlayCreator {
    pub const ID: &'static str = "gff3_overlay_creator_window";

    pub fn new(new_overlay_tx: Sender<OverlayCreatorMsg>) -> Self {
        Self {
            overlay_name: String::new(),
            new_overlay_tx,
            overlay_query: None,

            column_picker: Gff3ColumnPicker::default(),
            column_picker_open: false,
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        graph: &GraphQueryWorker,
        open: &mut bool,
        path_id: PathId,
        records: Arc<Gff3Records>,
        filtered_records: &[usize],
        // column: Option<&Gff3Column>,
    ) -> Option<egui::Response> {
        if let Some(query) = self.overlay_query.as_mut() {
            // dbg!();
            query.move_result_if_ready();
        }

        if let Some(ov_data) = self
            .overlay_query
            .as_mut()
            .and_then(|r| r.take_result_if_ready())
        {
            let msg = OverlayCreatorMsg::NewOverlay {
                name: self.overlay_name.clone(),
                data: ov_data,
            };

            self.overlay_name.clear();
            self.new_overlay_tx.send(msg).unwrap();

            self.overlay_query = None;
        }

        {
            let column_picker_open = &mut self.column_picker_open;

            self.column_picker.ui(ctx, column_picker_open);
        }

        let label = {
            let column_picker = &self.column_picker;
            let column = column_picker.chosen_column();

            if let Some(column) = column {
                format!("Use column {}", column)
            } else {
                format!("Choose column")
            }
        };

        egui::Window::new("Create Overlay")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |ui| {
                let column_picker_open = &mut self.column_picker_open;

                let column_picker_btn =
                    { ui.selectable_label(*column_picker_open, label) };

                if column_picker_btn.clicked() {
                    *column_picker_open = !*column_picker_open;
                }

                let name = &mut self.overlay_name;

                let _name_box = ui.horizontal(|ui| {
                    ui.label("Overlay name");
                    ui.separator();
                    ui.text_edit_singleline(name)
                });
                let column_picker = &self.column_picker;
                let column = column_picker.chosen_column();

                let create_overlay = ui.add(
                    egui::Button::new("Create overlay")
                        .enabled(column.is_some()),
                );

                if create_overlay.clicked() && self.overlay_query.is_none() {
                    println!("creating overlay");
                    if let Some(column) = column {
                        let indices = filtered_records
                            .iter()
                            .copied()
                            .collect::<Vec<_>>();

                        let column = column.to_owned();

                        let query = graph.run_query(move |graph| async move {
                            use rustc_hash::FxHashMap;

                            use rayon::prelude::*;

                            dbg!();

                            let t0 = std::time::Instant::now();
                            let colors_vec: Vec<(Vec<NodeId>, rgb::RGB<f32>)> =
                                indices
                                    .into_par_iter()
                                    .filter_map(|ix| {
                                        let record = records.records.get(ix)?;

                                        let color = gff3_column_hash_color(
                                            record, &column,
                                        );

                                        let steps = graph.path_basepair_range(
                                            path_id,
                                            record.start(),
                                            record.end(),
                                        )?;

                                        let ids = steps
                                            .into_iter()
                                            .map(|(h, _, _)| h.id())
                                            .collect();

                                        Some((ids, color))
                                    })
                                    .collect::<Vec<_>>();

                            println!(
                                "parallel processing took {} seconds",
                                t0.elapsed().as_secs_f64()
                            );

                            dbg!();

                            let t1 = std::time::Instant::now();
                            let mut node_colors: FxHashMap<
                                NodeId,
                                rgb::RGB<f32>,
                            > = FxHashMap::default();

                            for (ids, color) in colors_vec {
                                for id in ids {
                                    node_colors.insert(id, color);
                                }
                            }

                            println!(
                                "building color map took {} seconds",
                                t1.elapsed().as_secs_f64()
                            );

                            let t2 = std::time::Instant::now();
                            let mut data = vec![
                                rgb::RGB::new(0.3, 0.3, 0.3);
                                graph.node_count()
                            ];

                            for (id, color) in node_colors {
                                let ix = (id.0 - 1) as usize;
                                data[ix] = color;
                            }

                            println!(
                                "building color vector took {} seconds",
                                t2.elapsed().as_secs_f64()
                            );

                            OverlayData::RGB(data)
                        });

                        self.overlay_query = Some(query);
                    }
                }
            })
    }
}
