//! Silica's desktop shell on Vetro (plan: silica-desktop#254 §2, docs on the vetro
//! repository's `docs/shell-and-apps.md`). One process: it joins Ossido's shell channel
//! (`SILICA_SHELL=1`, set by Ossido for the shell it starts) and runs the shell's modules.
//!
//! Modules so far: the lock screen. Next, in order: the menu bar, the Dock, Faro and the
//! Launcher, notifications, Control Center, the switcher, the screenshot UI.
//!
//! `silica-shell --preview-lock PATH [W H]` renders the lock screen headless to a PNG.

mod apps;
mod cc;
mod dialogs;
mod dock;
mod faro;
mod file_icons;
mod lock;
mod menubar;
mod motion;
mod notifications;
mod popmenu;
mod screenshot;
mod status;
mod switcher;
mod system;
mod text;
mod time;
mod wallpaper;

use std::cell::RefCell;
use std::rc::Rc;

use vetro_lib::core::Ui;
use vetro_lib::core::app::{AppEvent, AppInfo};
use vetro_lib::core::shell::ShellEvent;
use vetro_lib::geom::Size;

/// The app id of the shell.
pub const APP_ID: &str = "dev.silica.Shell";

/// Everything the shell's modules share.
struct Shell {
    lock: lock::Shared,
    bar: menubar::Shared,
    dock: dock::Shared,
    /// The system's state (network, sound, battery, appearance).
    system: system::Shared,
    /// Control Center.
    cc: cc::Shared,
    /// Faro and the Launcher.
    faro: faro::Shared,
    /// Banners and Notification Center.
    notes: notifications::Shared,
    /// The app switcher.
    switcher: switcher::Shared,
    /// Screenshots.
    shots: screenshot::Shared,
    /// Gatekeeper, authentication and Force Quit.
    dialogs: dialogs::Shared,
    /// The desktop wallpaper and its surfaces.
    wallpaper: Rc<RefCell<Option<wallpaper::Choice>>>,
    backgrounds: RefCell<Vec<vetro_lib::core::ViewId>>,
}

impl Shell {
    fn new() -> Shell {
        let lock = lock::LockState {
            user: lock::user_name(),
            hour24: true,
            ..lock::LockState::default()
        };
        let bar = menubar::BarState {
            user: lock.user.clone(),
            hour24: true,
            ..menubar::BarState::default()
        };
        Shell {
            lock: Rc::new(RefCell::new(lock)),
            bar: Rc::new(RefCell::new(bar)),
            dock: Rc::new(RefCell::new(dock::DockState::default())),
            system: Rc::default(),
            cc: Rc::default(),
            faro: Rc::default(),
            notes: Rc::default(),
            switcher: Rc::default(),
            shots: Rc::default(),
            dialogs: Rc::default(),
            wallpaper: Rc::new(RefCell::new(None)),
            backgrounds: RefCell::new(Vec::new()),
        }
    }

    /// The settings changed (or arrived): the wallpaper and the clock format follow them.
    fn settings_changed(&self, ui: &mut Ui, settings: &vetro_lib::services::settings::SettingsMap) {
        let dark = ui.appearance.dark;
        let choice = wallpaper::choice(settings, dark);
        *self.wallpaper.borrow_mut() = choice.clone();
        for v in self.backgrounds.borrow().iter() {
            ui.request_paint(*v);
        }
        dock::settings_changed(ui, &self.dock, settings);
        notifications::settings_changed(&self.notes, settings);
        let mut l = self.lock.borrow_mut();
        l.wallpaper = choice;
        if let Some(h) = settings
            .get("menubar.clock.hour24")
            .and_then(vetro_lib::services::settings::SettingValue::as_bool)
        {
            l.hour24 = h;
            self.bar.borrow_mut().hour24 = h;
        }
    }

    /// Opens or closes Control Center under the first menu bar.
    fn toggle_cc(&self, ui: &mut Ui) {
        let Some(w) = self.bar.borrow().bars.first().map(|b| b.0) else {
            return;
        };
        let width = ui.window(w).map_or(0.0, |w| w.size.width);
        let bar = self.bar.clone();
        menubar::set_status_active(ui, &bar, status::Kind::ControlCenter, true);
        cc::toggle(
            ui,
            &self.cc,
            &self.system,
            w,
            width,
            Rc::new(move |ui: &mut Ui| {
                menubar::set_status_active(ui, &bar, status::Kind::ControlCenter, false)
            }),
        );
    }

    /// The open windows (window, app), the most recently focused last.
    fn windows(&self) -> Vec<(String, String)> {
        let d = self.dock.borrow();
        let mut v: Vec<(String, String)> = d
            .inputs
            .windows
            .iter()
            .map(|w| (w.id.clone(), w.app.clone()))
            .collect();
        // The most recently focused last.
        v.sort_by_key(|(id, _)| d.focus.iter().position(|f| f == id));
        v
    }

