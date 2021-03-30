#[allow(unused_imports)]
use vulkano::device::{Device, DeviceExtensions, RawDeviceExtensions};
#[allow(unused_imports)]
use vulkano::framebuffer::{
    Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass,
};
#[allow(unused_imports)]
use vulkano::instance::debug::{DebugCallback, MessageSeverity, MessageType};
use vulkano::{
    format::Format,
    image::{
        AttachmentImage, ImageAccess, ImageUsage, ImageViewAccess,
        SwapchainImage,
    },
};

use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::swapchain::{
    self, AcquireError, ColorSpace, FullscreenExclusive, PresentMode,
    SurfaceTransform, Swapchain, SwapchainCreationError,
};
use vulkano::sync::{self, FlushError, GpuFuture};
use vulkano::{
    command_buffer::{AutoCommandBufferBuilder, DynamicState, SubpassContents},
    pipeline::viewport::Viewport,
};

use vulkano_win::VkSurfaceBuild;

use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use gfaestus::app::mainview::*;
use gfaestus::app::{gui::*, AppConfigState};
use gfaestus::app::{App, AppConfigMsg, AppMsg};
use gfaestus::geometry::*;
use gfaestus::graph_query::*;
use gfaestus::input::*;
use gfaestus::render::nodes::OverlayCache;
use gfaestus::render::*;
use gfaestus::universe::*;
use gfaestus::util::*;
use gfaestus::view::View;

use rgb::*;

use anyhow::Result;

use ash::{
    extensions::{ext::DebugReport, khr::Surface},
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
};
use ash::{vk, Entry};

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

/*
fn construct_overlay<F: FnMut(&PackedGraph, Handle) -> RGB<f32>>(
    main_view: &MainView,
    graph_query: &GraphQuery,
    f: F,
) -> Result<(OverlayCache, Box<dyn GpuFuture>)> {
    let colors = graph_query.build_overlay_colors(f);

    let (overlay, future) =
        main_view.build_overlay_cache(colors.into_iter())?;

    Ok((overlay, future))
}
*/

