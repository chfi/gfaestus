use std::{path::PathBuf, sync::Arc};

use crossbeam::atomic::AtomicCell;

use futures::{
    channel::mpsc::{Receiver, Sender},
    Sink, SinkExt, Stream, StreamExt,
};

use bstr::{BStr, ByteSlice};
use rustc_hash::FxHashMap;

use anyhow::Result;

use futures::executor::ThreadPool;
use futures::task::{LocalSpawn, LocalSpawnExt, Spawn, SpawnExt};
use futures::{Future, FutureExt};

use crate::{asynchronous::AsyncResult, gluon::repl::GluonRepl};

use crate::gluon::GluonVM;

pub struct ReplWindow {
    repl: GluonRepl,

    line_input: String,
    output: String,

    output_tx: Sender<String>,
    output_rx: Receiver<String>,
}

impl ReplWindow {
    pub const ID: &'static str = "repl_window";

    pub fn new(repl: GluonRepl) -> Result<Self> {
        let line_input = String::new();
        let output = String::new();

        let (output_tx, output_rx) = futures::channel::mpsc::channel(16);

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

            output_tx,
            output_rx,
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

        while let Ok(msg) = self.output_rx.try_next() {
            if let Some(msg) = msg {
                self.output.push_str(&msg);
            }
        }

        egui::Window::new("REPL")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .default_pos(pos)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    let history = egui::TextEdit::multiline(&mut self.output)
                        .text_style(egui::TextStyle::Monospace)
                        .enabled(false);

                    ui.add(history);

                    let input_box =
                        ui.text_edit_singleline(&mut self.line_input);

                    if ui.button("Submit").clicked()
                        || (input_box.lost_focus()
                            && ui.input().key_pressed(egui::Key::Enter))
                    {
                        let future =
                            self.repl.gluon_vm.eval_line(&self.line_input);

                        self.line_input.clear();

                        let sender = self.output_tx.clone();
                        thread_pool
                            .spawn(async move {
                                let result = future.await;

                                let mut sender = sender;

                                match result {
                                    gluon::vm::api::IO::Value(_v) => {
                                        println!(" in Value");
                                        sender
                                            .send("\n".to_string())
                                            .await
                                            .unwrap();
                                    }
                                    gluon::vm::api::IO::Exception(err) => {
                                        // emit
                                        dbg!(err.clone());
                                        // println!("in err: {}", err);
                                        sender
                                            .send(format!("{}", err))
                                            .await
                                            .unwrap();
                                    }
                                }
                            })
                            .unwrap();
                    }
                });
            })
    }
}
