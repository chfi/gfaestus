#[allow(unused_imports)]
use vulkano::device::{Device, DeviceExtensions, RawDeviceExtensions};
#[allow(unused_imports)]
use vulkano::framebuffer::{
    Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass,
};
use vulkano::image::{AttachmentImage, ImageUsage, SwapchainImage};
#[allow(unused_imports)]
use vulkano::instance::debug::{DebugCallback, MessageSeverity, MessageType};

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

use std::sync::Arc;

use std::time::Instant;

use gfaestus::app::gui::*;
use gfaestus::app::mainview::*;
use gfaestus::geometry::*;
use gfaestus::input::*;
use gfaestus::render::*;
use gfaestus::universe::*;

use rgb::*;

use nalgebra_glm as glm;

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
    gfa_path: &str,
    layout_path: &str,
) -> Result<(Universe<FlatLayout>, GraphStats)> {
    let mut mmap = gfa::mmap::MmapGFA::new(gfa_path)?;

    let graph = gfaestus::gfa::load::packed_graph_from_mmap(&mut mmap)?;

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

    // let layout_file = args.get(2);

    eprintln!("loading GFA");
    let t = std::time::Instant::now();
    let init_t = std::time::Instant::now();

    let (universe, stats) =
        universe_from_gfa_layout(gfa_file, layout_file).unwrap();

    let (top_left, bottom_right) = universe.layout().bounding_box();

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

    let (device, mut queues) = Device::new(
        physical,
        physical.supported_features(),
        &device_ext,
        [(queue_family, 0.5)].iter().cloned(),
    )
    .unwrap();

    let queue = queues.next().unwrap();

    let (mut swapchain, images) = {
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

    let render_pass = Arc::new(
        vulkano::single_pass_renderpass!(
            device.clone(),
            attachments: {
                intermediary: {
                    load: Clear,
                    store: DontCare,
                    format: swapchain.format(),
                    samples: 8,
                },
                color: {
                    load: Clear,
                    store: Store,
                    format: swapchain.format(),
                    samples: 1,
                }
            },
            pass: {
                color: [intermediary],
                depth_stencil: {}
                resolve: [color],
            }
        )
        .unwrap(),
    );

    let mut main_view = MainView::new(queue.clone(), &render_pass).unwrap();

    let mut gui = GfaestusGui::new(queue.clone(), &render_pass).unwrap();

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
    main_view.set_initial_view(Some(top_left + (layout_dims / 2.0)), None);

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

    let mut framebuffers =
        window_size_update(&images, render_pass.clone(), &mut dynamic_state);

    let mut width = 100.0;
    let mut height = 100.0;

    if let Some(viewport) =
        dynamic_state.viewports.as_ref().and_then(|v| v.get(0))
    {
        width = viewport.dimensions[0];
        height = viewport.dimensions[1];
    }

    let input_action_handler = InputActionWorker::new();

    let semantic_input_rx = input_action_handler.clone_semantic_rx();
    let input_action_rx = input_action_handler.clone_action_rx();

    let mut recreate_swapchain = false;

    let mut previous_frame_end = {
        let fut = sync::now(device.clone()).join(line_future);
        Some(fut.boxed())
    };

    let mut last_time = Instant::now();

    let mut paused = false;

    const FRAME_HISTORY_LEN: usize = 10;
    let mut frame_time_history = [0.0f32; FRAME_HISTORY_LEN];
    let mut frame = 0;

    let mut draw_grid = true;

    let mut mouse_pos = Point { x: 0.0, y: 0.0 };

    let mut last_frame_t = std::time::Instant::now();

    println!("MainView.view: {:?}", main_view.view());

    let mut gui_screen_rect = Some(Point {
        x: width,
        y: height,
    });

    let anim_thread = main_view.anim_handler_thread();

    println!("initialized in {}", init_t.elapsed().as_secs_f32());

    event_loop.run(move |event, _, control_flow| {
        last_frame_t = std::time::Instant::now();
        let now = Instant::now();
        let delta = now.duration_since(last_time);

        last_time = now;

        if let Event::WindowEvent { event, .. } = &event {
            input_action_handler.send_window_event(&event);
        }

        let mut mouse_released = false;
        let mut mouse_pressed = false;
        while let Ok(semin) = semantic_input_rx.try_recv() {
            if let SemanticInput::MouseButtonPan(input_change) = semin {
                if input_change.released() {
                    mouse_released = true;
                    mouse_pressed = false;
                } else if input_change.pressed() {
                    mouse_released = false;
                    mouse_pressed = true;
                }
            }

            if let SemanticInput::OtherKey { key, pressed } = semin {
                use winit::event::VirtualKeyCode as Key;
                match key {
                    // Key::Key1 => {}
                    // Key::Key2 => {}
                    // Key::Key3 => {}
                    // Key::Key4 => {}
                    // Key::Key5 => {}
                    // Key::Key6 => {}
                    // Key::Key7 => {}
                    // Key::Key8 => {}
                    // Key::Key9 => {}
                    // Key::Key0 => {}
                    // Key::A => {}
                    // Key::B => {}
                    // Key::C => {}
                    // Key::D => {}
                    // Key::E => {}
                    // Key::F => {}
                    // Key::G => {}
                    // Key::H => {}
                    // Key::I => {}
                    // Key::J => {}
                    // Key::K => {}
                    // Key::L => {}
                    // Key::M => {}
                    // Key::N => {}
                    // Key::O => {}
                    // Key::P => {}
                    // Key::Q => {}
                    // Key::R => {}
                    // Key::S => {}
                    // Key::T => {}
                    // Key::U => {}
                    // Key::V => {}
                    // Key::W => {}
                    // Key::X => {}
                    // Key::Y => {}
                    // Key::Z => {}
                    // Key::Escape => {}
                    Key::F1 => {
                        if pressed {
                            gui.toggle_inspection_ui();
                        }
                    }
                    Key::F2 => {
                        if pressed {
                            gui.toggle_settings_ui();
                        }
                    }
                    Key::F3 => {
                        if pressed {
                            gui.toggle_memory_ui();
                        }
                    }
                    // Key::F2 => {}
                    // Key::F3 => {}
                    // Key::F4 => {}
                    // Key::F5 => {}
                    // Key::F6 => {}
                    // Key::F7 => {}
                    // Key::F8 => {}
                    // Key::F9 => {}
                    // Key::F10 => {}
                    // Key::F11 => {}
                    // Key::Pause => {}
                    // Key::Insert => {}
                    // Key::Home => {}
                    // Key::Delete => {}
                    // Key::End => {}
                    // Key::PageDown => {}
                    // Key::PageUp => {}
                    _ => {}
                }
            }
        }

        while let Ok(action) = input_action_rx.try_recv() {
            use InputAction as Action;
            match action {
                Action::KeyPan {
                    up,
                    right,
                    down,
                    left,
                } => {
                    let dx = match (left, right) {
                        (false, false) => 0.0,
                        (true, false) => -1.0,
                        (false, true) => 1.0,
                        (true, true) => 0.0,
                    };
                    let dy = match (up, down) {
                        (false, false) => 0.0,
                        (true, false) => -1.0,
                        (false, true) => 1.0,
                        (true, true) => 0.0,
                    };

                    let speed = 400.0;
                    let delta = Point {
                        x: dx * speed,
                        y: dy * speed,
                    };
                    // main_view.pan_const(Some(delta.x), Some(delta.y));
                    anim_thread.pan_const(Some(delta.x), Some(delta.y));
                }
                Action::PausePhysics => {
                    main_view.reset_view();
                    if paused {
                        gui.set_hover_node(Some(NodeId::from(123)));
                    } else {
                        gui.set_hover_node(None);
                    }
                    paused = !paused;
                }
                Action::ResetLayout => {
                    draw_grid = !draw_grid;
                    // layout = init_layout.clone();
                }
                Action::MousePan(focus) => {
                    if let Some(focus) = focus {
                        let egui_event = egui::Event::PointerButton {
                            pos: egui::Pos2 {
                                x: focus.x,
                                y: focus.y,
                            },
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        };

                        gui.push_event(egui_event);

                        let mut origin = focus;
                        origin.x -= width / 2.0;
                        origin.y -= height / 2.0;

                        if !gui.pointer_over_gui() {
                            let node_id_at = main_view
                                .read_node_id_at((width, height), focus)
                                .map(|nid| NodeId::from(nid as u64));
                            gui.set_selected_node(node_id_at);
                            // main_view.set_mouse_pan(Some(focus));
                            anim_thread.set_mouse_pan(Some(focus));
                        }
                    } else {
                        // main_view.set_mouse_pan(None);
                        anim_thread.set_mouse_pan(None);
                    }
                }
                Action::MouseZoom { focus, delta } => {
                    let _focus = focus;
                    // main_view.zoom_delta(delta);
                    anim_thread.zoom_delta(delta);
                }
                Action::MouseAt { point } => {
                    let mut screen_tgt = point;
                    screen_tgt.x -= width / 2.0;
                    screen_tgt.y -= height / 2.0;

                    mouse_pos = point;

                    let world_point = main_view
                        .view()
                        .screen_point_to_world((width, height), point);

                    gui.set_view_info_mouse(point, world_point);

                    let egui_event = egui::Event::PointerMoved(egui::Pos2 {
                        x: point.x,
                        y: point.y,
                    });

                    gui.push_event(egui_event);
                }
            }
        }

        if mouse_released || mouse_pressed {
            let egui_event = egui::Event::PointerButton {
                pos: egui::Pos2 {
                    x: mouse_pos.x,
                    y: mouse_pos.y,
                },
                button: egui::PointerButton::Primary,
                pressed: mouse_pressed,
                modifiers: Default::default(),
            };

            gui.push_event(egui_event);
        }

        anim_thread.set_mouse_pos(Some(mouse_pos));

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

                let node_id_at = main_view
                    .read_node_id_at((width, height), mouse_pos)
                    .map(|nid| NodeId::from(nid as u64));

                gui.set_hover_node(node_id_at);
                if mouse_pressed {
                    gui.set_selected_node(node_id_at);
                }

                gui.set_view_info_view(main_view.view());

                gui.begin_frame(gui_screen_rect);
                gui_screen_rect = None;

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

                    framebuffers = window_size_update(
                        &new_images,
                        render_pass.clone(),
                        &mut dynamic_state,
                    );

                    if let Some(viewport) =
                        dynamic_state.viewports.as_ref().and_then(|v| v.get(0))
                    {
                        width = viewport.dimensions[0];
                        height = viewport.dimensions[1];
                    }

                    gui_screen_rect = Some(Point {
                        x: width,
                        y: height,
                    });

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

                // let clear = if paused {
                //     [0.05, 0.0, 0.0, 1.0]
                // } else {
                //     [0.0, 0.0, 0.05, 1.0]
                // };
                let clear = [0.0, 0.0, 0.05, 1.0];
                // let clear = [0.7, 0.7, 0.7, 1.0];
                let clear_values = vec![clear.into(), clear.into()];

                let mut builder =
                    AutoCommandBufferBuilder::primary_one_time_submit(
                        device.clone(),
                        queue.family(),
                    )
                    .unwrap();

                builder
                    .begin_render_pass(
                        framebuffers[image_num].clone(),
                        SubpassContents::SecondaryCommandBuffers,
                        clear_values,
                    )
                    .unwrap();

                unsafe {
                    let secondary_buf = main_view
                        .draw_nodes(&dynamic_state, universe.offset)
                        .unwrap();
                    builder.execute_commands(secondary_buf).unwrap();
                }

                if draw_grid {
                    unsafe {
                        let cmd_buf =
                            main_view.draw_lines(&dynamic_state).unwrap();
                        builder.execute_commands(cmd_buf).unwrap();
                    }
                }

                let future = if let Some(gui_res) =
                    gui.end_frame_and_draw(&dynamic_state)
                {
                    let (cmd_buf, future) = gui_res.unwrap();
                    unsafe {
                        builder.execute_commands(cmd_buf).unwrap();
                    }
                    future.unwrap_or(sync::now(device.clone()).boxed())
                } else {
                    sync::now(device.clone()).boxed()
                };

                builder.end_render_pass().unwrap();

                let command_buffer = builder.build().unwrap();

                let future = previous_frame_end
                    .take()
                    .unwrap()
                    .join(acquire_future)
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
                    gui.set_frame_rate(frame, fps, avg);
                    // println!("time: {:.2}\tframe: {}", t, frame);
                    // println!("avg update time: {:.6}\t{} FPS", avg, fps);
                    // println!("node vertex & color count: {}", vertex_count);
                    // println!("view scale {}\tlast width: {}", view.scale, last_width);
                }

                frame += 1;
            }
            _ => (),
        }
    });
}

fn window_size_update(
    images: &[Arc<SwapchainImage<Window>>],
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    dynamic_state: &mut DynamicState,
) -> Vec<Arc<dyn FramebufferAbstract + Send + Sync>> {
    let dims = images[0].dimensions();
    let dimensions = [dims[0] as f32, dims[1] as f32];

    let viewport = Viewport {
        origin: [0.0, 0.0],
        dimensions,
        depth_range: 0.0..1.0,
    };
    dynamic_state.viewports = Some(vec![viewport]);

    let device = render_pass.device();

    images
        .iter()
        .map(|image| {
            let intermediary = AttachmentImage::transient_multisampled(
                device.clone(),
                dims,
                8,
                image.swapchain().format(),
            )
            .unwrap();

            Arc::new(
                Framebuffer::start(render_pass.clone())
                    .add(intermediary.clone())
                    .unwrap()
                    .add(image.clone())
                    .unwrap()
                    .build()
                    .unwrap(),
            ) as Arc<dyn FramebufferAbstract + Send + Sync>
        })
        .collect::<Vec<_>>()
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
