use crossbeam::atomic::AtomicCell;
use futures::future::RemoteHandle;
use handlegraph::pathhandlegraph::{
    GraphPathNames, GraphPaths, IntoPathIds, PathId,
};

use lazy_static::lazy_static;

use rustc_hash::FxHashSet;

use bstr::ByteSlice;
use parking_lot::Mutex;

use crate::gui::util as gui_util;

use crate::{
    app::{AppChannels, AppMsg, SharedState},
    geometry::{Point, Rect},
    gui::console::Console,
    reactor::Reactor,
    universe::Node,
    vulkan::compute::path_view::PathViewRenderer,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    PathId,
    Name,
    LengthBp,
}

lazy_static! {
    static ref ZOOM_UPDATE_FUTURE: Mutex<Option<RemoteHandle<()>>> =
        Mutex::new(None);
    static ref FILTERED_IDS: Mutex<Vec<PathId>> = Mutex::new(Vec::new());
    static ref NAME_FILTER: Mutex<String> = Mutex::new(String::new());
    static ref PREV_HASH: AtomicCell<u64> = AtomicCell::new(0);
    static ref MARK_PATHS: AtomicCell<bool> = AtomicCell::new(false);
    static ref VIEW_RANGE: AtomicCell<(f64, f64)> = AtomicCell::new((0.0, 1.0));
    static ref ROW_RANGE: AtomicCell<(usize, usize)> =
        (0, std::usize::MAX).into();
    static ref SORT_ORDER: AtomicCell<SortOrder> = SortOrder::PathId.into();
    static ref REV_SORT: AtomicCell<bool> = false.into();
    static ref MOUSE_OVER_IMG: AtomicCell<bool> = false.into();
}

pub struct PathPositionList {}

impl PathPositionList {
    pub const ID: &'static str = "path_position_list";

    fn apply_filter(reactor: &Reactor) {
        let needle = NAME_FILTER.lock();

        let mut filtered = FILTERED_IDS.lock();
        filtered.clear();

        // if there is no filter, only clear the filter list
        if needle.is_empty() {
            return;
        }

        let graph_query = reactor.graph_query.clone();
        let graph = graph_query.graph();

        let paths = graph.path_ids();

        let mut name_buf = Vec::new();

        filtered.extend(paths.filter_map(|p| {
            let name = graph.get_path_name(p)?;
            name_buf.clear();
            name_buf.extend(name);

            name_buf.contains_str(needle.as_bytes()).then(|| p)
        }));
    }

