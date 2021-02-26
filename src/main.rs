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

use gfaestus::app::gui::*;
use gfaestus::app::mainview::*;
use gfaestus::app::{App, AppMsg};
use gfaestus::geometry::*;
use gfaestus::graph_query::*;
use gfaestus::input::*;
use gfaestus::render::*;
use gfaestus::universe::*;
use gfaestus::util::*;

use rgb::*;

use anyhow::Result;

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

    let extensions = vulkano_win::required_extensions();

    /*
    let layers = vec![
        "VK_LAYER_MESA_device_select",
        "VK_LAYER_RENDERDOC_Capture",
        "VK_LAYER_KHRONOS_validation",
    ];

    let instance = Instance::new(None, &extensions, layers).unwrap();
    */

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

    // this has to be false for renderdoc capture replays to work, but
    // the application doesn't work outside renderdoc if it's false..
    // features.buffer_device_address = false;

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

    let single_pass_msaa_depth_offscreen =
        SinglePassMSAADepth::new(queue.clone(), None, Format::R8G8B8A8Unorm)
            .unwrap();

    let single_pass_dontcare =
        SinglePass::new(queue.clone(), swapchain.format(), false).unwrap();

    let post_draw_system =
        PostDrawSystem::new(queue.clone(), single_pass_dontcare.subpass());

    let (winit_tx, winit_rx) =
        crossbeam::channel::unbounded::<WindowEvent<'static>>();

    let input_manager = InputManager::new(winit_rx);

    // let input_manager = Arc::new(InputManager::new(winit_rx));
    // let input_manager_loop = {
    //     let input_manager = input_manager.clone();
    //     std::thread::spawn(move || input_manager.handle_events())
    // };

    let main_view_rx = input_manager.clone_main_view_rx();
    let gui_rx = input_manager.clone_gui_rx();

    let mut app = App::new(input_manager.clone_mouse_pos(), (100.0, 100.0));

    let mut main_view = MainView::new(
        queue.clone(),
        single_pass_msaa_depth_offscreen.subpass(),
        single_pass_dontcare.subpass(),
    )
    .unwrap();

    let mut gui =
        GfaestusGui::new(queue.clone(), single_pass_dontcare.subpass())
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
        let fut = sync::now(device.clone()).join(line_future);
        Some(fut.boxed())
    };

    const FRAME_HISTORY_LEN: usize = 10;
    let mut frame_time_history = [0.0f32; FRAME_HISTORY_LEN];
    let mut frame = 0;

    println!("MainView.view: {:?}", main_view.view());
    println!("initialized in {}", init_t.elapsed().as_secs_f32());

    let (app_msg_tx, app_msg_rx) = crossbeam::channel::unbounded::<AppMsg>();

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

        while let Ok(gui_in) = gui_rx.try_recv() {
            gui.apply_input(&app_msg_tx, gui_in);
        }

        while let Ok(main_view_in) = main_view_rx.try_recv() {
            main_view.apply_input(screen_dims, &app_msg_tx, main_view_in);
        }

        while let Ok(app_msg) = app_msg_rx.try_recv() {
            app.apply_app_msg(&app_msg);
        }

        gui.set_hover_node(app.hover_node());
        gui.set_selected_node(app.selected_node());

        let world_point = main_view
            .view()
            .screen_point_to_world(screen_dims, mouse_pos);

        gui.set_view_info_mouse(mouse_pos, world_point);

        if let Some(node_id) = gui.selected_node() {
            if gui.selected_node_info_id() != Some(node_id) {
                let request = GraphQueryRequest::NodeStats(node_id);
                let resp = graph_query.query_request_blocking(request);
                if let GraphQueryResp::NodeStats {
                    node_id,
                    len,
                    degree,
                    coverage,
                } = resp
                {
                    gui.set_selected_node_info(node_id, len, degree, coverage);
                }
            }
        }

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

                        let new_image = offscreen_image
                            .recreate(width as u32, height as u32)
                            .unwrap();

                        println!("recreated offscreen_image: {}", new_image);
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

                // the framebuffer used when drawing nodes to the offscreen image
                let msaa_depth_offscreen_framebuffer =
                    single_pass_msaa_depth_offscreen
                        .framebuffer(offscreen_image.image().clone())
                        .unwrap();

                // the framebuffer used when drawing the
                // post-processing stage and GUI to the screen --
                // since the post-processing shader fills every pixel
                // of the image, we can use the DontCare load op for
                // both the post-processing and the GUI

                // the framebuffer used when drawing the GUI to the
                // screen -- has to use a separate render pass where
                // the color image load op is DontCare
                let framebuffer = single_pass_dontcare
                    .framebuffer(images[image_num].clone())
                    .unwrap();

                let mut builder =
                    AutoCommandBufferBuilder::primary_one_time_submit(
                        device.clone(),
                        queue.family(),
                    )
                    .unwrap();

                let clear = [0.0, 0.0, 0.05, 1.0];
                let msaa_depth_clear_values =
                    vec![clear.into(), clear.into(), 1.0f32.into()];

                builder
                    .begin_render_pass(
                        msaa_depth_offscreen_framebuffer,
                        SubpassContents::SecondaryCommandBuffers,
                        msaa_depth_clear_values,
                    )
                    .unwrap();

                unsafe {
                    let secondary_buf = main_view
                        .draw_nodes(&dynamic_state, universe.offset)
                        .unwrap();
                    builder.execute_commands(secondary_buf).unwrap();
                }

                builder.end_render_pass().unwrap();

                let command_buffer = builder.build().unwrap();

                let first_pass_future = previous_frame_end
                    .take()
                    .unwrap()
                    .join(acquire_future)
                    .then_execute(queue.clone(), command_buffer)
                    .unwrap();

                let mut builder =
                    AutoCommandBufferBuilder::primary_one_time_submit(
                        device.clone(),
                        queue.family(),
                    )
                    .unwrap();

                builder
                    .begin_render_pass(
                        framebuffer.clone(),
                        SubpassContents::Inline,
                        vec![vulkano::format::ClearValue::None],
                    )
                    .unwrap();

                let os_img = offscreen_image.image().clone();
                let os_sampler = offscreen_image.sampler().clone();

                post_draw_system
                    .draw_primary(
                        &mut builder,
                        os_img,
                        os_sampler,
                        &dynamic_state,
                    )
                    .unwrap();

                builder.end_render_pass().unwrap();

                builder
                    .begin_render_pass(
                        framebuffer,
                        SubpassContents::SecondaryCommandBuffers,
                        vec![vulkano::format::ClearValue::None],
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
                        // builder.execute_commands(cmd_buf).unwrap();
                    }
                    future.unwrap_or(sync::now(device.clone()).boxed())
                } else {
                    sync::now(device.clone()).boxed()
                };

                builder.end_render_pass().unwrap();

                let command_buffer = builder.build().unwrap();

                let future = first_pass_future
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

