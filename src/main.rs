#[allow(unused_imports)]
use compute::EdgePreprocess;
use crossbeam::atomic::AtomicCell;
use gfaestus::context::{debug_context_action, pan_to_node_action, ContextMgr};
use gfaestus::quad_tree::QuadTree;
use gfaestus::reactor::{ModalError, ModalHandler, ModalSuccess, Reactor};
use gfaestus::script::plugins::colors::{hash_bytes, hash_color};
use gfaestus::vulkan::compute::path_view::{Path1DLayout, PathViewRenderer};
use gfaestus::vulkan::context::EdgeRendererType;
use gfaestus::vulkan::draw_system::edges::EdgeRenderer;
use gfaestus::vulkan::texture::{Gradients, Gradients_, Texture};

use parking_lot::RwLock;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;
use std::path::PathBuf;

use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::ControlFlow;

#[allow(unused_imports)]
use winit::window::{Window, WindowBuilder};

use gfaestus::app::{
    mainview::*, Args, OverlayCreatorMsg, OverlayState, Select,
};
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
    nodes::Overlay, post::PostProcessPipeline,
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

fn set_up_logger(args: &Args) -> Result<LoggerHandle> {
    let spec = match (args.trace, args.debug, args.quiet) {
        (true, _, _) => "trace",
        (_, true, _) => "debug",
        (_, _, true) => "",
        _ => "info",
    };

    let logger = Logger::try_with_env_or_str(spec)?
        .log_to_file(FileSpec::default().suppress_timestamp())
        .duplicate_to_stderr(Duplicate::Debug)
        .start()?;

    Ok(logger)
}

