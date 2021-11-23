use crossbeam::atomic::AtomicCell;
use futures::future::RemoteHandle;
use handlegraph::pathhandlegraph::{
    GraphPathNames, GraphPaths, PathId, PathSequences,
};

use lazy_static::lazy_static;

use rustc_hash::FxHashMap;

use bstr::ByteSlice;
use parking_lot::Mutex;

use crate::{
    app::{AppChannels, AppMsg, SharedState},
    geometry::{Point, Rect},
    gui::console::Console,
    reactor::Reactor,
    universe::Node,
    vulkan::compute::path_view::PathViewRenderer,
};

lazy_static! {
    static ref CONSOLE_ADDED: AtomicCell<bool> = AtomicCell::new(false);
    // static ref ZOOM_UPDATE_FUTURE: Mutex<Option<Box<dyn Future<Output = ()>>>> =
    static ref ZOOM_UPDATE_FUTURE: Mutex<Option<RemoteHandle<()>>> =
        Mutex::new(None);
}

pub struct PathPositionList {}

impl PathPositionList {
    pub const ID: &'static str = "path_position_list";

    pub const PATHS: &'static str = "gui/path_position_list/paths";
    pub const RELOAD: &'static str = "gui/path_position_list/reload";

    pub fn ui(
        ctx: &egui::CtxRef,
        open: &mut bool,
        console: &Console,
        reactor: &mut Reactor,
        channels: &AppChannels,
        shared_state: &SharedState,
        path_view: &PathViewRenderer,
        nodes: &[Node],
    ) {
        // hacky but works
        if !CONSOLE_ADDED.load() {
            let mut paths: Vec<rhai::Dynamic> = Vec::new();

            paths.push(rhai::Dynamic::from(PathId(0)));
            paths.push(rhai::Dynamic::from(PathId(1)));
            paths.push(rhai::Dynamic::from(PathId(2)));
            paths.push(rhai::Dynamic::from(PathId(3)));

            console.get_set.set_vars([
                (Self::PATHS, rhai::Dynamic::from(paths)),
                (Self::RELOAD, rhai::Dynamic::from(true)),
            ]);

            log::warn!("initialized PathPositionList");

            CONSOLE_ADDED.store(true);
        }

        egui::Window::new("Path View")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |ui| {

                if ui.button("Reset").clicked() {
                    path_view.reset_zoom();
                }

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

                                let graph_query = reactor.graph_query.clone();
                                let graph = graph_query.graph();
                                let path_pos =
                                    graph_query.path_positions();

                                let dy: f32 = 1.0 / 64.0;
                                let oy: f32 = dy / 2.0;

                                for (ix, path) in
                                    paths.into_iter().enumerate()
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
                                        Point { x: 1024.0, y: 32.0 },
                                    )
                                    .uv(Rect::new(p0, p1));
                                    let row = ui.add(img);

                                    ui.end_row();

                                    let interact = ui.interact(
                                        row.rect,
                                        egui::Id::new(Self::ID).with(ix),
                                        egui::Sense::click_and_drag()
                                    );


                                    if interact.dragged() {
                                        let delta = interact.drag_delta();
                                        // log::warn!("image drag delta: {}", delta.x);

                                        // the pan() function uses
                                        // pixels in terms of the
                                        // image data, so we need to
                                        // scale up the drag delta
                                        // here
                                        let scaled_delta = 2.0 * delta.x as f64;
                                        // let scaled_delta = 0.5 * delta.x as f64;

                                        path_view.pan(scaled_delta);

                                    }

                                    if interact.drag_released() {
                                        path_view.force_reload();
                                    }


                                    if let Some(pos) = interact.hover_pos() {


                                        let scroll_delta = ui.input().scroll_delta;

                                        if scroll_delta.y != 0.0 {
                                            log::warn!("scroll delta: {}", scroll_delta.y);


                                            let d = if scroll_delta.y > 0.0 {
                                                1.0 / 1.05
                                            } else {
                                                1.05
                                            };

                                            path_view.zoom(d);

                                            let path_view_state = path_view.state.clone();

                                            let fut = async move {
                                                use futures_timer::Delay;
                                                let delay = Delay::new(std::time::Duration::from_millis(150));
                                                delay.await;

                                                path_view_state.force_reload();
                                            };

                                            {
                                                let mut lock = ZOOM_UPDATE_FUTURE.lock();

                                                if let Ok(handle) = reactor.spawn(fut) {
                                                    *lock = Some(handle);
                                                } else {
                                                    *lock = None;
                                                }
                                            }


                                        }

                                        let rect = interact.rect;

                                        let p0 = Point::from(rect.min);
                                        let p = Point::from(pos);

                                        let width = rect.width();

                                        let p_ = p - p0;

                                        let n = (p_.x / width).clamp(0.0, 1.0);

                                        let pos = (path_len * n) as usize;

                                        let y = ix;
                                        let x = ((path_view.width as f32) * n) as usize;
                                        // let x = ((path_view.width as f32) * n * 2.0) as usize;

                                        let node = path_view.get_node_at(x, y);


                                        if let Some(node) = node {
                                            let ix = (node.0 - 1) as usize;
                                            if let Some(pos) = nodes.get(ix) {
                                                let world = pos.center();

                                                let view = shared_state.view();

                                                let screen = view.world_point_to_screen(world);

                                                let screen_rect = ctx.input().screen_rect();
                                                let dims = Point::new(screen_rect.width(), screen_rect.height());

                                                let screen = screen + dims / 2.0;

                                                let dims = shared_state.screen_dims();

                                                if screen.x > 0.0 && screen.y > 0.0 && screen.x < dims.width && screen.y < dims.height {
                                                    egui::show_tooltip_at(ctx, egui::Id::new("path_view_tooltip"), Some(screen.into()), |ui| {
                                                        //
                                                        ui.label(format!("{}", node.0));
                                                    });
                                                }
                                            }

                                            // let msg = AppMsg::goto_node(node);
                                            // channels.app_tx.send(msg).unwrap();
                                        }


                                        if interact.clicked() {


                                            log::warn!(
                                                "clicked at {}, pos {}, node {:?}",
                                                n,
                                                pos,
                                                node
                                            );

                                            if let Some(node) = node {
                                                let msg = AppMsg::goto_node(node);
                                                channels.app_tx.send(msg).unwrap();
                                            }
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
