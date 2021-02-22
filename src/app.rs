pub mod gui;
pub mod mainview;

use crossbeam::channel;

use handlegraph::handle::NodeId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMsg {
    SelectNode(Option<NodeId>),
    HoverNode(Option<NodeId>),
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct AppState {
    hover_node: Option<NodeId>,
    selected_node: Option<NodeId>,
}

// impl AppState {
//     pub fn
// }
