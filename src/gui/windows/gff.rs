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

use crate::asynchronous::AsyncResult;

use crate::annotations::{Gff3Record, Gff3Records};

pub struct Gff3RecordList {
    records: Gff3Records,
    filtered_records: Vec<usize>,

    offset: usize,
    slot_count: usize,

    filter_open: bool,
    filter: Gff3Filter,

    column_picker_open: bool,
    enabled_columns: EnabledColumns,
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

    pub fn new(records: Gff3Records) -> Self {
        let filtered_records = Vec::with_capacity(records.records.len());

        let filter = Gff3Filter::new(&records);
        let enabled_columns = EnabledColumns::new(&records);

        Self {
            records,
            filtered_records,
            offset: 0,
            slot_count: 20,

            filter_open: true,
            filter,

            column_picker_open: true,
            enabled_columns,
        }
    }

    fn ui_row(&self, record: &Gff3Record, ui: &mut egui::Ui) {
        ui.label(format!("{}", record.seq_id().as_bstr()));

        if self.enabled_columns.source {
            ui.label(format!("{}", record.source().as_bstr()));
        }

        if self.enabled_columns.type_ {
            ui.label(format!("{}", record.type_().as_bstr()));
        }
        ui.label(format!("{}", record.start()));
        ui.label(format!("{}", record.end()));

        if self.enabled_columns.frame {
            ui.label(format!("{}", record.frame().as_bstr()));
        }

        ui.end_row();
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
        // open_gff3_window: &mut bool,
    ) -> Option<egui::Response> {
        self.filter.ui(ctx, &mut self.filter_open);

        self.enabled_columns.ui(ctx, &mut self.column_picker_open);

        egui::Window::new("GFF3")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(600.0, 200.0))
            // .open(open_gff3_window)
            .show(ctx, |mut ui| {
                if ui.button("Apply filter").clicked() {
                    self.apply_filter();
                }

                if ui.button("Clear filter").clicked() {
                    self.clear_filter();
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
                        ui.end_row();

                        if self.filtered_records.is_empty() {
                            for i in 0..self.slot_count {
                                if let Some(record) =
                                    self.records.records.get(self.offset + i)
                                {
                                    self.ui_row(record, ui);
                                }
                            }
                        } else {
                            for i in 0..self.slot_count {
                                if let Some(record) = self
                                    .filtered_records
                                    .get(self.offset + i)
                                    .and_then(|&ix| {
                                        self.records.records.get(ix)
                                    })
                                {
                                    self.ui_row(record, ui);
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
            })
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
            .default_pos(egui::Pos2::new(300.0, 200.0))
            .open(open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    bool_label!(ui, source, "Source");
                    bool_label!(ui, type_, "Type");

                    // bool_label!(ui, score, "Score");
                    bool_label!(ui, frame, "Frame");
                });
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum FilterStringOp {
    None,
    Equal,
    Contains,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct FilterString {
    op: FilterStringOp,
    arg: String,
}

impl std::default::Default for FilterString {
    fn default() -> Self {
        Self {
            op: FilterStringOp::None,
            arg: String::new(),
        }
    }
}

impl FilterString {
    fn filter(&self, string: &[u8]) -> bool {
        match self.op {
            FilterStringOp::None => true,
            FilterStringOp::Equal => {
                let bytes = self.arg.as_bytes();
                string == bytes
            }
            FilterStringOp::Contains => {
                let bytes = self.arg.as_bytes();
                string.contains_str(bytes)
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let op = &mut self.op;
        let arg = &mut self.arg;

        ui.horizontal(|ui| {
            let _op_none = ui.radio_value(op, FilterStringOp::None, "None");
            let _op_equal = ui.radio_value(op, FilterStringOp::Equal, "Equal");
            let _op_contains =
                ui.radio_value(op, FilterStringOp::Contains, "Contains");
        });

        if *op != FilterStringOp::None {
            let _arg_edit = ui.text_edit_singleline(arg);
        }
    }
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum FilterNumOp {
    None,
    Equal,
    LessThan,
    MoreThan,
    InRange,
}

#[derive(Debug, PartialEq, PartialOrd)]
pub struct FilterNum<T: Numeric> {
    op: FilterNumOp,
    arg1: T,
    arg2: T,
}

impl<T: Numeric> std::default::Default for FilterNum<T> {
    fn default() -> Self {
        Self {
            op: FilterNumOp::None,
            arg1: T::from_f64(0.0),
            arg2: T::from_f64(0.0),
        }
    }
}

impl<T: Numeric> FilterNum<T> {
    fn filter(&self, val: T) -> bool {
        match self.op {
            FilterNumOp::None => true,
            FilterNumOp::Equal => val == self.arg1,
            FilterNumOp::LessThan => val < self.arg1,
            FilterNumOp::MoreThan => val > self.arg1,
            FilterNumOp::InRange => self.arg1 <= val && val < self.arg2,
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let op = &mut self.op;
        let arg1 = &mut self.arg1;
        let arg2 = &mut self.arg2;

        ui.horizontal(|ui| {
            let _op_none = ui.radio_value(op, FilterNumOp::None, "None");
            let _op_equal = ui.radio_value(op, FilterNumOp::Equal, "Equal");
            let _op_less =
                ui.radio_value(op, FilterNumOp::LessThan, "Less than");
            let _op_more =
                ui.radio_value(op, FilterNumOp::MoreThan, "More than");
            let _op_in_range =
                ui.radio_value(op, FilterNumOp::InRange, "In range");
        });

        let arg1_drag = egui::DragValue::new::<T>(arg1);
        // egui::DragValue::new::<T>(from_pos).clamp_range(from_range);

        let arg2_drag = egui::DragValue::new::<T>(arg2);

        if *op != FilterNumOp::None {
            ui.horizontal(|ui| {
                let _arg1_edit = ui.add(arg1_drag);
                if *op == FilterNumOp::InRange {
                    let _arg2_edit = ui.add(arg2_drag);
                }
            });
        }
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

    fn filter_record(&self, record: &Gff3Record) -> bool {
        self.seq_id.filter(record.seq_id())
            && self.source.filter(record.source())
            && self.type_.filter(record.type_())
            && self.start.filter(record.start())
            && self.end.filter(record.end())
            && self.frame.filter(record.frame())
        // && self.score.filter(record.score())
    }
}
