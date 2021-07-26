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

use crate::annotations::Gff3Record;

pub struct Annotations2D {
    ref_path: PathId,

    steps: Vec<(Handle, StepPtr, usize)>,
}

impl Annotations2D {
    pub fn new(graph: &GraphQuery, path: PathId) -> Option<Self> {
        let steps = graph.path_pos_steps(path)?;

        Some(Self {
            ref_path: path,
            steps,
        })
    }

    pub fn path(&self) -> PathId {
        self.ref_path
    }

    // returns world coordinates for the center of the nodes covered
    // by the annotation, if it exists
    pub fn location_for_record(
        &self,
        layout: impl GraphLayout,
        record: &Gff3Record,
    ) -> Option<Point> {
        let (start, end) = {
            let start = self
                .steps
                .binary_search_by_key(&record.start(), |(_, _, p)| *p);
            let end = self
                .steps
                .binary_search_by_key(&record.end(), |(_, _, p)| *p);

            let (start, end) = match (start, end) {
                (Ok(s), Ok(e)) => (s, e),
                (Ok(s), Err(e)) => (s, e),
                (Err(s), Ok(e)) => (s, e),
                (Err(s), Err(e)) => (s, e),
            };

            // get the handle of the start & end
            let start = self.steps.get(start)?.0;

            let end = {
                let ix = if end >= self.steps.len() {
                    self.steps.len() - 1
                } else {
                    end
                };

                self.steps.get(ix)?.0
            };

            Some((start, end))
        }?;

        let node_pos = layout.nodes();

        let start_ix = (start.0 - 1) as usize;

        let node = node_pos.get(start_ix)?;

        Some(node.center())
    }
}
