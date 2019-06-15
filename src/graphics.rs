use std::ffi::CStr;
use std::os::raw::c_void;
use std::ptr;
use std::sync::Arc;

use ash::extensions::ext::DebugUtils;
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::{vk, Device, Entry, Instance};

use crate::defer;

pub struct Core {
    pub entry: Entry,
    pub instance: Instance,

    pub debug_utils: Option<DebugUtils>,
    messenger: vk::DebugUtilsMessengerEXT,
}

impl Drop for Core {
    fn drop(&mut self) {
        unsafe {
            if let Some(ref utils) = self.debug_utils {
                utils.destroy_debug_utils_messenger(self.messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

impl Core {
    pub fn new(exts: &[&CStr]) -> Self {
        let entry = Entry::new().unwrap();

        unsafe {
            let supported_exts = entry.enumerate_instance_extension_properties().unwrap();
            let has_debug = supported_exts
                .iter()
                .any(|x| CStr::from_ptr(x.extension_name.as_ptr()) == DebugUtils::name());

            let mut exts = exts.iter().map(|x| x.as_ptr()).collect::<Vec<_>>();
            if has_debug {
                exts.push(DebugUtils::name().as_ptr());
            }

            let name = cstr!("rustlike");

            let instance = entry
                .create_instance(
                    &vk::InstanceCreateInfo::builder()
                        .application_info(
                            &vk::ApplicationInfo::builder()
                                .application_name(name)
                                .application_version(0)
                                .engine_name(name)
                                .engine_version(0)
                                .api_version(0),
                        )
                        .enabled_extension_names(&exts),
                    None,
                )
                .unwrap();
            let instance_guard = defer(|| instance.destroy_instance(None));
            let messenger_guard;
            let debug_utils;
            let messenger;
            if has_debug {
                let utils = DebugUtils::new(&entry, &instance);
                messenger = utils
                    .create_debug_utils_messenger(
                        &vk::DebugUtilsMessengerCreateInfoEXT::builder()
                            .message_severity(
                                vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR, // | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                                                                                    // | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                            )
                            .message_type(
                                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                            )
                            .pfn_user_callback(Some(messenger_callback))
                            .user_data(ptr::null_mut()),
                        None,
                    )
                    .unwrap();
                debug_utils = Some(utils);
                messenger_guard = Some(defer(|| {
                    debug_utils
                        .as_ref()
                        .unwrap()
                        .destroy_debug_utils_messenger(messenger, None)
                }));
            } else {
                debug_utils = None;
                messenger_guard = None;
                messenger = vk::DebugUtilsMessengerEXT::null();
            }

            instance_guard.disarm();
            messenger_guard.map(|x| x.disarm());
            Self {
                entry,
                instance,
                debug_utils,
                messenger,
            }
        }
    }
}

unsafe extern "system" fn messenger_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut c_void,
) -> vk::Bool32 {
    let callback_data = &*p_callback_data;
    eprintln!(
        "{:?} {}",
        message_severity,
        CStr::from_ptr(callback_data.p_message).to_string_lossy()
    );
    vk::FALSE
}

pub struct Graphics {
    pub core: Arc<Core>,
    pub physical: vk::PhysicalDevice,
    pub device: Arc<Device>,
    pub queue_family: u32,
    pub queue: vk::Queue,
    pub memory_properties: vk::PhysicalDeviceMemoryProperties,
    pub pipeline_cache: vk::PipelineCache,
}

impl Drop for Graphics {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline_cache(self.pipeline_cache, None);
            self.device.destroy_device(None);
        }
    }
}

impl Graphics {
    pub fn new(
        core: Arc<Core>,
        pipeline_cache_data: &[u8],
        device_exts: &[&CStr],
        mut device_filter: impl FnMut(vk::PhysicalDevice, u32) -> bool,
    ) -> Option<Self> {
        unsafe {
            let instance = &core.instance;
            let (physical, queue_family_index) = instance
                .enumerate_physical_devices()
                .unwrap()
                .into_iter()
                .find_map(|physical| {
                    instance
                        .get_physical_device_queue_family_properties(physical)
                        .iter()
                        .enumerate()
                        .find_map(|(queue_family_index, ref info)| {
                            let supports_graphic_and_surface =
                                info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                                    && device_filter(physical, queue_family_index as u32);
                            match supports_graphic_and_surface {
                                true => Some((physical, queue_family_index as u32)),
                                _ => None,
                            }
                        })
                })?;

            let device_exts = device_exts.iter().map(|x| x.as_ptr()).collect::<Vec<_>>();

            let device = Arc::new(
                instance
                    .create_device(
                        physical,
                        &vk::DeviceCreateInfo::builder()
                            .queue_create_infos(&[vk::DeviceQueueCreateInfo::builder()
                                .queue_family_index(queue_family_index)
                                .queue_priorities(&[1.0])
                                .build()])
                            .enabled_extension_names(&device_exts),
                        None,
                    )
                    .unwrap(),
            );
            let queue = device.get_device_queue(queue_family_index, 0);
            let memory_properties = instance.get_physical_device_memory_properties(physical);
            let pipeline_cache = device
                .create_pipeline_cache(
                    &vk::PipelineCacheCreateInfo::builder().initial_data(pipeline_cache_data),
                    None,
                )
                .unwrap();

            Some(Self {
                core,
                physical,
                device,
                queue_family: queue_family_index,
                queue,
                memory_properties,
                pipeline_cache,
            })
        }
    }
}
