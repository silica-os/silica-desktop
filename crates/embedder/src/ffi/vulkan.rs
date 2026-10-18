//! The engine's Vulkan renderer: a `VK_KHR_wayland_surface` swapchain on every view's
//! `wl_surface`. The driver (Mesa, NVIDIA) passes the buffers to the compositor as dmabufs,
//! as EGL would: the engine draws straight into the images the compositor shows.
//!
//! One view (an app window, a shell layer): the engine calls the root surface callbacks from
//! the raster thread. `next_image` takes an image of the swapchain (rebuilt if the size
//! changes), Impeller draws on it, `present` brings it to the presentation state and presents
//! it.
//!
//! Several views (silica-desktop#146, #203): the engine draws every view into a backing store
//! (`compositor.rs`). The backing store of a view is the next image of that view's swapchain
//! ([`VulkanSurface::acquire_target`]); `present_view` presents it
//! ([`VulkanSurface::present_target`]). No copy: the image the engine drew is the buffer
//! Ossido receives. The engine asks for a backing store every frame
//! (`avoid_backing_store_cache`), so it never draws into an image the compositor still holds.
//!
//! Presentation does not wait for the frame callback (MAILBOX or IMMEDIATE if available): the
//! pace comes from the engine's vsync, as with EGL.
//!
//! The queue is used by the engine (raster and IO threads) and by the embedder (presents,
//! swapchains of views removed from the platform thread): every use goes through [`QUEUE`],
//! and the engine's entry points for the queue are wrapped to take it (`embedder.h`, the
//! `get_instance_proc_address_callback` of `FlutterVulkanRendererConfig`).
//!
//! The integrated GPU is preferred (it uses less power and on a laptop keeps the discrete one
//! off); `SILICA_VULKAN_GPU=discrete` chooses the discrete one.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use ash::vk::{self, Handle};
use wayland_client::Connection;
use wayland_client::Proxy;
use wayland_client::protocol::wl_surface::WlSurface;

use super::Error;

fn vk_error(call: &'static str) -> impl Fn(vk::Result) -> Error {
    move |err| Error::Gl(call, format!("{err:?}"))
}

/// Instance and device extensions, to be declared to the engine too.
const INSTANCE_EXTENSIONS: [&CStr; 2] = [ash::khr::surface::NAME, ash::khr::wayland_surface::NAME];
const DEVICE_EXTENSIONS: [&CStr; 1] = [ash::khr::swapchain::NAME];

/// The lock of the one queue of the process (one engine, one device per process).
static QUEUE: Mutex<()> = Mutex::new(());

/// Takes the queue lock; a thread that panicked holding it left no state behind it.
fn queue_lock() -> MutexGuard<'static, ()> {
    QUEUE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The real entry points behind the engine's queue functions, resolved once.
#[derive(Clone, Copy)]
struct QueueFns {
    get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    get_device_proc_addr: Option<vk::PFN_vkGetDeviceProcAddr>,
    submit: Option<vk::PFN_vkQueueSubmit>,
    submit2: Option<vk::PFN_vkQueueSubmit2>,
    submit2_khr: Option<vk::PFN_vkQueueSubmit2>,
    wait_idle: Option<vk::PFN_vkQueueWaitIdle>,
    bind_sparse: Option<vk::PFN_vkQueueBindSparse>,
    present: Option<vk::PFN_vkQueuePresentKHR>,
}

static REAL: OnceLock<QueueFns> = OnceLock::new();

impl QueueFns {
    /// Resolves the queue functions through the loader's `vkGetInstanceProcAddr`: its
    /// trampolines dispatch on the queue, whatever the device.
    fn resolve(gipa: vk::PFN_vkGetInstanceProcAddr, instance: vk::Instance) -> Self {
        let get = |name: &CStr| {
            // SAFETY: gipa is the loader's vkGetInstanceProcAddr and instance a live instance
            // of it; name is a valid C string.
            unsafe { gipa(instance, name.as_ptr()) }
        };
        // SAFETY: (all the transmutes) the loader returns, for each name, a pointer to the function
        // with that name's signature, which is the PFN type it is converted to.
        unsafe {
            Self {
                get_instance_proc_addr: gipa,
                get_device_proc_addr: get(c"vkGetDeviceProcAddr")
                    .map(|f| std::mem::transmute::<_, vk::PFN_vkGetDeviceProcAddr>(f)),
                submit: get(c"vkQueueSubmit")
                    .map(|f| std::mem::transmute::<_, vk::PFN_vkQueueSubmit>(f)),
                submit2: get(c"vkQueueSubmit2")
                    .map(|f| std::mem::transmute::<_, vk::PFN_vkQueueSubmit2>(f)),
                submit2_khr: get(c"vkQueueSubmit2KHR")
                    .map(|f| std::mem::transmute::<_, vk::PFN_vkQueueSubmit2>(f)),
                wait_idle: get(c"vkQueueWaitIdle")
                    .map(|f| std::mem::transmute::<_, vk::PFN_vkQueueWaitIdle>(f)),
                bind_sparse: get(c"vkQueueBindSparse")
                    .map(|f| std::mem::transmute::<_, vk::PFN_vkQueueBindSparse>(f)),
                present: get(c"vkQueuePresentKHR")
                    .map(|f| std::mem::transmute::<_, vk::PFN_vkQueuePresentKHR>(f)),
            }
        }
    }
}

/// The wrapper the engine receives for `name` instead of the real function, if `name` uses the
/// queue (or resolves functions that do) and the real one exists.
fn locked_entry(name: &[u8]) -> Option<*mut c_void> {
    let real = REAL.get()?;
    match name {
        b"vkQueueSubmit" => real.submit.map(|_| locked_submit as *mut c_void),
        b"vkQueueSubmit2" => real.submit2.map(|_| locked_submit2 as *mut c_void),
        b"vkQueueSubmit2KHR" => real.submit2_khr.map(|_| locked_submit2_khr as *mut c_void),
        b"vkQueueWaitIdle" => real.wait_idle.map(|_| locked_wait_idle as *mut c_void),
        b"vkQueueBindSparse" => real.bind_sparse.map(|_| locked_bind_sparse as *mut c_void),
        b"vkQueuePresentKHR" => real.present.map(|_| locked_present as *mut c_void),
        b"vkGetDeviceProcAddr" => real
            .get_device_proc_addr
            .map(|_| wrapped_get_device_proc_addr as *mut c_void),
        b"vkGetInstanceProcAddr" => Some(wrapped_get_instance_proc_addr as *mut c_void),
        _ => None,
    }
}