/* vulkano debug stuff (for future reference)

let shader_non_semantic_info_ext = {
    // let ext_str = std::ffi::CString::from(b"VK_KHR_shader_non_semantic_info"[..]);
    // let ext_bstr = b"VK_KHR_shader_non_semantic_info";
    // let ext_bstr_vec: Vec<u8> = ext_bstr[..].into();
    let ext_str = std::ffi::CString::new("VK_KHR_shader_non_semantic_info").unwrap();
    let ext_str_2 = std::ffi::CString::new("VK_KHR_shader_non_semantic_info").unwrap();
    // let ext_str =
    //     std::ffi::CString::from(b"VK_KHR_shader_non_semantic_info"[..]);
    let raw_ext = vulkano::instance::RawInstanceExtensions::new(vec![ext_str]);

    // vulkano::instance::InstanceExtensions::from(&raw_ext)
    raw_ext
};

let raw_exts_core = vulkano::instance::RawInstanceExtensions::supported_by_core().unwrap();
// let raw_extensions = raw_extensions.union(&shader_non_semantic_info_ext);

println!("raw_exts_core: {:?}", raw_exts_core);

let raw_exts = vulkano::instance::RawInstanceExtensions::from(&extensions);

let extensions = raw_exts;
// let extensions = raw_exts.union(&shader_non_semantic_info_ext);
// let extensions = raw_exts.union(&shader_non_semantic_info_ext);
// enable vulkan debugging
// extensions.

// println!();
// println!("raw_exts_2: {:?}", raw_exts_2);
// let extensions = extensions.union(&shader_non_semantic_info_ext);
// println!();

// extensions.ext_debug_utils = true;

println!("extensions: {:?}", extensions);
println!();

println!("List of Vulkan debugging layers available to use:");
let mut layers = vulkano::instance::layers_list().unwrap();
while let Some(l) = layers.next() {
    println!("\t{}", l.name());
}
let layers = vec!["VK_LAYER_KHRONOS_validation"];
let instance = Instance::new(None, &extensions, layers).unwrap();



let raw_dev_ext = RawDeviceExtensions::from(&device_ext);
let debug_dev_ext = {
    let ext_str = std::ffi::CString::new("VK_KHR_shader_non_semantic_info").unwrap();
    let raw_ext = RawDeviceExtensions::new(vec![ext_str]);

    raw_ext
};

let device_ext = raw_dev_ext.union(&debug_dev_ext);

println!("device extensions: {:?}", device_ext);

println!("features: {:?}", physical.supported_features());



let severity = MessageSeverity {
    error: true,
    warning: true,
    information: true,
    verbose: true,
};

let ty = MessageType::all();

let _debug_callback = DebugCallback::new(&instance, severity, ty, |msg| {
    let severity = if msg.severity.error {
        "error"
    } else if msg.severity.warning {
        "warning"
    } else if msg.severity.information {
        "information"
    } else if msg.severity.verbose {
        "verbose"
    } else {
        panic!("no-impl");
    };

    let ty = if msg.ty.general {
        "general"
    } else if msg.ty.validation {
        "validation"
    } else if msg.ty.performance {
        "performance"
    } else {
        panic!("no-impl");
    };

    println!(
        "{} {} {}: {}",
        msg.layer_prefix, ty, severity, msg.description
    );
})
.ok();


*/
