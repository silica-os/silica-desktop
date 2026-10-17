//! Ossido's backends: winit (nested, for development), headless (tests and CI) and, in
//! `drm.rs`, the real session.

use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::pixman::PixmanRenderer;
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use std::time::{Duration, Instant};

use smithay::reexports::calloop::EventLoop;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::pixman;
use smithay::reexports::wayland_server::Display;
use smithay::utils::{Rectangle, Transform};
use tracing::{info, warn};

use crate::capture::{Capture, capture, capture_elements};
use crate::render::{RenderContext, output_elements};
use crate::state::{Ossido, State};

pub enum Backend {
    Winit(Box<WinitBackend>),
    /// No window: it draws only when a capture is needed, with GLES on an EGL device (the
    /// GPU or llvmpipe) if there is one, otherwise with pixman.
    Headless(Headless),
    /// The real session: DRM/KMS, libinput, libseat (`drm.rs`).
    #[cfg(feature = "drm")]
    Drm(Box<crate::drm::DrmBackend>),
}

impl Backend {
    /// The GLES renderer, if the backend has one (all except headless on pixman).
    pub fn gles(&mut self) -> Option<&mut GlesRenderer> {
        match self {
            Backend::Winit(backend) => Some(backend.graphics.renderer()),
            Backend::Headless(Headless::Gles(renderer)) => Some(renderer.as_mut()),
            Backend::Headless(Headless::Pixman(_)) => None,
            #[cfg(feature = "drm")]
            Backend::Drm(backend) => Some(&mut backend.renderer),
        }
    }
}

pub enum Headless {
    Gles(Box<GlesRenderer>),
    Pixman(PixmanRenderer),
}

/// The headless renderer, from `OSSIDO_HEADLESS_RENDERER`: GLES on llvmpipe (default),
/// `gpu` for GLES on the GPU, `pixman` without GLES. If EGL is missing, pixman.
fn headless_renderer() -> Result<Headless, Box<dyn std::error::Error>> {
    let choice = std::env::var("OSSIDO_HEADLESS_RENDERER").unwrap_or_default();
    if choice != "pixman" {
        match crate::egl::headless_renderer(choice == "gpu") {
            Ok(renderer) => return Ok(Headless::Gles(Box::new(renderer))),
            Err(err) => warn!("headless without GLES, using pixman: {err}"),
        }
    }
    Ok(Headless::Pixman(PixmanRenderer::new()?))
}

pub struct WinitBackend {
    pub graphics: WinitGraphicsBackend<GlesRenderer>,
    pub damage_tracker: OutputDamageTracker,
    /// The shape last given to the host's pointer (nested, the host draws the cursor).
    pub host_cursor: Option<smithay::input::pointer::CursorImageStatus>,
}

/// Refresh rate declared for the outputs, in millihertz.
const NESTED_REFRESH_MHZ: i32 = 60_000;
/// Interval of the frame callbacks in headless.
const HEADLESS_FRAME: Duration = Duration::from_micros(16_667);

fn new_output(name: &str, model: &str, mode: Mode, transform: Transform) -> Output {
    let output = Output::new(
        name.to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Silica".to_owned(),
            model: model.to_owned(),
        },
    );
    output.change_current_state(Some(mode), Some(transform), None, Some((0, 0).into()));
    output.set_preferred(mode);
    output
}

pub fn init_headless(
    event_loop: &mut EventLoop<'static, Ossido>,
    mut display: Display<State>,
    (width, height): (i32, i32),
) -> Result<Ossido, Box<dyn std::error::Error>> {
    let renderer = headless_renderer()?;
    let mode = Mode {
        size: (width, height).into(),
        refresh: NESTED_REFRESH_MHZ,
    };
    let output = new_output("headless", "Ossido headless", mode, Transform::Normal);
    let mut state = State::new(
        &mut display,
        event_loop.handle(),
        event_loop.get_signal(),
        output,
        Backend::Headless(renderer),
    )?;
    event_loop
        .handle()
        .insert_source(Timer::from_duration(HEADLESS_FRAME), |_, _, data| {
            data.state.advance_spaces(std::time::Instant::now());
            data.state.advance_panorama(std::time::Instant::now());
            data.state.send_frames();
            TimeoutAction::ToDuration(HEADLESS_FRAME)
        })?;
    if let Backend::Headless(Headless::Gles(renderer)) = &state.backend {
        state.software_renderer = is_software(renderer);
    }
    info!("backend headless pronto ({width}x{height})");
    Ok(Ossido { state, display })
}