use gfaestus::vulkan::*;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();

    let gfa_file = if let Some(name) = args.get(1) {
        name
    } else {
        eprintln!("must provide path to a GFA file");
        std::process::exit(1);
    };

    let layout_file = if let Some(name) = args.get(2) {
        name
    } else {
        eprintln!("must provide path to a layout file");
        std::process::exit(1);
    };

    eprintln!("loading GFA");
    let t = std::time::Instant::now();
    let init_t = std::time::Instant::now();

    let graph_query = GraphQuery::load_gfa(gfa_file).unwrap();

    let (universe, stats) =
        universe_from_gfa_layout(&graph_query, layout_file).unwrap();

    let (top_left, bottom_right) = universe.layout().bounding_box();

    eprintln!(
        "layout bounding box\t({:.2}, {:.2})\t({:.2}, {:.2})",
        top_left.x, top_left.y, bottom_right.x, bottom_right.y
    );
    eprintln!(
        "layout width: {:.2}\theight: {:.2}",
        bottom_right.x - top_left.x,
        bottom_right.y - top_left.y
    );

    // let init_layout = layout.clone();

    eprintln!("GFA loaded in {:.3} sec", t.elapsed().as_secs_f64());

    eprintln!(
        "Loaded {} nodes\t{} points",
        universe.layout().nodes().len(),
        universe.layout().nodes().len() * 2
    );

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Gfaestus")
        .with_inner_size(winit::dpi::PhysicalSize::new(800, 600))
        .build(&event_loop)
        .unwrap();

    let gfaestus = GfaestusVk::new(&window);

    if let Err(err) = &gfaestus {
        println!("{:?}", err.root_cause());
    }

    let mut gfaestus = gfaestus.unwrap();

    let (winit_tx, winit_rx) =
        crossbeam::channel::unbounded::<WindowEvent<'static>>();

    let input_manager = InputManager::new(winit_rx);

    let app_rx = input_manager.clone_app_rx();
    let main_view_rx = input_manager.clone_main_view_rx();
    let gui_rx = input_manager.clone_gui_rx();

    let mut app = App::new(input_manager.clone_mouse_pos(), (100.0, 100.0))
        .expect("error when creating App");

    let node_vertices = universe.new_vertices();

    let mut main_view = MainView::new(
        &gfaestus,
        gfaestus.swapchain_props,
        gfaestus.msaa_samples,
        gfaestus.render_pass,
    )
    .unwrap();

    let (mut gui, opts_from_gui) = GfaestusGui::new(
        &gfaestus,
        gfaestus.swapchain_props,
        gfaestus.msaa_samples,
        gfaestus.render_pass_dc,
    )
    .unwrap();

    gui.set_graph_stats(stats);

    main_view
        .node_draw_system
        .vertices
        .upload_vertices(&gfaestus, &node_vertices)
        .unwrap();

    // node_sys.upload_vertices(&gfaestus, &node_vertices).unwrap();

    let (app_msg_tx, app_msg_rx) = crossbeam::channel::unbounded::<AppMsg>();
    let (cfg_msg_tx, cfg_msg_rx) =
        crossbeam::channel::unbounded::<AppConfigMsg>();

    let (opts_to_gui, opts_from_app) =
        crossbeam::channel::unbounded::<AppConfigState>();

    // for (id, def) in app.all_theme_defs() {
    //     gui.update_theme_editor(id, def);
    // }

    let mut dirty_swapchain = false;

    // let mut command_buffer = gfaestus::vulkan::draw_system::GfaestusCmdBuf

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

        // hacky -- this should take place after mouse pos is updated
        // in egui but before input is sent to mainview
        // input_manager.set_mouse_over_gui(gui.pointer_over_gui());
        input_manager.handle_events();

        let screen_dims = app.dims();
        let mouse_pos = app.mouse_pos();

        // gui.push_event(egui::Event::PointerMoved(mouse_pos.into()));
        main_view.set_mouse_pos(Some(mouse_pos));
        main_view.set_screen_dims(screen_dims);

        // let hover_node = main_view
        //     .read_node_id_at(screen_dims, mouse_pos)
        //     .map(|nid| NodeId::from(nid as u64));

        // app_msg_tx.send(AppMsg::HoverNode(hover_node)).unwrap();

        while let Ok(app_in) = app_rx.try_recv() {
            app.apply_input(app_in);
        }

        while let Ok(gui_in) = gui_rx.try_recv() {
            gui.apply_input(&app_msg_tx, &cfg_msg_tx, gui_in);
        }

        // while let Ok(opt_in) = opts_from_gui.try_recv() {
        //     app.apply_app_config_state(opt_in);
        // }

        while let Ok(main_view_in) = main_view_rx.try_recv() {
            main_view.apply_input(screen_dims, &app_msg_tx, main_view_in);
        }

        while let Ok(app_msg) = app_msg_rx.try_recv() {
            app.apply_app_msg(&app_msg);
        }

        while let Ok(cfg_msg) = cfg_msg_rx.try_recv() {
            app.apply_app_config_msg(&cfg_msg);
        }

        match event {
            Event::NewEvents(_) => {
                // TODO
            }
            // Event::MainEventsCleared => {
            // }
            Event::RedrawEventsCleared => {
                if dirty_swapchain {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        app.update_dims([
                            size.width as f32,
                            size.height as f32,
                        ]);
                        gfaestus
                            .recreate_swapchain(Some([size.width, size.height]))
                            .unwrap();
                    } else {
                        return;
                    }
                }

                gui.begin_frame(Some(app.dims().into()));

                let meshes = gui.end_frame();

                // let command_buffer = gfaestus::vulkan::draw_system::GfaestusCmdBuf::frame(gfaestus.vk_context().device(), pool, render_pass, framebuffer, swapchain_props)

                let render_pass = gfaestus.render_pass;
                let render_pass_dc = gfaestus.render_pass_dc;
                let extent = gfaestus.swapchain_props.extent;

                gui.upload_texture(&gfaestus).unwrap();

                if !meshes.is_empty() {
                    gui.upload_vertices(&gfaestus, &meshes).unwrap();
                }

                let draw =
                    |cmd_buf: vk::CommandBuffer,
                     framebuffer: vk::Framebuffer,
                     framebuffer_dc: vk::Framebuffer| {
                        let size = window.inner_size();

                        main_view
                            .draw_nodes(
                                cmd_buf,
                                render_pass,
                                framebuffer,
                                framebuffer_dc,
                                [size.width as f32, size.height as f32],
                                Point::ZERO,
                            )
                            .unwrap();
                    };

                let draw_2 =
                    |cmd_buf: vk::CommandBuffer,
                     framebuffer: vk::Framebuffer,
                     framebuffer_dc: vk::Framebuffer| {
                        let size = window.inner_size();

                        gui.draw(
                            cmd_buf,
                            render_pass_dc,
                            framebuffer,
                            framebuffer_dc,
                            [size.width as f32, size.height as f32],
                        )
                        .unwrap();
                    };

                dirty_swapchain =
                    gfaestus.draw_frame_from(draw, draw_2).unwrap();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized { .. } => {
                    dirty_swapchain = true;
                }
                WindowEvent::MouseInput { button, state, .. } => {
                    // TODO
                }
                WindowEvent::CursorMoved { position, .. } => {
                    // TODO
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    // TODO
                }
                _ => (),
            },
            Event::LoopDestroyed => {
                gfaestus.wait_gpu_idle().unwrap();
                main_view.node_draw_system.destroy();
            }
            _ => (),
        }
    });
}

