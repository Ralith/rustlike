use std::ffi::CStr;
#[cfg(target_os = "macos")]
use std::mem;
use std::ops::Drop;
use std::sync::Arc;

#[cfg(target_os = "macos")]
use cocoa::appkit::{NSView, NSWindow};
#[cfg(target_os = "macos")]
use cocoa::base::id as cocoa_id;
#[cfg(target_os = "macos")]
use metal_rs::CoreAnimationLayer;
#[cfg(target_os = "macos")]
use objc::runtime::YES;

#[cfg(target_os = "windows")]
use ash::extensions::khr::Win32Surface;
#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
use ash::extensions::khr::XlibSurface;
use ash::extensions::khr::{Surface, Swapchain};
#[cfg(target_os = "macos")]
use ash::extensions::mvk::MacOSSurface;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::vk;

use crate::graphics::{Core, Graphics};

pub struct Window {
    _core: Arc<Core>,
    pub window: winit::Window,
    surface_loader: Surface,
    surface: vk::SurfaceKHR,
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            self.surface_loader.destroy_surface(self.surface, None);
        }
    }
}

impl Window {
    pub fn instance_exts() -> Vec<&'static CStr> {
        #[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
        let x = XlibSurface::name();
        #[cfg(target_os = "macos")]
        let x = MacOSSurface::name();
        #[cfg(windows)]
        let x = Win32Surface::name();
        vec![Surface::name(), x]
    }

    pub fn new(events_loop: &winit::EventsLoop, core: Arc<Core>, size: winit::dpi::LogicalSize) -> Self {
        let window = winit::WindowBuilder::new()
            .with_title("rustlike")
            .with_dimensions(size)
            .build(&events_loop)
            .unwrap();

        unsafe {
            let surface = create_surface(&core.entry, &core.instance, &window).unwrap();
            let surface_loader = Surface::new(&core.entry, &core.instance);

            Self {
                _core: core,
                window,
                surface_loader,
                surface,
            }
        }
    }

    pub fn supports(&self, physical: vk::PhysicalDevice, queue_family_index: u32) -> bool {
        unsafe {
            self.surface_loader.get_physical_device_surface_support(
                physical,
                queue_family_index,
                self.surface,
            )
        }
    }
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
unsafe fn create_surface<E: EntryV1_0, I: InstanceV1_0>(
    entry: &E,
    instance: &I,
    window: &winit::Window,
) -> Result<vk::SurfaceKHR, vk::Result> {
    use winit::os::unix::WindowExt;
    let x11_display = window.get_xlib_display().unwrap();
    let x11_window = window.get_xlib_window().unwrap();
    let x11_create_info = vk::XlibSurfaceCreateInfoKHR::builder()
        .window(x11_window)
        .dpy(x11_display as *mut vk::Display);

    let xlib_surface_loader = XlibSurface::new(entry, instance);
    xlib_surface_loader.create_xlib_surface(&x11_create_info, None)
}

#[cfg(target_os = "macos")]
unsafe fn create_surface<E: EntryV1_0, I: InstanceV1_0>(
    entry: &E,
    instance: &I,
    window: &winit::Window,
) -> Result<vk::SurfaceKHR, vk::Result> {
    use std::ptr;
    use winit::os::macos::WindowExt;

    let wnd: cocoa_id = mem::transmute(window.get_nswindow());

    let layer = CoreAnimationLayer::new();

    layer.set_edge_antialiasing_mask(0);
    layer.set_presents_with_transaction(false);
    layer.remove_all_animations();

    let view = wnd.contentView();

    layer.set_contents_scale(view.backingScaleFactor());
    view.setLayer(mem::transmute(layer.as_ref()));
    view.setWantsLayer(YES);

    let create_info = vk::MacOSSurfaceCreateInfoMVK {
        s_type: vk::StructureType::MACOS_SURFACE_CREATE_INFO_M,
        p_next: ptr::null(),
        flags: Default::default(),
        p_view: window.get_nsview() as *const c_void,
    };

    let macos_surface_loader = MacOSSurface::new(entry, instance);
    macos_surface_loader.create_mac_os_surface_mvk(&create_info, None)
}

#[cfg(target_os = "windows")]
unsafe fn create_surface<E: EntryV1_0, I: InstanceV1_0>(
    entry: &E,
    instance: &I,
    window: &winit::Window,
) -> Result<vk::SurfaceKHR, vk::Result> {
    use std::ptr;
    use winapi::shared::windef::HWND;
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winit::os::windows::WindowExt;

    let hwnd = window.get_hwnd() as HWND;
    let hinstance = GetModuleHandleW(ptr::null()) as *const c_void;
    let win32_create_info = vk::Win32SurfaceCreateInfoKHR {
        s_type: vk::StructureType::WIN32_SURFACE_CREATE_INFO_KHR,
        p_next: ptr::null(),
        flags: Default::default(),
        hinstance: hinstance,
        hwnd: hwnd as *const c_void,
    };
    let win32_surface_loader = Win32Surface::new(entry, instance);
    win32_surface_loader.create_win32_surface(&win32_create_info, None)
}

pub struct SwapchainMgr {
    format: vk::SurfaceFormatKHR,
    state: SwapchainState,
}