pub fn init_winit(
    event_loop: &mut EventLoop<'static, Ossido>,
    mut display: Display<State>,
    (width, height): (i32, i32),
) -> Result<Ossido, Box<dyn std::error::Error>> {
    // The nested window has the requested size in physical pixels, like the headless output.
    let attributes = smithay::reexports::winit::window::Window::default_attributes()
        .with_inner_size(smithay::reexports::winit::dpi::PhysicalSize::new(
            width.max(1).unsigned_abs(),
            height.max(1).unsigned_abs(),
        ))
        .with_title("Ossido")
        .with_visible(true);
    let (graphics, winit_loop) = winit::init_from_attributes::<GlesRenderer>(attributes)?;
    let mode = Mode {
        size: graphics.window_size(),
        refresh: NESTED_REFRESH_MHZ,
    };
    let output = new_output("winit", "Nested Ossido", mode, Transform::Flipped180);
    let damage_tracker = OutputDamageTracker::from_output(&output);

    let mut state = State::new(
        &mut display,
        event_loop.handle(),
        event_loop.get_signal(),
        output,
        Backend::Winit(Box::new(WinitBackend {
            graphics,
            damage_tracker,
            host_cursor: None,
        })),
    )?;
    if let Backend::Winit(backend) = &mut state.backend {
        state.software_renderer = is_software(backend.graphics.renderer());
        let dh = display.handle();
        state.dmabuf_global = Some(crate::dmabuf::create_global(
            &mut state.dmabuf_state,
            &dh,
            backend.graphics.renderer(),
        ));
    }

    event_loop
        .handle()
        .insert_source(winit_loop, |event, _, data| match event {
            WinitEvent::Resized { size, .. } => {
                let mode = Mode {
                    size,
                    refresh: NESTED_REFRESH_MHZ,
                };
                data.state
                    .output
                    .change_current_state(Some(mode), None, None, None);
                data.state.output.set_preferred(mode);
                data.state.output_resized();
            }
            WinitEvent::Input(event) => data.state.process_input_event(event),
            WinitEvent::Redraw => data.state.render_winit(),
            WinitEvent::CloseRequested => data.state.loop_signal.stop(),
            WinitEvent::Focus(_) => {}
        })?;
    info!("backend winit pronto");
    Ok(Ossido { state, display })
}

impl State {
    /// Draws the current output offscreen and returns it as RGBA.
    pub fn screenshot(&mut self) -> Result<Capture, String> {
        let speed = self.animation_speed;
        let now = Instant::now();
        self.genies.retain(|genie| !genie.finished(now, speed));
        let glass_quality = self.glass_quality();
        self.advance_spaces(Instant::now());
        self.advance_panorama(Instant::now());
        let space_shift = self.space_shift();
        let panorama = self.panorama_view();
        let spaces = (self.workspaces.count(), self.workspaces.active);
        let buttons = self.buttons_view();
        let alt = self.alt_held();
        let dark = self.dark();
        let drag_icon = self.drag_icon_view();
        let window_veil = self.window_veil_view();
        if let Some(renderer) = self.backend.gles() {
            let context = RenderContext {
                buttons,
                alt,
                clear_color: self.clear_color,
                animation_speed: self.animation_speed,
                closing: &self.closing,
                genies: &self.genies,
                now: Instant::now(),
                glass_quality,
                glass_level: self.theme_config.glass,
                space_shift,
                panorama,
                spaces,
                dark,
                drag_icon,
                window_veil,
            };
            let elements = output_elements(renderer, &self.space, &self.output, &context);
            return capture_elements::<_, GlesTexture, _>(
                renderer,
                &self.output,
                &elements,
                self.clear_color,
            );
        }
        match &mut self.backend {
            Backend::Headless(Headless::Pixman(renderer)) => {
                capture::<_, pixman::Image<'static, 'static>>(
                    renderer,
                    &self.output,
                    &self.space,
                    self.clear_color,
                )
            }
            _ => Err("no renderer for the screenshot".to_owned()),
        }
    }

