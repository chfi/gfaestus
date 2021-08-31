use std::{path::PathBuf, sync::Arc};

use clipboard::{ClipboardContext, ClipboardProvider};

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

use crossbeam::atomic::AtomicCell;

use crate::{app::OverlayState, geometry::*};

use crate::overlays::OverlayKind;

use crate::graph_query::GraphQuery;

use crate::input::binds::{
    BindableInput, KeyBind, MouseButtonBind, SystemInput, SystemInputBindings,
    WheelBind,
};

use crate::vulkan::{draw_system::gui::GuiPipeline, GfaestusVk};

pub struct Console<'a> {
    input_line: String,

    input_history: Vec<String>,
    output_history: Vec<String>,

    scope: rhai::Scope<'a>,

    request_focus: bool,
}

impl<'a> Console<'a> {
    pub const ID: &'static str = "quake_console";

    pub fn new() -> Self {
        let scope = rhai::Scope::new();

        Self {
            input_line: String::new(),

            input_history: Vec::new(),
            output_history: Vec::new(),

            scope,

            request_focus: false,
        }
    }

    pub fn ui(&mut self, ctx: &egui::CtxRef, is_down: bool) {
        if !is_down {
            return;
        }

        egui::Area::new(Self::ID)
            .enabled(is_down)
            .anchor(egui::Align2::CENTER_TOP, Point::new(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(ctx.input().screen_rect().width());

                let skip_count =
                    self.output_history.len().checked_sub(20).unwrap_or(0);

                for (ix, output_line) in self
                    .output_history
                    .iter()
                    .skip(skip_count)
                    .enumerate()
                    .take(20)
                {
                    ui.add(egui::Label::new(output_line).code());
                }

                let input = ui.add_sized(
                    ui.available_size(),
                    egui::TextEdit::singleline(&mut self.input_line)
                        .code_editor()
                        .lock_focus(true),
                );

                // hack to keep input
                if self.request_focus {
                    if input.has_focus() {
                        self.request_focus = false;
                    } else {
                        input.request_focus();
                    }
                }

                if input.lost_focus()
                    && ui.input().key_pressed(egui::Key::Enter)
                {
                    // TODO: actually handle input
                    let mut line =
                        String::with_capacity(self.input_line.capacity());
                    std::mem::swap(&mut self.input_line, &mut line);

                    self.input_history.push(line.clone());
                    self.output_history.push(line);

                    self.input_line.clear();

                    // input.request_focus() has to be called the
                    // frame *after* this piece of code is ran, hence
                    // the bool etc.
                    // input.request_focus();
                    self.request_focus = true;
                }
            });
        // });
    }
}