    pub fn ui(
        ctx: &egui::CtxRef,
        open: &mut bool,
        console: &Console,
        reactor: &Reactor,
        channels: &AppChannels,
        shared_state: &SharedState,
        path_view: &PathViewRenderer,
        nodes: &[Node],
    ) {
        let graph_query = reactor.graph_query.clone();
        let graph = graph_query.graph();

        let path_count = graph.path_count();

        let _inner_resp = egui::Window::new("Path View")
            .id(egui::Id::new(Self::ID))
            .open(open)
            .show(ctx, |ui| {
                let mut filter_changed = false;

                let mut to_mark: Vec<PathId> = Vec::new();

                let name_resp = ui.horizontal(|ui| {
                    if ui.button("Reset").clicked() {
                        path_view.reset_zoom();
                    }

                    let mut name_filter = NAME_FILTER.lock();
                    let text_entry = ui
                        .text_edit_singleline(&mut name_filter as &mut String);

                    if text_entry.changed() {
                        filter_changed = true;
                    }
                    name_filter.to_string()
                });

                let name = name_resp.inner;

                if filter_changed {
                    Self::apply_filter(reactor);
                }

                let scroll_align = gui_util::add_scroll_buttons(ui);

                ui.horizontal(|ui| {
                    let mut order = SORT_ORDER.load();
                    let rev = REV_SORT.load();
                    ui.selectable_value(&mut order, SortOrder::PathId, "Path Id");
                    ui.selectable_value(&mut order, SortOrder::Name, "Path Name");
                    ui.selectable_value(&mut order, SortOrder::LengthBp, "Path Length");

                    if ui.selectable_label(rev, "Reverse Sort").clicked() {
                        REV_SORT.fetch_xor(true);
                    };
                    SORT_ORDER.store(order);

                });

                let sort_order = SORT_ORDER.load();
                let rev_sort = REV_SORT.load();

                let (paths_to_show, num_rows) = {
                    let path_ids = match sort_order {
                        SortOrder::PathId => &path_view.path_id_order,
                        SortOrder::Name => &path_view.path_name_order,
                        SortOrder::LengthBp => &path_view.path_length_order,
                    };

                    (path_ids, graph.path_count())
                };


                let paths_to_show: Box<dyn Iterator<Item = _>>  = if name.is_empty() {

                    if rev_sort {
                        Box::new(paths_to_show.iter().rev()) as _
                    } else {
                        Box::new(paths_to_show.iter()) as _
                    }

                } else {
                    let filtered = FILTERED_IDS.lock();
                    let filtered = filtered.iter().copied().collect::<FxHashSet<_>>();


                    if rev_sort {
                        Box::new(paths_to_show.iter().rev().filter(move |path| {
                            filtered.contains(path)
                        })) as _
                    } else {
                        Box::new(paths_to_show.iter().filter(move |path| {
                            filtered.contains(path)
                        })) as _
                    }
                };

                let mut path_range = 0..num_rows;

                let row_height = 32.0;

                let enable_scrolling = !MOUSE_OVER_IMG.load();
                MOUSE_OVER_IMG.store(false);

                // egui::ScrollArea::vertical()
                gui_util::scrolled_area(ui, num_rows, scroll_align)
                    // .auto_shrink([true, true])
                    // .max_height((VISIBLE_ROWS.load() as f32) * row_height)
                    .enable_scrolling(enable_scrolling)
                    .show_rows(
                        ui,
                        row_height, // todo actually calculate this??
                        // VISIBLE_ROWS.load(),
                        num_rows,
                        |ui, range| {

                            let take_n = range.start.max(range.end) - range.start;
                            path_range = range;
                            if path_range.start > path_range.end {
                                path_range = path_range.end..path_range.end
                            }

                            let row_range = (path_range.start, path_range.end);


                            if row_range != ROW_RANGE.load() {
                                MARK_PATHS.store(true);
                                ROW_RANGE.store(row_range);
                            }

                            egui::Grid::new("path_position_list_grid").show(
                                ui,
                                |ui| {
                                    ui.label("Path");

                                    let mut rows = Vec::new();

                                    let path_pos = graph_query.path_positions();

                                    let dy: f32 = 1.0 / 64.0;
                                    let oy: f32 = dy / 2.0;

                                    let (mut left, mut right) =
                                        VIEW_RANGE.load();

                                    let l = left;
                                    let r = right;

                                    let left_v =
                                        egui::DragValue::new::<f64>(&mut left)
                                            .speed(0.01)
                                            .clamp_range(0.0..=(r - 0.005));
                                    let right_v =
                                        egui::DragValue::new::<f64>(&mut right)
                                            .speed(0.01)
                                            .clamp_range((l + 0.005)..=1.0);

                                    let left_w = ui.with_layout(
                                        egui::Layout::right_to_left(),
                                        |ui| ui.add(left_v),
                                    );

                                    ui.label("");

                                    let right_w = ui.with_layout(
                                        egui::Layout::left_to_right(),
                                        |ui| ui.add(right_v),
                                    );

                                    let edges =
                                        left_w.inner.union(right_w.inner);

                                    if edges.dragged() {
                                        VIEW_RANGE.store((left, right));
                                    } else if !(edges.dragged()
                                        || edges.has_focus())
                                    {
                                        VIEW_RANGE.store(path_view.view());
                                    }

                                    if (edges.changed()
                                        && (!edges.dragged()
                                            && !edges.has_focus()))
                                        || edges.lost_focus()
                                        || edges.drag_released()
                                    {
                                        path_view
                                            .set_visible_range(left, right);
                                        MARK_PATHS.store(true);
                                    }

                                    ui.end_row();

                                    for (i_ix, &path) in paths_to_show
                                        .enumerate()
                                        .skip(path_range.start)
                                        .take(take_n)
                                    {

                                        to_mark.push(path);

                                        let path_name = graph
                                            .get_path_name_vec(path)
                                            .unwrap();

                                        let path_len = path_pos
                                            .path_base_len(path)
                                            .unwrap()
                                            as f32;

                                        ui.label(format!(
                                            "{}",
                                            path_name.as_bstr()
                                        ));

                                        let ix = path_view
                                            .find_path_row(path)
                                            .unwrap_or(0);

                                        let y = oy + (dy * ix as f32);

                                        let p0 = Point::new(0.0, y);
                                        let p1 = Point::new(1.0, y);

                                        let (left_pos, right_pos) = {
                                            let len = path_len as f64;
                                            (
                                                (left * len) as usize,
                                                (right * len) as usize,
                                            )
                                        };

                                        let _left_lb = ui.with_layout(
                                            egui::Layout::right_to_left(),
                                            |ui| ui.label(left_pos),
                                        );

                                        let row = if path_view.initialized() {
                                            let img = egui::Image::new(
                                                egui::TextureId::User(1),
                                                Point { x: 512.0, y: 32.0 },
                                            )
                                            .uv(Rect::new(p0, p1));

                                            ui.add(img)
                                        } else {
                                            ui.label("loading")
                                        };

                                        let _right_lb = ui.with_layout(
                                            egui::Layout::left_to_right(),
                                            |ui| ui.label(right_pos),
                                        );

                                        ui.end_row();

                                        let interact = ui.interact(
                                            row.rect,
                                            egui::Id::new(Self::ID).with(i_ix),
                                            egui::Sense::click_and_drag(),
                                        );

                                        if interact.dragged() {
                                            let delta = interact.drag_delta();

                                            // the pan() function uses pixels in
                                            // terms of the image data, so we need
                                            // to scale up the drag delta here
                                            let n =
                                                delta.x / interact.rect.width();

                                            path_view.pan(n as f64);
                                        }

                                        if interact.drag_released() {
                                            MARK_PATHS.store(true);
                                        }

                                        if let Some(pos) = interact.hover_pos()
                                        {
                                            MOUSE_OVER_IMG.fetch_or(true);

                                            let scroll_delta =
                                                ui.input().scroll_delta;

                                            if scroll_delta.y != 0.0 {
                                                // log::warn!(
                                                //     "scroll delta: {}",
                                                //     scroll_delta.y
                                                // );

                                                let d = if scroll_delta.y > 0.0
                                                {
                                                    1.0 / 1.05
                                                } else {
                                                    1.05
                                                };

                                                path_view.zoom(d);

                                                let fut = async move {
                                                    let delay = futures_timer::Delay::new(
                                                        std::time::Duration::from_millis(
                                                            150,
                                                        ),
                                                    );
                                                    delay.await;

                                                    MARK_PATHS.store(true);
                                                };

                                                {
                                                    let mut lock =
                                                        ZOOM_UPDATE_FUTURE
                                                            .lock();

                                                    if let Ok(handle) =
                                                        reactor.spawn(fut)
                                                    {
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

                                            let n =
                                                (p_.x / width).clamp(0.0, 1.0);

                                            let pos = (path_len * n) as usize;

                                            let y = ix;
                                            let x = ((path_view.width as f32)
                                                * n)
                                                as usize;

                                            let node =
                                                path_view.get_node_at(x, y);

                                            if let Some(node) = node {
                                                let ix = (node.0 - 1) as usize;
                                                if let Some(pos) = nodes.get(ix)
                                                {
                                                    let world = pos.center();

                                                    let view =
                                                        shared_state.view();

                                                    let screen = view
                                                        .world_point_to_screen(
                                                            world,
                                                        );

                                                    let screen_rect = ctx
                                                        .input()
                                                        .screen_rect();
                                                    let dims = Point::new(
                                                        screen_rect.width(),
                                                        screen_rect.height(),
                                                    );

                                                    let screen =
                                                        screen + dims / 2.0;

                                                    let dims = shared_state
                                                        .screen_dims();

                                                    if screen.x > 0.0
                                                        && screen.y > 0.0
                                                        && screen.x < dims.width
                                                        && screen.y
                                                            < dims.height
                                                    {
                                                        egui::show_tooltip_at(
                                                        ctx,
                                                        egui::Id::new(
                                                            "path_view_tooltip",
                                                        ),
                                                        Some(screen.into()),
                                                        |ui| {
                                                            //
                                                            ui.label(format!(
                                                                "{}",
                                                                node.0
                                                            ));
                                                        },
                                                    );
                                                    }
                                                }
                                            }

                                            if interact.clicked() {
                                                log::warn!(
                                        "clicked at {}, pos {}, node {:?}",
                                        n,
                                        pos,
                                        node
                                    );

                                                if let Some(node) = node {
                                                    let msg =
                                                        AppMsg::goto_node(node);
                                                    channels
                                                        .app_tx
                                                        .send(msg)
                                                        .unwrap();
                                                }
                                            }
                                        }

                                        rows.push(interact);
                                    }
                                },
                            )
                        },
                    );

                if MARK_PATHS.load() {
                    path_view.mark_load_paths(to_mark).unwrap();
                    MARK_PATHS.store(false);
                }
            });
    }
}
