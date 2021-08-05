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
    annotations::{
        AnnotationCollection, AnnotationLabelSet, AnnotationRecord, BedColumn,
        BedRecord, BedRecords,
    },
    app::AppMsg,
    asynchronous::AsyncResult,
    graph_query::{GraphQuery, GraphQueryWorker},
    gui::{util::grid_row_label, windows::overlays::OverlayCreatorMsg, GuiMsg},
    overlays::OverlayData,
};

use super::{ColumnPickerMany, ColumnPickerOne, OverlayLabelSetCreator};

use crate::gui::windows::{
    file::FilePicker, filters::*, graph_picker::PathPicker,
};

pub struct BedRecordList {
    current_file: Option<String>,

    filtered_records: Vec<usize>,

    offset: usize,
    slot_count: usize,

    filter_open: bool,
    filters: HashMap<String, BedFilter>,

    column_picker_open: bool,
    enabled_columns: HashMap<String, ColumnPickerMany<BedRecords>>,

    path_picker_open: bool,
    path_picker: PathPicker,

    creator_open: bool,
    creator: OverlayLabelSetCreator,
    overlay_tx: Sender<OverlayCreatorMsg>,
}

impl BedRecordList {
    pub const ID: &'static str = "bed_record_list_window";

    pub fn new(
        path_picker: PathPicker,
        new_overlay_tx: Sender<OverlayCreatorMsg>,
    ) -> Self {
        let filtered_records = Vec::new();

        Self {
            current_file: None,

            filtered_records,

            offset: 0,
            slot_count: 20,

            filter_open: false,
            filters: HashMap::default(),

            column_picker_open: false,
            enabled_columns: HashMap::default(),

            path_picker_open: false,
            path_picker,

            creator_open: false,
            creator: OverlayLabelSetCreator::new(
                "bed_overlay_label_set_creator",
            ),
            overlay_tx: new_overlay_tx,
        }
    }

    pub fn scroll_to_label_record(
        &mut self,
        records: &BedRecords,
        column: &BedColumn,
        value: &[u8],
    ) {
        let ix = self
            .filtered_records
            .iter()
            .enumerate()
            .find(|&(_ix, record_ix)| {
                let record = &records.records[*record_ix];
                let column_values = record.get_all(column);
                column_values.iter().any(|&rec_val| rec_val == value)
            })
            .map(|(ix, _)| ix);

        if let Some(ix) = ix {
            self.offset = ix;
        }
    }

    fn ui_row(
        &self,
        ui: &mut egui::Ui,
        file_name: &str,
        records: &BedRecords,
        record: &BedRecord,
        index: usize,
    ) -> egui::Response {
        let mut fields: Vec<String> =
            vec![format!("{}", record.seq_id().as_bstr())];

        let enabled_columns = self.enabled_columns.get(file_name).unwrap();

        fields.push(format!("{}", record.start()));
        fields.push(format!("{}", record.end()));

        let columns = records.optional_columns();

        for key in columns.iter() {
            if enabled_columns.get_column(key) {
                let values = record.get_all(key);
                let label = {
                    let mut contents = String::new();
                    for (ix, val) in values.into_iter().enumerate() {
                        if ix != 0 {
                            contents.push_str("; ");
                        }
                        contents.push_str(&format!("{}", val.as_bstr()));
                    }
                    contents
                };

                fields.push(label);
            }
        }

        let fields_ref: Vec<&str> =
            fields.iter().map(|f| f.as_str()).collect::<Vec<_>>();

        let resp = grid_row_label(
            ui,
            egui::Id::new(ui.id().with(index)),
            &fields_ref,
            false,
        );
        ui.end_row();

        resp
    }

