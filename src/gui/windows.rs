#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use crate::geometry::*;
use crate::graph_query::{GraphQuery, GraphQueryRequest, GraphQueryResp};
use crate::view::View;

#[derive(Debug, Clone, Copy)]
pub enum ActiveDetails {
    Overview,
    NodeList,
    // PathList,
    // NodeDetails,
    // PathDetails,
}

pub struct GraphDetails {
    active: ActiveDetails,

    filtered_nodes: Option<Vec<NodeId>>,
    // filtered_paths: Option<Vec<PathId>>,

    // node_details: Option<NodeId>,
    // path_details: Option<PathId>,
}

impl GraphDetails {
    const ID: &'static str = "graph_details_window";

    pub fn ui(
        &self,
        ctx: &egui::CtxRef,
        show: &mut bool,
    ) -> Option<egui::Response> {
        unimplemented!();
    }
}
