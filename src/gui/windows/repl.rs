use anyhow::Result;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;

use std::sync::Arc;

use crate::geometry::Point;

use crate::gluon::repl::GluonRepl;

pub struct ReplWindow {
    repl: GluonRepl,

    line_input: String,
    output: String,
}

impl ReplWindow {
    pub const ID: &'static str = "repl_window";

    pub fn new(repl: GluonRepl) -> Result<Self> {
        let line_input = String::new();
        let output = String::new();

        let future = async {
            repl.gluon_vm
                .eval_line("let io @ { ? } = import! std.io")
                .await;
            repl.gluon_vm.eval_line("let repl = import! repl").await;
        };

        futures::executor::block_on(future);

        Ok(Self {
            repl,

            line_input,
            output,
        })
    }

    pub fn ui(
        &mut self,
        open: &mut bool,
        ctx: &egui::CtxRef,
        thread_pool: &ThreadPool,
    ) -> Option<egui::Response> {
        let scr = ctx.input().screen_rect();

        let pos = egui::pos2(scr.center().x + 150.0, scr.center().y - 60.0);

        while let Ok(msg) = self.repl.output_rx.try_recv() {
            self.output.push_str(&msg);
        }

        let line_count = self.output.lines().count();
        if line_count > 20 {
            let mut new_output = String::new();

            let lines = self.output.lines().skip(line_count - 20);

            for line in lines {
                new_output.push_str(line);
                new_output.push_str("\n");
            }

            self.output = new_output;
        }

        egui::Window::new("REPL")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .default_pos(pos)
            .show(ctx, |ui| {
                // ui.set_max_width(1000.0);

                ui.vertical(|ui| {
                    egui::ScrollArea::auto_sized().show(ui, |ui| {
                        let history =
                            egui::TextEdit::multiline(&mut self.output)
                                .text_style(egui::TextStyle::Monospace)
                                .enabled(false);

                        ui.add(history);
                    });

                    let input_box = ui.add(
                        egui::TextEdit::multiline(&mut self.line_input)
                            .text_style(egui::TextStyle::Monospace)
                            .desired_rows(1),
                    );

                    if ui.button("Submit").clicked()
                        || (input_box.has_focus()
                            && ui.input().key_pressed(egui::Key::Enter))
                    {
                        let future = self.repl.eval_line(&self.line_input);

                        self.line_input.clear();

                        thread_pool
                            .spawn(async move {
                                future.await;
                            })
                            .unwrap();
                    }
                });
            })
    }
}

#[derive(Debug)]
pub struct HistoryBox {
    lines: Vec<String>,
    desired_width: Option<f32>,
    desired_height_rows: usize,

    history_lines: usize,
}

// impl HistoryBox {
//     fn content_ui(self, ui: &mut egui::Ui) -> egui::Response {
impl egui::Widget for HistoryBox {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        use egui::epaint::text::Galley;
        use egui::Sense;

        let text_style = egui::TextStyle::Monospace;
        let line_spacing = ui.fonts().row_height(text_style);
        let available_width = ui.available_width();

        let start_ix = if self.lines.len() >= self.history_lines {
            self.lines.len() - self.history_lines
        } else {
            0
        };

        let mut galleys: Vec<(f32, Arc<Galley>)> =
            Vec::with_capacity(self.history_lines);
        let mut row_offset = 0.0f32;

        let mut desired_size = Point::ZERO;

        for line in self.lines[start_ix..].iter() {
            let galley = ui.fonts().layout_multiline(
                text_style,
                line.to_owned(),
                available_width,
            );
            let offset = row_offset;
            row_offset += galley.size.y + line_spacing;

            desired_size.x = desired_size.x.max(galley.size.x);
            desired_size.y += galley.size.y + line_spacing;

            galleys.push((offset, galley));
        }

        // let desired_width = 400.0;
        // let desired_height = (20 as f32) * line_spacing;
        let (auto_id, rect) = ui.allocate_space(desired_size.into());

        let response = ui.interact(rect, auto_id, Sense::hover());

        egui::ScrollArea::auto_sized().show(ui, |ui| {
            for (offset, galley) in galleys {
                let mut pos = response.rect.min;
                pos += (Point { x: 0.0, y: offset }).into();

                ui.painter().galley(
                    pos,
                    galley,
                    egui::Color32::from_rgb(200, 200, 200),
                );
            }
        });

        response
    }
}

/*
impl egui::Widget for HistoryBox {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        use egui::{Sense, Shape, Vec2};

        let frame = self.frame;
        let where_to_put_background = ui.painter().add(Shape::Noop);

        let margin = Vec2::new(4.0, 2.0);
        let max_rect = ui.available_rect_before_wrap().shrink2(margin);
        let mut content_ui = ui.child_ui(max_rect, *ui.layout());
        let response = self.content_ui(&mut content_ui);
        let frame_rect = response.rect.expand2(margin);
        let response = response | ui.allocate_rect(frame_rect, Sense::hover());

        if frame {
            let visuals = ui.style().interact(&response);
            let frame_rect = response.rect.expand(visuals.expansion);
            let shape = if response.has_focus() {
                Shape::Rect {
                    rect: frame_rect,
                    corner_radius: visuals.corner_radius,
                    // fill: ui.visuals().selection.bg_fill,
                    fill: ui.visuals().extreme_bg_color,
                    stroke: ui.visuals().selection.stroke,
                }
            } else {
                Shape::Rect {
                    rect: frame_rect,
                    corner_radius: visuals.corner_radius,
                    fill: ui.visuals().extreme_bg_color,
                    stroke: visuals.bg_stroke, // TODO: we want to show something here, or a text-edit field doesn't "pop".
                }
            };

            ui.painter().set(where_to_put_background, shape);
        }

        response
    }
}


*/
