use vulkano::descriptor::{descriptor_set::PersistentDescriptorSet, PipelineLayoutAbstract};
use vulkano::device::{Device, DeviceExtensions};
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer, CpuBufferPool},
    image::{AttachmentImage, Dimensions},
};
use vulkano::{
    command_buffer::{AutoCommandBufferBuilder, DynamicState, SubpassContents},
    pipeline::vertex::TwoBuffersDefinition,
};

use vulkano::pipeline::{viewport::Viewport, GraphicsPipeline};

use vulkano::swapchain::{
    self, AcquireError, ColorSpace, FullscreenExclusive, PresentMode, SurfaceTransform, Swapchain,
    SwapchainCreationError,
};
use vulkano::sync::{self, FlushError, GpuFuture};

use vulkano_win::VkSurfaceBuild;

use crossbeam::channel;

use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use std::sync::Arc;

use std::time::Instant;

use gfaestus::geometry::*;
use gfaestus::gfa::*;
use gfaestus::ui::events::{keyboard_input, mouse_wheel_input};
use gfaestus::ui::{UICmd, UIState, UIThread};
use gfaestus::view;
use gfaestus::view::View;

use gfaestus::input::*;

use gfaestus::layout::physics;
use gfaestus::layout::*;

use nalgebra_glm as glm;

