use draw_system::nodes::NodeOverlay;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::platform::unix::*;
use winit::window::{Window, WindowBuilder};

use gfaestus::app::mainview::*;
use gfaestus::app::{App, AppConfigMsg, AppConfigState, AppMsg};
use gfaestus::geometry::*;
use gfaestus::graph_query::*;
use gfaestus::input::*;
use gfaestus::overlays::*;
use gfaestus::universe::*;
use gfaestus::view::View;
use gfaestus::vulkan::render_pass::Framebuffers;

use gfaestus::gui::{widgets::*, windows::*, *};

use gfaestus::vulkan::draw_system::selection::{
    SelectionOutlineBlurPipeline, SelectionOutlineEdgePipeline,
};

use anyhow::Result;

use ash::version::DeviceV1_0;
use ash::{vk, Device};

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

    let graph_query = GraphQuery::load_gfa(gfa_file).unwrap();

    let (universe, stats) = universe_from_gfa_layout(&graph_query, layout_file).unwrap();

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
    // let event_loop: EventLoop<()> = EventLoop::new_x11().unwrap();
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

    let (winit_tx, winit_rx) = crossbeam::channel::unbounded::<WindowEvent<'static>>();

    let input_manager = InputManager::new(winit_rx);

    let app_rx = input_manager.clone_app_rx();
    let main_view_rx = input_manager.clone_main_view_rx();
    let gui_rx = input_manager.clone_gui_rx();

    let mut app =
        App::new(input_manager.clone_mouse_pos(), (100.0, 100.0)).expect("error when creating App");

    let node_vertices = universe.new_vertices();

    let mut main_view = MainView::new(
        &gfaestus,
        graph_query.node_count(),
        gfaestus.swapchain_props,
        gfaestus.msaa_samples,
        gfaestus.render_passes.nodes,
    )
    .unwrap();

    let (mut gui, opts_from_gui) = Gui::new(
        &gfaestus,
        app.overlay_state.clone(),
        input_manager.gui_focus_state().clone(),
        main_view.node_width().clone(),
        &graph_query,
        gfaestus.swapchain_props,
        gfaestus.msaa_samples,
        gfaestus.render_passes.gui,
    )
    .unwrap();

    let new_overlay_rx = gui.new_overlay_rx().clone();

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

    let (app_msg_tx, app_msg_rx) = crossbeam::channel::unbounded::<AppMsg>();
    let (cfg_msg_tx, cfg_msg_rx) = crossbeam::channel::unbounded::<AppConfigMsg>();

    let (opts_to_gui, opts_from_app) = crossbeam::channel::unbounded::<AppConfigState>();

    app.themes
        .upload_to_gpu(&gfaestus, &mut main_view.node_draw_system.theme_pipeline)
        .unwrap();

    main_view
        .node_draw_system
        .theme_pipeline
        .set_active_theme(0)
        .unwrap();

    let mut dirty_swapchain = false;

    let mut gluonvm = gfaestus::gluon::GluonVM::new().unwrap();

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
        // gfaestus.render_passes.gui,
        gfaestus.node_attachments.mask_resolve,
        // gfaestus.offscreen_attachment.color,
    )
    .unwrap();

    let gui_msg_tx = gui.clone_gui_msg_tx();

    let mut snarl_overlay = SnarlOverlay::new(&gfaestus, graph_query.node_count()).unwrap();

    let snarls = [(1, 10), (3, 7), (100, 200), (120, 180)];

    for (a, b) in std::array::IntoIter::new(snarls) {
        let device = gfaestus.vk_context().device();
        let snarl = (NodeId::from(a), NodeId::from(b));

        snarl_overlay.add_snarl(device, snarl).unwrap();
    }
    dbg!();

    let overlay = snarl_overlay.into_overlay();
    dbg!();
    main_view
        .node_draw_system
        .overlay_pipeline
        .update_overlay(0, overlay);

    let graph_arc = graph_query.graph_arc().clone();
    let graph_handle = gfaestus::gluon::GraphHandle::new(graph_arc);

    gluonvm.test_graph_handle(&graph_handle);

    let overlay_colors = gluonvm.example_overlay(&graph_handle).unwrap();

    println!("built overlay colors for {} nodes", overlay_colors.len());

    let mut overlay_2 =
        NodeOverlay::new_empty("gluon_overlay", &gfaestus, graph_query.node_count()).unwrap();

    overlay_2
        .update_overlay(
            gfaestus.vk_context().device(),
            overlay_colors
                .iter()
                .enumerate()
                .map(|(ix, col)| (NodeId::from((ix as u64) + 1), *col)),
        )
        .unwrap();

    main_view
        .node_draw_system
        .overlay_pipeline
        .update_overlay(1, overlay_2);

    dbg!();
    main_view
        .node_draw_system
        .overlay_pipeline
        .set_active_overlay(Some(1))
        .unwrap();

    let mut next_overlay_id = 2;

    gui.populate_overlay_list(main_view.node_draw_system.overlay_pipeline.overlay_names());

    dbg!();
    const FRAME_HISTORY_LEN: usize = 10;
    let mut frame_time_history = [0.0f32; FRAME_HISTORY_LEN];
    let mut frame = 0;

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

        gui_msg_tx.send(GuiMsg::SetLightMode).unwrap();

        let screen_dims = app.dims();

        match event {
            Event::NewEvents(_) => {
                // hacky -- this should take place after mouse pos is updated
                // in egui but before input is sent to mainview
                input_manager.set_mouse_over_gui(gui.pointer_over_gui());
                input_manager.handle_events(&gui_msg_tx);

                let mouse_pos = app.mouse_pos();

                gui.push_event(egui::Event::PointerMoved(mouse_pos.into()));
                main_view.set_mouse_pos(Some(mouse_pos));
                main_view.set_screen_dims(screen_dims);

                let hover_node = main_view
                    .read_node_id_at(mouse_pos)
                    .map(|nid| NodeId::from(nid as u64));

                app_msg_tx.send(AppMsg::HoverNode(hover_node)).unwrap();

                gui.set_hover_node(hover_node);

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

                while let Ok(app_in) = app_rx.try_recv() {
                    app.apply_input(app_in);
                }

                while let Ok(gui_in) = gui_rx.try_recv() {
                    gui.apply_input(&app_msg_tx, &cfg_msg_tx, gui_in);
                }

                gui.apply_received_gui_msgs();

                // while let Ok(opt_in) = opts_from_gui.try_recv() {
                //     app.apply_app_config_state(opt_in);
                // }

                while let Ok(main_view_in) = main_view_rx.try_recv() {
                    main_view.apply_input(screen_dims, app.mouse_pos(), &app_msg_tx, main_view_in);
                }

                while let Ok(app_msg) = app_msg_rx.try_recv() {
                    app.apply_app_msg(&app_msg);
                }

                while let Ok(cfg_msg) = cfg_msg_rx.try_recv() {
                    app.apply_app_config_msg(&cfg_msg);
                }

                while let Ok(new_overlay) = new_overlay_rx.try_recv() {
                    match new_overlay {
                        OverlayCreatorMsg::NewOverlay { name, colors } => {
                            println!("Received new overlay");
                            let mut overlay =
                                NodeOverlay::new_empty(&name, &gfaestus, graph_query.node_count())
                                    .unwrap();

                            overlay
                                .update_overlay(
                                    gfaestus.vk_context().device(),
                                    colors
                                        .iter()
                                        .enumerate()
                                        .map(|(ix, col)| (NodeId::from((ix as u64) + 1), *col)),
                                )
                                .unwrap();

                            main_view
                                .node_draw_system
                                .overlay_pipeline
                                .update_overlay(next_overlay_id, overlay);
                            //

                            main_view
                                .node_draw_system
                                .overlay_pipeline
                                .set_active_overlay(Some(1))
                                .unwrap();

                            next_overlay_id += 1;

                            gui.populate_overlay_list(
                                main_view.node_draw_system.overlay_pipeline.overlay_names(),
                            );
                        }
                    }
                }
            }
            Event::MainEventsCleared => {
                let screen_dims = app.dims();
                let mouse_pos = app.mouse_pos();
                main_view.update_view_animation(screen_dims, mouse_pos);
            }
            Event::RedrawEventsCleared => {
                let frame_t = std::time::Instant::now();

                main_view
                    .node_draw_system
                    .overlay_pipeline
                    .set_active_overlay(app.overlay_state.current_overlay());

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
                    } else {
                        return;
                    }
                }

                gui.begin_frame(Some(app.dims().into()), &graph_query, &graph_handle);

                let meshes = gui.end_frame();

                gui.upload_texture(&gfaestus).unwrap();

                if !meshes.is_empty() {
                    gui.upload_vertices(&gfaestus, &meshes).unwrap();
                }

                // let device = gfaestus.vk_context().device().clone();

                let node_pass = gfaestus.render_passes.nodes;
                let edge_pass = gfaestus.render_passes.selection_edge_detect;
                let blur_pass = gfaestus.render_passes.selection_blur;
                let gui_pass = gfaestus.render_passes.gui;

                let node_id_image = gfaestus.node_attachments.id_resolve.image;
                // let node_mask_image =
                //     gfaestus.node_attachments.mask_resolve.image;

                let offscreen_image = gfaestus.offscreen_attachment.color.image;

                main_view
                    .node_draw_system
                    .theme_pipeline
                    .set_active_theme(app.themes.active_theme())
                    .unwrap();

                let use_overlay = app.overlay_state.use_overlay();

                let draw =
                    |device: &Device, cmd_buf: vk::CommandBuffer, framebuffers: &Framebuffers| {
                        let size = window.inner_size();

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

                        main_view
                            .draw_nodes(
                                cmd_buf,
                                node_pass,
                                framebuffers,
                                [size.width as f32, size.height as f32],
                                Point::ZERO,
                                use_overlay,
                            )
                            .unwrap();

                        unsafe {
                            // let (image_memory_barrier, _src_stage, _dst_stage) =
                            //     GfaestusVk::image_transition_barrier(
                            //         node_id_image,
                            //         vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            //         vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                            //     );

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
                                // gui_pass,
                                blur_pass,
                                framebuffers,
                                [size.width as f32, size.height as f32],
                            )
                            .unwrap();

                        gui.draw(
                            cmd_buf,
                            gui_pass,
                            framebuffers,
                            [size.width as f32, size.height as f32],
                        )
                        .unwrap();
                    };

                dirty_swapchain = gfaestus.draw_frame_from(draw).unwrap();

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
                )
                .unwrap();

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

                let device = gfaestus.vk_context().device();

                main_view.selection_buffer.destroy(device);
                main_view.node_id_buffer.destroy(device);
                main_view.node_draw_system.destroy();

                gui.draw_system.destroy();

                selection_edge.destroy(device);
                selection_blur.destroy(device);
            }
            _ => (),
        }
    });
}
