//! Silica's desktop shell on Vetro (plan: silica-desktop#254 §2, docs on the vetro
//! repository's `docs/shell-and-apps.md`). It joins Ossido's shell channel (`SILICA_SHELL=1`,
//! set by Ossido for the shell components it starts) and runs the shell's modules.
//!
//! `silica-shell --role ROLE` runs one part of the shell, as the reference system splits its
//! shell into processes (silica-desktop#261); Ossido keeps each one running (`--keep-shell`)
//! and restarts it alone when it dies. Roles: `wallpaper`, `menubar` (the mark and the app's
//! menus), `controlcenter` (Control Center and the status items), `dock` (the Dock and the
//! app switcher), `faro` (Faro and the Launcher), `notifications` (banners and Notification
//! Center), `lock` (the lock screen, authentication, Gatekeeper and Force Quit) and
//! `screenshot`. The roles talk through Ossido: shell commands (`faro.toggle`,
//! `launcher.toggle`, `notifications.toggle`, ...) reach every component and each handles its
//! own; a broadcast tells the status items that Notification Center is open. Without
//! `--role` one process runs everything.
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
use vetro_lib::core::shell::{ShellEvent, ShellRequest};
use vetro_lib::geom::Size;

/// The app id of the shell (a role's process adds its name: `dev.silica.Shell.Dock`).
pub const APP_ID: &str = "dev.silica.Shell";

/// The broadcast that tells the status items Notification Center is open (`1`) or not.
const CENTER_OPEN: &str = "shell.notifications.center";

/// The part of the shell a process runs (silica-desktop#261).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Everything, in one process.
    All,
    Wallpaper,
    MenuBar,
    ControlCenter,
    Dock,
    Faro,
    Notifications,
    Lock,
    Screenshot,
}

impl Role {
    const EACH: [Role; 8] = [
        Role::Wallpaper,
        Role::MenuBar,
        Role::ControlCenter,
        Role::Dock,
        Role::Faro,
        Role::Notifications,
        Role::Lock,
        Role::Screenshot,
    ];

    /// The role's name on the command line.
    fn name(self) -> &'static str {
        match self {
            Role::All => "all",
            Role::Wallpaper => "wallpaper",
            Role::MenuBar => "menubar",
            Role::ControlCenter => "controlcenter",
            Role::Dock => "dock",
            Role::Faro => "faro",
            Role::Notifications => "notifications",
            Role::Lock => "lock",
            Role::Screenshot => "screenshot",
        }
    }

    fn parse(name: &str) -> Option<Role> {
        std::iter::once(Role::All)
            .chain(Role::EACH)
            .find(|r| r.name() == name)
    }

    /// The role of this process: `--role NAME`, or everything.
    fn from_args(args: &[String]) -> Result<Role, String> {
        let Some(i) = args.iter().position(|a| a == "--role") else {
            return Ok(Role::All);
        };
        let name = args.get(i + 1).map(String::as_str).unwrap_or_default();
        Role::parse(name).ok_or_else(|| {
            let names: Vec<&str> = Role::EACH.iter().map(|r| r.name()).collect();
            format!("unknown role {name:?} (roles: {})", names.join(", "))
        })
    }

    /// Whether this process runs `part`.
    fn runs(self, part: Role) -> bool {
        self == Role::All || self == part
    }

    /// The process's app id: one per role, so that the roles are separate apps.
    fn app_id(self) -> String {
        match self {
            Role::All => APP_ID.to_string(),
            r => {
                let name = r.name();
                let mut chars = name.chars();
                let title: String = chars
                    .next()
                    .map(|c| c.to_ascii_uppercase())
                    .into_iter()
                    .chain(chars)
                    .collect();
                format!("{APP_ID}.{title}")
            }
        }
    }
}

/// The open windows as the shell channel reports them: (window, app) and the focus order.
#[derive(Default)]
struct Windows {
    list: Vec<(String, String)>,
    focus: Vec<String>,
}

impl Windows {
    fn changed(&mut self, id: &str, app: &str, open: bool) {
        self.list.retain(|(w, _)| w != id);
        if open {
            // The shell's own windows (its panels) are not apps.
            if !app.starts_with(APP_ID) {
                self.list.push((id.to_string(), app.to_string()));
            }
        } else {
            self.focus.retain(|f| f != id);
        }
    }

    fn focused(&mut self, id: &str) {
        self.focus.retain(|f| f != id);
        self.focus.push(id.to_string());
    }

    /// The windows, the most recently focused last.
    fn ordered(&self) -> Vec<(String, String)> {
        let mut v = self.list.clone();
        v.sort_by_key(|(id, _)| self.focus.iter().position(|f| f == id));
        v
    }
}

