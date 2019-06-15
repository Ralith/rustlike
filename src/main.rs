use std::fs;
use std::sync::Arc;

use ash::extensions::khr::Swapchain;
use ash::version::DeviceV1_0;
use ash::vk;
use specs::RunNow;

use rustlike::*;

fn main() {
    let dirs = directories::ProjectDirs::from("", "", "rustlike").unwrap();
    let pipeline_cache_path = dirs.cache_dir().join("pipeline_cache");
    let pipeline_cache_data = fs::read(&pipeline_cache_path).unwrap_or_else(|_| vec![]);

    let mut events_loop = winit::EventsLoop::new();
    let core = Arc::new(graphics::Core::new(&window::Window::instance_exts()));
    let mut window_size = winit::dpi::LogicalSize::new(1280.0, 720.0);
    let window = Arc::new(window::Window::new(&events_loop, core.clone(), window_size));
    let gfx = Arc::new(
        graphics::Graphics::new(
            core,
            &pipeline_cache_data,
            &[Swapchain::name()],
            |physical, queue_family| window.supports(physical, queue_family),
        )
        .unwrap(),
    );
    drop(pipeline_cache_data);
    let mut swapchain = window::SwapchainMgr::new(window.clone(), gfx.clone());
    let mut render = render::Render::new(gfx.clone());
    unsafe {
        render.rebuild_framebuffers(
            swapchain.extent(),
            swapchain.frames().iter().map(|x| x.view),
        );
    }

    let mut state = state::State::new();

    let image_available = unsafe {
        gfx.device
            .create_semaphore(&Default::default(), None)
            .unwrap()
    };
    let render_complete = unsafe {
        gfx.device
            .create_semaphore(&Default::default(), None)
            .unwrap()
    };
    let _sem_guard = defer(|| unsafe {
        gfx.device.destroy_semaphore(image_available, None);
        gfx.device.destroy_semaphore(render_complete, None);
    });

    let mut running = true;
    while running {
        let mut suboptimal;
        unsafe {
            let image_index = loop {
                match swapchain.acquire_next_image(image_available) {
                    Ok((idx, sub)) => {
                        suboptimal = sub;
                        break idx;
                    }
                    Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                        swapchain.update();
                        render.rebuild_framebuffers(
                            swapchain.extent(),
                            swapchain.frames().iter().map(|x| x.view),
                        );
                    }
                    Err(e) => {
                        panic!("{}", e);
                    }
                }
            };
            let extent = swapchain.extent();
            render.set_scissors(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            });
            render.set_viewport(vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            });
            render.set_fb_index(image_index);
            render.run_now(&state.world.res);
            gfx.device
                .queue_submit(
                    gfx.queue,
                    &[vk::SubmitInfo::builder()
                        .wait_semaphores(&[image_available])
                        .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
                        .command_buffers(&[render.cmd()])
                        .signal_semaphores(&[render_complete])
                        .build()],
                    vk::Fence::null(),
                )
                .unwrap();
            match swapchain.queue_present(gfx.queue, render_complete, image_index) {
                Ok(false) => {}
                Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    suboptimal = true;
                }
                Err(e) => panic!("{}", e),
            };
            gfx.device.queue_wait_idle(gfx.queue).unwrap(); // FIXME
        }
        events_loop.poll_events(|e| {
            use winit::{ElementState, Event, MouseButton, WindowEvent};
            match e {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        running = false;
                    }
                    WindowEvent::Resized(size) => {
                        suboptimal = true;
                        window_size = size;
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        let f = window.window.get_hidpi_factor() as f32;
                        state.move_cursor(&(na::Vector2::new(
                            (position.x - window_size.width / 2.0) as f32,
                            -(position.y - window_size.height / 2.0) as f32,
                        ) * f));
                    }
                    WindowEvent::MouseInput {
                        button: MouseButton::Left,
                        state: s,
                        ..
                    } => {
                        state.cursor_pressed(s == ElementState::Pressed);
                    }
                    _ => {}
                },
                _ => {}
            }
        });
        if suboptimal {
            unsafe {
                swapchain.update();
                render.rebuild_framebuffers(
                    swapchain.extent(),
                    swapchain.frames().iter().map(|x| x.view),
                );
            }
        }
        state.step();
    }
    let pipeline_cache_data = unsafe {
        gfx.device
            .get_pipeline_cache_data(gfx.pipeline_cache)
            .unwrap()
    };
    if let Err(e) = fs::create_dir_all(dirs.cache_dir())
        .and_then(|()| fs::write(&pipeline_cache_path, &pipeline_cache_data))
    {
        eprintln!("failed to save pipeline cache: {}", e);
    }
}
