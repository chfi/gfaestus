pub mod gff;

use std::collections::HashMap;

pub use gff::*;

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

use crate::annotations::{AnnotationCollection, AnnotationRecord};

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

    pub fn update_columns(&mut self, records: &T) {
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

pub struct ColumnPickerMany<T: AnnotationCollection> {
    enabled_columns: HashMap<T::ColumnKey, bool>,

    id: egui::Id,
}

impl<T: AnnotationCollection> ColumnPickerMany<T> {
    pub fn new(id_source: &str) -> Self {
        let id = egui::Id::new(id_source);

        Self {
            enabled_columns: Default::default(),

            id,
        }
    }

    pub fn update_columns(&mut self, records: &T) {
        let columns = records.all_columns();
        self.enabled_columns = columns.into_iter().map(|c| (c, false)).collect()
    }

    pub fn get_column(&self, column: &T::ColumnKey) -> bool {
        self.enabled_columns.get(column).copied().unwrap_or(false)
    }

    pub fn set_column(&mut self, column: &T::ColumnKey, to: bool) {
        self.enabled_columns.insert(column.clone(), to);
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        pos: impl Into<egui::Pos2>,
        open: &mut bool,
        window_name: &str,
    ) -> Option<egui::Response> {
        egui::Window::new(window_name)
            .id(self.id)
            .fixed_pos(pos)
            .collapsible(false)
            .open(open)
            .show(ctx, |ui| {
                let max_height = ui.input().screen_rect.height() - 250.0;
                ui.set_max_height(max_height);

                let mut columns =
                    self.enabled_columns.iter_mut().collect::<Vec<_>>();

                columns.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

                let (optional, mandatory): (Vec<_>, Vec<_>) = columns
                    .into_iter()
                    .partition(|(col, _en)| T::is_column_optional(col));

                let scroll_height = (max_height / 2.0) - 50.0;

                ui.collapsing("Mandatory fields", |mut ui| {
                    egui::ScrollArea::from_max_height(scroll_height).show(
                        &mut ui,
                        |ui| {
                            for (key, enabled) in mandatory.into_iter() {
                                if ui
                                    .selectable_label(*enabled, key.to_string())
                                    .clicked()
                                {
                                    *enabled = !*enabled;
                                }
                            }
                        },
                    );
                });

                ui.collapsing("Optional fields", |mut ui| {
                    egui::ScrollArea::from_max_height(scroll_height).show(
                        &mut ui,
                        |ui| {
                            for (key, enabled) in optional.into_iter() {
                                if ui
                                    .selectable_label(*enabled, key.to_string())
                                    .clicked()
                                {
                                    *enabled = !*enabled;
                                }
                            }
                        },
                    );
                });
            })
    }
}
