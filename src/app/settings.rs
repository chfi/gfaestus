use anyhow::Result;
use crossbeam::atomic::AtomicCell;
use crossbeam::channel;
use handlegraph::handle::NodeId;
use rustc_hash::FxHashSet;
use std::sync::Arc;

use crate::vulkan::draw_system::edges::{EdgeRendererConfig, EdgesUBO};

use super::theme::ThemeDef;

#[derive(Debug, Default, Clone)]
pub struct AppSettings {
    node_width: Arc<NodeWidth>,

    edge_renderer: Arc<AtomicCell<EdgesUBO>>,
}

impl AppSettings {
    pub fn node_width(&self) -> &Arc<NodeWidth> {
        &self.node_width
    }

    pub fn edge_renderer(&self) -> &Arc<AtomicCell<EdgesUBO>> {
        &self.edge_renderer
    }

    pub fn update_edge_renderer(&self, conf: EdgesUBO) {
        self.edge_renderer.store(conf);
    }
}

#[derive(Debug)]
pub struct NodeWidth {
    min_node_width: AtomicCell<f32>,
    max_node_width: AtomicCell<f32>,

    min_scale: AtomicCell<f32>,
    max_scale: AtomicCell<f32>,
}

impl NodeWidth {
    pub fn min_node_width(&self) -> f32 {
        self.min_node_width.load()
    }

    pub fn max_node_width(&self) -> f32 {
        self.max_node_width.load()
    }
    pub fn min_scale(&self) -> f32 {
        self.min_scale.load()
    }

    pub fn max_scale(&self) -> f32 {
        self.max_scale.load()
    }

    pub fn set_min_node_width(&self, width: f32) {
        self.min_node_width.store(width);
    }

    pub fn set_max_node_width(&self, width: f32) {
        self.max_node_width.store(width);
    }

    pub fn set_min_scale(&self, width: f32) {
        self.min_scale.store(width);
    }

    pub fn set_max_scale(&self, width: f32) {
        self.max_scale.store(width);
    }
}

impl std::default::Default for NodeWidth {
    fn default() -> Self {
        Self {
            min_node_width: AtomicCell::new(0.1),
            max_node_width: AtomicCell::new(100.0),

            min_scale: AtomicCell::new(0.1),
            max_scale: AtomicCell::new(200.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveRenderLayers {
    node_color: bool,
    node_mask: bool,

    selection_outline: bool,
    selection_outline_edge: bool,
    selection_outline_blur: bool,

    lines: bool,
    gui: bool,
}

impl std::default::Default for ActiveRenderLayers {
    fn default() -> Self {
        Self {
            node_color: true,
            node_mask: true,
            selection_outline: true,
            selection_outline_edge: true,
            selection_outline_blur: true,
            lines: false,
            gui: true,
        }
    }
}

impl ActiveRenderLayers {
    pub fn none() -> Self {
        Self {
            node_color: false,
            node_mask: false,
            selection_outline: false,
            selection_outline_edge: false,
            selection_outline_blur: false,
            lines: false,
            gui: false,
        }
    }

    pub fn all() -> Self {
        Self {
            node_color: true,
            node_mask: true,
            selection_outline: true,
            selection_outline_edge: true,
            selection_outline_blur: true,
            lines: true,
            gui: true,
        }
    }

    pub fn toggle(self, with: Self) -> Self {
        Self {
            node_color: if with.node_color {
                !self.node_color
            } else {
                self.node_color
            },
            node_mask: if with.node_mask {
                !self.node_mask
            } else {
                self.node_mask
            },
            selection_outline: if with.selection_outline {
                !self.selection_outline
            } else {
                self.selection_outline
            },
            selection_outline_edge: if with.selection_outline_edge {
                !self.selection_outline_edge
            } else {
                self.selection_outline_edge
            },
            selection_outline_blur: if with.selection_outline_blur {
                !self.selection_outline_blur
            } else {
                self.selection_outline_blur
            },
            lines: if with.lines { !self.lines } else { self.lines },
            gui: if with.gui { !self.gui } else { self.gui },
        }
    }
}
