use futures::executor::ThreadPool;
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

use crossbeam::channel::Sender;
use std::{collections::HashMap, sync::Arc};

use bstr::ByteSlice;

use rustc_hash::{FxHashMap, FxHashSet};

use anyhow::Result;

use crate::{
    annotations::{AnnotationLabelSet, Gff3Column},
    app::AppMsg,
    asynchronous::AsyncResult,
    graph_query::{GraphQuery, GraphQueryWorker},
    gui::{windows::overlays::OverlayCreatorMsg, GuiMsg},
    overlays::OverlayData,
};

use crate::annotations::{
    AnnotationCollection, AnnotationRecord, Gff3Record, Gff3Records,
};

use super::{ColumnPickerMany, ColumnPickerOne, OverlayLabelSetCreator};

use crate::gui::windows::{
    file::FilePicker, filters::*, graph_picker::PathPicker,
};

pub struct Gff3RecordList {
    current_file: Option<String>,

    filtered_records: Vec<usize>,

    offset: usize,
    slot_count: usize,

    filter_open: bool,
    gff3_filters: HashMap<String, Gff3Filter>,

    column_picker_open: bool,
    gff3_enabled_columns: HashMap<String, ColumnPickerMany<Gff3Records>>,

    path_picker_open: bool,
    path_picker: PathPicker,

    file_picker: FilePicker,
    file_picker_open: bool,

    gff3_load_result: Option<AsyncResult<Result<Gff3Records>>>,

    overlay_creator: Gff3OverlayCreator,
    overlay_creator_open: bool,

    creator: OverlayLabelSetCreator,
}

impl Gff3RecordList {
    pub const ID: &'static str = "gff_record_list_window";

    pub fn calculate_annotations(
        &self,
        graph: &GraphQuery,
        records: &Gff3Records,
    ) -> Option<(PathId, FxHashMap<NodeId, Vec<String>>)> {
        if self.filtered_records.is_empty() {
            return None;
        }

        let (path_id, name) = self.path_picker.active_path()?;

        let offset = crate::annotations::path_name_offset(name.as_bytes());

        let steps = graph.path_pos_steps(path_id)?;

        let mut result: FxHashMap<NodeId, Vec<String>> = FxHashMap::default();

        for &record_ix in self.filtered_records.iter() {
            let record = records.records.get(record_ix)?;

            if let Some(range) = crate::annotations::path_step_range(
                &steps,
                offset,
                record.start(),
                record.end(),
            ) {
                if let Some(name) =
                    record.get_tag(b"Name").and_then(|n| n.first())
                {
                    if let Some((mid, _, _)) = range.get(range.len() / 2) {
                        let label = format!("{}", name.as_bstr());
                        result.entry(mid.id()).or_default().push(label);
                    }
                }
            }
        }

        for labels in result.values_mut() {
            labels.sort();
            labels.dedup();
            labels.shrink_to_fit();
        }

        Some((path_id, result))
    }

    pub fn new(
        path_picker: PathPicker,
        new_overlay_tx: Sender<OverlayCreatorMsg>,
    ) -> Self {
        let filtered_records = Vec::new();

        let pwd = std::fs::canonicalize("./").unwrap();
        let file_picker = FilePicker::new(
            egui::Id::with(egui::Id::new(Self::ID), "file_picker"),
            pwd,
        )
        .unwrap();

        let overlay_creator = Gff3OverlayCreator::new(new_overlay_tx);

        Self {
            current_file: None,

            filtered_records,

            offset: 0,
            slot_count: 20,

            filter_open: false,
            gff3_filters: HashMap::default(),

            column_picker_open: false,
            gff3_enabled_columns: HashMap::default(),

            path_picker_open: false,
            path_picker,

            file_picker_open: false,
            file_picker,

            gff3_load_result: None,

            overlay_creator_open: false,
            overlay_creator,

            creator: OverlayLabelSetCreator::new("overlay_label_set_creator"),
        }
    }

