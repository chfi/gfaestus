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
    active_path: Option<PathId>,

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
            active_path: None,
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
                unimplemented!();
            })
    }
}
