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
    // bed_filters: HashMap<String, Gff3Filter>,
    column_picker_open: bool,
    bed_enabled_columns: HashMap<String, ColumnPickerMany<BedRecords>>,

    path_picker_open: bool,
    path_picker: PathPicker,

    creator_open: bool,
    creator: OverlayLabelSetCreator,
    overlay_tx: Sender<OverlayCreatorMsg>,
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