/// `vkGetDeviceProcAddr` for the engine: the queue functions come back wrapped.
unsafe extern "system" fn wrapped_get_device_proc_addr(
    device: vk::Device,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    if name.is_null() {
        return None;
    }
    // SAFETY: the caller passes a NUL-terminated function name.
    let wanted = unsafe { CStr::from_ptr(name) };
    if let Some(entry) = locked_entry(wanted.to_bytes()) {
        // SAFETY: entry is one of the `extern "system"` functions of this module; the caller
        // converts it back to the signature of the name it asked for, which it has.
        return Some(unsafe {
            std::mem::transmute::<*mut c_void, unsafe extern "system" fn()>(entry)
        });
    }
    let real = REAL.get()?.get_device_proc_addr?;
    // SAFETY: the real vkGetDeviceProcAddr with the caller's arguments.
    unsafe { real(device, name) }
}

/// `vkGetInstanceProcAddr` for the engine, if it asks for it by name.
unsafe extern "system" fn wrapped_get_instance_proc_addr(
    instance: vk::Instance,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    if name.is_null() {
        return None;
    }
    // SAFETY: the caller passes a NUL-terminated function name.
    let wanted = unsafe { CStr::from_ptr(name) };
    if let Some(entry) = locked_entry(wanted.to_bytes()) {
        // SAFETY: as in `wrapped_get_device_proc_addr`.
        return Some(unsafe {
            std::mem::transmute::<*mut c_void, unsafe extern "system" fn()>(entry)
        });
    }
    let real = REAL.get()?.get_instance_proc_addr;
    // SAFETY: the real vkGetInstanceProcAddr with the caller's arguments.
    unsafe { real(instance, name) }
}

unsafe extern "system" fn locked_submit(
    queue: vk::Queue,
    count: u32,
    submits: *const vk::SubmitInfo<'_>,
    fence: vk::Fence,
) -> vk::Result {
    let Some(real) = REAL.get().and_then(|f| f.submit) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let _queue = queue_lock();
    // SAFETY: the real function with the caller's arguments, under the queue lock.
    unsafe { real(queue, count, submits, fence) }
}

unsafe extern "system" fn locked_submit2(
    queue: vk::Queue,
    count: u32,
    submits: *const vk::SubmitInfo2<'_>,
    fence: vk::Fence,
) -> vk::Result {
    let Some(real) = REAL.get().and_then(|f| f.submit2) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let _queue = queue_lock();
    // SAFETY: as in `locked_submit`.
    unsafe { real(queue, count, submits, fence) }
}

unsafe extern "system" fn locked_submit2_khr(
    queue: vk::Queue,
    count: u32,
    submits: *const vk::SubmitInfo2<'_>,
    fence: vk::Fence,
) -> vk::Result {
    let Some(real) = REAL.get().and_then(|f| f.submit2_khr) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let _queue = queue_lock();
    // SAFETY: as in `locked_submit`.
    unsafe { real(queue, count, submits, fence) }
}

unsafe extern "system" fn locked_wait_idle(queue: vk::Queue) -> vk::Result {
    let Some(real) = REAL.get().and_then(|f| f.wait_idle) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let _queue = queue_lock();
    // SAFETY: as in `locked_submit`.
    unsafe { real(queue) }
}

unsafe extern "system" fn locked_bind_sparse(
    queue: vk::Queue,
    count: u32,
    info: *const vk::BindSparseInfo<'_>,
    fence: vk::Fence,
) -> vk::Result {
    let Some(real) = REAL.get().and_then(|f| f.bind_sparse) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let _queue = queue_lock();
    // SAFETY: as in `locked_submit`.
    unsafe { real(queue, count, info, fence) }
}

unsafe extern "system" fn locked_present(
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR<'_>,
) -> vk::Result {
    let Some(real) = REAL.get().and_then(|f| f.present) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let _queue = queue_lock();
    // SAFETY: as in `locked_submit`.
    unsafe { real(queue, info) }
}

/// A swapchain and what is needed to present its images.
struct Swap {
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    extent: vk::Extent2D,
    format: vk::Format,
    /// Commands that bring every image from the layout the renderer leaves it in to
    /// PRESENT_SRC_KHR.
    transitions: Vec<vk::CommandBuffer>,
    /// The image taken and not yet presented. A backing store dropped without being presented
    /// (a frame the engine discarded) leaves it here: the next frame of the view draws on it.
    current: Option<u32>,
}

impl Swap {
    const EMPTY: Swap = Swap {
        swapchain: vk::SwapchainKHR::null(),
        images: Vec::new(),
        extent: vk::Extent2D {
            width: 0,
            height: 0,
        },
        format: vk::Format::UNDEFINED,
        transitions: Vec::new(),
        current: None,
    };
}

/// The Vulkan surface and swapchain of one view.
struct Chain {
    surface: vk::SurfaceKHR,
    swap: Swap,
    /// Handed out as a backing store and not given back yet: a view has one layer (no
    /// platform views), so a second backing store in the same frame is refused.
    outstanding: bool,
    /// Drawn and waiting to be presented with the other views of the frame.
    queued: Option<u32>,
    /// The size in pixels the engine was last given for the view (its window metrics), for a
    /// transparent frame before the engine drew one ([`VulkanSurface::map_empty`]).
    size: (u32, u32),
}

/// What the engine needs to draw a view's frame into a swapchain image (`compositor.rs`).
pub struct VulkanTarget {
    pub view_id: i64,
    pub image: u64,
    pub format: u32,
}

