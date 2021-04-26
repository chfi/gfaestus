use anyhow::Result;
use crossbeam::channel;
use handlegraph::handle::NodeId;
use rustc_hash::FxHashSet;
use std::sync::Arc;

use super::theme::ThemeDef;

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

#[derive(Debug, Clone, PartialEq)]
pub enum AppConfigState {
    // Theme { id: ThemeId, def: ThemeDef },
    ToggleOverlay,
    // RenderLayers { active: ActiveRenderLayers },
}