    fn select_record(
        &self,
        app_msg_tx: &crossbeam::channel::Sender<AppMsg>,
        graph_query: &GraphQuery,
        record: &BedRecord,
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

    fn apply_filter(&mut self, file_name: &str, records: &BedRecords) {
        self.filtered_records.clear();

        eprintln!("applying filter");
        let total = records.records.len();

        let records = &records.records;
        let filter = self.filters.get(file_name).unwrap();
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
        records: &Arc<BedRecords>,
    ) -> Option<egui::Response> {
        let active_path_name = self
            .path_picker
            .active_path()
            .map(|(_id, name)| name.to_owned());

        if !self.enabled_columns.contains_key(file_name) {
            let mut enabled_columns: ColumnPickerMany<BedRecords> =
                ColumnPickerMany::new(file_name);

            enabled_columns.update_columns(records);

            use BedColumn as Bed;
            for col in [Bed::Chr, Bed::Start, Bed::End] {
                enabled_columns.set_column(&col, true);
                enabled_columns.hide_column_from_gui(&col, true);
            }

            self.enabled_columns
                .insert(file_name.to_string(), enabled_columns);
        }

        {
            let filter = self
                .filters
                .entry(file_name.to_string())
                .or_insert(BedFilter::new(records));

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
                self.creator.column_picker_bed.update_columns(records);
            }

            self.creator.ui_bed(
                ctx,
                &self.overlay_tx,
                app_msg_tx,
                graph_query,
                &mut self.creator_open,
                file_name,
                path,
                records.clone(),
                &self.filtered_records,
            );
        }

        let resp = egui::Window::new("GFF3")
            .id(egui::Id::new(Self::ID))
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

                    let creator_btn = ui.button("Label & Overlay creator");

                    if creator_btn.clicked() {
                        self.creator_open = !self.creator_open;
                    }
                });

                ui.horizontal(|ui| {
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
                                self.filters.get_mut(file_name).unwrap();
                            filter.chr_range_filter(chr, start, end);
                        }
                    }
                });

                use BedColumn as Bed;

                let enabled_columns =
                    self.enabled_columns.get(file_name).unwrap();

                let grid = egui::Grid::new("bed_record_list_grid")
                    .striped(true)
                    .show(&mut ui, |ui| {
                        ui.label("chr");
                        ui.label("start");
                        ui.label("end");

                        let columns = records.optional_columns();

                        for key in columns.iter() {
                            if enabled_columns.get_column(key) {
                                ui.label(&format!("{}", key));
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
                                                ui, file_name, records, record,
                                                i,
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
                                            ui, file_name, records, record, i,
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
                self.enabled_columns.get_mut(file_name).unwrap();
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
pub struct BedFilter {
    chr: FilterString,

    start: FilterNum<usize>,
    end: FilterNum<usize>,

    rest: Vec<FilterString>,
    // score: FilterNum<f64>,

    // frame: FilterString,

    // attributes: HashMap<Vec<u8>, FilterString>,
}

impl BedFilter {
    pub const ID: &'static str = "bed_filter_window";

    fn new(records: &BedRecords) -> Self {
        let opt_cols = records.optional_columns();
        let rest = opt_cols
            .into_iter()
            .map(|_| FilterString::default())
            .collect::<Vec<_>>();

        Self {
            rest,
            ..BedFilter::default()
        }
    }

    fn chr_range_filter(&mut self, chr: &[u8], start: usize, end: usize) {
        if let Ok(chr) = chr.to_str().map(String::from) {
            self.chr.op = FilterStringOp::ContainedIn;
            self.chr.arg = chr;
        }
        self.range_filter(start, end);
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
                    ui.label("chr");
                    self.chr.ui(ui);
                    ui.separator();

                    ui.label("start");
                    self.start.ui(ui);
                    ui.separator();

                    ui.label("end");
                    self.end.ui(ui);
                    ui.separator();
                });

                ui.collapsing("Remaining Columns", |mut ui| {
                    egui::ScrollArea::from_max_height(
                        ui.input().screen_rect.height() - 250.0,
                    )
                    .show(&mut ui, |ui| {
                        let mut col_filters = self
                            .rest
                            .iter_mut()
                            .enumerate()
                            .collect::<Vec<_>>();

                        col_filters.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

                        for (_count, (index, filter)) in
                            col_filters.into_iter().enumerate()
                        {
                            ui.label(&index.to_string());
                            filter.ui(ui);
                            ui.separator();
                        }
                    });
                });
            })
    }

    fn rest_filter(&self, record: &BedRecord) -> bool {
        self.rest.iter().enumerate().all(|(ix, filter)| {
            if matches!(filter.op, FilterStringOp::None) {
                return true;
            }

            let values = record.get_all(&BedColumn::Index(ix));
            values.iter().any(|v| filter.filter_bytes(v))
        })
    }

    fn filter_record(&self, record: &BedRecord) -> bool {
        self.chr.filter_bytes(record.seq_id())
            && self.start.filter(record.start())
            && self.end.filter(record.end())
            && self.rest_filter(record)
    }
}

pub(crate) fn bed_column_hash_color(
    record: &BedRecord,
    column: &BedColumn,
) -> Option<rgb::RGB<f32>> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::default();

    match column {
        BedColumn::Start => record.start().hash(&mut hasher),
        BedColumn::End => record.end().hash(&mut hasher),
        other => {
            record.get_all(other).hash(&mut hasher);
        }
    }

    let (r, g, b) = crate::gluon::hash_node_color(hasher.finish());

    Some(rgb::RGB::new(r, g, b))
}