/// The app's instance, device and swapchains. Lives in [`super::Callbacks`] while the engine
/// runs.
pub struct VulkanSurface {
    entry: ash::Entry,
    /// The drivers loaded here for the instance (`create_instance`), kept until the end.
    _drivers: Vec<libloading::Library>,
    instance: ash::Instance,
    physical: vk::PhysicalDevice,
    device: ash::Device,
    queue_family: u32,
    queue: vk::Queue,
    surface_fn: ash::khr::surface::Instance,
    wayland_fn: ash::khr::wayland_surface::Instance,
    swapchain_fn: ash::khr::swapchain::Device,
    /// The connection's `wl_display`, for the surfaces of the views added later.
    display: usize,
    pool: vk::CommandPool,
    acquired: vk::Fence,
    transitioned: vk::Semaphore,
    /// The swapchain of every view, the main one ([`MAIN_VIEW`]) included. Used on the raster
    /// thread; views are added and removed on the platform thread.
    chains: Mutex<BTreeMap<i64, Chain>>,
    /// The views that have presented at least one frame.
    presented: Mutex<BTreeSet<i64>>,
    main_presented: AtomicBool,
    /// The layout the renderer leaves its images in (`VkImageLayout`): COLOR_ATTACHMENT_OPTIMAL
    /// for Impeller's root surface and for Skia, GENERAL for Impeller's backing stores.
    drawn_layout: AtomicI32,
    instance_extensions: Vec<*const c_char>,
    device_extensions: Vec<*const c_char>,
    name: String,
}

/// The view of the root surface.
const MAIN_VIEW: i64 = 0;

// SAFETY: instance, device, queue and Vulkan objects are handles the API allows to use from
// any thread as long as access to the queue and the swapchains is synchronized externally:
// the queue goes through QUEUE (the engine's calls too, `locked_entry`), the swapchains sit
// behind the `chains` Mutex. The extension pointers point to static constants; `display` is
// the address of the connection's wl_display, which outlives the engine.
unsafe impl Send for VulkanSurface {}
// SAFETY: see above.
unsafe impl Sync for VulkanSurface {}

