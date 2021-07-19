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

use crate::{asynchronous::AsyncResult, graph_query::GraphQuery};

use crate::annotations::{Gff3Record, Gff3Records};

pub struct PathPickerSource {
    paths: Arc<Vec<(PathId, String)>>,

    id_counter: usize,
}

pub struct PathPicker {
    paths: Arc<Vec<(PathId, String)>>,
    filtered_paths: Vec<usize>,

    id: usize,
    active_path_index: Option<usize>,

    offset: usize,
    slot_count: usize,
}

impl PathPickerSource {
    pub fn new(graph_query: &GraphQuery) -> Result<Self> {
        let graph = graph_query.graph();
        let paths_vec = graph
            .path_ids()
            .filter_map(|id| {
                let name = graph.get_path_name_vec(id)?;
                let name = name.to_str().ok()?;

                Some((id, name.to_string()))
            })
            .collect::<Vec<_>>();

        let paths = Arc::new(paths_vec);

        Ok(Self {
            paths,
            id_counter: 0,
        })
    }

    pub fn create_picker(&mut self) -> PathPicker {
        let paths = self.paths.clone();
        let filtered_paths = Vec::with_capacity(paths.len());

        let offset = 0;
        let slot_count = 20;

        let id = self.id_counter;
        self.id_counter += 1;

        PathPicker {
            paths,
            filtered_paths,
            id,
            active_path_index: None,
            offset,
            slot_count,
        }
    }
}

impl PathPicker {
    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
    ) -> Option<egui::Response> {
        egui::Window::new("Path picker")
            .id(egui::Id::new(("Path picker", self.id)))
            .open(open)
            .show(ctx, |mut ui| {
                let grid = egui::Grid::new("path_picker_list_grid")
                    .striped(true)
                    .show(&mut ui, |ui| {
                        let active_path_index = self.active_path_index;

                        if self.filtered_paths.is_empty() {
                            for i in 0..self.slot_count {
                                let index = self.offset + i;

                                if let Some((_path_id, name)) =
                                    self.paths.get(index)
                                {
                                    if ui
                                        .selectable_label(
                                            active_path_index == Some(index),
                                            name,
                                        )
                                        .clicked()
                                    {
                                        self.active_path_index = Some(index);
                                    }
                                    ui.end_row();
                                }
                            }
                        } else {
                            for i in 0..self.slot_count {
                                if let Some((index, name)) = self
                                    .filtered_paths
                                    .get(self.offset + i)
                                    .and_then(|&ix| {
                                        let (_, name) = self.paths.get(ix)?;
                                        Some((ix, name))
                                    })
                                {
                                    if ui
                                        .selectable_label(
                                            active_path_index == Some(index),
                                            name,
                                        )
                                        .clicked()
                                    {
                                        self.active_path_index = Some(index);
                                    }
                                    ui.end_row();
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
                            (self.paths.len() - self.slot_count) as isize,
                        );
                        self.offset = offset as usize;
                    }
                }
            })
    }

    pub fn active_path(&self) -> Option<(PathId, &str)> {
        let ix = self.active_path_index?;
        let (id, name) = self.paths.get(ix)?;

        Some((*id, name))
    }
}