impl SwapchainMgr {
    pub fn new(window: Arc<Window>, gfx: Arc<Graphics>) -> Self {
        let surface_formats = unsafe {
            window
                .surface_loader
                .get_physical_device_surface_formats(gfx.physical, window.surface)
                .unwrap()
        };
        let desired_format = vk::SurfaceFormatKHR {
            format: vk::Format::B8G8R8A8_SRGB,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
        };
        if (surface_formats.len() != 1
            || (surface_formats[0].format != vk::Format::UNDEFINED
                || surface_formats[0].color_space != desired_format.color_space))
            && surface_formats.iter().all(|x| {
                x.format != desired_format.format || x.color_space != desired_format.color_space
            })
        {
            panic!("no SRGBA8 surface format: {:?}", surface_formats);
        }

        Self {
            state: unsafe { SwapchainState::new(window, gfx, desired_format, None) },
            format: desired_format,
        }
    }

    /// Recreate the swapchain based on the window's current capabilities
    ///
    /// # Safety
    /// - There must be no current or future operations on the swapchain
    pub unsafe fn update(&mut self) {
        self.state = SwapchainState::new(
            self.state.window.clone(),
            self.state.gfx.clone(),
            self.format,
            Some(&self.state),
        );
    }

    pub unsafe fn acquire_next_image(
        &self,
        signal_sem: vk::Semaphore,
    ) -> Result<(u32, bool), vk::Result> {
        self.state.loader.acquire_next_image(
            self.state.handle,
            std::u64::MAX,
            signal_sem,
            vk::Fence::null(),
        )
    }

    pub unsafe fn queue_present(
        &self,
        queue: vk::Queue,
        wait_sem: vk::Semaphore,
        index: u32,
    ) -> Result<bool, vk::Result> {
        self.state.loader.queue_present(
            queue,
            &vk::PresentInfoKHR::builder()
                .wait_semaphores(&[wait_sem])
                .swapchains(&[self.state.handle])
                .image_indices(&[index]),
        )
    }

    pub fn extent(&self) -> vk::Extent2D {
        self.state.extent
    }

    pub fn frames(&self) -> &[Frame] {
        &self.state.frames
    }
}

struct SwapchainState {
    window: Arc<Window>,
    gfx: Arc<Graphics>,
    extent: vk::Extent2D,
    handle: vk::SwapchainKHR,
    loader: Arc<Swapchain>,
    frames: Vec<Frame>,
}

impl SwapchainState {
    unsafe fn new(
        window: Arc<Window>,
        gfx: Arc<Graphics>,
        format: vk::SurfaceFormatKHR,
        old: Option<&Self>,
    ) -> Self {
        let device = &*gfx.device;
        let loader = old.map_or_else(
            || Arc::new(Swapchain::new(&gfx.core.instance, &*device)),
            |x| x.loader.clone(),
        );
        let capabilities = window
            .surface_loader
            .get_physical_device_surface_capabilities(gfx.physical, window.surface)
            .unwrap();

        let surface_capabilities = window
            .surface_loader
            .get_physical_device_surface_capabilities(gfx.physical, window.surface)
            .unwrap();
        let extent = match surface_capabilities.current_extent.width {
            std::u32::MAX => vk::Extent2D {
                width: 1280,
                height: 1024,
            },
            _ => surface_capabilities.current_extent,
        };
        let pre_transform = if surface_capabilities
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            surface_capabilities.current_transform
        };
        let present_modes = window
            .surface_loader
            .get_physical_device_surface_present_modes(gfx.physical, window.surface)
            .unwrap();
        let present_mode = present_modes
            .iter()
            .cloned()
            .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);

        let image_count = if capabilities.max_image_count > 0 {
            capabilities
                .max_image_count
                .min(capabilities.min_image_count + 1)
        } else {
            capabilities.min_image_count + 1
        };

        let handle = loader
            .create_swapchain(
                &vk::SwapchainCreateInfoKHR::builder()
                    .surface(window.surface)
                    .min_image_count(image_count)
                    .image_color_space(format.color_space)
                    .image_format(format.format)
                    .image_extent(extent)
                    .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                    .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .pre_transform(pre_transform)
                    .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                    .present_mode(present_mode)
                    .clipped(true)
                    .image_array_layers(1)
                    .old_swapchain(old.map_or_else(vk::SwapchainKHR::null, |x| x.handle)),
                None,
            )
            .unwrap();

        let frames = loader
            .get_swapchain_images(handle)
            .unwrap()
            .into_iter()
            .map(|image| {
                let view = device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::builder()
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(format.format)
                            .components(vk::ComponentMapping {
                                r: vk::ComponentSwizzle::R,
                                g: vk::ComponentSwizzle::G,
                                b: vk::ComponentSwizzle::B,
                                a: vk::ComponentSwizzle::A,
                            })
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            })
                            .image(image),
                        None,
                    )
                    .unwrap();
                Frame { image, view }
            })
            .collect();

        Self {
            window,
            gfx,
            extent,
            handle,
            loader,
            frames,
        }
    }
}

impl Drop for SwapchainState {
    fn drop(&mut self) {
        unsafe {
            for frame in &self.frames {
                self.gfx.device.destroy_image_view(frame.view, None);
            }
            self.loader.destroy_swapchain(self.handle, None);
        }
    }
}

pub struct Frame {
    pub image: vk::Image,
    pub view: vk::ImageView,
}