impl VulkanSurface {
    /// Instance, device and swapchain for `surface`, of `size` physical pixels.
    pub fn new(conn: &Connection, surface: &WlSurface, size: (u32, u32)) -> Result<Self, Error> {
        // SAFETY: loads the system libvulkan (the Khronos loader), which exposes the standard
        // API checked by ash.
        let entry =
            unsafe { ash::Entry::load() }.map_err(|err| Error::Gl("libvulkan", err.to_string()))?;
        crate::trace::mark("vulkan loader loaded");
        let extensions: Vec<*const c_char> =
            INSTANCE_EXTENSIONS.iter().map(|e| e.as_ptr()).collect();
        let (instance, drivers) = create_instance(&entry, &extensions)?;
        crate::trace::mark("vulkan instance created");

        let wayland_fn = ash::khr::wayland_surface::Instance::new(&entry, &instance);
        let surface_fn = ash::khr::surface::Instance::new(&entry, &instance);
        let display = conn.backend().display_ptr() as usize;
        let vk_surface = create_surface(&wayland_fn, display, surface)?;

        let (physical, queue_family, name) = pick_device(&instance, &surface_fn, vk_surface)
            .ok_or(Error::Gl(
                "vkEnumeratePhysicalDevices",
                "no Vulkan GPU presents on this surface".to_owned(),
            ))?;
        crate::trace::mark("vulkan device picked");
        let priorities = [1.0f32];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&priorities)];
        let device_extensions: Vec<*const c_char> =
            DEVICE_EXTENSIONS.iter().map(|e| e.as_ptr()).collect();
        let device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&device_extensions);
        // SAFETY: physical comes from this instance; device_info is valid for the call.
        let device = unsafe { instance.create_device(physical, &device_info, None) }
            .map_err(vk_error("vkCreateDevice"))?;
        crate::trace::mark("vulkan device created");
        // SAFETY: the family was created with one queue (index 0).
        let queue = unsafe { device.get_device_queue(queue_family, 0) };
        let swapchain_fn = ash::khr::swapchain::Device::new(&instance, &device);
        // One engine per process: the first instance's entry points serve every later lookup.
        let _ = REAL.set(QueueFns::resolve(
            entry.static_fn().get_instance_proc_addr,
            instance.handle(),
        ));

        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        // SAFETY: valid device, valid structure.
        let pool = unsafe { device.create_command_pool(&pool_info, None) }
            .map_err(vk_error("vkCreateCommandPool"))?;
        // SAFETY: as above.
        let acquired = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }
            .map_err(vk_error("vkCreateFence"))?;
        // SAFETY: as above.
        let transitioned =
            unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }
                .map_err(vk_error("vkCreateSemaphore"))?;
        let main = Chain {
            surface: vk_surface,
            swap: Swap::EMPTY,
            outstanding: false,
            queued: None,
            size: (0, 0),
        };
        let this = Self {
            entry,
            _drivers: drivers,
            instance,
            physical,
            device,
            queue_family,
            queue,
            surface_fn,
            wayland_fn,
            swapchain_fn,
            display,
            pool,
            acquired,
            transitioned,
            chains: Mutex::new(BTreeMap::from([(MAIN_VIEW, main)])),
            presented: Mutex::new(BTreeSet::new()),
            main_presented: AtomicBool::new(false),
            drawn_layout: AtomicI32::new(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL.as_raw()),
            instance_extensions: extensions,
            device_extensions,
            name,
        };
        if let Ok(mut chains) = this.chains.lock()
            && let Some(chain) = chains.get_mut(&MAIN_VIEW)
        {
            this.rebuild(chain, size)?;
        }
        Ok(this)
    }

    /// The chosen GPU, for the log and the measurements.
    pub fn renderer(&self) -> &str {
        &self.name
    }

    /// The main view has presented a frame.
    pub fn presented(&self) -> bool {
        self.main_presented.load(Ordering::Acquire)
    }

    /// Whether view `view_id` has presented a frame.
    pub fn view_presented(&self, view_id: i64) -> bool {
        if view_id == MAIN_VIEW {
            return self.presented();
        }
        self.presented
            .lock()
            .map(|views| views.contains(&view_id))
            .unwrap_or(false)
    }

    pub(super) fn handles(&self) -> (*mut c_void, *mut c_void, *mut c_void, u32, *mut c_void) {
        (
            self.instance.handle().as_raw() as *mut c_void,
            self.physical.as_raw() as *mut c_void,
            self.device.handle().as_raw() as *mut c_void,
            self.queue_family,
            self.queue.as_raw() as *mut c_void,
        )
    }

    pub(super) fn instance_extensions(&self) -> &[*const c_char] {
        &self.instance_extensions
    }

    pub(super) fn device_extensions(&self) -> &[*const c_char] {
        &self.device_extensions
    }

    /// `vkGetInstanceProcAddr` for the engine; the functions that use the queue come back
    /// wrapped to take [`QUEUE`].
    pub(super) fn proc_address(&self, instance: *mut c_void, name: &CStr) -> *mut c_void {
        if let Some(entry) = locked_entry(name.to_bytes()) {
            return entry;
        }
        let instance = vk::Instance::from_raw(instance as u64);
        // SAFETY: name is a valid C string; instance is null or the one created here (the engine
        // passes the one we gave it).
        let f = unsafe { (self.entry.static_fn().get_instance_proc_addr)(instance, name.as_ptr()) };
        f.map_or(std::ptr::null_mut(), |f| f as *mut c_void)
    }

    /// The engine composes several views through backing stores (`compositor.rs`): every view
    /// presents from its own swapchain. `impeller`: the renderer is Impeller, which leaves its
    /// backing stores in GENERAL; Skia leaves them in COLOR_ATTACHMENT_OPTIMAL.
    pub fn set_multi_view(&self, impeller: bool) {
        let layout = if impeller {
            vk::ImageLayout::GENERAL
        } else {
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        };
        self.drawn_layout.store(layout.as_raw(), Ordering::Release);
        // The transitions recorded for the main swapchain assumed the root surface's layout:
        // rebuilt at the first frame.
        if let Ok(mut chains) = self.chains.lock()
            && let Some(chain) = chains.get_mut(&MAIN_VIEW)
        {
            chain.swap.extent = vk::Extent2D::default();
        }
    }

    /// A Vulkan surface for view `view_id` on its `wl_surface` (several windows or layers).
    /// Its swapchain is built at its first frame, with the size the engine draws it at.
    pub fn add_view(&self, view_id: i64, surface: &WlSurface) -> Result<(), Error> {
        let vk_surface = create_surface(&self.wayland_fn, self.display, surface)?;
        // SAFETY: family index of this device, surface of this instance.
        let supported = unsafe {
            self.surface_fn.get_physical_device_surface_support(
                self.physical,
                self.queue_family,
                vk_surface,
            )
        }
        .unwrap_or(false);
        if !supported {
            // SAFETY: the surface was just created and nothing uses it.
            unsafe { self.surface_fn.destroy_surface(vk_surface, None) };
            return Err(Error::Gl(
                "vkGetPhysicalDeviceSurfaceSupportKHR",
                format!("the GPU does not present on view {view_id}"),
            ));
        }
        let chain = Chain {
            surface: vk_surface,
            swap: Swap::EMPTY,
            outstanding: false,
            queued: None,
            size: (0, 0),
        };
        let old = self
            .chains
            .lock()
            .ok()
            .and_then(|mut chains| chains.insert(view_id, chain));
        if let Some(old) = old {
            self.destroy_chain(old);
        }
        Ok(())
    }

    /// The size in pixels view `view_id` is given (its window metrics), before its first frame
    /// too.
    pub fn set_view_size(&self, view_id: i64, size: (u32, u32)) {
        if let Ok(mut chains) = self.chains.lock()
            && let Some(chain) = chains.get_mut(&view_id)
        {
            chain.size = size;
        }
    }

    /// Maps view `view_id` with a transparent frame at its size if it has shown nothing yet
    /// (its scene was empty from the start): a shell layer that takes input on part of its
    /// surface needs to be on screen for it. Nothing if the view already showed a frame, is
    /// drawing one, or has no size yet. Whether it presented.
    pub fn map_empty(&self, view_id: i64) -> bool {
        let Ok(mut chains) = self.chains.lock() else {
            return false;
        };
        let Some(chain) = chains.get_mut(&view_id) else {
            return false;
        };
        let extent = chain.swap.extent;
        let size = chain.size;
        if extent.width > 0 && extent.height > 0 || size.0 == 0 || size.1 == 0 || chain.outstanding
        {
            return false;
        }
        self.clear_chain(chain, size)
    }

    /// Destroys the swapchain and the surface of view `view_id`, after the engine removed the
    /// view and before its `wl_surface` goes.
    pub fn remove_view(&self, view_id: i64) {
        if view_id == MAIN_VIEW {
            return;
        }
        let chain = self
            .chains
            .lock()
            .ok()
            .and_then(|mut chains| chains.remove(&view_id));
        if let Some(chain) = chain {
            self.destroy_chain(chain);
        }
        if let Ok(mut presented) = self.presented.lock() {
            presented.remove(&view_id);
        }
    }

    fn destroy_chain(&self, chain: Chain) {
        let _queue = queue_lock();
        // SAFETY: under the queue lock nobody submits; once the queue is idle no work uses the
        // swapchain's images or its commands. The surface goes after its swapchain.
        unsafe {
            let _ = self.device.queue_wait_idle(self.queue);
            if !chain.swap.transitions.is_empty() {
                self.device
                    .free_command_buffers(self.pool, &chain.swap.transitions);
            }
            if chain.swap.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_fn
                    .destroy_swapchain(chain.swap.swapchain, None);
            }
            self.surface_fn.destroy_surface(chain.surface, None);
        }
    }

    /// Rebuilds `chain`'s swapchain for `size` physical pixels (the old one becomes
    /// `old_swapchain`).
    fn rebuild(&self, chain: &mut Chain, size: (u32, u32)) -> Result<(), Error> {
        let surface = chain.surface;
        chain.queued = None;
        let swap = &mut chain.swap;
        // SAFETY: the calls use a valid device and surface of this instance.
        let (caps, formats, modes) = unsafe {
            (
                self.surface_fn
                    .get_physical_device_surface_capabilities(self.physical, surface)
                    .map_err(vk_error("vkGetPhysicalDeviceSurfaceCapabilitiesKHR"))?,
                self.surface_fn
                    .get_physical_device_surface_formats(self.physical, surface)
                    .map_err(vk_error("vkGetPhysicalDeviceSurfaceFormatsKHR"))?,
                self.surface_fn
                    .get_physical_device_surface_present_modes(self.physical, surface)
                    .map_err(vk_error("vkGetPhysicalDeviceSurfacePresentModesKHR"))?,
            )
        };
        let format = formats
            .iter()
            .find(|f| f.format == vk::Format::B8G8R8A8_UNORM)
            .or_else(|| {
                formats
                    .iter()
                    .find(|f| f.format == vk::Format::R8G8B8A8_UNORM)
            })
            .or(formats.first())
            .copied()
            .ok_or(Error::Gl(
                "vkGetPhysicalDeviceSurfaceFormatsKHR",
                "no format".to_owned(),
            ))?;
        // The pace comes from the engine's vsync: no waiting for the frame callback.
        let mode = [vk::PresentModeKHR::MAILBOX, vk::PresentModeKHR::IMMEDIATE]
            .into_iter()
            .find(|m| modes.contains(m))
            .unwrap_or(vk::PresentModeKHR::FIFO);
        // Shell surfaces are transparent: premultiplied alpha.
        let alpha = [
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::INHERIT,
            vk::CompositeAlphaFlagsKHR::OPAQUE,
        ]
        .into_iter()
        .find(|a| caps.supported_composite_alpha.contains(*a))
        .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE);
        let extent = swap_extent(&caps, size);
        let mut count = (caps.min_image_count + 1).max(3);
        if caps.max_image_count > 0 {
            count = count.min(caps.max_image_count);
        }
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | (caps.supported_usage_flags
                & (vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::INPUT_ATTACHMENT));
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(usage)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
            .composite_alpha(alpha)
            .present_mode(mode)
            .clipped(true)
            .old_swapchain(swap.swapchain);
        let created = {
            let _queue = queue_lock();
            // SAFETY: no pending work uses the old swapchain (the queue is waited on, under its
            // lock); the structure is valid for the call. The old swapchain is retired by the
            // call whether or not it succeeds, and it and its commands are no longer in use
            // once the queue is idle: both are destroyed here.
            unsafe {
                let created = self
                    .device
                    .queue_wait_idle(self.queue)
                    .map_err(vk_error("vkQueueWaitIdle"))
                    .and_then(|()| {
                        self.swapchain_fn
                            .create_swapchain(&info, None)
                            .map_err(vk_error("vkCreateSwapchainKHR"))
                    });
                if !swap.transitions.is_empty() {
                    self.device
                        .free_command_buffers(self.pool, &swap.transitions);
                }
                if swap.swapchain != vk::SwapchainKHR::null() {
                    self.swapchain_fn.destroy_swapchain(swap.swapchain, None);
                }
                created
            }
        };
        *swap = Swap::EMPTY;
        let swapchain = created?;
        // SAFETY: swapchain just created by this device.
        let images = match unsafe { self.swapchain_fn.get_swapchain_images(swapchain) } {
            Ok(images) => images,
            Err(err) => {
                // SAFETY: just created, never used.
                unsafe { self.swapchain_fn.destroy_swapchain(swapchain, None) };
                return Err(vk_error("vkGetSwapchainImagesKHR")(err));
            }
        };
        let alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(u32::try_from(images.len()).unwrap_or(0));
        // SAFETY: valid pool, valid structure.
        let transitions = match unsafe { self.device.allocate_command_buffers(&alloc) } {
            Ok(transitions) => transitions,
            Err(err) => {
                // SAFETY: just created, never used.
                unsafe { self.swapchain_fn.destroy_swapchain(swapchain, None) };
                return Err(vk_error("vkAllocateCommandBuffers")(err));
            }
        };
        swap.swapchain = swapchain;
        swap.extent = extent;
        swap.format = format.format;
        swap.transitions = transitions;
        swap.images = images;
        let old_layout = vk::ImageLayout::from_raw(self.drawn_layout.load(Ordering::Acquire));
        for (&cmd, &image) in swap.transitions.iter().zip(&swap.images) {
            let barrier = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                .old_layout(old_layout)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .level_count(1)
                        .layer_count(1),
                );
            // SAFETY: cmd just allocated from this pool; the recording uses valid objects.
            unsafe {
                self.device
                    .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                    .map_err(vk_error("vkBeginCommandBuffer"))?;
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                );
                self.device
                    .end_command_buffer(cmd)
                    .map_err(vk_error("vkEndCommandBuffer"))?;
            }
        }
        Ok(())
    }

    /// An image of `chain`'s swapchain for a frame of `size` pixels: the one taken and not
    /// presented, or the next one (the swapchain is rebuilt if the size changed).
    fn acquire(&self, chain: &mut Chain, size: (u32, u32)) -> Option<(u64, u32)> {
        let extent = chain.swap.extent;
        if (extent.width, extent.height) != size
            && size.0 > 0
            && size.1 > 0
            && let Err(err) = self.rebuild(chain, size)
        {
            tracing::warn!("swapchain: {err}");
            return None;
        }
        if let Some(index) = chain.swap.current {
            let image = chain.swap.images.get(index as usize)?;
            return Some((image.as_raw(), chain.swap.format.as_raw() as u32));
        }
        for _ in 0..2 {
            // SAFETY: valid swapchain and fence; the fence is waited on before reusing it.
            let acquired = unsafe {
                self.swapchain_fn.acquire_next_image(
                    chain.swap.swapchain,
                    u64::MAX,
                    vk::Semaphore::null(),
                    self.acquired,
                )
            };
            match acquired {
                Ok((index, _suboptimal)) => {
                    // SAFETY: the fence was passed to the acquisition that just succeeded.
                    let waited = unsafe {
                        self.device
                            .wait_for_fences(&[self.acquired], true, u64::MAX)
                            .and_then(|()| self.device.reset_fences(&[self.acquired]))
                    };
                    if let Err(err) = waited {
                        tracing::warn!("vkWaitForFences: {err:?}");
                        return None;
                    }
                    chain.swap.current = Some(index);
                    let image = chain.swap.images.get(index as usize)?;
                    return Some((image.as_raw(), chain.swap.format.as_raw() as u32));
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    if let Err(err) = self.rebuild(chain, size) {
                        tracing::warn!("swapchain: {err}");
                        return None;
                    }
                }
                Err(err) => {
                    tracing::warn!("vkAcquireNextImageKHR: {err:?}");
                    return None;
                }
            }
        }
        None
    }

    /// Brings the queued image of every chain to the presentation state and presents them
    /// together: one submission of their transitions and one `vkQueuePresentKHR` for all the
    /// swapchains. Venus waits for the GPU inside every present call: with one call per view a
    /// frame of several views would wait once per view. Returns, per view presented, whether
    /// it reached the compositor.
    fn present_queued(&self, chains: &mut BTreeMap<i64, Chain>) -> Vec<(i64, bool)> {
        let mut ids = Vec::new();
        let mut commands = Vec::new();
        let mut swapchains = Vec::new();
        let mut indices = Vec::new();
        for (id, chain) in chains.iter_mut() {
            let Some(index) = chain.queued.take() else {
                continue;
            };
            let Some(&cmd) = chain.swap.transitions.get(index as usize) else {
                continue;
            };
            ids.push(*id);
            commands.push(cmd);
            swapchains.push(chain.swap.swapchain);
            indices.push(index);
        }
        if ids.is_empty() {
            return Vec::new();
        }
        let signal = [self.transitioned];
        let submit = vk::SubmitInfo::default()
            .command_buffers(&commands)
            .signal_semaphores(&signal);
        let mut results = vec![vk::Result::SUCCESS; ids.len()];
        let result = {
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal)
                .swapchains(&swapchains)
                .image_indices(&indices)
                .results(&mut results);
            let _queue = queue_lock();
            // SAFETY: each transition command is recorded for the image presented with it; the
            // queue is used under its lock; the semaphore orders the presentation after the
            // transitions, and the submission comes after the engine's drawing on the same
            // queue. `results` has one entry per swapchain.
            unsafe {
                self.device
                    .queue_submit(self.queue, &[submit], vk::Fence::null())
                    .map_err(|err| (err, false))
                    .and_then(|()| {
                        self.swapchain_fn
                            .queue_present(self.queue, &present)
                            .map_err(|err| (err, true))
                    })
            }
        };
        let submitted = match result {
            Ok(_) => true,
            Err((err, presented)) => {
                if err != vk::Result::ERROR_OUT_OF_DATE_KHR {
                    tracing::warn!("vkQueueSubmit/vkQueuePresentKHR: {err:?}");
                }
                presented
            }
        };
        ids.into_iter()
            .zip(results)
            .map(|(id, result)| {
                let presented = submitted
                    && match result {
                        vk::Result::SUCCESS | vk::Result::SUBOPTIMAL_KHR => true,
                        // Rebuilt at the next frame (the extent no longer matches the surface).
                        vk::Result::ERROR_OUT_OF_DATE_KHR => {
                            if let Some(chain) = chains.get_mut(&id) {
                                chain.swap.extent = vk::Extent2D::default();
                            }
                            true
                        }
                        err => {
                            tracing::warn!("view {id}: vkQueuePresentKHR: {err:?}");
                            false
                        }
                    };
                (id, presented)
            })
            .collect()
    }

    /// Records which views have shown a frame.
    fn note_presented(&self, presented: &[(i64, bool)]) {
        for &(id, done) in presented {
            if !done {
                continue;
            }
            if id == MAIN_VIEW {
                self.main_presented.store(true, Ordering::Release);
            } else if let Ok(mut views) = self.presented.lock() {
                views.insert(id);
            }
        }
    }

    /// An image of the main swapchain for a frame of `size` pixels (null if not possible): the
    /// root surface of one view.
    pub(super) fn next_image(&self, size: (u32, u32)) -> (u64, u32) {
        let Ok(mut chains) = self.chains.lock() else {
            return (0, 0);
        };
        chains
            .get_mut(&MAIN_VIEW)
            .and_then(|chain| self.acquire(chain, size))
            .unwrap_or((0, 0))
    }

    /// Presents the main swapchain's current image (the root surface of one view).
    pub(super) fn present(&self) -> bool {
        let Ok(mut chains) = self.chains.lock() else {
            return false;
        };
        let Some(chain) = chains.get_mut(&MAIN_VIEW) else {
            return false;
        };
        chain.queued = chain.swap.current.take();
        let presented = self.present_queued(&mut chains);
        drop(chains);
        self.note_presented(&presented);
        presented.iter().any(|&(_, done)| done)
    }

    /// The backing store of view `view_id` for a frame of `size` pixels: the next image of the
    /// view's swapchain. None if the view is gone, already has its backing store for this
    /// frame, or the swapchain fails.
    pub(super) fn acquire_target(&self, view_id: i64, size: (u32, u32)) -> Option<VulkanTarget> {
        let mut chains = self.chains.lock().ok()?;
        let chain = chains.get_mut(&view_id)?;
        if chain.outstanding {
            tracing::warn!("view {view_id}: a second layer in one frame is not supported");
            return None;
        }
        let start = std::time::Instant::now();
        let (image, format) = self.acquire(chain, size)?;
        slow("acquire", view_id, start);
        chain.outstanding = true;
        Some(VulkanTarget {
            view_id,
            image,
            format,
        })
    }

    /// Queues the frame drawn into `target` for presentation: the views of a frame are
    /// presented together when the raster task ends ([`Self::flush_presents`]).
    pub(super) fn present_target(&self, target: &VulkanTarget) -> bool {
        let Ok(mut chains) = self.chains.lock() else {
            return false;
        };
        let Some(chain) = chains.get_mut(&target.view_id) else {
            // Removed while this frame was drawn.
            return true;
        };
        let current = chain
            .swap
            .current
            .and_then(|index| chain.swap.images.get(index as usize))
            .map(|image| image.as_raw());
        if current != Some(target.image) {
            // The swapchain was rebuilt since: the frame is lost, the next one comes.
            return true;
        }
        chain.queued = chain.swap.current.take();
        true
    }

    /// Presents the frames queued by [`Self::present_target`]: on the raster thread, after
    /// every raster task.
    pub(super) fn flush_presents(&self) {
        let Ok(mut chains) = self.chains.lock() else {
            return;
        };
        if chains.values().all(|chain| chain.queued.is_none()) {
            return;
        }
        let start = std::time::Instant::now();
        let presented = self.present_queued(&mut chains);
        drop(chains);
        slow("present", presented.len() as i64, start);
        self.note_presented(&presented);
    }

    /// Presents a fully transparent frame on view `view_id`, at the view's size. With a
    /// compositor the engine draws no layer for an empty scene (a closed panel, a layer
    /// with nothing left to show): without this the view's last frame would stay on screen.
    pub(super) fn present_clear(&self, view_id: i64) -> bool {
        let Ok(mut chains) = self.chains.lock() else {
            return false;
        };
        let Some(chain) = chains.get_mut(&view_id) else {
            return true;
        };
        let extent = chain.swap.extent;
        if extent.width == 0 || extent.height == 0 {
            // Nothing shown yet: nothing to clear.
            return true;
        }
        if chain.outstanding {
            return false;
        }
        // At the view's size now when the engine has told it (a layer that resized while its
        // scene was empty, the hidden Dock growing to the screen height): the frame of the old
        // size would not fit the new configure, and the layer would stay where it was.
        let size = if chain.size.0 > 0 && chain.size.1 > 0 {
            chain.size
        } else {
            (extent.width, extent.height)
        };
        self.clear_chain(chain, size)
    }

    /// Presents a fully transparent frame of `size` pixels on `chain` (its lock held).
    fn clear_chain(&self, chain: &mut Chain, size: (u32, u32)) -> bool {
        let Some((image, _)) = self.acquire(chain, size) else {
            return false;
        };
        let Some(index) = chain.swap.current.take() else {
            return false;
        };
        let image = vk::Image::from_raw(image);
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let barrier = |old, new, src, dst| {
            vk::ImageMemoryBarrier::default()
                .src_access_mask(src)
                .dst_access_mask(dst)
                .old_layout(old)
                .new_layout(new)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)
        };
        let alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: valid pool and device; the command buffer and the fence are used once here
        // and freed after the fence says the GPU is done with them. The image is the one just
        // acquired; the clear needs TRANSFER_DST, which the swapchain asks for when supported.
        unsafe {
            let Some(cmd) = self
                .device
                .allocate_command_buffers(&alloc)
                .ok()
                .and_then(|buffers| buffers.first().copied())
            else {
                return false;
            };
            let Ok(fence) = self
                .device
                .create_fence(&vk::FenceCreateInfo::default(), None)
            else {
                self.device.free_command_buffers(self.pool, &[cmd]);
                return false;
            };
            let recorded = self
                .device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .and_then(|()| {
                    self.device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::TOP_OF_PIPE,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[barrier(
                            vk::ImageLayout::UNDEFINED,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            vk::AccessFlags::empty(),
                            vk::AccessFlags::TRANSFER_WRITE,
                        )],
                    );
                    self.device.cmd_clear_color_image(
                        cmd,
                        image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 0.0],
                        },
                        &[range],
                    );
                    self.device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[barrier(
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            vk::ImageLayout::PRESENT_SRC_KHR,
                            vk::AccessFlags::TRANSFER_WRITE,
                            vk::AccessFlags::MEMORY_READ,
                        )],
                    );
                    self.device.end_command_buffer(cmd)
                });
            let commands = [cmd];
            let signal = [self.transitioned];
            let submit = vk::SubmitInfo::default()
                .command_buffers(&commands)
                .signal_semaphores(&signal);
            let swapchains = [chain.swap.swapchain];
            let indices = [index];
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal)
                .swapchains(&swapchains)
                .image_indices(&indices);
            let presented = recorded.is_ok() && {
                let _queue = queue_lock();
                self.device
                    .queue_submit(self.queue, &[submit], fence)
                    .and_then(|()| self.swapchain_fn.queue_present(self.queue, &present))
                    .is_ok()
            };
            let _ = self.device.wait_for_fences(&[fence], true, 1_000_000_000);
            self.device.destroy_fence(fence, None);
            self.device.free_command_buffers(self.pool, &[cmd]);
            presented
        }
    }

    /// The engine gave `target` back: the view may have its next backing store.
    pub(super) fn release_target(&self, target: &VulkanTarget) {
        if let Ok(mut chains) = self.chains.lock()
            && let Some(chain) = chains.get_mut(&target.view_id)
        {
            chain.outstanding = false;
        }
    }
}