    /// Opens or closes Notification Center (the clock stays highlighted while it is open).
    fn toggle_center(&self, ui: &mut Ui) {
        let bar = self.bar.clone();
        menubar::set_status_active(ui, &bar, status::Kind::Clock, true);
        notifications::toggle_center(
            ui,
            &self.notes,
            Some(Rc::new(move |ui: &mut Ui| {
                menubar::set_status_active(ui, &bar, status::Kind::Clock, false)
            })),
        );
    }

    fn event(&self, ui: &mut Ui, e: &AppEvent) {
        match e {
            AppEvent::Shell(ShellEvent::Broadcast { topic, value })
                if topic == "session.locked" =>
            {
                if value == "1" {
                    lock::lock(ui, &self.lock);
                } else {
                    lock::forget(ui, &self.lock);
                }
            }
            AppEvent::Shell(ShellEvent::Broadcast { topic, value }) if topic == "screenshot" => {
                screenshot::captured(ui, &self.shots, value);
            }
            AppEvent::Shell(ShellEvent::Command(id)) => match id.as_str() {
                "lock.now" => lock::lock(ui, &self.lock),
                "faro.toggle" => faro::toggle(ui, &self.faro, faro::Mode::Faro),
                "launcher.toggle" => faro::toggle(ui, &self.faro, faro::Mode::Launcher),
                "controlcenter.toggle" => self.toggle_cc(ui),
                "notifications.toggle" => self.toggle_center(ui),
                "screenshot.selection" => screenshot::select(ui, &self.shots, false, false),
                "screenshot.selection_clipboard" => {
                    screenshot::select(ui, &self.shots, true, false)
                }
                "screenshot.toolbar" => screenshot::toggle_bar(ui, &self.shots),
                "app.force_quit_panel" => {
                    let windows = self.windows();
                    dialogs::force_quit(ui, &self.dialogs, &windows);
                }
                s if s.starts_with("switcher.") => {
                    let windows = self.windows();
                    switcher::command(ui, &self.switcher, s, &windows);
                }
                _ => tracing::debug!("shell command {id} not handled"),
            },
            AppEvent::Shell(ShellEvent::Toplevel {
                identifier,
                app_id,
                title,
            }) => dock::window_changed(ui, &self.dock, identifier, app_id, title, true),
            AppEvent::Shell(ShellEvent::ToplevelClosed(identifier)) => {
                dock::window_changed(ui, &self.dock, identifier, "", "", false);
            }
            AppEvent::Shell(ShellEvent::Focused(identifier)) => {
                dock::focused(ui, &self.dock, identifier)
            }
            AppEvent::Shell(ShellEvent::AppMenu {
                app_id,
                serial,
                json,
            }) => menubar::set_app_menu(ui, &self.bar, app_id.clone(), *serial, json),
            AppEvent::Shell(ShellEvent::Minimized {
                identifier,
                minimized,
            }) => dock::minimized(ui, &self.dock, identifier, *minimized),
            AppEvent::Shell(ShellEvent::Thumbnail {
                identifier,
                width,
                height,
                rgba,
            }) => dock::thumbnail(ui, &self.dock, identifier, *width, *height, rgba.clone()),
            AppEvent::OutputsChanged => {
                fit_wallpapers(ui);
                for v in self.backgrounds.borrow().iter() {
                    ui.request_paint(*v);
                }
            }
            AppEvent::Quit => ui.request(vetro_lib::core::Request::Quit),
            _ => {}
        }
    }
}

/// Connects the status items to the system's state and to Control Center.
fn install_status(ui: &mut Ui, shell: &Rc<Shell>) {
    let s = shell.clone();
    shell
        .system
        .borrow_mut()
        .listeners
        .push(Rc::new(move |ui: &mut Ui| {
            let state = s.system.borrow().state.clone();
            s.bar.borrow_mut().system = state.clone();
            s.notes.borrow_mut().dnd = state.dnd;
            s.lock.borrow_mut().system = state;
            menubar::update_status(ui, &s.bar);
            lock::update_status(ui, &s.lock);
            cc::repaint(ui, &s.cc);
        }));
    let s = Rc::downgrade(shell);
    let on_click: status::OnClick = Rc::new(move |ui, kind, item| {
        let Some(s) = s.upgrade() else {
            return;
        };
        match kind {
            status::Kind::Search => faro::toggle(ui, &s.faro, faro::Mode::Faro),
            status::Kind::Clock => s.toggle_center(ui),
            _ => {
                let Some(w) = ui.window_of(item) else {
                    return;
                };
                let width = ui.window(w).map_or(0.0, |w| w.size.width);
                let bar = s.bar.clone();
                menubar::set_status_active(ui, &bar, status::Kind::ControlCenter, true);
                cc::toggle(
                    ui,
                    &s.cc,
                    &s.system,
                    w,
                    width,
                    Rc::new(move |ui: &mut Ui| {
                        menubar::set_status_active(ui, &bar, status::Kind::ControlCenter, false)
                    }),
                );
            }
        }
    });
    shell.bar.borrow_mut().on_status = Some(on_click);
    system::watch(ui, shell.system.clone());
}

