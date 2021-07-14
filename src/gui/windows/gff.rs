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

// use crate::graph_query::{GraphQuery, GraphQueryWorker};
use crate::{
    app::{AppMsg, Select},
    geometry::*,
};

use crate::annotations::Gff3Record;

pub struct Gff3RecordList {
    records: Vec<Gff3Record>,

    offset: usize,

    // fetched_for_offset: Option<usize>,
    slot_count: usize,
    // update_slots: bool,
}

impl Gff3RecordList {
    pub const ID: &'static str = "gff_record_list_window";

    // pub fn new(records: Vec<Gff3Record>) -> Result<Self> {
    pub fn new(records: Vec<Gff3Record>) -> Self {
        Self {
            records,
            offset: 0,
            // fetched_for_offset: None,
            slot_count: 20,
            // update_slots: true,
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open_gff3_window: &mut bool,
    ) -> Option<egui::Response> {
        egui::Window::new("GFF3")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(600.0, 200.0))
            .open(open_gff3_window)
            .show(ctx, |mut ui| {
                egui::Grid::new("gff3_record_list_grid").striped(true).show(
                    &mut ui,
                    |ui| {
                        ui.label("seq_id");
                        ui.label("source");
                        ui.label("type");
                        ui.label("start");
                        ui.label("end");
                        ui.end_row();

                        for i in 0..self.slot_count {
                            if let Some(record) =
                                self.records.get(self.offset + i)
                            {
                                // let seq_id = record.seq_id().to_str().unwrap();
                                // let source = record.source().to_str().unwrap();
                                // let type_ = record.type_().to_str().unwrap();

                                ui.label(format!(
                                    "{}",
                                    record.seq_id().as_bstr()
                                ));
                                ui.label(format!(
                                    "{}",
                                    record.source().as_bstr()
                                ));
                                ui.label(format!(
                                    "{}",
                                    record.type_().as_bstr()
                                ));
                                ui.label(format!("{}", record.start()));
                                ui.label(format!("{}", record.end()));

                                ui.end_row();
                            }
                        }
                    },
                );
            })
    }
}