/// Logs (debug) a swapchain call of view `view_id` that blocked the raster thread for more
/// than 2 ms.
fn slow(call: &str, view_id: i64, start: std::time::Instant) {
    let elapsed = start.elapsed();
    if elapsed > std::time::Duration::from_millis(2) {
        tracing::debug!(
            "{call} ({view_id}) took {:.1} ms",
            elapsed.as_secs_f64() * 1000.0
        );
    }
}

/// A Vulkan surface on `surface`, a `wl_surface` of the connection whose display is `display`.
fn create_surface(
    wayland_fn: &ash::khr::wayland_surface::Instance,
    display: usize,
    surface: &WlSurface,
) -> Result<vk::SurfaceKHR, Error> {
    let info = vk::WaylandSurfaceCreateInfoKHR::default()
        .display(display as *mut vk::wl_display)
        .surface(surface.id().as_ptr().cast::<vk::wl_surface>());
    // SAFETY: display and surface are the libwayland objects of the connection and of the view,
    // alive longer than the Vulkan surface (destroyed in `destroy_chain` or Drop, before the
    // wl_surface).
    unsafe { wayland_fn.create_wayland_surface(&info, None) }
        .map_err(vk_error("vkCreateWaylandSurfaceKHR"))
}

/// The size of a swapchain for a frame of `size` pixels: the surface's own, when it has one,
/// otherwise `size` within the surface's limits (Wayland: the client decides).
fn swap_extent(caps: &vk::SurfaceCapabilitiesKHR, size: (u32, u32)) -> vk::Extent2D {
    if caps.current_extent.width != u32::MAX {
        return caps.current_extent;
    }
    let min = caps.min_image_extent;
    let max = caps.max_image_extent;
    vk::Extent2D {
        width: size
            .0
            .clamp(min.width.max(1), max.width.max(min.width.max(1))),
        height: size
            .1
            .clamp(min.height.max(1), max.height.max(min.height.max(1))),
    }
}