/*
fn main_old() {
    let args = std::env::args().collect::<Vec<_>>();

    let gfa_file = if let Some(name) = args.get(1) {
        name
    } else {
        eprintln!("must provide path to a GFA file");
        std::process::exit(1);
    };

    let layout_file = if let Some(name) = args.get(2) {
        name
    } else {
        eprintln!("must provide path to a layout file");
        std::process::exit(1);
    };

    eprintln!("loading GFA");
    let t = std::time::Instant::now();
    let init_t = std::time::Instant::now();

    let graph_query = GraphQuery::load_gfa(gfa_file).unwrap();

    let (universe, stats) =
        universe_from_gfa_layout(&graph_query, layout_file).unwrap();

    let (top_left, bottom_right) = universe.layout().bounding_box();

    eprintln!(
        "layout bounding box\t({:.2}, {:.2})\t({:.2}, {:.2})",
        top_left.x, top_left.y, bottom_right.x, bottom_right.y
    );
    eprintln!(
        "layout width: {:.2}\theight: {:.2}",
        bottom_right.x - top_left.x,
        bottom_right.y - top_left.y
    );

    // let init_layout = layout.clone();

    eprintln!("GFA loaded in {:.3} sec", t.elapsed().as_secs_f64());

    eprintln!(
        "Loaded {} nodes\t{} points",
        universe.layout().nodes().len(),
        universe.layout().nodes().len() * 2
    );

    let extensions = vulkano_win::required_extensions();

    let instance = Instance::new(None, &extensions, None).unwrap();
    let physical = PhysicalDevice::enumerate(&instance).next().unwrap();

    let event_loop = EventLoop::new();
    let surface = WindowBuilder::new()
        .build_vk_surface(&event_loop, instance.clone())
        .unwrap();

    let queue_family = physical
        .queue_families()
        .find(|&q| {
            q.supports_graphics() && surface.is_supported(q).unwrap_or(false)
        })
        .unwrap();

    let device_ext = DeviceExtensions {
        khr_swapchain: true,
        khr_storage_buffer_storage_class: true,
        ..DeviceExtensions::none()
    };

    let features = physical.supported_features().clone();

    let (device, mut queues) = Device::new(
        physical,
        &features,
        &device_ext,
        [(queue_family, 0.5)].iter().cloned(),
    )
    .unwrap();

    let queue = queues.next().unwrap();

    let (mut swapchain, mut images) = {
        let caps = surface.capabilities(physical).unwrap();
        let alpha = caps.supported_composite_alpha.iter().next().unwrap();
        let format = caps.supported_formats[0].0;
        let dimensions: [u32; 2] = surface.window().inner_size().into();

        let mut img_usage = ImageUsage::color_attachment();
        img_usage.transfer_destination = true;

        Swapchain::new(
            device.clone(),
            surface.clone(),
            caps.min_image_count,
            format,
            dimensions,
            1,
            img_usage,
            &queue,
            SurfaceTransform::Identity,
            alpha,
            PresentMode::Fifo,
            FullscreenExclusive::Default,
            true,
            ColorSpace::SrgbNonLinear,
        )
        .unwrap()
    };

    let mut render_pipeline =
        RenderPipeline::new(queue.clone(), None, swapchain.format(), 800, 600)
            .unwrap();

    let post_draw_system = PostDrawSystem::new(
        queue.clone(),
        Subpass::from(render_pipeline.post_processing_pass().clone(), 0)
            .unwrap(),
        Subpass::from(render_pipeline.post_processing_pass().clone(), 0)
            .unwrap(),
    );

    let post_draw_system_final = PostDrawSystem::new(
        queue.clone(),
        Subpass::from(render_pipeline.final_pass().clone(), 0).unwrap(),
        Subpass::from(render_pipeline.final_pass().clone(), 0).unwrap(),
    );

    let (winit_tx, winit_rx) =
        crossbeam::channel::unbounded::<WindowEvent<'static>>();

    let input_manager = InputManager::new(winit_rx);

    let app_rx = input_manager.clone_app_rx();
    let main_view_rx = input_manager.clone_main_view_rx();
    let gui_rx = input_manager.clone_gui_rx();

    let mut app = App::new(
        queue.clone(),
        input_manager.clone_mouse_pos(),
        (100.0, 100.0),
    )
    .expect("error when creating App");

    let (mut main_view, mv_future) = MainView::new(
        queue.clone(),
        Subpass::from(render_pipeline.nodes_pass().clone(), 0).unwrap(),
        Subpass::from(render_pipeline.final_pass().clone(), 0).unwrap(),
    )
    .unwrap();

    let (mut gui, opts_from_gui) = GfaestusGui::new(
        queue.clone(),
        Subpass::from(render_pipeline.final_pass().clone(), 0).unwrap(),
    )
    .unwrap();

    gui.set_graph_stats(stats);

    let mut vec_vertices: Vec<Vertex> = Vec::new();
    universe.layout().node_line_vertices(&mut vec_vertices);

    main_view.set_vertices(vec_vertices.iter().copied());

    let mut dynamic_state = DynamicState {
        line_width: None,
        viewports: None,
        scissors: None,
        compare_mask: None,
        write_mask: None,
        reference: None,
    };

    let layout_dims = bottom_right - top_left;
    main_view.set_view_center(top_left + (layout_dims / 2.0));
    main_view
        .set_initial_view(Some(top_left + (layout_dims / 2.0)), Some(60.0));

    let (_line_buf_ix, line_future) = {
        let mut lines: Vec<(Point, Point)> = Vec::new();

        let cell_size = 8192.0;

        let cols = (layout_dims.x / cell_size).ceil() as usize;
        let rows = (layout_dims.y / cell_size).ceil() as usize;

        println!("grid dimensions: {} rows\t{} columns", rows, cols);

        let grid_w = (cols * 8192) as f32;
        let grid_h = (rows * 8192) as f32;

        let tl = top_left;

        for row in 0..rows {
            let y = tl.y + (row as f32) * cell_size;
            let x0 = tl.x;
            let x1 = tl.x + grid_w;
            lines.push((Point { x: x0, y }, Point { x: x1, y }));
        }

        for col in 0..cols {
            let x = tl.x + (col as f32) * cell_size;
            let y0 = tl.y;
            let y1 = tl.y + grid_h;
            lines.push((Point { x, y: y0 }, Point { x, y: y1 }));
        }

        main_view
            .add_lines(&lines, RGB::new(1.0, 1.0, 1.0))
            .unwrap()
    };

    update_viewport(&images[0], &mut dynamic_state);

    if let Some(viewport) =
        dynamic_state.viewports.as_ref().and_then(|v| v.get(0))
    {
        let width = viewport.dimensions[0];
        let height = viewport.dimensions[1];

        app.update_dims((width, height));
    }

    let mut offscreen_image = {
        let dims = app.dims();
        println!(
            "creating offscreen image with dimensions {}, {}",
            dims.width as u32, dims.height as u32
        );

        OffscreenImage::new(
            queue.clone(),
            dims.width as u32,
            dims.height as u32,
        )
        .unwrap()
    };

    let mut recreate_swapchain = false;

    let mut previous_frame_end = {
        let fut = sync::now(device.clone()).join(line_future).join(mv_future);
        Some(fut.boxed())
    };

    main_view
        .prepare_themes(
            app.themes().sampler(),
            app.themes().primary(),
            app.themes().secondary(),
        )
        .unwrap();

    const FRAME_HISTORY_LEN: usize = 10;
    let mut frame_time_history = [0.0f32; FRAME_HISTORY_LEN];
    let mut frame = 0;

    println!("MainView.view: {:?}", main_view.view());
    println!("initialized in {}", init_t.elapsed().as_secs_f32());

    let (app_msg_tx, app_msg_rx) = crossbeam::channel::unbounded::<AppMsg>();
    let (cfg_msg_tx, cfg_msg_rx) =
        crossbeam::channel::unbounded::<AppConfigMsg>();

    let (opts_to_gui, opts_from_app) =
        crossbeam::channel::unbounded::<AppConfigState>();

    for (id, def) in app.all_theme_defs() {
        gui.update_theme_editor(id, def);
    }

    let mut cached_overlay: Option<OverlayCache> = None;
    let mut overlay_future: Option<Box<dyn GpuFuture>> = None;

    {
        let graph = graph_query.graph();

        let mut min_nonzero_coverage = std::usize::MAX;
        let mut max_coverage = 0;

        for handle in graph.handles() {
            let coverage = graph
                .steps_on_handle(handle)
                .map(|s| s.count())
                .unwrap_or(0usize);

            min_nonzero_coverage = min_nonzero_coverage.min(coverage);
            max_coverage = max_coverage.max(coverage);
        }

        let (overlay, future) =
            construct_overlay(&main_view, &graph_query, |graph, handle| {
                let coverage = graph
                    .steps_on_handle(handle)
                    .map(|s| s.count())
                    .unwrap_or(0usize);

                if coverage >= min_nonzero_coverage {
                    let norm = (coverage as f32) / (max_coverage as f32);
                    let norm = 0.8 * norm;
                    RGB::new(0.2 + norm, 0.1, 0.1)
                } else {
                    RGB::new(0.05, 0.05, 0.05)
                }
            })
            .unwrap();

        cached_overlay = Some(overlay);
        overlay_future = Some(future);
    }

    event_loop.run(move |event, _, control_flow| {
        // TODO handle scale factor change before calling to_static() on event

        let event = if let Some(ev) = event.to_static() {
            ev
        } else {
            return;
        };

        if let Event::WindowEvent { event, .. } = &event {
            let ev = event.clone();
            winit_tx.send(ev).unwrap();
        }

        // hacky -- this should take place after mouse pos is updated
        // in egui but before input is sent to mainview
        input_manager.set_mouse_over_gui(gui.pointer_over_gui());
        input_manager.handle_events();

        let screen_dims = app.dims();
        let mouse_pos = app.mouse_pos();

        gui.push_event(egui::Event::PointerMoved(mouse_pos.into()));
        main_view.set_mouse_pos(Some(mouse_pos));

        let hover_node = main_view
            .read_node_id_at(screen_dims, mouse_pos)
            .map(|nid| NodeId::from(nid as u64));

        app_msg_tx.send(AppMsg::HoverNode(hover_node)).unwrap();

        while let Ok(app_in) = app_rx.try_recv() {
            app.apply_input(app_in);
        }

        while let Ok(gui_in) = gui_rx.try_recv() {
            gui.apply_input(&app_msg_tx, &cfg_msg_tx, gui_in);
        }

        while let Ok(opt_in) = opts_from_gui.try_recv() {
            app.apply_app_config_state(opt_in);
        }

        while let Ok(main_view_in) = main_view_rx.try_recv() {
            main_view.apply_input(screen_dims, &app_msg_tx, main_view_in);
        }

        while let Ok(app_msg) = app_msg_rx.try_recv() {
            app.apply_app_msg(&app_msg);
        }

        while let Ok(cfg_msg) = cfg_msg_rx.try_recv() {
            app.apply_app_config_msg(&cfg_msg);
        }

        if app.dark_active_theme() {
            gui.set_dark_mode();
        } else {
            gui.set_light_mode();
        }

        gui.set_overlay_state(app.use_overlay);

        gui.set_render_config(
            app.nodes_color,
            app.selection_edge,
            app.selection_edge_detect,
            app.selection_edge_blur,
        );

        gui.set_hover_node(app.hover_node());

        if let Some(selected) = app.selected_nodes() {
            if selected.len() == 1 {
                let node_id = selected.iter().next().copied().unwrap();

                if gui.selected_node() != Some(node_id) {
                    let request = GraphQueryRequest::NodeStats(node_id);
                    let resp = graph_query.query_request_blocking(request);
                    if let GraphQueryResp::NodeStats {
                        node_id,
                        len,
                        degree,
                        coverage,
                    } = resp
                    {
                        gui.one_selection(node_id, len, degree, coverage);
                    }
                }
            } else {
                gui.many_selection(selected.len());
            }

            main_view.update_node_selection(selected).unwrap();
        } else {
            gui.no_selection();
            main_view.clear_node_selection().unwrap();
        }

        let world_point = main_view
            .view()
            .screen_point_to_world(screen_dims, mouse_pos);

        gui.set_view_info_mouse(mouse_pos, world_point);

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => {
                recreate_swapchain = true;
            }
            Event::RedrawEventsCleared => {
                let frame_t = std::time::Instant::now();

                previous_frame_end.as_mut().unwrap().cleanup_finished();

                gui.set_view_info_view(main_view.view());

                gui.begin_frame(Some(app.dims().into()));

                if recreate_swapchain {
                    let dimensions: [u32; 2] =
                        surface.window().inner_size().into();

                    let (new_swapchain, new_images) =
                        match swapchain.recreate_with_dimensions(dimensions) {
                            Ok(r) => r,
                            Err(
                                SwapchainCreationError::UnsupportedDimensions,
                            ) => return,
                            Err(e) => {
                                panic!("Failed to recreate swapchain: {:?}", e)
                            }
                        };

                    swapchain = new_swapchain;
                    images = new_images;

                    update_viewport(&images[0], &mut dynamic_state);

                    if let Some(viewport) =
                        dynamic_state.viewports.as_ref().and_then(|v| v.get(0))
                    {
                        let width = viewport.dimensions[0];
                        let height = viewport.dimensions[1];

                        app.update_dims((width, height));

                        render_pipeline
                            .recreate_offscreen(width as u32, height as u32)
                            .unwrap();

                        let _recreated_image = offscreen_image
                            .recreate(width as u32, height as u32)
                            .unwrap();
                    }

                    recreate_swapchain = false;
                }

                let (image_num, suboptimal, acquire_future) =
                    match swapchain::acquire_next_image(swapchain.clone(), None)
                    {
                        Ok(r) => r,
                        Err(AcquireError::OutOfDate) => {
                            recreate_swapchain = true;
                            return;
                        }
                        Err(e) => {
                            panic!("Failed to acquire next image: {:?}", e)
                        }
                    };

                if suboptimal {
                    recreate_swapchain = true;
                }

                let theme_cache_invalid = {
                    if let Some((theme_id, theme)) = app.active_theme() {
                        main_view
                            .valid_theme_cache(theme_id, theme.color_hash())
                    } else {
                        true
                    }
                };

                let theme_future = if theme_cache_invalid {
                    let to_upload = app.themes().themes_to_upload();
                    let sampler = app.themes().sampler();

                    for (id, theme) in to_upload {
                        main_view.cache_theme(sampler, id, theme).unwrap();
                    }

                    app.theme_upload_future()
                } else {
                    None
                };

                let (theme_id, theme) = app.active_theme().unwrap();

                // the framebuffer used when drawing nodes to the offscreen image
                let nodes_framebuffer =
                    render_pipeline.nodes_framebuffer().unwrap();

                let clear = theme.clear();

                let nodes_clear_values = vec![
                    clear.into(),
                    [0.0, 0.0, 0.0, 0.0].into(),
                    clear.into(),
                    1.0f32.into(),
                    [0.0, 0.0, 0.0, 0.0].into(),
                ];

                let selection_edge_framebuffer =
                    render_pipeline.selection_edge_color_framebuffer().unwrap();

                let selection_edge_clear_values =
                    vec![[0.0, 0.0, 0.0, 0.0].into()];

                // the framebuffer used when drawing the
                // post-processing stage and GUI to the screen --
                // since the post-processing shader fills every pixel
                // of the image, we can use the DontCare load op for
                // both the post-processing and the GUI

                // the framebuffer used when drawing the GUI to the
                // screen -- has to use a separate render pass where
                // the color image load op is DontCare
                let final_framebuffer = render_pipeline
                    .final_framebuffer(images[image_num].clone())
                    .unwrap();

                let final_clear_values = vec![
                    vulkano::format::ClearValue::None,
                    vulkano::format::ClearValue::None,
                ];

                let mut builder =
                    AutoCommandBufferBuilder::primary_one_time_submit(
                        device.clone(),
                        queue.family(),
                    )
                    .unwrap();

                builder
                    .begin_render_pass(
                        nodes_framebuffer,
                        SubpassContents::SecondaryCommandBuffers,
                        nodes_clear_values,
                    )
                    .unwrap();

                unsafe {
                    let secondary_buf =
                        if cached_overlay.is_some() && app.use_overlay {
                            main_view
                                .draw_nodes(
                                    &dynamic_state,
                                    universe.offset,
                                    theme_id,
                                    cached_overlay.as_ref(),
                                )
                                .unwrap()
                        } else {
                            main_view
                                .draw_nodes(
                                    &dynamic_state,
                                    universe.offset,
                                    theme_id,
                                    None,
                                )
                                .unwrap()
                        };

                    builder.execute_commands(secondary_buf).unwrap();
                }

                builder.end_render_pass().unwrap();

                let command_buffer = builder.build().unwrap();

                let first_pass_future = {
                    let mut prev = previous_frame_end
                        .take()
                        .unwrap()
                        .join(acquire_future)
                        .boxed();

                    if let Some(future) = overlay_future.take() {
                        prev = prev.join(future).boxed();
                    }

                    if let Some(future) = theme_future {
                        prev = prev.join(future).boxed();
                    }

                    prev.then_execute(queue.clone(), command_buffer)
                        .unwrap()
                        .boxed()
                };

                let mut builder =
                    AutoCommandBufferBuilder::primary_one_time_submit(
                        device.clone(),
                        queue.family(),
                    )
                    .unwrap();

                let nodes_color_img =
                    render_pipeline.nodes_color().image().clone();
                let nodes_color_sampler =
                    render_pipeline.nodes_color().sampler().clone();

                let nodes_mask_img =
                    render_pipeline.nodes_mask().image().clone();
                let nodes_mask_sampler =
                    render_pipeline.nodes_mask().sampler().clone();

                builder
                    .begin_render_pass(
                        selection_edge_framebuffer,
                        SubpassContents::Inline,
                        selection_edge_clear_values,
                    )
                    .unwrap();

                if app.selection_edge {
                    if app.selection_edge_detect {
                        post_draw_system
                            .edge_primary(
                                &mut builder,
                                nodes_mask_img,
                                nodes_mask_sampler,
                                &dynamic_state,
                                true,
                            )
                            .unwrap();
                    } else {
                        post_draw_system
                            .blur_primary(
                                &mut builder,
                                nodes_mask_img,
                                nodes_mask_sampler,
                                &dynamic_state,
                                false,
                            )
                            .unwrap();
                    }
                }

                builder.end_render_pass().unwrap();

                let command_buffer = builder.build().unwrap();

                let second_pass_future = first_pass_future
                    .then_execute(queue.clone(), command_buffer)
                    .unwrap();

                let mut builder =
                    AutoCommandBufferBuilder::primary_one_time_submit(
                        device.clone(),
                        queue.family(),
                    )
                    .unwrap();

                if !app.nodes_color {
                    builder
                        .clear_color_image(
                            nodes_color_img.clone(),
                            [0.0, 0.0, 0.0, 1.0].into(),
                        )
                        .unwrap();
                }

                builder
                    .begin_render_pass(
                        final_framebuffer.clone(),
                        SubpassContents::Inline,
                        final_clear_values.clone(),
                    )
                    .unwrap();

                let selection_edge_img =
                    render_pipeline.selection_edge_color().image().clone();
                let selection_edge_sampler =
                    render_pipeline.selection_edge_color().sampler().clone();

                post_draw_system_final
                    .blur_primary(
                        &mut builder,
                        nodes_color_img,
                        nodes_color_sampler,
                        &dynamic_state,
                        false,
                    )
                    .unwrap();

                if app.selection_edge {
                    if app.selection_edge_blur {
                        post_draw_system_final
                            .blur_primary(
                                &mut builder,
                                selection_edge_img,
                                selection_edge_sampler,
                                &dynamic_state,
                                true,
                            )
                            .unwrap();
                    } else {
                        post_draw_system_final
                            .blur_primary(
                                &mut builder,
                                selection_edge_img,
                                selection_edge_sampler,
                                &dynamic_state,
                                false,
                            )
                            .unwrap();
                    }
                }

                builder.end_render_pass().unwrap();

                builder
                    .begin_render_pass(
                        final_framebuffer,
                        SubpassContents::SecondaryCommandBuffers,
                        final_clear_values,
                    )
                    .unwrap();

                if main_view.draw_grid {
                    unsafe {
                        let cmd_buf =
                            main_view.draw_lines(&dynamic_state).unwrap();
                        builder.execute_commands(cmd_buf).unwrap();
                    }
                }

                let future = if let Some(gui_result) =
                    gui.end_frame_and_draw(&dynamic_state)
                {
                    let (cmd_bufs, future) = gui_result.unwrap();
                    unsafe {
                        builder.execute_commands_from_vec(cmd_bufs).unwrap();
                    }
                    future.unwrap_or(sync::now(device.clone()).boxed())
                } else {
                    sync::now(device.clone()).boxed()
                };

                builder.end_render_pass().unwrap();

                let command_buffer = builder.build().unwrap();

                let future = second_pass_future
                    .join(future)
                    .then_execute(queue.clone(), command_buffer)
                    .unwrap()
                    .then_swapchain_present(
                        queue.clone(),
                        swapchain.clone(),
                        image_num,
                    )
                    .then_signal_fence_and_flush();

                match future {
                    Ok(future) => {
                        future.wait(None).unwrap();
                        previous_frame_end = Some(future.boxed());
                    }
                    Err(FlushError::OutOfDate) => {
                        recreate_swapchain = true;
                        previous_frame_end =
                            Some(sync::now(device.clone()).boxed());
                    }
                    Err(e) => {
                        eprintln!("Failed to flush future: {:?}", e);
                        previous_frame_end =
                            Some(sync::now(device.clone()).boxed());
                    }
                }

                let frame_time = frame_t.elapsed().as_secs_f32();
                frame_time_history[frame % frame_time_history.len()] =
                    frame_time;

                if frame > FRAME_HISTORY_LEN && frame % FRAME_HISTORY_LEN == 0 {
                    let ft_sum: f32 = frame_time_history.iter().sum();
                    let avg = ft_sum / (FRAME_HISTORY_LEN as f32);
                    let fps = 1.0 / avg;
                    let avg_ms = avg * 1000.0;
                    gui.set_frame_rate(frame, fps, avg_ms);
                }

                frame += 1;
            }
            _ => (),
        }
    });
}

fn update_viewport(
    image: &SwapchainImage<Window>,
    dynamic_state: &mut DynamicState,
) {
    let dims = image.dimensions();
    let dimensions = [dims[0] as f32, dims[1] as f32];

    let viewport = Viewport {
        origin: [0.0, 0.0],
        dimensions,
        depth_range: 0.0..1.0,
    };
    dynamic_state.viewports = Some(vec![viewport]);
}
*/