    fn render_winit(&mut self) {
        let now = Instant::now();
        let speed = self.animation_speed;
        let glass_quality = self.glass_quality();
        self.closing.retain(|closing| !closing.finished(now, speed));
        self.genies.retain(|genie| !genie.finished(now, speed));
        self.advance_spaces(now);
        self.advance_panorama(now);
        let space_shift = self.space_shift();
        let panorama = self.panorama_view();
        let spaces = (self.workspaces.count(), self.workspaces.active);
        let buttons = self.buttons_view();
        let alt = self.alt_held();
        let dark = self.dark();
        let drag_icon = self.drag_icon_view();
        let window_veil = self.window_veil_view();
        let cursor = self.effective_cursor();
        let Backend::Winit(backend) = &mut self.backend else {
            return;
        };
        // Nested, the host draws the pointer: it gets the named shape (a client's own cursor
        // surface cannot be passed on, the arrow stands in for it).
        if backend.host_cursor.as_ref() != Some(&cursor) {
            use smithay::input::pointer::{CursorIcon, CursorImageStatus};
            let window = backend.graphics.window();
            match &cursor {
                CursorImageStatus::Hidden => window.set_cursor_visible(false),
                CursorImageStatus::Named(icon) => {
                    window.set_cursor_visible(true);
                    window.set_cursor(*icon);
                }
                CursorImageStatus::Surface(_) => {
                    window.set_cursor_visible(true);
                    window.set_cursor(CursorIcon::Default);
                }
            }
            backend.host_cursor = Some(cursor);
        }
        let size = backend.graphics.window_size();
        let damage = Rectangle::from_size(size);
        let clear_color = self.clear_color;
        let context = RenderContext {
            buttons: buttons.clone(),
            alt,
            clear_color,
            animation_speed: speed,
            closing: &self.closing,
            genies: &self.genies,
            now,
            glass_quality,
            glass_level: self.theme_config.glass,
            space_shift,
            panorama,
            spaces,
            dark,
            drag_icon,
            window_veil,
        };
        let elements = output_elements(
            backend.graphics.renderer(),
            &self.space,
            &self.output,
            &context,
        );
        let rendered = match backend.graphics.bind() {
            Ok((renderer, mut framebuffer)) => backend
                .damage_tracker
                .render_output(renderer, &mut framebuffer, 0, &elements, clear_color)
                .map(|_| ())
                .map_err(|err| format!("{err:?}")),
            Err(err) => Err(format!("{err:?}")),
        };
        match rendered {
            Ok(()) => {
                if let Err(err) = backend.graphics.submit(Some(&[damage])) {
                    warn!("frame submit failed: {err:?}");
                }
            }
            Err(err) => warn!("render failed: {err}"),
        }
        backend.graphics.window().request_redraw();
        self.perf.rendered(now.elapsed());
        self.send_frames();
    }
}

impl State {
    /// Switches to another virtual terminal (Ctrl+Alt+F1…F12): only the real session has
    /// VTs, the other backends ignore it.
    pub fn change_vt(&mut self, vt: i32) {
        #[cfg(feature = "drm")]
        self.drm_change_vt(vt);
        #[cfg(not(feature = "drm"))]
        let _ = vt;
    }

    /// The snapshot of the closing window, to fade it out (only with GLES).
    pub fn start_closing(&mut self, window: &smithay::desktop::Window) {
        if self.animation_speed <= 0.0 {
            return;
        }
        let dark = self.dark();
        let Some(renderer) = self.backend.gles() else {
            return;
        };
        let now = Instant::now();
        let speed = self.animation_speed;
        self.closing.retain(|closing| !closing.finished(now, speed));
        if let Some(closing) =
            crate::render::snapshot_window(renderer, &self.space, &self.output, window, dark)
        {
            self.closing.push(closing);
        }
    }
}

