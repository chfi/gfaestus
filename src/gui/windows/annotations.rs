pub mod gff;

pub use gff::*;

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

use anyhow::Result;

use crate::{
    geometry::Point,
    graph_query::{GraphQuery, GraphQueryWorker},
    universe::GraphLayout,
};

use crate::annotations::{AnnotationCollection, AnnotationRecord, Gff3Record};

// pub struct ColumnPickerOne<T: AnnotationRecord> {
pub struct ColumnPickerOne<T: AnnotationCollection> {
    columns: Vec<T::ColumnKey>,
    chosen_column: Option<usize>,

    id: egui::Id,
}

impl<T: AnnotationCollection> ColumnPickerOne<T> {
    pub fn new(id_source: &str) -> Self {
        let id = egui::Id::new(id_source);

        Self {
            columns: Vec::new(),
            chosen_column: None,

            id,
        }
    }

    pub fn update_attributes(&mut self, records: &T) {
        self.chosen_column = None;
        self.columns = records.all_columns();
    }

    pub fn chosen_column(&self) -> Option<&T::ColumnKey> {
        let ix = self.chosen_column?;
        self.columns.get(ix)
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        window_name: &str,
    ) -> Option<egui::Response> {
        egui::Window::new(window_name).id(self.id).open(open).show(
            ctx,
            |mut ui| {
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
            },
        )
    }
}
