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

#[derive(Debug, PartialEq, PartialOrd)]
pub enum FilterString {
    None,
    Equal(Vec<u8>),
    Contains(Vec<u8>),
    // Prefix(Vec<u8>),
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum FilterOrd<T: PartialOrd + Copy> {
    None,
    Equal(T),
    LessThan(T),
    MoreThan(T),
    // Range(T, T),
}

impl std::default::Default for FilterString {
    fn default() -> Self {
        Self::None
    }
}

impl<T: PartialOrd + Copy> std::default::Default for FilterOrd<T> {
    fn default() -> Self {
        Self::None
    }
}

impl FilterString {
    fn filter(&self, string: &[u8]) -> bool {
        match self {
            Self::None => true,
            Self::Equal(arg) => string == arg,
            Self::Contains(arg) => string.contains_str(arg.as_slice()),
        }
    }

    fn variant_string(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Equal(arg) => "Equal",
            Self::Contains(arg) => "Contains",
        }
    }

    fn variant_ix(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::Equal(arg) => 1,
            Self::Contains(arg) => 2,
        }
    }

    fn variants() -> [&'static str; 3] {
        ["None", "Equal", "Contains"]
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
            .show(ctx, |mut ui| {
                let seq_id = &mut self.seq_id;

                let seq_id_none =
                    ui.radio_value(seq_id, FilterString::None, "None");
                let seq_id_eq = ui.radio_value(
                    seq_id,
                    FilterString::Equal(vec![]),
                    "Equal",
                );
                let seq_id_contains = ui.radio_value(
                    seq_id,
                    FilterString::Contains(vec![]),
                    "Contains",
                );

                if seq_id_none.clicked()
                    || seq_id_eq.clicked()
                    || seq_id_contains.clicked()
                {
                    let vars = FilterString::variants();
                    println!("switched to {}", seq_id.variant_string());
                }
            })
    }
}
