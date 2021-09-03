#[allow(unused_imports)]
use compute::EdgePreprocess;
use gfaestus::annotations::{BedRecords, ClusterCache, Gff3Records};
use gfaestus::gui::console::Console;
use gfaestus::vulkan::draw_system::edges::EdgeRenderer;
use rustc_hash::FxHashMap;
use texture::Gradients;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::platform::unix::*;
#[allow(unused_imports)]
use winit::window::{Window, WindowBuilder};

use argh::FromArgs;

use gfaestus::app::mainview::*;
use gfaestus::app::{App, AppMsg};
use gfaestus::geometry::*;
use gfaestus::graph_query::*;
use gfaestus::input::*;
use gfaestus::overlays::*;
use gfaestus::universe::*;
use gfaestus::view::View;
use gfaestus::vulkan::render_pass::Framebuffers;

use gfaestus::gui::{widgets::*, windows::*, *};

use gfaestus::vulkan::debug;

#[allow(unused_imports)]
use gfaestus::vulkan::draw_system::{
    nodes::{NodeOverlay, NodeOverlayValue, Overlay},
    post::PostProcessPipeline,
};

use gfaestus::vulkan::draw_system::selection::{
    SelectionOutlineBlurPipeline, SelectionOutlineEdgePipeline,
};

use gfaestus::vulkan::compute::{
    ComputeManager, GpuSelection, NodeTranslation,
};

use anyhow::Result;

use ash::version::DeviceV1_0;
use ash::{vk, Device};

#[allow(unused_imports)]
use futures::executor::{ThreadPool, ThreadPoolBuilder};

use std::collections::HashMap;
use std::sync::Arc;

#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

#[allow(unused_imports)]
use handlegraph::packedgraph::PackedGraph;

use gfaestus::vulkan::*;

use flexi_logger::{Duplicate, FileSpec, Logger, LoggerHandle};

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

fn universe_from_gfa_layout(
    graph_query: &GraphQuery,
    layout_path: &str,
) -> Result<(Universe<FlatLayout>, GraphStats)> {
    let graph = graph_query.graph();

    let universe = Universe::from_laid_out_graph(&graph, layout_path)?;

    let stats = GraphStats {
        node_count: graph.node_count(),
        edge_count: graph.edge_count(),
        path_count: graph.path_count(),
        total_len: graph.total_length(),
    };

    Ok((universe, stats))
}

fn set_up_logger() -> Result<LoggerHandle> {
    let logger = Logger::try_with_env_or_str("info")?
        .log_to_file(FileSpec::default())
        .duplicate_to_stderr(Duplicate::Debug)
        .start()?;

    Ok(logger)
}

