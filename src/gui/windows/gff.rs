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
use std::sync::Arc;

use bstr::ByteSlice;

use rustc_hash::FxHashSet;

use anyhow::Result;

use crate::asynchronous::AsyncResult;

use crate::annotations::Gff3Record;

pub struct Gff3RecordList {
    records: Vec<Gff3Record>,

    filtered_records: Vec<Gff3Record>,

    offset: usize,

    slot_count: usize,

    filter: Gff3Filter,
}

impl Gff3RecordList {
    pub const ID: &'static str = "gff_record_list_window";

    pub fn new(records: Vec<Gff3Record>) -> Self {
        let filtered_records = Vec::with_capacity(records.len());

        Self {
            records,
            filtered_records,
            offset: 0,
            slot_count: 30,

            filter: Gff3Filter::default(),
        }
    }

    fn ui_row(record: &Gff3Record, ui: &mut egui::Ui) {
        ui.label(format!("{}", record.seq_id().as_bstr()));
        ui.label(format!("{}", record.source().as_bstr()));
        ui.label(format!("{}", record.type_().as_bstr()));
        ui.label(format!("{}", record.start()));
        ui.label(format!("{}", record.end()));

        ui.end_row();
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        // open_gff3_window: &mut bool,
    ) -> Option<egui::Response> {
        egui::Window::new("GFF3")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(600.0, 200.0))
            // .open(open_gff3_window)
            .show(ctx, |mut ui| {
                let records = if self.filtered_records.is_empty() {
                    &self.records
                } else {
                    &self.filtered_records
                };

                let grid = egui::Grid::new("gff3_record_list_grid")
                    .striped(true)
                    .show(&mut ui, |ui| {
                        ui.label("seq_id");
                        ui.label("source");
                        ui.label("type");
                        ui.label("start");
                        ui.label("end");
                        ui.end_row();

                        for i in 0..self.slot_count {
                            if let Some(record) = records.get(self.offset + i) {
                                Self::ui_row(record, ui);
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

                        offset =
                            offset.clamp(0, (self.records.len() - 1) as isize);
                        self.offset = offset as usize;
                    }
                }
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
    fn filter(&self, string: &str) -> bool {
        match self.op {
            FilterStringOp::None => true,
            FilterStringOp::Equal => string == self.arg,
            FilterStringOp::Contains => string.contains(&self.arg),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let op = &mut self.op;
        let arg = &mut self.arg;

        let _op_none = ui.radio_value(op, FilterStringOp::None, "None");
        let _op_equal = ui.radio_value(op, FilterStringOp::Equal, "Equal");
        let _op_contains =
            ui.radio_value(op, FilterStringOp::Contains, "Contains");

        // let op_radios = op_none.union(op_equal).union(op_contains);

        let _arg_edit = ui.text_edit_singleline(arg);

        // if op_radios.clicked() {
        // }
    }
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum FilterOrd<T: PartialOrd + Copy> {
    None,
    Equal(T),
    LessThan(T),
    MoreThan(T),
    // Range(T, T),
}

impl<T: PartialOrd + Copy> std::default::Default for FilterOrd<T> {
    fn default() -> Self {
        Self::None
    }
}

impl<T: PartialOrd + Copy> FilterOrd<T> {
    fn filter(&self, value: T) -> bool {
        match self {
            Self::None => true,
            Self::Equal(arg) => value == *arg,
            Self::LessThan(arg) => value < *arg,
            Self::MoreThan(arg) => value > *arg,
        }
    }
}

#[derive(Debug)]
pub struct Gff3Filter {
    seq_id: FilterString,
    source: FilterString,
    type_: FilterString,

    start: FilterOrd<usize>,
    end: FilterOrd<usize>,

    score: FilterOrd<Option<f64>>,
    // attributes: ??
}

impl std::default::Default for Gff3Filter {
    fn default() -> Self {
        Self {
            seq_id: FilterString::default(),
            source: FilterString::default(),
            type_: FilterString::default(),

            start: FilterOrd::default(),
            end: FilterOrd::default(),

            score: FilterOrd::default(),
        }
    }
}

impl Gff3Filter {
    pub const ID: &'static str = "gff_filter_window";

    pub fn ui(&mut self, ctx: &egui::CtxRef) -> Option<egui::Response> {
        egui::Window::new("GFF3 Filter")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(600.0, 200.0))
            // .open(open_gff3_window)
            .show(ctx, |ui| {
                ui.label("seq_id");
                self.seq_id.ui(ui);
                ui.separator();

                ui.label("source");
                self.source.ui(ui);
                ui.separator();

                ui.label("type");
                self.type_.ui(ui);
                ui.separator();

                if ui.button("debug print").clicked() {
                    eprintln!("seq_id: {:?}", self.seq_id);
                    eprintln!("source: {:?}", self.source);
                    eprintln!("type:   {:?}", self.type_);
                }
            })
    }
}