fn main() {
    let required_extensions = vulkano_win::required_extensions();
    let instance = Instance::new(None, &required_extensions, None).unwrap();
    let physical = PhysicalDevice::enumerate(&instance).next().unwrap();

    let event_loop = EventLoop::new();
    let surface = WindowBuilder::new()
        .build_vk_surface(&event_loop, instance.clone())
        .unwrap();

    let queue_family = physical
        .queue_families()
        .find(|&q| q.supports_graphics() && surface.is_supported(q).unwrap_or(false))
        .unwrap();

    let device_ext = DeviceExtensions {
        khr_swapchain: true,
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

        Swapchain::new(
            device.clone(),
            surface.clone(),
            caps.min_image_count,
            format,
            dimensions,
            1,
            ImageUsage::color_attachment(),
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

    let vertex_buffer_pool: CpuBufferPool<Vertex> = CpuBufferPool::vertex_buffer(device.clone());
    let color_buffer_pool: CpuBufferPool<Color> = CpuBufferPool::vertex_buffer(device.clone());

    let _ = include_str!("../shaders/fragment.frag");
    let _ = include_str!("../shaders/vertex.vert");
    // let _ = include_str!("../shaders/point.vert");
    // let _ = include_str!("../shaders/point.frag");
    // let _ = include_str!("../shaders/geometry.geom");

    mod simple_vert {
        vulkano_shaders::shader! {
            ty: "vertex",
            path: "shaders/vertex.vert",
        }
    }

    mod simple_frag {
        vulkano_shaders::shader! {
            ty: "fragment",
            path: "shaders/fragment.frag",
        }
    }

    /*
    mod point_vert {
        vulkano_shaders::shader! {
            ty: "vertex",
            path: "shaders/point.vert",
        }
    }

    mod point_frag {
        vulkano_shaders::shader! {
            ty: "fragment",
            path: "shaders/point.frag",
        }
    }

    mod rect_geom {
        vulkano_shaders::shader! {
            ty: "geometry",
            path: "shaders/geometry.geom",
        }
    }
    */

    let simple_vert = simple_vert::Shader::load(device.clone()).unwrap();
    let simple_frag = simple_frag::Shader::load(device.clone()).unwrap();

    let uniform_buffer =
        CpuBufferPool::<simple_vert::ty::View>::new(device.clone(), BufferUsage::uniform_buffer());

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

    let pipeline = Arc::new(
        GraphicsPipeline::start()
            .vertex_input(TwoBuffersDefinition::<Vertex, Color>::new())
            .vertex_shader(simple_vert.main_entry_point(), ())
            .triangle_list()
            .viewports_dynamic_scissors_irrelevant(1)
            .fragment_shader(simple_frag.main_entry_point(), ())
            .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
            .blend_alpha_blending()
            .build(device.clone())
            .unwrap(),
    );

    let mut dynamic_state = DynamicState {
        line_width: None,
        viewports: None,
        scissors: None,
        compare_mask: None,
        write_mask: None,
        reference: None,
    };

    let mut spines = test_spines();

    let mut view: View = View::default();

    let mut framebuffers = window_size_update(&images, render_pass.clone(), &mut dynamic_state);

    let mut width = 100.0;
    let mut height = 100.0;

    if let Some(viewport) = dynamic_state.viewports.as_ref().and_then(|v| v.get(0)) {
        view.width = viewport.dimensions[0];
        view.height = viewport.dimensions[1];

        width = viewport.dimensions[0];
        height = viewport.dimensions[1];
    }

    let (ui_thread, ui_cmd_tx, view_rx) = UIThread::new(width, height);

    let input_action_handler = InputActionWorker::new();
    // let input_handler = input_event_handler();

    let semantic_input_rx = input_action_handler.clone_semantic_rx();
    let input_action_rx = input_action_handler.clone_action_rx();

    let mut recreate_swapchain = false;

    let mut previous_frame_end = Some(sync::now(device.clone()).boxed());

    let mut last_time = Instant::now();
    let mut t = 0.0;

    let mut since_last_update = 0.0;

    let mut paused = false;

    event_loop.run(move |event, _, control_flow| {
        let now = Instant::now();
        let delta = now.duration_since(last_time);

        t += delta.as_secs_f32();

        if !paused {
            since_last_update += delta.as_secs_f32();

            if since_last_update > 0.01 {
                physics::repulsion_spines(since_last_update, &mut spines);

                since_last_update = 0.0;
            }
        }

        last_time = now;

        if let Event::WindowEvent { event, .. } = &event {
            input_action_handler.send_window_event(&event);
        }

        while let Ok(action) = input_action_rx.try_recv() {
            println!("received action: {:?}", action);
        }

        match event {
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input, .. },
                ..
            } => {
                keyboard_input(&ui_cmd_tx, input);

                use winit::event::VirtualKeyCode as Key;

                let pressed = input.state == winit::event::ElementState::Pressed;

                if pressed {
                    if let Some(Key::Space) = input.virtual_keycode {
                        paused = !paused;
                    }
                    if let Some(Key::Return) = input.virtual_keycode {
                        spines = test_spines();
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::MouseWheel { delta, .. },
                ..
            } => {
                mouse_wheel_input(&ui_cmd_tx, delta);
            }
            /*
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                if let Some(viewport) = dynamic_state.viewports.as_ref().and_then(|v| v.get(0)) {
                    let pos_x = position.x as f32;
                    let pos_y = position.y as f32;
                    let norm_x = pos_x / viewport.dimensions[0];
                    let norm_y = pos_y / viewport.dimensions[1];
                    // view.center.x = 0.5 + (norm_x / -2.0);
                    // view.center.y = 0.5 + (norm_y / -2.0);
                    // view.center.x = (norm_x / -2.0);
                    // view.center.y = (norm_y / -2.0);

                    // ui_cmd_tx.send(UICmd::Zoom { delta: 0.05 });

                    view.center.x = 0.0;
                    view.center.y = 0.0;

                    view.width = viewport.dimensions[0];
                    view.height = viewport.dimensions[1];
                }
            }
            */
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
                previous_frame_end.as_mut().unwrap().cleanup_finished();

                if recreate_swapchain {
                    let dimensions: [u32; 2] = surface.window().inner_size().into();

                    let (new_swapchain, new_images) =
                        match swapchain.recreate_with_dimensions(dimensions) {
                            Ok(r) => r,
                            Err(SwapchainCreationError::UnsupportedDimensions) => return,
                            Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
                        };

                    swapchain = new_swapchain;

                    if let Some(viewport) = dynamic_state.viewports.as_ref().and_then(|v| v.get(0))
                    {
                        view.width = viewport.dimensions[0];
                        view.height = viewport.dimensions[1];

                        width = viewport.dimensions[0];
                        height = viewport.dimensions[1];
                    }

                    framebuffers =
                        window_size_update(&new_images, render_pass.clone(), &mut dynamic_state);
                    recreate_swapchain = false;
                }

                let (image_num, suboptimal, acquire_future) =
                    match swapchain::acquire_next_image(swapchain.clone(), None) {
                        Ok(r) => r,
                        Err(AcquireError::OutOfDate) => {
                            recreate_swapchain = true;
                            return;
                        }
                        Err(e) => panic!("Failed to acquire next image: {:?}", e),
                    };

                if suboptimal {
                    recreate_swapchain = true;
                }

                if let Ok(latest_view) = view_rx.recv() {
                    view = latest_view;
                }

                let layout = pipeline.layout().descriptor_set_layout(0).unwrap();

                let clear = [0.0, 0.0, 0.05, 1.0];
                let clear_values = vec![clear.into(), clear.into()];

                let mut builder = AutoCommandBufferBuilder::primary_one_time_submit(
                    device.clone(),
                    queue.family(),
                )
                .unwrap();

                builder
                    .begin_render_pass(
                        framebuffers[image_num].clone(),
                        SubpassContents::Inline,
                        clear_values,
                    )
                    .unwrap();

                let spine_vertices = spines.iter().map(|s| s.vertices()).collect::<Vec<_>>();

                let spine_matrices = spines.iter().map(|s| s.model_matrix()).collect::<Vec<_>>();

                for ((vxs, cols), model) in spine_vertices.iter().zip(spine_matrices.iter()) {
                    let vertex_buffer = vertex_buffer_pool.chunk(vxs.iter().copied()).unwrap();
                    let color_buffer = color_buffer_pool.chunk(cols.iter().copied()).unwrap();

                    let transformation = {
                        let mat = view.to_scaled_matrix();

                        let mat = mat * model;

                        let view_data = view::mat4_to_array(&mat);

                        let matrix = simple_vert::ty::View { view: view_data };
                        uniform_buffer.next(matrix).unwrap()
                    };

                    let set = Arc::new(
                        PersistentDescriptorSet::start(layout.clone())
                            .add_buffer(transformation)
                            .unwrap()
                            .build()
                            .unwrap(),
                    );

                    builder
                        .draw(
                            pipeline.clone(),
                            &dynamic_state,
                            (vertex_buffer, color_buffer),
                            set.clone(),
                            (),
                        )
                        .unwrap();
                }

                builder.end_render_pass().unwrap();

                let command_buffer = builder.build().unwrap();

                let future = previous_frame_end
                    .take()
                    .unwrap()
                    .join(acquire_future)
                    .then_execute(queue.clone(), command_buffer)
                    .unwrap()
                    .then_swapchain_present(queue.clone(), swapchain.clone(), image_num)
                    .then_signal_fence_and_flush();

                match future {
                    Ok(future) => {
                        previous_frame_end = Some(future.boxed());
                    }
                    Err(FlushError::OutOfDate) => {
                        recreate_swapchain = true;
                        previous_frame_end = Some(sync::now(device.clone()).boxed());
                    }
                    Err(e) => {
                        println!("Failed to flush future: {:?}", e);
                        previous_frame_end = Some(sync::now(device.clone()).boxed());
                    }
                }
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