fn main() {
    let args: Args = argh::from_env();

    let _logger = set_up_logger().unwrap();

    let gfa_file = &args.gfa;
    let layout_file = &args.layout;

    let event_loop: EventLoop<()> = if args.force_x11 {
        if let Ok(ev_loop) = EventLoop::new_x11() {
            ev_loop
        } else {
            error!("Error initializing X11 window, falling back to default");
            EventLoop::new()
        }
    } else {
        EventLoop::new()
    };

    let window = WindowBuilder::new()
        .with_title("Gfaestus")
        .with_inner_size(winit::dpi::PhysicalSize::new(800, 600))
        .build(&event_loop)
        .unwrap();

    let mut gfaestus = match GfaestusVk::new(&window) {
        Ok(app) => app,
        Err(err) => {
            error!("Error initializing Gfaestus");
            error!("{:?}", err.root_cause());
            std::process::exit(1);
        }
    };

    let num_cpus = num_cpus::get();

    let futures_cpus;
    let rayon_cpus;

    // TODO this has to be done much more intelligently
    if num_cpus < 4 {
        futures_cpus = 1;
        rayon_cpus = 1;
    } else if num_cpus == 4 {
        futures_cpus = 1;
        rayon_cpus = 2;
    } else if num_cpus <= 6 {
        futures_cpus = 2;
        rayon_cpus = num_cpus - 3;
    } else {
        futures_cpus = 3;
        rayon_cpus = num_cpus - 4;
    }

    // TODO make sure to set thread pool size to less than number of CPUs
    let thread_pool = ThreadPoolBuilder::new()
        .pool_size(futures_cpus)
        .create()
        .unwrap();

    let rayon_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(rayon_cpus)
        .build()
        .unwrap();

    info!("Loading GFA");
    let t = std::time::Instant::now();

    let graph_query = Arc::new(GraphQuery::load_gfa(gfa_file).unwrap());

    let mut reactor = gfaestus::reactor::Reactor::init(
        thread_pool.clone(),
        rayon_pool,
        graph_query.clone(),
    );

    let graph_query_worker =
        GraphQueryWorker::new(graph_query.clone(), thread_pool.clone());

    let (mut universe, stats) =
        universe_from_gfa_layout(&graph_query, layout_file).unwrap();

    let (top_left, bottom_right) = universe.layout().bounding_box();

    let _center = Point {
        x: top_left.x + (bottom_right.x - top_left.x) / 2.0,
        y: top_left.y + (bottom_right.y - top_left.y) / 2.0,
    };

    info!(
        "layout bounding box\t({:.2}, {:.2})\t({:.2}, {:.2})",
        top_left.x, top_left.y, bottom_right.x, bottom_right.y
    );
    info!(
        "layout width: {:.2}\theight: {:.2}",
        bottom_right.x - top_left.x,
        bottom_right.y - top_left.y
    );

    info!("GFA loaded in {:.3} sec", t.elapsed().as_secs_f64());

    info!(
        "Loaded {} nodes\t{} points",
        universe.layout().nodes().len(),
        universe.layout().nodes().len() * 2
    );

    let mut compute_manager = ComputeManager::new(
        gfaestus.vk_context().device().clone(),
        gfaestus.graphics_family_index,
        gfaestus.graphics_queue,
    )
    .unwrap();

    let gpu_selection =
        GpuSelection::new(&gfaestus, graph_query.node_count()).unwrap();

    let node_translation =
        NodeTranslation::new(&gfaestus, graph_query.node_count()).unwrap();

    let mut select_fence_id: Option<usize> = None;
    let mut translate_fence_id: Option<usize> = None;

    let (winit_tx, winit_rx) =
        crossbeam::channel::unbounded::<WindowEvent<'static>>();

    let mut app = App::new((100.0, 100.0)).expect("error when creating App");

    let mut input_manager = InputManager::new(winit_rx, app.shared_state());

    input_manager.add_binding(winit::event::VirtualKeyCode::A, move || {
        println!("i'm a bound command!");
    });

    let app_rx = input_manager.clone_app_rx();
    let main_view_rx = input_manager.clone_main_view_rx();
    let gui_rx = input_manager.clone_gui_rx();

    let node_vertices = universe.new_vertices();

    let mut main_view = MainView::new(
        &gfaestus,
        app.clone_channels(),
        app.settings.clone(),
        app.shared_state().clone(),
        // app.settings.node_width().clone(),
        graph_query.node_count(),
    )
    .unwrap();

    let mut gui = Gui::new(
        &gfaestus,
        &mut reactor,
        app.shared_state().clone(),
        app.channels(),
        app.settings.clone(),
        &graph_query,
    )
    .unwrap();

    if let Some(script_file) = args.run_script.as_ref() {
        warn!("executing script file {}", script_file);
        gui.console
            .eval_file(&mut reactor, true, script_file)
            .unwrap();
    }

    let mut initial_view: Option<View> = None;
    let mut initialized_view = false;

    let new_overlay_rx = reactor.overlay_create_rx.clone();

    gui.app_view_state().graph_stats().send(GraphStatsMsg {
        node_count: Some(stats.node_count),
        edge_count: Some(stats.edge_count),
        path_count: Some(stats.path_count),
        total_len: Some(stats.total_len),
    });

    main_view
        .node_draw_system
        .vertices
        .upload_vertices(&gfaestus, &node_vertices)
        .unwrap();

    let use_quad_renderer = {
        let vk_ctx = gfaestus.vk_context();

        if vk_ctx.portability_subset {
            let subset_features =
                gfaestus.vk_context().portability_features().unwrap();
            println!("subset_features: {:?}", subset_features);
            subset_features.tessellation_isolines == vk::FALSE
        } else {
            false
        }
    };

    if use_quad_renderer {
        warn!("using the quad edge renderer");
    } else {
        warn!("using the isoline edge renderer");
    }

    let mut edge_renderer = EdgeRenderer::new(
        &gfaestus,
        &graph_query.graph_arc(),
        universe.layout(),
        gfaestus.msaa_samples,
        gfaestus.render_passes.edges,
        use_quad_renderer,
    )
    .unwrap();

    app.themes
        .upload_to_gpu(
            &gfaestus,
            &mut main_view.node_draw_system.theme_pipeline,
        )
        .unwrap();

    main_view
        .node_draw_system
        .theme_pipeline
        .set_active_theme(0)
        .unwrap();

    let mut dirty_swapchain = false;

    let mut selection_edge = SelectionOutlineEdgePipeline::new(
        &gfaestus,
        1,
        gfaestus.render_passes.selection_edge_detect,
        gfaestus.node_attachments.mask_resolve,
    )
    .unwrap();

    let mut selection_blur = SelectionOutlineBlurPipeline::new(
        &gfaestus,
        1,
        gfaestus.render_passes.selection_blur,
        gfaestus.node_attachments.mask_resolve,
    )
    .unwrap();

    let gui_msg_tx = gui.clone_gui_msg_tx();

    let gradients = Gradients::initialize(
        &gfaestus,
        gfaestus.transient_command_pool,
        gfaestus.graphics_queue,
        1024,
    )
    .unwrap();

    gui.populate_overlay_list(
        main_view
            .node_draw_system
            .overlay_pipelines
            .overlay_names()
            .into_iter(),
    );

    const FRAME_HISTORY_LEN: usize = 10;
    let mut frame_time_history = [0.0f32; FRAME_HISTORY_LEN];
    let mut frame = 0;

    // hack to make the initial view correct -- we need to have the
    // event loop run and get a resize event before we know the
    // correct size, but we don't want to modify the current view
    // whenever the window resizes, so we use a timeout instead
    let initial_resize_timer = std::time::Instant::now();

    if app.themes.is_active_theme_dark() {
        gui_msg_tx.send(GuiMsg::SetDarkMode).unwrap();
    } else {
        gui_msg_tx.send(GuiMsg::SetLightMode).unwrap();
    }

    /*
    let mut fence_id: Option<usize> = None;
    let mut translate_timer = std::time::Instant::now();
    */

    let mut cluster_caches: HashMap<String, ClusterCache> = HashMap::default();
    let mut step_caches: FxHashMap<PathId, Vec<(Handle, _, usize)>> =
        FxHashMap::default();

    event_loop.run(move |event, _, control_flow| {

        *control_flow = ControlFlow::Poll;

        let event = if let Some(ev) = event.to_static() {
            ev
        } else {
            return;
        };

        if let Event::WindowEvent { event, .. } = &event {
            let ev = event.clone();
            winit_tx.send(ev).unwrap();
        }

        let screen_dims = app.dims();

        match event {
            Event::NewEvents(_) => {
                if initial_resize_timer.elapsed().as_millis() > 100 && !initialized_view {
                    main_view.reset_view();
                    initialized_view = true;
                }

                // hacky -- this should take place after mouse pos is updated
                // in egui but before input is sent to mainview
                input_manager.handle_events(&gui_msg_tx);

                let mouse_pos = app.mouse_pos();

                gui.push_event(egui::Event::PointerMoved(mouse_pos.into()));

                let hover_node = main_view
                    .read_node_id_at(mouse_pos)
                    .map(|nid| NodeId::from(nid as u64));

                app.channels().app_tx.send(AppMsg::HoverNode(hover_node)).unwrap();

                gui.set_hover_node(hover_node);

                if app.selection_changed() {
                    if let Some(selected) = app.selected_nodes() {
                        let mut nodes = selected.iter().copied().collect::<Vec<_>>();
                        nodes.sort();

                        gui.app_view_state()
                            .node_list()
                            .send(NodeListMsg::SetFiltered(nodes));

                        main_view.update_node_selection(selected).unwrap();
                    } else {
                        gui.app_view_state()
                            .node_list()
                            .send(NodeListMsg::SetFiltered(Vec::new()));

                        main_view.clear_node_selection().unwrap();
                    }
                }

                while let Ok((key_code, command)) = app.channels().binds_rx.try_recv() {
                    if let Some(cmd) = command {
                        input_manager.add_binding(key_code, cmd);
                        // input_manager.add_binding(key_code, Box::new(cmd));
                    } else {
                    }
                }

                while let Ok(app_in) = app_rx.try_recv() {
                    app.apply_input(app_in, &gui_msg_tx);
                }

                while let Ok(gui_in) = gui_rx.try_recv() {
                    gui.apply_input(&app.channels().app_tx, gui_in);
                }

                while let Ok(main_view_in) = main_view_rx.try_recv() {
                    main_view.apply_input(screen_dims, app.mouse_pos(), main_view_in);
                }

                while let Ok(app_msg) = app.channels().app_rx.try_recv() {


                    if let AppMsg::RectSelect(rect) = &app_msg {

                        if select_fence_id.is_none() && translate_fence_id.is_none() {
                            let fence_id = gpu_selection.rectangle_select(
                                &mut compute_manager,
                                &main_view.node_draw_system.vertices,
                                *rect
                            ).unwrap();

                            select_fence_id = Some(fence_id);
                        }

                    }

                    if let AppMsg::TranslateSelected(delta) = &app_msg {
                        if select_fence_id.is_none() && translate_fence_id.is_none() {

                            let fence_id = node_translation
                                .translate_nodes(
                                    &mut compute_manager,
                                    &main_view.node_draw_system.vertices,
                                    &main_view.selection_buffer,
                                    *delta
                                ).unwrap();


                            translate_fence_id = Some(fence_id);
                        }
                    }

                    app.apply_app_msg(
                        main_view.main_view_msg_tx(),
                        &gui_msg_tx,
                        universe.layout().nodes(),
                        app_msg,
                    );
                }

                gui.apply_received_gui_msgs();

                while let Ok(main_view_msg) = main_view.main_view_msg_rx().try_recv() {
                    main_view.apply_msg(main_view_msg);
                }

                while let Ok(new_overlay) = new_overlay_rx.try_recv() {
                    if let Ok(_) = handle_new_overlay(
                        &gfaestus,
                        &mut main_view,
                        graph_query.node_count(),
                        new_overlay
                    ) {
                        gui.populate_overlay_list(
                            main_view
                                .node_draw_system
                                .overlay_pipelines
                                .overlay_names()
                                .into_iter(),
                        );
                    }
                }
            }
            Event::MainEventsCleared => {
                let screen_dims = app.dims();
                let mouse_pos = app.mouse_pos();
                main_view.update_view_animation(screen_dims, mouse_pos);

                let edge_ubo = app.settings.edge_renderer().load();

                edge_renderer.write_ubo(&edge_ubo).unwrap();

            }
            Event::RedrawEventsCleared => {


                let edge_ubo = app.settings.edge_renderer().load();
                let edge_width = edge_ubo.edge_width;

                if let Some(fid) = translate_fence_id {
                    if compute_manager.is_fence_ready(fid).unwrap() {
                        compute_manager.block_on_fence(fid).unwrap();
                        compute_manager.free_fence(fid, false).unwrap();

                        universe.update_positions_from_gpu(&gfaestus,
                                                           &main_view.node_draw_system.vertices).unwrap();

                        translate_fence_id = None;
                    }
                }

                if let Some(fid) = select_fence_id {

                    if compute_manager.is_fence_ready(fid).unwrap() {
                        let t = std::time::Instant::now();
                        compute_manager.block_on_fence(fid).unwrap();
                        compute_manager.free_fence(fid, false).unwrap();
                        trace!("block & free took {} ns", t.elapsed().as_nanos());

                        let t = std::time::Instant::now();
                        GfaestusVk::copy_buffer(gfaestus.vk_context().device(),
                                                gfaestus.transient_command_pool,
                                                gfaestus.graphics_queue,
                                                gpu_selection.selection_buffer.buffer,
                                                main_view.selection_buffer.buffer,
                                                main_view.selection_buffer.size);
                        trace!("buffer copy took {} ns", t.elapsed().as_nanos());


                        let t = std::time::Instant::now();
                        main_view
                            .selection_buffer
                            .fill_selection_set(gfaestus
                                                .vk_context()
                                                .device())
                            .unwrap();
                        trace!("fill_selection_set took {} ns", t.elapsed().as_nanos());

                        use gfaestus::app::Select;

                        let t = std::time::Instant::now();
                        app.channels().app_tx
                            .send(AppMsg::Selection(Select::Many {
                            nodes: main_view
                                .selection_buffer
                                .selection_set()
                                .clone(),
                            clear: true }))
                            .unwrap();
                        trace!("send took {} ns", t.elapsed().as_nanos());


                        select_fence_id = None;
                    }
                }

                let frame_t = std::time::Instant::now();

                if dirty_swapchain {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        app.update_dims([size.width as f32, size.height as f32]);
                        gfaestus
                            .recreate_swapchain(Some([size.width, size.height]))
                            .unwrap();

                        selection_edge.write_descriptor_set(
                            gfaestus.vk_context().device(),
                            gfaestus.node_attachments.mask_resolve,
                        );

                        selection_blur.write_descriptor_set(
                            gfaestus.vk_context().device(),
                            gfaestus.offscreen_attachment.color,
                        );

                        main_view
                            .recreate_node_id_buffer(&gfaestus, size.width, size.height)
                            .unwrap();

                        let new_initial_view =
                            View::from_dims_and_target(app.dims(), top_left, bottom_right);
                        if initial_view.is_none()
                            && initial_resize_timer.elapsed().as_millis() > 100
                        {
                            main_view.set_view(new_initial_view);
                            initial_view = Some(new_initial_view);
                        }

                        main_view.set_initial_view(
                            Some(new_initial_view.center),
                            Some(new_initial_view.scale),
                        );
                    } else {
                        return;
                    }
                }

                gui.begin_frame(
                    &mut reactor,
                    Some(app.dims().into()),
                    &graph_query,
                    &graph_query_worker,
                    app.annotations(),
                );

                let annotations = app.annotations();


                for label_set in annotations.visible_label_sets() {

                    if !step_caches.contains_key(&label_set.path_id) {
                        let steps = graph_query.path_pos_steps(label_set.path_id).unwrap();
                        step_caches.insert(label_set.path_id, steps);
                    }

                    let steps = step_caches.get(&label_set.path_id).unwrap();

                    let label_radius = app.settings.label_radius().load();

                    use gfaestus::annotations::AnnotationColumn;

                    let column = &label_set.column;

                    let records: &dyn std::any::Any = match column {
                        AnnotationColumn::Gff3(_) => {
                            let records: &Gff3Records = app
                                .annotations()
                                .get_gff3(&label_set.annotation_name)
                                .unwrap();

                            let records_any: &dyn std::any::Any = records as _;
                            records_any
                        }
                        AnnotationColumn::Bed(_) => {
                            let records: &BedRecords = app
                                .annotations()
                                .get_bed(&label_set.annotation_name)
                                .unwrap();

                            let records_any: &dyn std::any::Any = records as _;
                            records_any
                        }
                    };


                    if !cluster_caches.contains_key(label_set.name()) {
                        let cluster_cache = ClusterCache::new_cluster(
                            &steps,
                            universe.layout().nodes(),
                            label_set,
                            app.shared_state().view(),
                            label_radius
                        );

                        cluster_caches.insert(label_set.name().to_string(),
                                              cluster_cache);
                    }

                    let cluster_cache = cluster_caches
                        .get_mut(label_set.name())
                        .unwrap();

                    cluster_cache
                        .rebuild_cluster(
                            &steps,
                            universe.layout().nodes(),
                            app.shared_state().view(),
                            label_radius
                        );

                    for (node, cluster_indices) in cluster_cache.node_labels.iter() {
                        let mut y_offset = 20.0;
                        let mut count = 0;

                        let label_indices = &cluster_indices.label_indices;

                        for &label_ix in label_indices.iter() {

                            let label = &cluster_cache.label_set.label_strings()[label_ix];
                            let offset = &cluster_cache
                                .cluster_offsets[cluster_indices.offset_ix];

                            let anchor_dir = Point::new(-offset.x, -offset.y);
                            let offset = *offset * 20.0;

                            let rect = gfaestus::gui::text::draw_text_at_node_anchor(
                                &gui.ctx,
                                universe.layout().nodes(),
                                app.shared_state().view(),
                                *node,
                                offset + Point::new(0.0, y_offset),
                                anchor_dir,
                                label
                            );

                            if let Some(rect) = rect {
                                let rect = rect.resize(0.98);
                                if rect.contains(app.mouse_pos()) {
                                    gfaestus::gui::text::draw_rect(&gui.ctx, rect);

                                    // hacky way to check for a click
                                    // for now, because i can't figure
                                    // egui out
                                    if gui.ctx.input().pointer.any_click() {
                                        match column {
                                            AnnotationColumn::Gff3(col) => {
                                                if let Some(gff) = records.downcast_ref::<Gff3Records>() {
                                                    gui.scroll_to_gff_record(gff, col, label.as_bytes());
                                                }
                                            }
                                            AnnotationColumn::Bed(col) => {
                                                if let Some(bed) = records.downcast_ref::<BedRecords>() {
                                                    gui.scroll_to_bed_record(bed, col, label.as_bytes());
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            y_offset += 15.0;
                            count += 1;

                            if count > 10 {
                                let count = count.min(label_indices.len());
                                let rem = label_indices.len() - count;

                                if rem > 0 {
                                    let more_label = format!("and {} more", rem);

                                    gfaestus::gui::text::draw_text_at_node_anchor(
                                        &gui.ctx,
                                        universe.layout().nodes(),
                                        app.shared_state().view(),
                                        *node,
                                        offset + Point::new(0.0, y_offset),
                                        anchor_dir,
                                        &more_label
                                    );
                                }
                                break;
                            }
                        }
                    }
                }


                let meshes = gui.end_frame();

                gui.upload_texture(&gfaestus).unwrap();

                if !meshes.is_empty() {
                    gui.upload_vertices(&gfaestus, &meshes).unwrap();
                }

                let node_pass = gfaestus.render_passes.nodes;
                let edges_pass = gfaestus.render_passes.edges;
                let edge_pass = gfaestus.render_passes.selection_edge_detect;
                let blur_pass = gfaestus.render_passes.selection_blur;
                let gui_pass = gfaestus.render_passes.gui;

                let node_id_image = gfaestus.node_attachments.id_resolve.image;

                let offscreen_image = gfaestus.offscreen_attachment.color.image;

                main_view
                    .node_draw_system
                    .theme_pipeline
                    .set_active_theme(app.themes.active_theme())
                    .unwrap();

                let use_overlay = app.shared_state().overlay_state().use_overlay();

                let overlay =
                    app.shared_state().overlay_state().current_overlay();
                let push_descriptor = gfaestus.vk_context().push_descriptor().clone();

                let current_view = app.shared_state().view();

                let edges_enabled = app.shared_state().edges_enabled();

                let debug_utils = gfaestus.vk_context().debug_utils().map(|u| u.to_owned());

                let debug_utils = debug_utils.as_ref();

                let swapchain_dims = gfaestus.swapchain_dims();

                let draw =
                    |device: &Device, cmd_buf: vk::CommandBuffer, framebuffers: &Framebuffers| {
                        // let size = window.inner_size();
                        let size = swapchain_dims;

                        // let dims: [u32; 2] = swapchain_dims.into();

                        // if [size.width, size.height] != dims {
                        //     return;
                        // }


                        debug::begin_cmd_buf_label(
                            debug_utils,
                            cmd_buf,
                            "Image transitions"
                        );

                        unsafe {
                            let offscreen_image_barrier = vk::ImageMemoryBarrier::builder()
                                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                                .image(offscreen_image)
                                .subresource_range(vk::ImageSubresourceRange {
                                    aspect_mask: vk::ImageAspectFlags::COLOR,
                                    base_mip_level: 0,
                                    level_count: 1,
                                    base_array_layer: 0,
                                    layer_count: 1,
                                })
                                .build();

                            let memory_barriers = [];
                            let buffer_memory_barriers = [];
                            let image_memory_barriers = [offscreen_image_barrier];
                            device.cmd_pipeline_barrier(
                                cmd_buf,
                                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                                vk::PipelineStageFlags::FRAGMENT_SHADER,
                                vk::DependencyFlags::BY_REGION,
                                &memory_barriers,
                                &buffer_memory_barriers,
                                &image_memory_barriers,
                            );
                        }

                        debug::end_cmd_buf_label(debug_utils, cmd_buf);

                        debug::begin_cmd_buf_label(
                            debug_utils,
                            cmd_buf,
                            "Nodes",
                        );

                        let gradient_name = app.shared_state().overlay_state().gradient();
                        let gradient = gradients.gradient(gradient_name).unwrap();

                        main_view.draw_nodes(
                            cmd_buf,
                            node_pass,
                            framebuffers,
                            size.into(),
                            Point::ZERO,
                            overlay,
                            gradient,
                            use_overlay,
                        ).unwrap();


                        debug::end_cmd_buf_label(debug_utils, cmd_buf);

                        if edges_enabled {

                            debug::begin_cmd_buf_label(
                                debug_utils,
                                cmd_buf,
                                "Edges",
                            );

                            /*
                            edge_pipeline.preprocess_cmd(
                                cmd_buf,
                                current_view,
                                [size.width as f32, size.height as f32]
                            ).unwrap();

                            edge_pipeline.preprocess_memory_barrier(cmd_buf).unwrap();
                            */

                            edge_renderer.draw(
                                cmd_buf,
                                edge_width,
                                &main_view.node_draw_system.vertices,
                                edges_pass,
                                framebuffers,
                                size.into(),
                                2.0,
                                current_view,
                                Point::ZERO,
                            ).unwrap();

                            debug::end_cmd_buf_label(debug_utils, cmd_buf);
                        }


                        unsafe {
                            let image_memory_barrier = vk::ImageMemoryBarrier::builder()
                                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                                .image(node_id_image)
                                .subresource_range(vk::ImageSubresourceRange {
                                    aspect_mask: vk::ImageAspectFlags::COLOR,
                                    base_mip_level: 0,
                                    level_count: 1,
                                    base_array_layer: 0,
                                    layer_count: 1,
                                })
                                .build();

                            let memory_barriers = [];
                            let buffer_memory_barriers = [];
                            let image_memory_barriers = [image_memory_barrier];
                            device.cmd_pipeline_barrier(
                                cmd_buf,
                                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                                vk::PipelineStageFlags::FRAGMENT_SHADER,
                                vk::DependencyFlags::BY_REGION,
                                &memory_barriers,
                                &buffer_memory_barriers,
                                &image_memory_barriers,
                            );
                        }

                        debug::begin_cmd_buf_label(
                            debug_utils,
                            cmd_buf,
                            "Node selection border",
                        );

                        selection_edge
                            .draw(
                                &device,
                                cmd_buf,
                                edge_pass,
                                framebuffers,
                                [size.width as f32, size.height as f32],
                            )
                            .unwrap();

                        unsafe {
                            let image_memory_barrier = vk::ImageMemoryBarrier::builder()
                                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                                .image(offscreen_image)
                                .subresource_range(vk::ImageSubresourceRange {
                                    aspect_mask: vk::ImageAspectFlags::COLOR,
                                    base_mip_level: 0,
                                    level_count: 1,
                                    base_array_layer: 0,
                                    layer_count: 1,
                                })
                                .build();

                            let memory_barriers = [];
                            let buffer_memory_barriers = [];
                            let image_memory_barriers = [image_memory_barrier];
                            // let image_memory_barriers = [];
                            device.cmd_pipeline_barrier(
                                cmd_buf,
                                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                                vk::PipelineStageFlags::FRAGMENT_SHADER,
                                vk::DependencyFlags::BY_REGION,
                                &memory_barriers,
                                &buffer_memory_barriers,
                                &image_memory_barriers,
                            );
                        }

                        selection_blur
                            .draw(
                                &device,
                                cmd_buf,
                                blur_pass,
                                framebuffers,
                                [size.width as f32, size.height as f32],
                            )
                            .unwrap();

                        debug::end_cmd_buf_label(debug_utils, cmd_buf);

                        debug::begin_cmd_buf_label(
                            debug_utils,
                            cmd_buf,
                            "GUI",
                        );

                        gui.draw(
                            cmd_buf,
                            gui_pass,
                            framebuffers,
                            size.into(),
                            // [size.width as f32, size.height as f32],
                            &push_descriptor,
                            &gradients,
                        )
                        .unwrap();

                        debug::end_cmd_buf_label(debug_utils, cmd_buf);

                    };

                let size = window.inner_size();
                dirty_swapchain = gfaestus.draw_frame_from([size.width, size.height], draw).unwrap();

                if !dirty_swapchain {
                    let screen_dims = app.dims();

                    GfaestusVk::copy_image_to_buffer(
                        gfaestus.vk_context().device(),
                        gfaestus.transient_command_pool,
                        gfaestus.graphics_queue,
                        gfaestus.node_attachments.id_resolve.image,
                        main_view.node_id_buffer(),
                        vk::Extent2D {
                            width: screen_dims.width as u32,
                            height: screen_dims.height as u32,
                        },
                    ).unwrap();
                }

                let frame_time = frame_t.elapsed().as_secs_f32();
                frame_time_history[frame % frame_time_history.len()] = frame_time;

                if frame > FRAME_HISTORY_LEN && frame % FRAME_HISTORY_LEN == 0 {
                    let ft_sum: f32 = frame_time_history.iter().sum();
                    let avg = ft_sum / (FRAME_HISTORY_LEN as f32);
                    let fps = 1.0 / avg;
                    let avg_ms = avg * 1000.0;

                    gui.app_view_state().fps().send(FrameRateMsg(FrameRate {
                        fps,
                        frame_time: avg_ms,
                        frame,
                    }));
                }

                frame += 1;
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized { .. } => {
                    dirty_swapchain = true;
                }
                _ => (),
            },
            Event::LoopDestroyed => {
                gfaestus.wait_gpu_idle().unwrap();

                let device = gfaestus.vk_context().device();

                main_view.selection_buffer.destroy(device);
                main_view.node_id_buffer.destroy(device);
                main_view.node_draw_system.destroy(&gfaestus);

                gui.draw_system.destroy(&gfaestus.allocator);

                selection_edge.destroy(device);
                selection_blur.destroy(device);
            }
            _ => (),
        }
    });
}

fn handle_new_overlay(
    app: &GfaestusVk,
    main_view: &mut MainView,
    node_count: usize,
    msg: OverlayCreatorMsg,
) -> Result<()> {
    let OverlayCreatorMsg::NewOverlay { name, data } = msg;

    let overlay = match data {
        OverlayData::RGB(data) => {
            let mut overlay =
                NodeOverlay::new_empty_rgb(&name, app, node_count).unwrap();

            overlay
                .update_overlay(
                    app.vk_context().device(),
                    data.iter()
                        .enumerate()
                        .map(|(ix, col)| (NodeId::from((ix as u64) + 1), *col)),
                )
                .unwrap();

            Overlay::RGB(overlay)
        }
        OverlayData::Value(data) => {
            let mut overlay =
                NodeOverlayValue::new_empty_value(&name, &app, node_count)
                    .unwrap();

            overlay
                .update_overlay(
                    app.vk_context().device(),
                    data.iter()
                        .enumerate()
                        .map(|(ix, v)| (NodeId::from((ix as u64) + 1), *v)),
                )
                .unwrap();

            Overlay::Value(overlay)
        }
    };

    main_view
        .node_draw_system
        .overlay_pipelines
        .create_overlay(overlay);

    Ok(())
}

#[derive(FromArgs)]
/// Gfaestus
pub struct Args {
    /// the GFA file to load
    #[argh(positional)]
    gfa: String,

    /// the layout file to use
    #[argh(positional)]
    layout: String,

    /// load and run a script file at startup, e.g. for configuration
    #[argh(option)]
    run_script: Option<String>,

    /// force use of x11 window (debugging)
    #[argh(switch)]
    force_x11: bool,

    /// suppress log messages
    #[argh(switch, short = 'q')]
    quiet: bool,

    /// log debug messages
    #[argh(switch, short = 'd')]
    debug: bool,

    // #[argh(switch, short = 'l')]
    /// whether or not to log to a file in the working directory
    #[argh(switch)]
    log_to_file: bool,
}