/// Wallpapers are kept at the largest output's size (device pixels).
fn fit_wallpapers(ui: &mut Ui) {
    let px = ui
        .outputs()
        .iter()
        .map(|o| (o.size.width.max(o.size.height) * o.scale.max(1.0)).ceil() as i32)
        .max()
        .unwrap_or(1920);
    wallpaper::set_max_pixels(px);
}

/// Updates the clock at the next minute, and again after that.
fn schedule_clock(ui: &mut Ui, shell: Rc<Shell>) {
    let wait = 60.0 - f64::from(time::now().second.min(59));
    ui.after(wait, move |ui| {
        menubar::update_status(ui, &shell.bar);
        schedule_clock(ui, shell.clone());
    });
}

fn preview_lock(args: &[String]) -> Option<i32> {
    let i = args.iter().position(|a| a == "--preview-lock")?;
    let path = args.get(i + 1)?.clone();
    let w: f32 = args
        .get(i + 2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1920.0);
    let h: f32 = args
        .get(i + 3)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1200.0);
    let mut ui = Ui::new_headless();
    let mut appearance = ui.appearance;
    appearance.dark = true;
    ui.set_appearance(appearance);
    ui.set_outputs(vec![vetro_lib::core::shell::OutputInfo {
        name: "preview".into(),
        position: vetro_lib::geom::Point::ZERO,
        size: Size::new(w, h),
        scale: 1.0,
    }]);
    let shell = Shell::new();
    shell.lock.borrow_mut().wallpaper = wallpaper::choice(&Default::default(), true);
    lock::lock(&mut ui, &shell.lock);
    let Some(win) = shell.lock.borrow().windows.first().copied() else {
        return Some(1);
    };
    let png = ui
        .snapshot(win, 1.0)
        .and_then(|img| img.encode(None, skia_safe::EncodedImageFormat::PNG, 100));
    match png {
        Some(data) => match std::fs::write(&path, data.as_bytes()) {
            Ok(()) => Some(0),
            Err(e) => {
                eprintln!("silica-shell: {path}: {e}");
                Some(1)
            }
        },
        None => Some(1),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = preview_lock(&args) {
        std::process::exit(code);
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let shell = Rc::new(Shell::new());
    let r = vetro_lib::platform::run(APP_ID, move |ui| {
        ui.set_app_info(AppInfo {
            id: APP_ID.into(),
            name: "Silica Shell".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            copyright: String::new(),
            quit_enabled: false,
        });
        let s = shell.clone();
        ui.on_app_event(move |ui, e| s.event(ui, e));
        if let Some(services) = vetro_lib::platform::services() {
            let s = shell.clone();
            let settings = services.settings().get();
            shell.settings_changed(ui, &settings);
            let sub = services.settings().subscribe(move |map| {
                let map = map.clone();
                let s2 = s.clone();
                vetro_lib::platform::post(move |ui| s2.settings_changed(ui, &map));
            });
            // The subscription lives as long as the process.
            std::mem::forget(sub);
        }
        ui.connect_shell();
        fit_wallpapers(ui);
        // Skia keeps decoded images in a resource cache: a few MB are enough for the shell.
        skia_safe::graphics::set_resource_cache_total_bytes_limit(8 << 20);
        if shell.wallpaper.borrow().is_none() {
            let choice = wallpaper::choice(&Default::default(), ui.appearance.dark);
            *shell.wallpaper.borrow_mut() = choice.clone();
            shell.lock.borrow_mut().wallpaper = choice;
        }
        *shell.backgrounds.borrow_mut() = wallpaper::open_background(ui, &shell.wallpaper);
        install_status(ui, &shell);
        notifications::start(ui, &shell.notes);
        dialogs::start(&shell.dialogs);
        menubar::open(ui, &shell.bar);
        {
            let faro = shell.faro.clone();
            shell.dock.borrow_mut().on_launcher = Some(Rc::new(move |ui: &mut Ui| {
                faro::toggle(ui, &faro, faro::Mode::Launcher)
            }));
        }
        dock::open(ui, &shell.dock);
        // The menu bar's clock, each minute on the minute.
        schedule_clock(ui, shell.clone());
        // Start-up decodes large images and scales them down: give the freed memory back.
        ui.after(3.0, |_| vetro_lib::platform::trim_memory());
        ui.after(10.0, |_| vetro_lib::platform::trim_memory());
    });
    if let Err(e) = r {
        eprintln!("silica-shell: {e}");
        std::process::exit(1);
    }
}
