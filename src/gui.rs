use std::sync::Arc;

#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use anyhow::Result;

use rustc_hash::FxHashMap;

use crossbeam::channel;
use parking_lot::Mutex;

mod theme_editor;

use theme_editor::*;

use crate::geometry::*;
use crate::view::View;
use crate::{app::RenderConfigOpts, vulkan::render_pass::Framebuffers};

use crate::input::binds::{
    BindableInput, InputPayload, KeyBind, MouseButtonBind, SystemInput,
    SystemInputBindings, WheelBind,
};
use crate::input::MousePos;

// use super::theme::{ThemeDef, ThemeId};

use crate::app::settings::AppConfigState;

use crate::vulkan::{
    context::VkContext,
    draw_system::gui::{GuiPipeline, GuiVertex, GuiVertices},
    GfaestusVk, SwapchainProperties,
};

use ash::vk;

pub mod widgets;

use widgets::*;

pub struct Gui {
    ctx: egui::CtxRef,
    frame_input: FrameInput,

    draw_system: GuiPipeline,
    // widgets: FxHashMap<String,

    // windows:
}

struct ActiveWindows {
    egui_inspection_ui: bool,
    egui_settings_ui: bool,
    egui_memory_ui: bool,

    graph_info: bool,

    selection_info: bool,

    theme_editor: bool,

    options: bool,
}

struct ActiveWidgets {
    fps: bool,
    graph_stats: bool,
    view_info: bool,
    selected_node: bool,
}

/// Wrapper for input events that are fed into egui
#[derive(Debug, Default, Clone)]
struct FrameInput {
    events: Vec<egui::Event>,
    scroll_delta: f32,
}

impl FrameInput {
    fn into_raw_input(&mut self) -> egui::RawInput {
        let mut raw_input = egui::RawInput::default();
        // TODO maybe use clone_from and clear self.events instead, to reduce allocations
        raw_input.events = std::mem::take(&mut self.events);
        raw_input.scroll_delta = egui::Vec2 {
            x: 0.0,
            y: self.scroll_delta,
        };
        self.scroll_delta = 0.0;

        raw_input
    }
}
