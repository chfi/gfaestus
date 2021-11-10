use crossbeam::atomic::AtomicCell;
use handlegraph::pathhandlegraph::{GraphPathNames, PathId};

use lazy_static::lazy_static;

use bstr::ByteSlice;

use crate::{gui::console::Console, reactor::Reactor};

lazy_static! {
    static ref CONSOLE_ADDED: AtomicCell<bool> = AtomicCell::new(false);
}

pub struct PathPositionList {}

impl PathPositionList {
    pub const ID: &'static str = "path_position_list";

    pub const PATHS: &'static str = "gui/path_position_list/paths";

    pub fn ui(
        ctx: &egui::CtxRef,
        open: &mut bool,
        console: &Console,
        reactor: &mut Reactor,
    ) {
        // hacky but works
        if !CONSOLE_ADDED.load() {
            let mut paths: Vec<rhai::Dynamic> = Vec::new();

            paths.push(rhai::Dynamic::from(PathId(0)));
            paths.push(rhai::Dynamic::from(PathId(1)));
            paths.push(rhai::Dynamic::from(PathId(2)));
            paths.push(rhai::Dynamic::from(PathId(3)));

            console
                .get_set
                .set_vars([(Self::PATHS, rhai::Dynamic::from(paths))]);

            log::warn!("initialized PathPositionList");

            CONSOLE_ADDED.store(true);
        }

        egui::Window::new("Path View")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |ui| {
                if let Some(paths) = console.get_set.get_var(Self::PATHS) {
                    let paths: Vec<rhai::Dynamic> = paths.cast();

                    for (ix, path) in paths.into_iter().enumerate() {
                        let path: PathId = path.cast();

                        let path_name = reactor
                            .graph_query
                            .graph
                            .get_path_name_vec(path)
                            .unwrap();

                        ui.label(format!(
                            "{} - {}",
                            path.0,
                            path_name.as_bstr()
                        ));
                    }
                }
            });
    }
}