impl State {
    /// The genie of a window still in the space: towards the Dock (minimize) or from the Dock
    /// (restore, the window just remapped stays hidden until the genie ends).
    /// Nothing with animations off or without GLES.
    pub fn start_genie(&mut self, window: &smithay::desktop::Window, restoring: bool) {
        if self.animation_speed <= 0.0 {
            return;
        }
        let Some(target) = self.minimize_target(window) else {
            return;
        };
        let dark = self.dark();
        let Some(renderer) = self.backend.gles() else {
            return;
        };
        let now = Instant::now();
        let speed = self.animation_speed;
        // A window has only one genie: the new one replaces the old one.
        self.genies
            .retain(|genie| !genie.finished(now, speed) && !genie.hides(window));
        let effect = self.minimize_effect;
        if let Some(mut genie) = crate::render::snapshot_genie(
            renderer,
            &self.space,
            &self.output,
            window,
            target,
            restoring,
            dark,
        ) {
            genie.effect = effect;
            self.genies.push(genie);
        }
    }
}

/// The thumbnail of a minimized window, in its data: premultiplied RGBA, at most
/// `THUMBNAIL_MAX` pixels per side.
#[derive(Default)]
pub struct Thumbnail(pub std::sync::Mutex<Option<(u32, u32, Vec<u8>)>>);

pub const THUMBNAIL_MAX: u32 = 256;

impl State {
    /// Photographs the window frame (without shadow) and keeps it, reduced, in its data.
    pub fn capture_thumbnail(&mut self, window: &smithay::desktop::Window) {
        let capture = match self.window_capture(window, false) {
            Ok((capture, _)) => capture,
            Err(err) => {
                warn!("thumbnail not read: {err}");
                return;
            }
        };
        let small = shrink(&capture.rgba, capture.width, capture.height, THUMBNAIL_MAX);
        window
            .user_data()
            .insert_if_missing_threadsafe(Thumbnail::default);
        if let Some(slot) = window.user_data().get::<Thumbnail>()
            && let Ok(mut slot) = slot.0.lock()
        {
            *slot = small;
        }
    }

    /// The window on its own (frame and bar included), at full resolution; with `shadow`, its
    /// shadow too on a transparent background (what the screen shows of it). Also where the
    /// image's top-left pixel sits on the output, in physical pixels.
    pub fn window_capture(
        &mut self,
        window: &smithay::desktop::Window,
        shadow: bool,
    ) -> Result<(crate::capture::Capture, (f64, f64)), String> {
        use smithay::backend::renderer::ExportMem;
        let dark = self.dark();
        let Some(renderer) = self.backend.gles() else {
            return Err("window capture not available with pixman".to_owned());
        };
        let snapshot =
            crate::render::snapshot_window(renderer, &self.space, &self.output, window, dark)
                .ok_or("window cannot be drawn")?;
        let scale = self.output.current_scale().fractional_scale();
        let (Some(geometry), Some(origin)) = (
            self.space.element_geometry(window),
            self.space.output_geometry(&self.output),
        ) else {
            return Err("window not on screen".to_owned());
        };
        let mut frame = crate::decorations::frame_of(window, geometry);
        frame.loc -= origin.loc;
        let frame = frame.to_physical_precise_round(scale);
        // With the shadow, the file is the frame plus fixed transparent margins (SCREENSHOT-2),
        // not the whole shadow of the screen; what the shadow does not cover stays clear.
        let (ml, mt, mr, mb) = crate::capture::window_margins(scale);
        let padded = Rectangle::<i32, smithay::utils::Physical>::new(
            (frame.loc.x - ml, frame.loc.y - mt).into(),
            (frame.size.w + ml + mr, frame.size.h + mt + mb).into(),
        );
        let visible = if shadow {
            padded
                .intersection(snapshot.bbox)
                .ok_or("window off screen")?
        } else {
            frame
                .intersection(snapshot.bbox)
                .ok_or("window off screen")?
        };
        let region = Rectangle::<i32, smithay::utils::Buffer>::new(
            (visible.loc - snapshot.bbox.loc).to_logical(1).to_buffer(
                1,
                Transform::Normal,
                &visible.size.to_logical(1),
            ),
            visible.size.to_logical(1).to_buffer(1, Transform::Normal),
        );
        let pixels = renderer
            .copy_texture(
                &snapshot.texture,
                region,
                smithay::backend::allocator::Fourcc::Abgr8888,
            )
            .and_then(|mapping| renderer.map_texture(&mapping).map(<[u8]>::to_vec))
            .map_err(|err| format!("readback: {err:?}"))?;
        let (Ok(width), Ok(height)) =
            (u32::try_from(visible.size.w), u32::try_from(visible.size.h))
        else {
            return Err("invalid size".to_owned());
        };
        let capture = crate::capture::Capture {
            width,
            height,
            rgba: pixels,
        };
        if !shadow {
            return Ok((
                capture,
                (f64::from(visible.loc.x), f64::from(visible.loc.y)),
            ));
        }
        let (Ok(full_w), Ok(full_h)) = (u32::try_from(padded.size.w), u32::try_from(padded.size.h))
        else {
            return Err("invalid size".to_owned());
        };
        let canvas = crate::capture::paste_on_transparent(
            &capture,
            visible.loc.x - padded.loc.x,
            visible.loc.y - padded.loc.y,
            full_w,
            full_h,
        );
        Ok((canvas, (f64::from(padded.loc.x), f64::from(padded.loc.y))))
    }
}

