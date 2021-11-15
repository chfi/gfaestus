use crossbeam::atomic::AtomicCell;
use handlegraph::pathhandlegraph::{
    GraphPathNames, GraphPaths, PathId, PathSequences,
};

use lazy_static::lazy_static;

use rustc_hash::FxHashMap;

use bstr::ByteSlice;
use parking_lot::Mutex;

use crate::{
    geometry::{Point, Rect},
    gui::console::Console,
    reactor::Reactor,
};

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

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("path_position_list_grid").show(
                            ui,
                            |ui| {
                                ui.label("Path");
                                ui.separator();

                                ui.label("-");
                                ui.end_row();

                                let mut rows = Vec::new();

                                let graph = reactor.graph_query.graph();
                                let path_pos =
                                    reactor.graph_query.path_positions();

                                let dy: f32 = 1.0 / 64.0;
                                let oy: f32 = dy / 2.0;

                                for (ix, path) in
                                    paths.into_iter().enumerate().rev()
                                {
                                    let path: PathId = path.cast();

                                    let path_name =
                                        graph.get_path_name_vec(path).unwrap();

                                    let path_len =
                                        path_pos.path_base_len(path).unwrap()
                                            as f32;

                                    // let mut row =
                                    ui.label(format!("{}", path.0));
                                    ui.separator();

                                    ui.label(format!(
                                        "{}",
                                        path_name.as_bstr()
                                    ));

                                    ui.separator();

                                    let y = oy + (dy * ix as f32);

                                    let p0 = Point::new(0.0, y);
                                    let p1 = Point::new(1.0, y);

                                    let img = egui::Image::new(
                                        egui::TextureId::User(1),
                                        Point { x: 256.0, y: 64.0 },
                                    )
                                    .uv(Rect::new(p0, p1));
                                    let row = ui.add(img);

                                    ui.end_row();

                                    let interact = ui.interact(
                                        row.rect,
                                        egui::Id::new(Self::ID).with(ix),
                                        egui::Sense::hover()
                                            .union(egui::Sense::click()),
                                    );

                                    if let Some(pos) = interact.hover_pos() {
                                        let rect = interact.rect;

                                        let p0 = Point::from(rect.min);
                                        let p = Point::from(pos);

                                        let width = rect.width();

                                        let p_ = p - p0;

                                        let n = (p_.x / width).clamp(0.0, 1.0);

                                        let pos = (path_len * n) as usize;

                                        /*
                                        if let Some(step) = path_pos.find_step_at_base(path, pos) {
                                            log::warn!(
                                                "step {:?}", step
                                            );

                                            if let Some(handle) = graph.path_handle_at_step(path, step) {
                                                log::warn!("handle {:?}", handle);
                                                egui::show_tooltip(
                                                    ctx,
                                                    egui::Id::new("path_position_list_tooltip"),
                                                    |ui| {
                                                        ui.label(handle.id());
                                                    },
                                                );

                                            }
                                        }
                                        */

                                        if interact.clicked() {
                                            log::warn!(
                                                "clicked at {}, pos {}",
                                                n,
                                                pos
                                            );
                                        }
                                    }

                                    rows.push(interact);
                                }
                            },
                        )
                    });
                }
            });
    }
}
