//! Silica's desktop shell on Vetro (plan: silica-desktop#254 §2, docs on the vetro
//! repository's `docs/shell-and-apps.md`). One process: it joins Ossido's shell channel
//! (`SILICA_SHELL=1`, set by Ossido for the shell it starts) and runs the shell's modules.
//!
//! Modules so far: the lock screen. Next, in order: the menu bar, the Dock, Faro and the
//! Launcher, notifications, Control Center, the switcher, the screenshot UI.
//!
//! `silica-shell --preview-lock PATH [W H]` renders the lock screen headless to a PNG.

mod lock;
mod menubar;
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
            AppEvent::Shell(ShellEvent::Command(id)) if id == "lock.now" => {
                lock::lock(ui, &self.lock);
            }
            AppEvent::Shell(ShellEvent::AppMenu {
                app_id,
                serial,
                json,
            }) => menubar::set_app_menu(ui, &self.bar, app_id.clone(), *serial, json),
            AppEvent::Quit => ui.request(vetro_lib::core::Request::Quit),
            _ => {}
        }
    }
}

/// Updates the clock at the next minute, and again after that.
fn schedule_clock(ui: &mut Ui, shell: Rc<Shell>) {
    let wait = 60.0 - f64::from(time::now().second.min(59));
    ui.after(wait, move |ui| {
        menubar::tick_clock(ui, &shell.bar);
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
        if shell.wallpaper.borrow().is_none() {
            let choice = wallpaper::choice(&Default::default(), ui.appearance.dark);
            *shell.wallpaper.borrow_mut() = choice.clone();
            shell.lock.borrow_mut().wallpaper = choice;
        }
        *shell.backgrounds.borrow_mut() = wallpaper::open_background(ui, &shell.wallpaper);
        menubar::open(ui, &shell.bar);
        // The menu bar's clock, each minute on the minute.
        schedule_clock(ui, shell.clone());
    });
    if let Err(e) = r {
        eprintln!("silica-shell: {e}");
        std::process::exit(1);
    }
}
