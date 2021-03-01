pub mod gui;
pub mod mainview;
pub mod node_flags;

use node_flags::*;

use crossbeam::channel;

use handlegraph::handle::NodeId;

use crate::geometry::*;
use crate::input::MousePos;
use crate::view::*;

pub struct App {
    mouse_pos: MousePos,
    screen_dims: ScreenDims,

    hover_node: Option<NodeId>,
    selected_node: Option<NodeId>,

    selection: NodeSelection,

    pub selection_edge_detect: bool,
    pub selection_edge_blur: bool,
    pub selection_edge: bool,
    pub nodes_color: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeSelection {
    None,
    One(NodeId),
    Many(Vec<NodeId>),
}

impl std::default::Default for NodeSelection {
    fn default() -> Self {
        Self::None
    }
}

// impl NodeFlags {

// }

// pub struct NodeSele

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMsg {
    SelectNode(Option<NodeId>),
    HoverNode(Option<NodeId>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppConfigMsg {
    ToggleSelectionEdgeDetect,
    ToggleSelectionEdgeBlur,
    ToggleSelectionOutline,
    ToggleNodesColor,
    // Toggle(RenderConfigOpts),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RenderConfigOpts {
    SelOutlineEdge,
    SelOutlineBlur,
    SelOutline,
    NodesColor,
}

impl App {
    pub fn new<Dims: Into<ScreenDims>>(
        mouse_pos: MousePos,
        screen_dims: Dims,
    ) -> Self {
        Self {
            mouse_pos,
            screen_dims: screen_dims.into(),
            hover_node: None,
            selected_node: None,

            selection: NodeSelection::None,

            selection_edge_detect: true,
            selection_edge_blur: true,
            selection_edge: true,
            nodes_color: true,
        }
    }

    pub fn hover_node(&self) -> Option<NodeId> {
        self.hover_node
    }

    pub fn selected_node(&self) -> Option<NodeId> {
        self.selected_node
    }

    pub fn selection(&self) -> &NodeSelection {
        &self.selection
    }

    pub fn single_selection(&self) -> Option<NodeId> {
        if let NodeSelection::One(n) = &self.selection {
            Some(*n)
        } else {
            None
        }
    }

    pub fn dims(&self) -> ScreenDims {
        self.screen_dims
    }

    pub fn mouse_pos(&self) -> Point {
        self.mouse_pos.read()
    }

    pub fn update_dims<Dims: Into<ScreenDims>>(&mut self, screen_dims: Dims) {
        self.screen_dims = screen_dims.into();
    }

    pub fn apply_app_msg(&mut self, msg: &AppMsg) {
        match msg {
            AppMsg::SelectNode(id) => self.selected_node = *id,
            AppMsg::HoverNode(id) => self.hover_node = *id,
        }
    }

    pub fn apply_app_config_msg(&mut self, msg: &AppConfigMsg) {
        match msg {
            AppConfigMsg::ToggleSelectionEdgeDetect => {
                self.selection_edge_detect = !self.selection_edge_detect
            }
            AppConfigMsg::ToggleSelectionEdgeBlur => {
                self.selection_edge_blur = !self.selection_edge_blur
            }
            AppConfigMsg::ToggleSelectionOutline => {
                self.selection_edge = !self.selection_edge
            }
            AppConfigMsg::ToggleNodesColor => {
                self.nodes_color = !self.nodes_color
            }
        }
    }
}