    // also hacky
    pub fn scroll_to_record_by_name(
        &mut self,
        records: &Gff3Records,
        name: &[u8],
    ) {
        let ix = self
            .filtered_records
            .iter()
            .enumerate()
            .find(|&(_ix, record_ix)| {
                let record = &records.records[*record_ix];

                if let Some(record_names) = record.get_tag(b"Name") {
                    record_names.iter().any(|rn| rn == name)
                } else {
                    false
                }
            })
            .map(|(ix, _)| ix);

        if let Some(ix) = ix {
            self.offset = ix;
        }
    }

    fn ui_row(
        &self,
        file_name: &str,
        records: &Gff3Records,
        record: &Gff3Record,
        ui: &mut egui::Ui,
    ) -> egui::Response {
        let mut resp = ui.label(format!("{}", record.seq_id().as_bstr()));

        use Gff3Column as Gff;

        let enabled_columns = self.gff3_enabled_columns.get(file_name).unwrap();

        if enabled_columns.get_column(&Gff::Source) {
            resp =
                resp.union(ui.label(format!("{}", record.source().as_bstr())));
        }

        if enabled_columns.get_column(&Gff::Type) {
            resp =
                resp.union(ui.label(format!("{}", record.type_().as_bstr())));
        }
        resp = resp.union(ui.label(format!("{}", record.start())));
        resp = resp.union(ui.label(format!("{}", record.end())));

        if enabled_columns.get_column(&Gff::Frame) {
            resp =
                resp.union(ui.label(format!("{}", record.frame().as_bstr())));
        }

        let mut keys = records.attribute_keys.iter().collect::<Vec<_>>();
        keys.sort_by(|k1, k2| k1.cmp(k2));

        let attrs = record.attributes();

        for key in keys {
            if enabled_columns.get_column(&Gff::Attribute(key.to_owned())) {
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
        let active_path = self.path_picker.active_path();

        if let Some((path_id, name)) = active_path {
            let mut start = record.start();
            let mut end = record.end();

            if let Some(offset) =
                crate::annotations::path_name_offset(name.as_bytes())
            {
                start -= offset;
                end -= offset;
            }

            if let Some(range) =
                graph_query.path_basepair_range(path_id, start, end)
            {
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

    fn apply_filter(&mut self, file_name: &str, records: &Gff3Records) {
        self.filtered_records.clear();

        eprintln!("applying filter");
        let total = records.records.len();

        let records = &records.records;
        let filter = self.gff3_filters.get(file_name).unwrap();
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

    fn list_id() -> egui::Id {
        egui::Id::with(egui::Id::new(Self::ID), "record_list")
    }

    pub fn active_path_id(&self) -> Option<PathId> {
        let (path, _) = self.path_picker.active_path()?;
        Some(path)
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        graph_query: &GraphQueryWorker,
        app_msg_tx: &crossbeam::channel::Sender<AppMsg>,
        file_name: &str,
        records: &Arc<Gff3Records>,
    ) -> Option<egui::Response> {
        let active_path_name = self
            .path_picker
            .active_path()
            .map(|(_id, name)| name.to_owned());

        if !self.gff3_enabled_columns.contains_key(file_name) {
            let mut enabled_columns: ColumnPickerMany<Gff3Records> =
                ColumnPickerMany::new(file_name);

            enabled_columns.update_columns(records);

            use Gff3Column as Gff;
            for col in [Gff::Source, Gff::Type, Gff::Frame] {
                enabled_columns.set_column(&col, true);
            }

            for col in [Gff::SeqId, Gff::Start, Gff::End, Gff::Strand] {
                enabled_columns.hide_column_from_gui(&col, true);
            }

            self.gff3_enabled_columns
                .insert(file_name.to_string(), enabled_columns);
        }

        {
            let filter = self
                .gff3_filters
                .entry(file_name.to_string())
                .or_insert(Gff3Filter::new(records));

            filter.ui(ctx, &mut self.filter_open);
        }

        if self.current_file.as_ref().map(|s| s.as_str()) != Some(file_name) {
            self.current_file = Some(file_name.to_string());
            self.apply_filter(file_name, records);
        }

        self.path_picker.ui(ctx, &mut self.path_picker_open);

        if let Some(path) = self.path_picker.active_path().map(|(p, _)| p) {
            if self
                .creator
                .current_annotation_file
                .as_ref()
                .map(|s| s.as_str())
                != Some(file_name)
            {
                self.creator.current_annotation_file =
                    Some(file_name.to_string());
                self.creator.column_picker_gff3.update_columns(records);
            }

            self.creator.ui(
                ctx,
                &self.overlay_creator.new_overlay_tx,
                app_msg_tx,
                graph_query,
                &mut self.overlay_creator_open,
                file_name,
                path,
                records.clone(),
                &self.filtered_records,
            );

            /*
            if self
                .overlay_creator
                .current_file
                .as_ref()
                .map(|s| s.as_str())
                != Some(file_name)
            {
                self.overlay_creator.current_file = Some(file_name.to_string());
                self.overlay_creator.column_picker.update_columns(records);
            }

            self.overlay_creator.ui(
                ctx,
                graph_query,
                &mut self.overlay_creator_open,
                path,
                records.clone(),
                &self.filtered_records,
            );
            */
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

                ui.label(file_name);
                ui.separator();

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
                        self.apply_filter(file_name, records);
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

                ui.horizontal(|ui| {
                    let apply_labels_btn = ui.add(
                        egui::Button::new("Apply annotations")
                            .enabled(active_path_name.is_some()),
                    );

                    let path_name_range = if let Some(name) = &active_path_name
                    {
                        let n = name.as_bytes();
                        crate::annotations::path_name_chr_range(n.as_bytes())
                    } else {
                        None
                    };

                    let range_filter_btn = ui.add(
                        egui::Button::new("Filter by path range")
                            .enabled(path_name_range.is_some()),
                    );

                    if let Some((chr, start, end)) = path_name_range {
                        if range_filter_btn.clicked() {
                            let filter =
                                self.gff3_filters.get_mut(file_name).unwrap();
                            filter.chr_range_filter(chr, start, end);
                        }
                    }

                    if apply_labels_btn.clicked() {
                        if let Some((path, labels)) = self
                            .calculate_annotations(graph_query.graph(), records)
                        {
                            app_msg_tx
                                .send(AppMsg::SetNodeLabels { path, labels })
                                .unwrap();
                        }
                    }
                });

                use Gff3Column as Gff;

                let enabled_columns =
                    self.gff3_enabled_columns.get(file_name).unwrap();

                let grid = egui::Grid::new("gff3_record_list_grid")
                    .striped(true)
                    .show(&mut ui, |ui| {
                        ui.label("seq_id");
                        if enabled_columns.get_column(&Gff::Source) {
                            ui.label("source");
                        }
                        if enabled_columns.get_column(&Gff::Type) {
                            ui.label("type");
                        }
                        ui.label("start");
                        ui.label("end");
                        if enabled_columns.get_column(&Gff::Frame) {
                            ui.label("frame");
                        }

                        let mut keys =
                            records.attribute_keys.iter().collect::<Vec<_>>();
                        keys.sort_by(|k1, k2| k1.cmp(k2));

                        for key in keys {
                            if enabled_columns
                                .get_column(&Gff::Attribute(key.to_owned()))
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
                                            self.ui_row(
                                                file_name, records, record, ui,
                                            ),
                                            record,
                                        )
                                    },
                                )
                            } else {
                                self.filtered_records
                                    .get(self.offset + i)
                                    .and_then(|&ix| {
                                        let record = records.records.get(ix)?;
                                        let row = self.ui_row(
                                            file_name, records, record, ui,
                                        );
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
            let enabled_columns =
                self.gff3_enabled_columns.get_mut(file_name).unwrap();
            enabled_columns.ui(
                ctx,
                pos,
                &mut self.column_picker_open,
                "Gff3 Columns",
            );
        }

        resp
    }
}

#[derive(Debug, Default)]
pub struct Gff3Filter {
    seq_id: FilterString,
    source: FilterString,
    type_: FilterString,

    start: FilterNum<usize>,
    end: FilterNum<usize>,

    score: FilterNum<f64>,

    frame: FilterString,

    attributes: HashMap<Vec<u8>, FilterString>,
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

    fn range_filter(&mut self, mut start: usize, mut end: usize) {
        if start > 0 {
            start -= 1;
        }

        end += 1;

        self.start.op = FilterNumOp::MoreThan;
        self.start.arg1 = start;

        self.end.op = FilterNumOp::LessThan;
        self.end.arg1 = end;
    }

    fn chr_range_filter(&mut self, seq_id: &[u8], start: usize, end: usize) {
        if let Ok(seq_id) = seq_id.to_str().map(String::from) {
            self.seq_id.op = FilterStringOp::ContainedIn;
            self.seq_id.arg = seq_id;
        }
        self.range_filter(start, end);
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
                    .show(&mut ui, |ui| {
                        let mut attr_filters =
                            self.attributes.iter_mut().collect::<Vec<_>>();

                        attr_filters.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

                        for (_count, (key, filter)) in
                            attr_filters.into_iter().enumerate()
                        {
                            ui.label(key.to_str().unwrap());
                            filter.ui(ui);
                            ui.separator();
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

pub struct Gff3OverlayCreator {
    overlay_name: String,

    new_overlay_tx: Sender<OverlayCreatorMsg>,
    overlay_query: Option<AsyncResult<OverlayData>>,

    column_picker: ColumnPickerOne<Gff3Records>,
    column_picker_open: bool,

    current_file: Option<String>,
}

pub(crate) fn gff3_column_hash_color(
    record: &Gff3Record,
    column: &Gff3Column,
) -> Option<rgb::RGB<f32>> {
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
            } else {
                return None;
            }
        }
    }

    let (r, g, b) = crate::gluon::hash_node_color(hasher.finish());

    Some(rgb::RGB::new(r, g, b))
}

impl Gff3OverlayCreator {
    pub const ID: &'static str = "gff3_overlay_creator_window";

    pub fn new(new_overlay_tx: Sender<OverlayCreatorMsg>) -> Self {
        Self {
            overlay_name: String::new(),
            new_overlay_tx,
            overlay_query: None,

            column_picker: ColumnPickerOne::new("gff3_overlay_creator_picker"),
            column_picker_open: false,

            current_file: None,
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
    ) -> Option<egui::Response> {
        if let Some(query) = self.overlay_query.as_mut() {
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

            self.column_picker
                .ui(ctx, column_picker_open, "GFF3 Columns");
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
                            use rayon::prelude::*;

                            use crate::annotations as annots;

                            dbg!();

                            let steps = graph.path_pos_steps(path_id).unwrap();

                            let offset = graph
                                .graph()
                                .get_path_name_vec(path_id)
                                .and_then(|name| {
                                    annots::path_name_offset(&name)
                                });

                            println!("using annotation offset {:?}", offset);

                            let t0 = std::time::Instant::now();
                            let colors_vec: Vec<(Vec<NodeId>, rgb::RGB<f32>)> =
                                indices
                                    .into_par_iter()
                                    .filter_map(|ix| {
                                        let record = records.records.get(ix)?;

                                        let color = gff3_column_hash_color(
                                            record, &column,
                                        )?;

                                        let range = annots::path_step_range(
                                            &steps,
                                            offset,
                                            record.start(),
                                            record.end(),
                                        )?;

                                        let ids = range
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
                            let applied_records_count = colors_vec.len();
                            println!(
                                "colored record count: {}",
                                applied_records_count
                            );
                            let colored_node_count: usize = colors_vec
                                .iter()
                                .map(|(nodes, _)| nodes.len())
                                .sum();
                            println!(
                                "colored node count: {}",
                                colored_node_count
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
                            let mut data =
                                vec![
                                    rgb::RGBA::new(0.3, 0.3, 0.3, 0.3);
                                    graph.node_count()
                                ];

                            for (id, color) in node_colors {
                                let ix = (id.0 - 1) as usize;
                                data[ix] = rgb::RGBA::new(
                                    color.r, color.g, color.b, 1.0,
                                );
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