/// `vk_icdGetInstanceProcAddr`, the entry point of the loader-driver interface.
type DriverEntry = unsafe extern "system" fn(vk::Instance, *const c_char) -> vk::PFN_vkVoidFunction;

/// The instance, with only the drivers of this machine's GPUs when they are known
/// (`crate::vulkan_drivers`, #191): the loader would otherwise open every installed driver.
/// The drivers loaded here are returned, to stay loaded as long as the instance.
fn create_instance(
    entry: &ash::Entry,
    extensions: &[*const c_char],
) -> Result<(ash::Instance, Vec<libloading::Library>), Error> {
    let app = vk::ApplicationInfo::default()
        .application_name(c"silica-embedder")
        .api_version(vk::API_VERSION_1_1);
    let (drivers, entries) = load_drivers();
    if !entries.is_empty() {
        let infos: Vec<vk::DirectDriverLoadingInfoLUNARG<'_>> = entries
            .iter()
            .map(|&get| {
                vk::DirectDriverLoadingInfoLUNARG::default().pfn_get_instance_proc_addr(Some(get))
            })
            .collect();
        let mut list = vk::DirectDriverLoadingListLUNARG::default()
            .mode(vk::DirectDriverLoadingModeLUNARG::EXCLUSIVE)
            .drivers(&infos);
        let mut with_direct = extensions.to_vec();
        with_direct.push(ash::lunarg::direct_driver_loading::NAME.as_ptr());
        let create = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&with_direct)
            .push_next(&mut list);
        // SAFETY: create is a valid structure with pointers to data alive for the call; the
        // drivers' entry points come from libraries that stay loaded with the instance.
        match unsafe { entry.create_instance(&create, None) } {
            Ok(instance) => return Ok((instance, drivers)),
            Err(err) => tracing::info!("Vulkan with the GPU's drivers only: {err:?}, loading all"),
        }
    }
    let create = vk::InstanceCreateInfo::default()
        .application_info(&app)
        .enabled_extension_names(extensions);
    // SAFETY: create is a valid structure with pointers to data alive for the call.
    let instance =
        unsafe { entry.create_instance(&create, None) }.map_err(vk_error("vkCreateInstance"))?;
    Ok((instance, Vec::new()))
}