fn main() -> Result<()> {
    let args: Args = argh::from_env();

    let _logger = set_up_logger(&args)?;

    log::debug!("Logger initalized");

    let gfa_file = &args.gfa;
    let layout_file = &args.layout;
    log::debug!("using {} and {}", gfa_file, layout_file);

    let (mut gfaestus, event_loop, window) = match GfaestusVk::new(&args) {
        Ok(app) => app,
        Err(err) => {
            error!("Error initializing Gfaestus");
            error!("{:?}", err.root_cause());
            std::process::exit(1);
        }
    };

    let renderer_config = gfaestus.vk_context().renderer_config;

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

    log::debug!("futures thread pool: {}", futures_cpus);
    log::debug!("rayon   thread pool: {}", rayon_cpus);

    // TODO make sure to set thread pool size to less than number of CPUs
    let thread_pool =
        ThreadPoolBuilder::new().pool_size(futures_cpus).create()?;

    let rayon_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(rayon_cpus)
        .build()?;

    info!("Loading GFA");
    let t = std::time::Instant::now();

    let graph_query = Arc::new(GraphQuery::load_gfa(gfa_file)?);

    let layout_1d = Arc::new(Path1DLayout::new(graph_query.graph()));

    let graph_query_worker =
        GraphQueryWorker::new(graph_query.clone(), thread_pool.clone());

    let (mut universe, stats) =
        universe_from_gfa_layout(&graph_query, layout_file)?;

    let (top_left, bottom_right) = universe.layout().bounding_box();

    let tree_bounding_box = {
        let tl = top_left;
        let br = bottom_right;

        let p0 = tl - (br - tl) * 0.2;
        let p1 = br + (br - tl) * 0.2;

        Rect::new(p0, p1)
    };

    let mut app = App::new(
        (100.0, 100.0),
        thread_pool.clone(),
        rayon_pool,
        graph_query.clone(),
        tree_bounding_box,
    )
    .expect("error when creating App");

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
    )?;

    let gpu_selection = GpuSelection::new(&gfaestus, graph_query.node_count())?;

    let node_translation =
        NodeTranslation::new(&gfaestus, graph_query.node_count())?;

    let mut select_fence_id: Option<usize> = None;
    let mut translate_fence_id: Option<usize> = None;

    let mut prev_overlay: Option<usize> = None;
    let mut prev_gradient = app.shared_state().overlay_state().gradient();

    let (winit_tx, winit_rx) =
        crossbeam::channel::unbounded::<WindowEvent<'static>>();

    let mut input_manager = InputManager::new(winit_rx, app.shared_state());

    let app_rx = input_manager.clone_app_rx();
    let main_view_rx = input_manager.clone_main_view_rx();
    let gui_rx = input_manager.clone_gui_rx();

    let node_vertices = universe.node_vertices();

    let mut main_view = MainView::new(
        &gfaestus,
        app.clone_channels(),
        app.settings.clone(),
        app.shared_state().clone(),
        graph_query.node_count(),
    )
    .unwrap();

    let path_view = Arc::new(
        PathViewRenderer::new(
            &gfaestus,
            main_view
                .node_draw_system
                .pipelines
                .pipeline_rgb
                .descriptor_set_layout,
            main_view
                .node_draw_system
                .pipelines
                .pipeline_value
                .descriptor_set_layout,
            &graph_query,
        )
        .unwrap(),
    );

    let mut gui = Gui::new(&app, &gfaestus, &path_view)?;

    // create default overlays
    {
        let node_seq_script = "
fn node_color(id) {
  let h = handle(id, false);
  let seq = graph.sequence(h);
  let hash = hash_bytes(seq);
  let color = hash_color(hash);
  color
}
";

        let step_count_script = "
fn node_color(id) {
  let h = handle(id, false);

  let steps = graph.steps_on_handle(h);
  let count = 0.0;

  for step in steps {
    count += 1.0;
  }

  count
}
";

        create_overlay(
            app.shared_state().overlay_state(),
            &gfaestus,
            &mut main_view,
            &app.reactor,
            "Node Seq Hash",
            node_seq_script,
        )
        .expect("Error creating node seq hash overlay");

        create_overlay(
            app.shared_state().overlay_state(),
            &gfaestus,
            &mut main_view,
            &app.reactor,
            "Node Step Count",
            step_count_script,
        )
        .expect("Error creating step count overlay");
    }

    app.shared_state()
        .overlay_state
        .set_current_overlay(Some(0));

    let mut initial_view: Option<View> = None;
    let mut initialized_view = false;

    let new_overlay_rx = app.channels().new_overlay_rx.clone();

    let mut modal_handler =
        ModalHandler::new(app.shared_state().show_modal.to_owned());

    gui.app_view_state().graph_stats().send(GraphStatsMsg {
        node_count: Some(stats.node_count),
        edge_count: Some(stats.edge_count),
        path_count: Some(stats.path_count),
        total_len: Some(stats.total_len),
    });

    main_view
        .node_draw_system
        .vertices
        .upload_vertices(&gfaestus, &node_vertices)?;

    let mut edge_renderer = if gfaestus.vk_context().renderer_config.edges
        == EdgeRendererType::Disabled
    {
        log::warn!(
            "Device does not support tessellation shaders, disabling edges"
        );
        None
    } else {
        let edge_renderer = EdgeRenderer::new(
            &gfaestus,
            &graph_query.graph_arc(),
            universe.layout(),
        )?;

        Some(edge_renderer)
    };

    let mut dirty_swapchain = false;

    let mut selection_edge = SelectionOutlineEdgePipeline::new(&gfaestus, 1)?;

    let mut selection_blur = SelectionOutlineBlurPipeline::new(&gfaestus, 1)?;

    let gui_msg_tx = app.channels().gui_tx.clone();

    dbg!();
    let gradients_ = Gradients_::initialize(
        &gfaestus,
        gfaestus.transient_command_pool,
        gfaestus.graphics_queue,
        1024,
    )?;

    dbg!();
    gui.draw_system.add_texture(&gfaestus, gradients_.texture)?;

    dbg!();

    let mut upload_path_view_texture = true;

    let gradients = Gradients::initialize(
        &gfaestus,
        gfaestus.transient_command_pool,
        gfaestus.graphics_queue,
        1024,
    )?;

    gui.populate_overlay_list(
        main_view
            .node_draw_system
            .pipelines
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

    gui_msg_tx.send(GuiMsg::SetLightMode)?;

    let mut context_mgr = ContextMgr::default();

    {
        macro_rules! set_type_name {
            ($type:ty) => {
                context_mgr.set_type_name::<$type>(stringify!($type));
            };
        }

        set_type_name!(NodeId);
        set_type_name!(PathId);
        set_type_name!(FxHashSet<NodeId>);
    }

    let dbg_action = debug_context_action(&context_mgr);

    context_mgr.register_action("Debug print", dbg_action);

    if let Err(e) = context_mgr
        .load_rhai_modules("./scripts/context_actions/".into(), &gui.console)
    {
        log::error!("Error loading context actions: {:?}", e);
    }

    if let Some(script_file) = args.run_script.as_ref() {
        if script_file == "-" {
            use bstr::ByteSlice;
            use std::io::prelude::*;

            let mut stdin = std::io::stdin();
            let mut script_bytes = Vec::new();
            let read = stdin.read_to_end(&mut script_bytes)?;

            if let Ok(script) = script_bytes[0..read].to_str() {
                // warn!("executing script {}", script_file);

                if let Err(e) = gui.console.eval(&mut app.reactor, true, script)
                {
                    log::error!("Error executing stdin script:\n{:?}", e);
                }
            }
        } else {
            warn!("executing script file {}", script_file);
            if let Err(e) =
                gui.console.eval_file(&mut app.reactor, true, script_file)
            {
                log::error!("Error executing script {}:\n{:?}", script_file, e);
            }
        }
    }

    {
        for annot_path in &args.annotation_files {
            if annot_path.exists() {
                if let Some(path_str) = annot_path.to_str() {
                    let script = format!("load_collection(\"{}\");", path_str);
                    log::warn!("executing script: {}", script);

                    if let Err(e) =
                        gui.console.eval(&mut app.reactor, true, &script)
                    {
                        log::error!(
                            "Error loading annotation file {}:\n{:?}",
                            path_str,
                            e
                        );
                    }
                }
            }
        }
    }

    let timer = std::time::Instant::now();

    event_loop.run(move |event, _, control_flow| {

        *control_flow = ControlFlow::Poll;

        // NB: AFAIK the only event that isn't 'static is the window
        // scale change (for high DPI displays), as it returns a
        // reference
        // so until the corresponding support is added, those events
        // are simply ignored here
        let event = if let Some(ev) = event.to_static() {
            ev
        } else {
            return;
        };

        if let Event::WindowEvent { event, .. } = &event {
            if let WindowEvent::MouseInput { state, button, .. } = event {
                if *state == ElementState::Pressed &&
                    *button == MouseButton::Right {
                        context_mgr.open_context_menu(&gui.ctx);
                        context_mgr.set_position(app.shared_state().mouse_pos());
                }
            }
        }

        while let Ok(callback) = app.channels().modal_rx.try_recv() {
            let _ = modal_handler.set_prepared_active(callback);
        }

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
                input_manager.handle_events(&mut app.reactor, &gui_msg_tx);

                let mouse_pos = app.mouse_pos();

                gui.push_event(egui::Event::PointerMoved(mouse_pos.into()));

                let hover_node = main_view
                    .read_node_id_at(mouse_pos)
                    .map(|nid| NodeId::from(nid as u64));

                app.shared_state().hover_node.store(hover_node);

                if app.selection_changed() {
                    if let Some(selected) = app.selected_nodes() {

                        log::warn!("sending selection");
                        /*
                        context_menu
                            .tx()
                            .send(ContextEntry::Selection { nodes: selected.to_owned() })

                            .unwrap();
                        */

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
                        &gui.console.input_tx(),
                        universe.layout().nodes(),
                        app_msg,
                    );
                }

                gui.apply_received_gui_msgs(&mut app.reactor);

                while let Ok(main_view_msg) = main_view.main_view_msg_rx().try_recv() {
                    main_view.apply_msg(main_view_msg);
                }

                while let Ok(new_overlay) = new_overlay_rx.try_recv() {
                    if let Ok(_) = handle_new_overlay(
                        app.shared_state().overlay_state(),
                        &gfaestus,
                        &mut main_view,
                        graph_query.node_count(),
                        new_overlay
                    ) {
                        gui.populate_overlay_list(
                            main_view
                                .node_draw_system
                                .pipelines
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

                for er in edge_renderer.iter_mut() {
                    er.write_ubo(&edge_ubo).unwrap();
                }

                let focus = &app.shared_state().gui_focus_state;
                if !focus.mouse_over_gui() {
                    main_view.produce_context(&context_mgr);
                    // main_view.send_context(context_menu.tx());
                }
            }
            Event::RedrawEventsCleared => {


                if path_view.should_reload() {
                    path_view.load_paths_1d(&mut app.reactor, &layout_1d).unwrap();
                    // path_view.load_paths(&mut app.reactor).unwrap();
                }

                app.reactor
                    .gpu_tasks
                    .execute_all(&gfaestus,
                                 gfaestus.transient_command_pool,
                                 gfaestus.graphics_queue).unwrap();


                // TODO this timer is just to make sure everything has
                // been initialized; it should probably be replaced by
                // checking the frame count
                if timer.elapsed().as_millis() > 400 {
                    let cur_overlay = app.shared_state().overlay_state().current_overlay();
                    let cur_gradient = app.shared_state().overlay_state().gradient();

                    if path_view.fence_id().is_none()
                        && (cur_overlay != prev_overlay ||
                            path_view.should_rerender() ||
                            // rerender_path_view ||
                            cur_gradient != prev_gradient)
                    {
                        // log::warn!("doing the paths");

                        prev_overlay = cur_overlay;
                        prev_gradient = cur_gradient;

                        let overlay =
                            app.shared_state().overlay_state().current_overlay().unwrap();

                        let rgb_overlay_desc = main_view
                            .node_draw_system
                            .pipelines
                            .pipeline_rgb
                            .overlay_set;

                        let val_overlay_desc = main_view
                            .node_draw_system
                            .pipelines
                            .pipeline_value
                            .overlay_set;

                        let overlay_kind = main_view
                            .node_draw_system
                            .pipelines
                            .overlay_kind(overlay).unwrap();


                        path_view
                            .dispatch_managed(&mut compute_manager,
                                              &gfaestus,
                                              rgb_overlay_desc,
                                              val_overlay_desc,
                                              overlay_kind,
                            ).unwrap();

                    }
                }


                log::trace!("Event::RedrawEventsCleared");
                let edge_ubo = app.settings.edge_renderer().load();
                let edge_width = edge_ubo.edge_width;

                if let Some(fid) = translate_fence_id {
                    if compute_manager.is_fence_ready(fid).unwrap() {
                        log::trace!("Node translation fence ready");
                        compute_manager.block_on_fence(fid).unwrap();
                        compute_manager.free_fence(fid, false).unwrap();

                        log::trace!("Compute fence freed, updating CPU node positions");
                        universe.update_positions_from_gpu(&gfaestus,
                                                           &main_view.node_draw_system.vertices).unwrap();

                        translate_fence_id = None;
                    }
                }



                if let Some(fid) = path_view.fence_id() {
                    if compute_manager.is_fence_ready(fid).unwrap() {
                        log::trace!("Path view fence ready");
                        path_view.block_on_fence(&mut compute_manager);

                        // app.shared_state().tmp.store(true);

                        if upload_path_view_texture {
                            upload_path_view_texture = false;

                            let tex_id = gui
                                .draw_system
                                .add_texture(&gfaestus,
                                             path_view.output_image
                                ).unwrap();


                            log::warn!("uploaded path view texture: {:?}", tex_id);
                        }
                    }
                }

                if let Some(fid) = select_fence_id {

                    if compute_manager.is_fence_ready(fid).unwrap() {
                        log::trace!("Node selection fence ready");
                        compute_manager.block_on_fence(fid).unwrap();
                        compute_manager.free_fence(fid, false).unwrap();

                        GfaestusVk::copy_buffer(gfaestus.vk_context().device(),
                                                gfaestus.transient_command_pool,
                                                gfaestus.graphics_queue,
                                                gpu_selection.selection_buffer.buffer,
                                                main_view.selection_buffer.buffer,
                                                main_view.selection_buffer.size);
                        log::trace!("Copied selection buffer to main view");


                        let t = std::time::Instant::now();
                        main_view
                            .selection_buffer
                            .fill_selection_set(gfaestus
                                                .vk_context()
                                                .device())
                            .unwrap();
                        log::trace!("Updated CPU selection buffer");
                        trace!("fill_selection_set took {} ns", t.elapsed().as_nanos());

                        app.channels().app_tx
                            .send(AppMsg::Selection(Select::Many {
                            nodes: main_view
                                .selection_buffer
                                .selection_set()
                                .clone(),
                            clear: true }))
                            .unwrap();


                        select_fence_id = None;
                    }
                }

                let frame_t = std::time::Instant::now();

                if dirty_swapchain {
                    let size = window.inner_size();
                    log::trace!("Dirty swapchain, reconstructing");
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
                        log::debug!("Can't recreate swapchain with a zero resolution");
                        return;
                    }
                }

                let _ = gui.console.eval_next(&mut app.reactor, true);


                gui.begin_frame(
                    &app,
                    &context_mgr,
                    universe.layout().nodes(),
                );


                modal_handler.show(&gui.ctx);


                // {
                //     let ctx = &gui.ctx;
                //     let clipboard = &mut gui.clipboard_ctx;

                    // if open_context.load() {
                    //     context_menu.recv_contexts();
                    //     context_menu.open_context_menu(&gui.ctx);
                    //     open_context.store(false);
                    // }

                    // context_menu.show(ctx, &app.reactor, clipboard);
                // }

                {
                    let shared_state = app.shared_state();
                    let view = shared_state.view();
                    let labels = app.labels();
                    let cluster_tree = labels.cluster(tree_bounding_box,
                                                      app.settings.label_radius().load(),
                                                      view);
                    cluster_tree.draw_labels(labels, &gui.ctx, shared_state);
                }

                // context_mgr.end_frame();


                context_mgr.begin_frame();
                context_mgr.show(&gui.ctx, &app);

                let meshes = gui.end_frame(&mut app.reactor);

                gui.upload_egui_texture(&gfaestus).unwrap();

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

                let overlay =
                    app.shared_state().overlay_state().current_overlay();

                let current_view = app.shared_state().view();

                let edges_enabled = app.shared_state().edges_enabled();

                // TODO this should also check tess. isoline support etc. i think
                let edges_enabled = edges_enabled &&
                    !matches!(renderer_config.edges, EdgeRendererType::Disabled);

                let debug_utils = gfaestus.vk_context().debug_utils().map(|u| u.to_owned());

                let debug_utils = debug_utils.as_ref();

                let swapchain_dims = gfaestus.swapchain_dims();

                let draw =
                    |device: &Device, cmd_buf: vk::CommandBuffer, framebuffers: &Framebuffers| {
                        log::trace!("In draw_frame_from callback");
                        let size = swapchain_dims;

                        debug::begin_cmd_buf_label(
                            debug_utils,
                            cmd_buf,
                            "Image transitions"
                        );

                        log::trace!("Pre-rendering image transitions");
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

                        log::trace!("Drawing nodes");
                        main_view.draw_nodes(
                            cmd_buf,
                            node_pass,
                            framebuffers,
                            size.into(),
                            Point::ZERO,
                            overlay,
                            gradient,
                        ).unwrap();


                        debug::end_cmd_buf_label(debug_utils, cmd_buf);

                        if edges_enabled {

                            log::trace!("Drawing edges");
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

                            for er in edge_renderer.iter_mut() {
                                er.draw(
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
                            }

                            debug::end_cmd_buf_label(debug_utils, cmd_buf);
                        }


                        log::trace!("Post-edge image transitions");
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

                        log::trace!("Drawing selection border edge detection");
                        selection_edge
                            .draw(
                                &device,
                                cmd_buf,
                                edge_pass,
                                framebuffers,
                                [size.width as f32, size.height as f32],
                            )
                            .unwrap();

                        log::trace!("Selection border edge detection -- image transitions");
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

                        log::trace!("Drawing selection border blur");
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

                        log::trace!("Drawing GUI");
                        gui.draw(
                            cmd_buf,
                            gui_pass,
                            framebuffers,
                            size.into(),
                        )
                        .unwrap();

                        debug::end_cmd_buf_label(debug_utils, cmd_buf);

                        log::trace!("End of draw_frame_from callback");
                    };

                let size = window.inner_size();
                dirty_swapchain = gfaestus.draw_frame_from([size.width, size.height], draw).unwrap();

                if !dirty_swapchain {
                    let screen_dims = app.dims();

                    log::trace!("Copying node ID image to buffer");
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

                log::trace!("Calculating FPS");
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
                    log::trace!("WindowEvent::CloseRequested");
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized { .. } => {
                    dirty_swapchain = true;
                }
                _ => (),
            },
            Event::LoopDestroyed => {
                log::trace!("Event::LoopDestroyed");

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

    Ok(())
}

fn handle_new_overlay(
    overlay_state: &OverlayState,
    app: &GfaestusVk,
    main_view: &mut MainView,
    node_count: usize,
    msg: OverlayCreatorMsg,
) -> Result<()> {
    let OverlayCreatorMsg::NewOverlay { name, data } = msg;

    let overlay = match data {
        OverlayData::RGB(data) => {
            let mut overlay =
                Overlay::new_empty_rgb(&name, app, node_count).unwrap();

            overlay
                .update_rgb_overlay(
                    data.iter()
                        .enumerate()
                        .map(|(ix, col)| (NodeId::from((ix as u64) + 1), *col)),
                )
                .unwrap();

            overlay
        }
        OverlayData::Value(data) => {
            let mut overlay =
                Overlay::new_empty_value(&name, &app, node_count).unwrap();

            overlay
                .update_value_overlay(
                    data.iter()
                        .enumerate()
                        .map(|(ix, v)| (NodeId::from((ix as u64) + 1), *v)),
                )
                .unwrap();

            overlay
        }
    };

    let id = main_view.node_draw_system.pipelines.create_overlay(overlay);
    overlay_state.current_overlay.store(Some(id));

    Ok(())
}

fn create_overlay(
    overlay_state: &OverlayState,
    app: &GfaestusVk,
    main_view: &mut MainView,
    reactor: &Reactor,
    name: &str,
    script: &str,
) -> Result<()> {
    let node_count = reactor.graph_query.graph.node_count();

    let script_config = gfaestus::script::ScriptConfig {
        default_color: rgb::RGBA::new(0.3, 0.3, 0.3, 0.3),
        target: gfaestus::script::ScriptTarget::Nodes,
    };

    if let Ok(data) = gfaestus::script::overlay_colors_tgt(
        &reactor.rayon_pool,
        &script_config,
        &reactor.graph_query,
        script,
    ) {
        let msg = OverlayCreatorMsg::NewOverlay {
            name: name.to_string(),
            data,
        };
        handle_new_overlay(overlay_state, app, main_view, node_count, msg)?;
    }

    Ok(())
}

fn draw_tree<T>(ctx: &egui::CtxRef, tree: &QuadTree<T>, app: &App)
where
    T: Clone + ToString,
{
    let view = app.shared_state().view();
    let s = app.shared_state().mouse_pos();
    let dims = app.dims();
    let w = view.screen_point_to_world(dims, s);

    for leaf in tree.leaves() {
        gfaestus::gui::text::draw_rect_world(ctx, view, leaf.boundary(), None);

        let points = leaf.points();
        let data = leaf.data();
        for (point, val) in points.into_iter().zip(data.into_iter()) {
            gfaestus::gui::text::draw_text_at_world_point(
                ctx,
                view,
                *point,
                &val.to_string(),
            );
        }
    }

    if let Some(closest) = tree.nearest_leaf(w) {
        let rect = closest.boundary();
        let color = rgb::RGBA::new(0.8, 0.1, 0.1, 1.0);
        gfaestus::gui::text::draw_rect_world(ctx, view, rect, Some(color));
    }
}