/// Everything the shell's modules share.
struct Shell {
    /// The part of the shell this process runs.
    role: Role,
    /// The open windows (the switcher, Force Quit).
    windows: RefCell<Windows>,
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
    fn new(role: Role) -> Shell {
        let lock = lock::LockState {
            user: lock::user_name(),
            hour24: true,
            ..lock::LockState::default()
        };
        let bar = menubar::BarState {
            user: lock.user.clone(),
            hour24: true,
            parts: match role {
                Role::MenuBar => menubar::Parts::Titles,
                Role::ControlCenter => menubar::Parts::Status,
                _ => menubar::Parts::All,
            },
            ..menubar::BarState::default()
        };
        Shell {
            role,
            windows: RefCell::default(),
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
        if self.role.runs(Role::Dock) {
            dock::settings_changed(ui, &self.dock, settings);
        }
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
        self.windows.borrow().ordered()
    }

    /// Opens or closes Notification Center (the clock stays highlighted while it is open:
    /// in this process, or in the status items' through a broadcast).
    fn toggle_center(&self, ui: &mut Ui) {
        let bar = self.bar.clone();
        let separate = self.role != Role::All;
        let mark = move |ui: &mut Ui, open: bool| {
            menubar::set_status_active(ui, &bar, status::Kind::Clock, open);
            if separate {
                let value = if open { "1" } else { "0" };
                ui.shell(ShellRequest::Broadcast(CENTER_OPEN.into(), value.into()));
            }
        };
        mark(ui, true);
        notifications::toggle_center(
            ui,
            &self.notes,
            Some(Rc::new(move |ui: &mut Ui| mark(ui, false))),
        );
    }

    /// Runs a shell command of another role: here, or through Ossido, which hands it to every
    /// shell component.
    fn command(&self, ui: &mut Ui, part: Role, id: &str) {
        if self.role.runs(part) {
            self.handle_command(ui, id);
        } else {
            ui.shell(ShellRequest::Execute(id.into()));
        }
    }

    /// A shell command, if it is this process's to handle.
    fn handle_command(&self, ui: &mut Ui, id: &str) {
        let r = self.role;
        match id {
            "lock.now" if r.runs(Role::Lock) => lock::lock(ui, &self.lock),
            "faro.toggle" if r.runs(Role::Faro) => faro::toggle(ui, &self.faro, faro::Mode::Faro),
            "launcher.toggle" if r.runs(Role::Faro) => {
                faro::toggle(ui, &self.faro, faro::Mode::Launcher)
            }
            "controlcenter.toggle" if r.runs(Role::ControlCenter) => self.toggle_cc(ui),
            "notifications.toggle" if r.runs(Role::Notifications) => self.toggle_center(ui),
            "screenshot.selection" if r.runs(Role::Screenshot) => {
                screenshot::select(ui, &self.shots, false, false)
            }
            "screenshot.selection_clipboard" if r.runs(Role::Screenshot) => {
                screenshot::select(ui, &self.shots, true, false)
            }
            "screenshot.toolbar" if r.runs(Role::Screenshot) => {
                screenshot::toggle_bar(ui, &self.shots)
            }
            "app.force_quit_panel" if r.runs(Role::Lock) => {
                let windows = self.windows();
                dialogs::force_quit(ui, &self.dialogs, &windows);
            }
            s if s.starts_with("switcher.") && r.runs(Role::Dock) => {
                let windows = self.windows();
                switcher::command(ui, &self.switcher, s, &windows);
            }
            _ => tracing::debug!("shell command {id} not handled by {}", r.name()),
        }
    }

    fn event(&self, ui: &mut Ui, e: &AppEvent) {
        let r = self.role;
        match e {
            AppEvent::Shell(ShellEvent::Broadcast { topic, value })
                if topic == "session.locked" && r.runs(Role::Lock) =>
            {
                // A lock role started again while the session is locked hears it at once
                // (Ossido repeats the last broadcasts) and takes the lock over (#252).
                if value == "1" {
                    lock::lock(ui, &self.lock);
                } else {
                    lock::forget(ui, &self.lock);
                }
            }
            AppEvent::Shell(ShellEvent::Broadcast { topic, value })
                if topic == "screenshot" && r.runs(Role::Screenshot) =>
            {
                screenshot::captured(ui, &self.shots, value);
            }
            AppEvent::Shell(ShellEvent::Broadcast { topic, value })
                if topic == CENTER_OPEN && r == Role::ControlCenter =>
            {
                menubar::set_status_active(ui, &self.bar, status::Kind::Clock, value == "1");
            }
            AppEvent::Shell(ShellEvent::Command(id)) => self.handle_command(ui, id),
            AppEvent::Shell(ShellEvent::Toplevel {
                identifier,
                app_id,
                title,
            }) => {
                self.windows.borrow_mut().changed(identifier, app_id, true);
                if r.runs(Role::Dock) {
                    dock::window_changed(ui, &self.dock, identifier, app_id, title, true);
                }
            }
            AppEvent::Shell(ShellEvent::ToplevelClosed(identifier)) => {
                self.windows.borrow_mut().changed(identifier, "", false);
                if r.runs(Role::Dock) {
                    dock::window_changed(ui, &self.dock, identifier, "", "", false);
                }
            }
            AppEvent::Shell(ShellEvent::Focused(identifier)) => {
                self.windows.borrow_mut().focused(identifier);
                if r.runs(Role::Dock) {
                    dock::focused(ui, &self.dock, identifier)
                }
            }
            AppEvent::Shell(ShellEvent::AppMenu {
                app_id,
                serial,
                json,
            }) if r.runs(Role::MenuBar) => {
                menubar::set_app_menu(ui, &self.bar, app_id.clone(), *serial, json)
            }
            AppEvent::Shell(ShellEvent::Minimized {
                identifier,
                minimized,
            }) if r.runs(Role::Dock) => dock::minimized(ui, &self.dock, identifier, *minimized),
            AppEvent::Shell(ShellEvent::Thumbnail {
                identifier,
                width,
                height,
                rgba,
            }) if r.runs(Role::Dock) => {
                dock::thumbnail(ui, &self.dock, identifier, *width, *height, rgba.clone())
            }
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

/// Opens the parts of the shell this process runs.
fn open_parts(ui: &mut Ui, shell: &Rc<Shell>) {
    let r = shell.role;
    if r.runs(Role::Wallpaper) {
        *shell.backgrounds.borrow_mut() = wallpaper::open_background(ui, &shell.wallpaper);
    }
    // The system's state: the status items and Control Center, the lock screen's items, Do
    // Not Disturb for the banners.
    if r.runs(Role::ControlCenter) || r.runs(Role::Lock) || r.runs(Role::Notifications) {
        install_status(ui, shell);
    }
    if r.runs(Role::Notifications) {
        notifications::start(ui, &shell.notes);
    }
    if r.runs(Role::Lock) {
        dialogs::start(&shell.dialogs);
    }
    if r.runs(Role::MenuBar) || r.runs(Role::ControlCenter) {
        menubar::open(ui, &shell.bar);
    }
    if r.runs(Role::ControlCenter) {
        // The clock, each minute on the minute.
        schedule_clock(ui, shell.clone());
    }
    if r.runs(Role::Dock) {
        let s = Rc::downgrade(shell);
        shell.dock.borrow_mut().on_launcher = Some(Rc::new(move |ui: &mut Ui| {
            if let Some(s) = s.upgrade() {
                s.command(ui, Role::Faro, "launcher.toggle");
            }
        }));
        dock::open(ui, &shell.dock);
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
            status::Kind::Search => s.command(ui, Role::Faro, "faro.toggle"),
            status::Kind::Clock => s.command(ui, Role::Notifications, "notifications.toggle"),
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
    let shell = Shell::new(Role::Lock);
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
    let role = match Role::from_args(&args) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("silica-shell: {e}");
            std::process::exit(2);
        }
    };
    let app_id = role.app_id();
    let shell = Rc::new(Shell::new(role));
    let r = vetro_lib::platform::run(&app_id.clone(), move |ui| {
        ui.set_app_info(AppInfo {
            id: app_id,
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
        open_parts(ui, &shell);
        // Start-up decodes large images and scales them down: give the freed memory back.
        ui.after(3.0, |_| vetro_lib::platform::trim_memory());
        ui.after(10.0, |_| vetro_lib::platform::trim_memory());
    });
    if let Err(e) = r {
        eprintln!("silica-shell: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn roles_come_from_the_command_line() {
        assert_eq!(Role::from_args(&args(&["silica-shell"])), Ok(Role::All));
        assert_eq!(
            Role::from_args(&args(&["silica-shell", "--role", "dock"])),
            Ok(Role::Dock)
        );
        assert!(Role::from_args(&args(&["silica-shell", "--role", "finder"])).is_err());
        assert!(Role::from_args(&args(&["silica-shell", "--role"])).is_err());
        for r in Role::EACH {
            assert_eq!(Role::parse(r.name()), Some(r));
            assert!(r.app_id().starts_with(APP_ID));
            assert_ne!(r.app_id(), APP_ID);
        }
        assert_eq!(
            Role::ControlCenter.app_id(),
            "dev.silica.Shell.Controlcenter"
        );
        assert_eq!(Role::All.app_id(), APP_ID);
        assert!(Role::All.runs(Role::Dock));
        assert!(Role::Dock.runs(Role::Dock));
        assert!(!Role::Dock.runs(Role::Faro));
    }

    #[test]
    fn windows_keep_the_focus_order_without_the_shell() {
        let mut w = Windows::default();
        w.changed("a", "org.gnome.Ptyxis", true);
        w.changed("b", "dev.frascella.Mail", true);
        w.changed("p", "dev.silica.Shell.Dock", true);
        w.focused("b");
        w.focused("a");
        assert_eq!(
            w.ordered(),
            vec![
                ("b".to_string(), "dev.frascella.Mail".to_string()),
                ("a".to_string(), "org.gnome.Ptyxis".to_string()),
            ]
        );
        w.changed("a", "", false);
        assert_eq!(w.ordered().len(), 1);
        assert!(!w.focus.contains(&"a".to_string()));
    }
}
