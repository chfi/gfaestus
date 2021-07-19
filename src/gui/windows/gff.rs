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
use std::{collections::HashMap, sync::Arc};

use bstr::ByteSlice;

use rustc_hash::FxHashSet;

use anyhow::Result;
use egui::emath::Numeric;

use crate::{
    app::AppMsg, asynchronous::AsyncResult, geometry::Point,
    graph_query::GraphQuery,
};

use crate::annotations::{Gff3Record, Gff3Records};

use super::filters::*;
use super::graph_picker::PathPicker;

pub struct Gff3RecordList {
    records: Gff3Records,
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

    pub fn new(records: Gff3Records, path_picker: PathPicker) -> Self {
        let filtered_records = Vec::with_capacity(records.records.len());

        let filter = Gff3Filter::new(&records);
        let enabled_columns = EnabledColumns::new(&records);

        Self {
            records,
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
        }
    }

    fn ui_row(&self, record: &Gff3Record, ui: &mut egui::Ui) -> egui::Response {
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

        let mut keys = self.records.attribute_keys.iter().collect::<Vec<_>>();
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

    fn apply_filter(&mut self) {
        self.filtered_records.clear();

        eprintln!("applying filter");
        let total = self.records.records.len();

        let records = &self.records.records;
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
        graph_query: &GraphQuery,
        app_msg_tx: &crossbeam::channel::Sender<AppMsg>,
        // open_gff3_window: &mut bool,
    ) -> Option<egui::Response> {
        let active_path_name = self
            .path_picker
            .active_path()
            .map(|(_id, name)| name.to_owned());

        self.filter.ui(ctx, &mut self.filter_open);

        self.path_picker.ui(ctx, &mut self.path_picker_open);

        let resp = egui::Window::new("GFF3")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(600.0, 200.0))
            // .collapsible(true)
            // .open(open_gff3_window)
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
                        self.apply_filter();
                    }

                    if ui.button("Clear filter").clicked() {
                        self.clear_filter();
                    }
                });

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

                        let mut keys = self
                            .records
                            .attribute_keys
                            .iter()
                            .collect::<Vec<_>>();
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
                                self.records.records.get(self.offset + i).map(
                                    |record| (self.ui_row(record, ui), record),
                                )
                            } else {
                                self.filtered_records
                                    .get(self.offset + i)
                                    .and_then(|&ix| {
                                        let record =
                                            self.records.records.get(ix)?;
                                        let row = self.ui_row(record, ui);
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
                                        graph_query,
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
                            (self.records.records.len() - self.slot_count)
                                as isize,
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

impl EnabledColumns {
    pub const ID: &'static str = "gff_column_picker_window";

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