/// Reduces an RGBA image (average of the pixels of every box) to at most `max` per side.
pub fn shrink(pixels: &[u8], w: u32, h: u32, max: u32) -> Option<(u32, u32, Vec<u8>)> {
    if w == 0 || h == 0 || pixels.len() < (w as usize) * (h as usize) * 4 {
        return None;
    }
    let factor = (w.max(h) as f64 / max as f64).max(1.0);
    let (tw, th) = (
        ((w as f64 / factor).round() as u32).max(1),
        ((h as f64 / factor).round() as u32).max(1),
    );
    let mut out = vec![0u8; (tw as usize) * (th as usize) * 4];
    for ty in 0..th {
        let y0 = (ty as f64 * factor) as u32;
        let y1 = (((ty + 1) as f64 * factor) as u32).clamp(y0 + 1, h);
        for tx in 0..tw {
            let x0 = (tx as f64 * factor) as u32;
            let x1 = (((tx + 1) as f64 * factor) as u32).clamp(x0 + 1, w);
            let mut sum = [0u64; 4];
            let mut n = 0u64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y as usize) * (w as usize) + x as usize) * 4;
                    if let Some(px) = pixels.get(i..i + 4) {
                        for (s, &v) in sum.iter_mut().zip(px) {
                            *s += u64::from(v);
                        }
                        n += 1;
                    }
                }
            }
            let o = ((ty as usize) * (tw as usize) + tx as usize) * 4;
            if let Some(dst) = out.get_mut(o..o + 4) {
                for (d, s) in dst.iter_mut().zip(sum) {
                    *d = (s / n.max(1)) as u8;
                }
            }
        }
    }
    Some((tw, th, out))
}

/// The renderer runs on a software rasterizer (EGL_MESA_device_software).
fn is_software(renderer: &GlesRenderer) -> bool {
    smithay::backend::egl::EGLDevice::device_for_display(renderer.egl_context().display())
        .is_ok_and(|device| device.is_software())
}

impl State {
    /// The bar buttons to draw in this frame (fading symbols, pressed).
    pub fn buttons_view(&self) -> Option<crate::render::ButtonsView> {
        let now = Instant::now();
        let ms = |t: Instant| now.saturating_duration_since(t).as_secs_f64() * 1000.0;
        let speed = self.animation_speed;
        let scaled = |t: Instant| {
            if speed <= 0.0 {
                f64::MAX
            } else {
                ms(t) / speed
            }
        };
        let pressed = self
            .titlebar_press
            .as_ref()
            .filter(|_| self.titlebar_press_inside);
        let hover = self.buttons_hover.as_ref();
        let window = hover.map(|h| &h.window).or(pressed.map(|(w, _)| w))?;
        let symbols = match hover.filter(|h| &h.window == window) {
            Some(h) => {
                ossido_render::titlebar::symbols_opacity(scaled(h.entered), h.left.map(scaled))
            }
            None => 0.0,
        };
        Some(crate::render::ButtonsView {
            window: window.clone(),
            symbols,
            pressed: pressed
                .filter(|(w, _)| w == window)
                .and_then(|(_, part)| part.button()),
        })
    }

    /// ⌥ held on the keyboard.
    pub fn alt_held(&self) -> bool {
        self.seat
            .get_keyboard()
            .is_some_and(|keyboard| keyboard.modifier_state().alt)
    }
}
