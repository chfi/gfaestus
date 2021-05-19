use anyhow::Result;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;

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

        egui::Window::new("REPL")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .default_pos(pos)
            .show(ctx, |ui| {
                ui.set_max_width(1000.0);

                ui.vertical(|ui| {
                    let history = egui::TextEdit::multiline(&mut self.output)
                        .text_style(egui::TextStyle::Monospace)
                        .enabled(false);

                    ui.add(history);

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