/// The drivers of this machine's GPUs and their entry points: empty when they are not known
/// or none loads.
fn load_drivers() -> (Vec<libloading::Library>, Vec<DriverEntry>) {
    let Some(drivers) = crate::vulkan_drivers::for_this_machine() else {
        return (Vec::new(), Vec::new());
    };
    let mut libraries = Vec::new();
    let mut entries = Vec::new();
    for path in drivers.into_iter().map(|driver| driver.library) {
        // SAFETY: a Vulkan driver named by its installed manifest, the library the loader
        // itself would open; its initializers have no preconditions.
        let library = match unsafe { libloading::Library::new(&path) } {
            Ok(library) => library,
            Err(err) => {
                tracing::info!("Vulkan driver {}: {err}", path.display());
                continue;
            }
        };
        // SAFETY: the symbol of the loader-driver interface, which has this signature.
        let get =
            unsafe { library.get::<DriverEntry>(b"vk_icdGetInstanceProcAddr\0") }.map(|get| *get);
        match get {
            Ok(get) => {
                entries.push(get);
                libraries.push(library);
            }
            Err(err) => tracing::info!("Vulkan driver {}: {err}", path.display()),
        }
    }
    (libraries, entries)
}

/// The GPU that presents on this surface: integrated, then discrete (or the other way round
/// with `SILICA_VULKAN_GPU=discrete`), never the one on the CPU if there is another.
fn pick_device(
    instance: &ash::Instance,
    surface_fn: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Option<(vk::PhysicalDevice, u32, String)> {
    let discrete_first = std::env::var("SILICA_VULKAN_GPU").is_ok_and(|v| v == "discrete");
    // SAFETY: valid instance.
    let devices = unsafe { instance.enumerate_physical_devices() }.ok()?;
    let mut best: Option<(u32, vk::PhysicalDevice, u32, String)> = None;
    for device in devices {
        // SAFETY: device comes from this instance.
        let props = unsafe { instance.get_physical_device_properties(device) };
        if props.api_version < vk::API_VERSION_1_1 {
            continue;
        }
        // SAFETY: as above.
        let families = unsafe { instance.get_physical_device_queue_family_properties(device) };
        let family = families.iter().enumerate().find_map(|(i, f)| {
            let i = u32::try_from(i).ok()?;
            // SAFETY: family index of this device, surface of this instance.
            let present =
                unsafe { surface_fn.get_physical_device_surface_support(device, i, surface) }
                    .unwrap_or(false);
            (f.queue_flags.contains(vk::QueueFlags::GRAPHICS) && present).then_some(i)
        });
        let Some(family) = family else {
            continue;
        };
        let score = match props.device_type {
            vk::PhysicalDeviceType::INTEGRATED_GPU if !discrete_first => 3,
            vk::PhysicalDeviceType::DISCRETE_GPU if discrete_first => 3,
            vk::PhysicalDeviceType::INTEGRATED_GPU | vk::PhysicalDeviceType::DISCRETE_GPU => 2,
            vk::PhysicalDeviceType::CPU => 0,
            _ => 1,
        };
        let name = props
            .device_name_as_c_str()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if best.as_ref().is_none_or(|(s, ..)| score > *s) {
            best = Some((score, device, family, name));
        }
    }
    best.map(|(_, device, family, name)| (device, family, name))
}

impl Drop for VulkanSurface {
    fn drop(&mut self) {
        let chains = self
            .chains
            .get_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        for chain in chains.into_values() {
            self.destroy_chain(chain);
        }
        // The engine is already shut down: nobody uses the queue, images and commands anymore.
        // SAFETY: every object was created here and is destroyed once, in reverse order,
        // after the device has finished its work.
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_semaphore(self.transitioned, None);
            self.device.destroy_fence(self.acquired, None);
            self.device.destroy_command_pool(self.pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(current: (u32, u32), min: (u32, u32), max: (u32, u32)) -> vk::SurfaceCapabilitiesKHR {
        vk::SurfaceCapabilitiesKHR {
            current_extent: vk::Extent2D {
                width: current.0,
                height: current.1,
            },
            min_image_extent: vk::Extent2D {
                width: min.0,
                height: min.1,
            },
            max_image_extent: vk::Extent2D {
                width: max.0,
                height: max.1,
            },
            ..Default::default()
        }
    }

    #[test]
    fn a_wayland_swapchain_takes_the_frame_size_within_the_limits() {
        let wayland = caps((u32::MAX, u32::MAX), (1, 1), (16384, 16384));
        let sides = |e: vk::Extent2D| (e.width, e.height);
        assert_eq!(sides(swap_extent(&wayland, (1280, 30))), (1280, 30));
        assert_eq!(swap_extent(&wayland, (0, 0)).width, 1);
        assert_eq!(swap_extent(&wayland, (99999, 5)).width, 16384);
        // A driver reporting a maximum below its minimum (hostile or broken) does not panic.
        let broken = caps((u32::MAX, u32::MAX), (64, 64), (0, 0));
        assert_eq!(sides(swap_extent(&broken, (10, 10))), (64, 64));
        let fixed = caps((800, 600), (1, 1), (16384, 16384));
        assert_eq!(swap_extent(&fixed, (1280, 30)).width, 800);
    }

    #[test]
    fn the_queue_functions_are_wrapped_only_once_resolved() {
        // Before an instance exists there is nothing to wrap: the engine gets the real ones.
        if REAL.get().is_none() {
            assert!(locked_entry(b"vkQueueSubmit").is_none());
        }
        assert!(locked_entry(b"vkCreateImage").is_none());
    }
}
