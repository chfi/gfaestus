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

use log::debug;

use crossbeam::atomic::AtomicCell;
use rustc_hash::FxHashMap;

use crate::graph_query::GraphQuery;
use crate::overlays::OverlayKind;
use crate::{app::OverlayState, geometry::*};

pub struct Console<'a> {
    input_line: String,

    input_history: Vec<String>,
    output_history: Vec<String>,

    scope: rhai::Scope<'a>,

    request_focus: bool,
}

impl<'a> Console<'a> {
    pub const ID: &'static str = "quake_console";
    pub const ID_TEXT: &'static str = "quake_console_input";

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

    fn create_engine() -> rhai::Engine {
        use rhai::plugin::*;

        let mut engine = rhai::Engine::new();

        engine.register_type::<NodeId>();
        engine.register_type::<Handle>();

        let handle = exported_module!(crate::script::plugins::handle_plugin);

        engine.register_global_module(handle.into());

        engine.register_fn("print_test", || {
            println!("hello world");
        });

        engine
    }

    pub fn eval(&mut self) -> Result<()> {
        let engine = Self::create_engine();

        debug!("evaluating: {}", &self.input_line);

        let result = engine.eval_with_scope::<rhai::Dynamic>(
            &mut self.scope,
            &self.input_line,
        );
        match result {
            Ok(result) => {
                debug!("Eval success!");
                self.output_history.push(format!("{:?}", result));
            }
            Err(err) => {
                debug!("Eval error: {:?}", err);
                self.output_history.push(format!("Error: {:?}", err));
            }
        }

        Ok(())
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
                        .id(egui::Id::new(Self::ID_TEXT))
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
                    self.input_history.push(self.input_line.clone());
                    self.output_history.push(format!("> {}", self.input_line));

                    self.eval().unwrap();

                    let mut line =
                        String::with_capacity(self.input_line.capacity());
                    std::mem::swap(&mut self.input_line, &mut line);

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
