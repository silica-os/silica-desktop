//! System dialogs the shell draws: the Gatekeeper alerts (Trust1 prompts,
//! docs/design/amalfi/app-installation.md), the administrator authentication (polkit requests
//! through Authentication1), and the Force Quit panel (`app.force_quit_panel`). The alerts use
//! the one binding anatomy (components.md "Alerts (binding)"): default button first, the
//! Escape one last.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use futures_lite::StreamExt;
use vetro_lib::amalfi::alerts::{AlertButton, AlertSpec, ButtonKind, alert};
use vetro_lib::amalfi::fields::{FieldKind, TextField};
use vetro_lib::amalfi::label::SymbolImage;
use vetro_lib::amalfi::text::TextRole;
use vetro_lib::core::shell::ShellRequest;
use vetro_lib::core::{Prop, Proposal, Signal, Ui, Value, ViewId, WindowId, WindowKind};
use vetro_lib::services::types::AuthRequest;
use zbus::zvariant::OwnedValue;

use crate::apps;

/// The open dialogs.
#[derive(Default)]
pub struct DialogState {
    /// Gatekeeper prompts on screen, by id.
    prompts: HashMap<u32, WindowId>,
    /// Authentication requests on screen, by id: window, user field, password field.
    auth: HashMap<u32, (WindowId, ViewId, ViewId)>,
    /// The Force Quit panel.
    force_quit: Option<WindowId>,
}

/// Shared.
pub type Shared = Rc<RefCell<DialogState>>;

fn text(info: &HashMap<String, OwnedValue>, key: &str) -> String {
    info.get(key)
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_default()
}

/// Shows `spec` in a panel of its own, centred; `respond` gets the button index (or `None`
/// for Escape) and the window then closes.
fn present(
    ui: &mut Ui,
    spec: AlertSpec,
    title: &str,
    respond: impl Fn(&mut Ui, Option<usize>) + 'static,
) -> WindowId {
    let a = alert(ui, spec);
    let size = ui.measure(a, Proposal::new(None, None));
    let w = ui.create_window(WindowKind::Panel, size);
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
        win.title = title.into();
        win.decorated = false;
    }
    ui.set_content(w, a);
    ui.show_window(w);
    ui.set_focus(a, false);
    let r = Rc::new(respond);
    let r2 = r.clone();
    ui.connect(a, Signal::Activated, move |ui, _, v| {
        let i = v.as_int().and_then(|i| usize::try_from(i).ok());
        r(ui, i);
    });
    ui.connect(a, Signal::CloseRequested, move |ui, _, _| r2(ui, None));
    w
}

fn icon_view(ui: &mut Ui, image: Option<skia_safe::Image>, symbol: &str) -> ViewId {
    match image {
        Some(img) => ui.create(crate::dock::menus::IconView {
            image: img,
            size: 56.0,
        }),
        None => {
            let mut s = SymbolImage::new(symbol);
            s.size = 56.0;
            ui.create(s)
        }
    }
}

// ----- Gatekeeper ----------------------------------------------------------------------------

/// What a prompt shows: title, message, detail lines, buttons (choice, label, kind) with the
/// default first, and the choice of Escape.
pub struct PromptTexts {
    /// The title.
    pub title: String,
    /// The message.
    pub message: Option<String>,
    /// Detail lines (tertiary).
    pub detail: Vec<String>,
    /// The buttons.
    pub buttons: Vec<(&'static str, &'static str, ButtonKind)>,
    /// What Escape answers.
    pub escape: &'static str,
    /// The critical badge.
    pub badge: bool,
}

/// The host (or file name) of a quarantine origin.
fn origin_line(origin: &str) -> Option<String> {
    if origin.is_empty() {
        return None;
    }
    if let Some(rest) = origin.strip_prefix("file://") {
        return rest.rsplit('/').find(|p| !p.is_empty()).map(str::to_string);
    }
    let after = origin.split_once("://").map_or(origin, |x| x.1);
    let host = after.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    (!host.is_empty()).then(|| host.to_string())
}

/// The texts of a Gatekeeper prompt (app-installation.md "Variants").
pub fn prompt_texts(verdict: &str, info: &HashMap<String, OwnedValue>) -> PromptTexts {
    let name = {
        let n = text(info, "name");
        if n.is_empty() { text(info, "path") } else { n }
    };
    match verdict {
        "confirm" => {
            let dev = text(info, "developer");
            let developer = if text(info, "kind") == "appimage" || dev.is_empty() {
                "Developer: unknown".to_string()
            } else if text(info, "developer_status") == "verified" {
                format!("Developer: {dev}")
            } else {
                format!("Developer: {dev} (self-signed)")
            };
            let mut detail: Vec<String> = origin_line(&text(info, "origin")).into_iter().collect();
            detail.push(developer);
            PromptTexts {
                title: format!(
                    "\u{201c}{name}\u{201d} is an app downloaded from the Internet. Are you sure you want to open it?"
                ),
                message: None,
                detail,
                buttons: vec![
                    ("cancel", "Cancel", ButtonKind::Primary),
                    ("open", "Open", ButtonKind::Secondary),
                ],
                escape: "cancel",
                badge: false,
            }
        }
        "malware" => PromptTexts {
            title: format!("\u{201c}{name}\u{201d} will damage your computer."),
            message: Some("Its developer\u{2019}s key was revoked. Move it to the Trash.".into()),
            detail: Vec::new(),
            buttons: vec![
                ("trash", "Move to Trash", ButtonKind::Destructive),
                ("cancel", "Done", ButtonKind::Secondary),
            ],
            escape: "cancel",
            badge: true,
        },
        "volume" => PromptTexts {
            title: format!("\u{201c}{name}\u{201d} is running from a disk image."),
            message: Some(
                "Move it to Applications to keep it after the disk image is ejected.".into(),
            ),
            detail: Vec::new(),
            buttons: vec![
                ("move", "Move to Applications", ButtonKind::Primary),
                ("cancel", "Not Now", ButtonKind::Secondary),
            ],
            escape: "cancel",
            badge: false,
        },
        _ => PromptTexts {
            title: format!("\u{201c}{name}\u{201d} Not Opened"),
            message: Some(format!(
                "Silica cannot verify that \u{201c}{name}\u{201d} is free of malware or has not been modified."
            )),
            detail: Vec::new(),
            buttons: vec![
                ("cancel", "Done", ButtonKind::Primary),
                ("trash", "Move to Trash", ButtonKind::Secondary),
            ],
            escape: "cancel",
            badge: true,
        },
    }
}

fn answer(id: u32, choice: &'static str) {
    vetro_lib::platform::spawn(async move {
        if let Some(s) = vetro_lib::platform::services()
            && let Ok(p) = s.trust().await
        {
            let _ = p.answer(id, choice).await;
        }
    });
}

/// Shows Gatekeeper prompt `id`.
pub fn show_prompt(
    ui: &mut Ui,
    state: &Shared,
    id: u32,
    verdict: &str,
    info: &HashMap<String, OwnedValue>,
) {
    if state.borrow().prompts.contains_key(&id) {
        return;
    }
    let t = prompt_texts(verdict, info);
    let dark = ui.appearance.dark;
    let app_icon = {
        let app = text(info, "id");
        apps::entry(&app).and_then(|e| apps::icon(&e, dark))
    };
    let icon = icon_view(ui, app_icon, "app");
    let (message, role) = match (&t.message, t.detail.is_empty()) {
        (Some(m), _) => (Some(m.clone()), None),
        (None, false) => (Some(t.detail.join("\n")), Some(TextRole::Tertiary)),
        (None, true) => (None, None),
    };
    let choices: Vec<&'static str> = t.buttons.iter().map(|b| b.0).collect();
    let spec = AlertSpec {
        icon: Some(icon),
        title: t.title.clone(),
        message,
        grouped_text: true,
        buttons: t
            .buttons
            .iter()
            .map(|(_, l, k)| AlertButton::new(l, *k))
            .collect(),
        stacked: true,
        wide_buttons: true,
        badge: t.badge,
        message_role: role,
        ..AlertSpec::default()
    };
    let escape = t.escape;
    let st = state.clone();
    let w = present(ui, spec, "Gatekeeper", move |ui, i| {
        let choice = i.and_then(|i| choices.get(i).copied()).unwrap_or(escape);
        answer(id, choice);
        if let Some(w) = st.borrow_mut().prompts.remove(&id) {
            ui.close_window(w);
        }
    });
    state.borrow_mut().prompts.insert(id, w);
}

/// A prompt ended elsewhere (answered or timed out).
pub fn prompt_closed(ui: &mut Ui, state: &Shared, id: u32) {
    if let Some(w) = state.borrow_mut().prompts.remove(&id) {
        ui.close_window(w);
    }
}

// ----- Authentication ------------------------------------------------------------------------

/// Shows authentication request `r`: the app's icon, the title, the user name and password
/// fields, Allow and Cancel.
pub fn show_auth(ui: &mut Ui, state: &Shared, r: &AuthRequest) {
    if state.borrow().auth.contains_key(&r.id) {
        return;
    }
    let dark = ui.appearance.dark;
    let image = apps::entry(&r.app)
        .and_then(|e| apps::icon(&e, dark))
        .or_else(|| apps::icon_named(&r.icon));
    let icon = icon_view(ui, image, "lock");
    let title = if r.app.is_empty() {
        "A system change needs an administrator\u{2019}s permission.".to_string()
    } else {
        let name = apps::entry(&r.app).map_or(r.app.clone(), |e| e.name);
        format!("{name} needs an administrator\u{2019}s permission.")
    };
    let user = ui.create(TextField::new(FieldKind::Text));
    let _ = ui.set(user, Prop::Placeholder, Value::from("User name"));
    let default_user = r
        .users
        .iter()
        .find(|(u, _)| *u == r.default_user)
        .or(r.users.first())
        .map(|(u, _)| u.clone())
        .unwrap_or_default();
    let _ = ui.set(user, Prop::Text, Value::Str(default_user));
    let pass = ui.create(TextField::new(FieldKind::Secure));
    let _ = ui.set(pass, Prop::Placeholder, Value::from("Password"));
    for f in [user, pass] {
        let _ = ui.set(f, Prop::Width, Value::from(240.0));
    }
    let spec = AlertSpec {
        icon: Some(icon),
        title,
        message: Some(if r.message.is_empty() {
            "Type the password to allow it.".into()
        } else {
            r.message.clone()
        }),
        grouped_text: true,
        accessories: vec![user, pass],
        buttons: vec![
            AlertButton::new("Allow", ButtonKind::Primary),
            AlertButton::new("Cancel", ButtonKind::Secondary),
        ],
        stacked: true,
        wide_buttons: true,
        ..AlertSpec::default()
    };
    let id = r.id;
    let st = state.clone();
    let w = present(ui, spec, "Authentication", move |ui, i| {
        if i == Some(0) {
            let u = ui
                .get(user, Prop::Text)
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            let p = ui
                .get(pass, Prop::Text)
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            vetro_lib::platform::spawn(async move {
                if let Some(s) = vetro_lib::platform::services()
                    && let Ok(proxy) = s.authentication().await
                {
                    // A wrong password keeps the dialog: it closes on the Closed signal.
                    let _ = proxy.respond(id, &u, &p).await;
                }
            });
            let _ = ui.set(pass, Prop::Text, Value::Str(String::new()));
        } else {
            vetro_lib::platform::spawn(async move {
                if let Some(s) = vetro_lib::platform::services()
                    && let Ok(proxy) = s.authentication().await
                {
                    let _ = proxy.cancel(id).await;
                }
            });
            if let Some((w, _, _)) = st.borrow_mut().auth.remove(&id) {
                ui.close_window(w);
            }
        }
    });
    // A wrong password keeps the dialog; it goes when the daemon says the request closed.
    state.borrow_mut().auth.insert(id, (w, user, pass));
    ui.set_focus(pass, true);
}

/// An authentication request ended.
pub fn auth_closed(ui: &mut Ui, state: &Shared, id: u32) {
    if let Some((w, _, _)) = state.borrow_mut().auth.remove(&id) {
        ui.close_window(w);
    }
}

/// Follows Trust1 prompts and Authentication1 requests.
pub fn start(state: &Shared) {
    let st = state.clone();
    vetro_lib::platform::spawn(async move {
        let Some(services) = vetro_lib::platform::services() else {
            return;
        };
        let trust = async {
            let Ok(p) = services.trust().await else {
                return;
            };
            let _ = p.register_agent().await;
            if let Ok(list) = p.pending().await {
                let s = st.clone();
                vetro_lib::platform::post(move |ui| {
                    for x in list {
                        show_prompt(ui, &s, x.id, &x.verdict, &x.info);
                    }
                });
            }
            let (Ok(mut req), Ok(mut closed)) = (
                p.receive_prompt_requested().await,
                p.receive_prompt_closed().await,
            ) else {
                return;
            };
            let s1 = st.clone();
            let a = async move {
                while let Some(sig) = req.next().await {
                    if let Ok(x) = sig.args() {
                        let (id, verdict, info) = (x.id, x.verdict.to_string(), x.info.clone());
                        let s = s1.clone();
                        vetro_lib::platform::post(move |ui| {
                            show_prompt(ui, &s, id, &verdict, &info)
                        });
                    }
                }
            };
            let s2 = st.clone();
            let b = async move {
                while let Some(sig) = closed.next().await {
                    if let Ok(x) = sig.args() {
                        let id = x.id;
                        let s = s2.clone();
                        vetro_lib::platform::post(move |ui| prompt_closed(ui, &s, id));
                    }
                }
            };
            futures_lite::future::zip(a, b).await;
        };
        let auth = async {
            let Ok(p) = services.authentication().await else {
                return;
            };
            if let Ok(list) = p.pending().await {
                let s = st.clone();
                vetro_lib::platform::post(move |ui| {
                    for r in list {
                        show_auth(ui, &s, &r);
                    }
                });
            }
            let (Ok(mut req), Ok(mut closed)) =
                (p.receive_requested().await, p.receive_closed().await)
            else {
                return;
            };
            let s1 = st.clone();
            let a = async move {
                while let Some(sig) = req.next().await {
                    if let Ok(x) = sig.args() {
                        let r = x.request.clone();
                        let s = s1.clone();
                        vetro_lib::platform::post(move |ui| show_auth(ui, &s, &r));
                    }
                }
            };
            let s2 = st.clone();
            let b = async move {
                while let Some(sig) = closed.next().await {
                    if let Ok(x) = sig.args() {
                        let id = x.id;
                        let s = s2.clone();
                        vetro_lib::platform::post(move |ui| auth_closed(ui, &s, id));
                    }
                }
            };
            futures_lite::future::zip(a, b).await;
        };
        futures_lite::future::zip(trust, auth).await;
    });
}

// ----- Force Quit ----------------------------------------------------------------------------

/// Opens the Force Quit panel over the running apps (`windows`: window, app, most recent last).
pub fn force_quit(ui: &mut Ui, state: &Shared, windows: &[(String, String)]) {
    if let Some(w) = state.borrow_mut().force_quit.take() {
        ui.close_window(w);
    }
    let mut apps_list: Vec<(String, String)> = Vec::new();
    for (_, app) in windows.iter().rev() {
        if app.is_empty() || apps_list.iter().any(|(a, _)| a == app) {
            continue;
        }
        let name = apps::entry(app).map_or_else(
            || app.rsplit('.').next().unwrap_or(app).to_string(),
            |e| e.name,
        );
        apps_list.push((app.clone(), name));
    }
    let rows: Vec<String> = apps_list.iter().map(|(_, n)| n.clone()).collect();
    let list = crate::dialogs::list_view(ui, &rows);
    let spec = AlertSpec {
        icon: None,
        title: "Quit apps".into(),
        message: Some("If an app does not respond, choose it in the list and click \u{201c}Quit\u{201d}. This panel also opens with \u{2318}\u{2325}Esc.".into()),
        accessories: vec![list.0],
        buttons: vec![
            AlertButton::new("Quit", ButtonKind::Destructive),
            AlertButton::new("Cancel", ButtonKind::Secondary),
        ],
        stacked: true,
        wide_buttons: true,
        ..AlertSpec::default()
    };
    let st = state.clone();
    let chosen = list.1;
    let w = present(ui, spec, "Force Quit", move |ui, i| {
        if i == Some(0)
            && let Some((app, _)) = chosen.borrow().and_then(|k| apps_list.get(k))
        {
            let params = serde_json::json!({ "app_id": app }).to_string();
            ui.shell(ShellRequest::ExecuteWith("app.force_quit".into(), params));
        }
        if let Some(w) = st.borrow_mut().force_quit.take() {
            ui.close_window(w);
        }
    });
    state.borrow_mut().force_quit = Some(w);
}

/// A simple selectable list (the Force Quit apps); returns its view and the chosen row.
pub fn list_view(ui: &mut Ui, rows: &[String]) -> (ViewId, Rc<RefCell<Option<usize>>>) {
    let chosen = Rc::new(RefCell::new((!rows.is_empty()).then_some(0)));
    let v = ui.create(AppList {
        rows: rows.to_vec(),
        chosen: chosen.clone(),
        labels: Vec::new(),
    });
    let _ = ui.set(v, Prop::Width, Value::from(240.0));
    (v, chosen)
}

/// The Force Quit list: 22 pt rows in a bordered box, the chosen one highlighted.
struct AppList {
    rows: Vec<String>,
    chosen: Rc<RefCell<Option<usize>>>,
    labels: Vec<vetro_lib::amalfi::foundation::Label>,
}

impl vetro_lib::core::Widget for AppList {
    fn kind(&self) -> &'static str {
        "List"
    }

    fn measure(
        &mut self,
        _cx: &mut vetro_lib::core::LayoutCx<'_>,
        p: Proposal,
    ) -> vetro_lib::geom::Size {
        let w = p.width.filter(|w| w.is_finite()).unwrap_or(240.0);
        vetro_lib::geom::Size::new(w, (self.rows.len().max(4) as f32 * 22.0 + 8.0).min(200.0))
    }

    fn paint(&mut self, cx: &mut vetro_lib::core::PaintCx<'_>) {
        use vetro_lib::shape::Shape;
        use vetro_lib::style::{Border, Style};
        let theme = *cx.theme();
        let c = theme.colors;
        let r = cx.bounds();
        cx.rec.layer(
            "Box",
            r,
            Shape::rounded(6.0),
            Style::fill(c.surface_sunken).with_border(Border::inside(c.line_border, 1.0)),
        );
        let chosen = *self.chosen.borrow();
        for (i, name) in self.rows.iter().enumerate() {
            let row = vetro_lib::geom::Rect::new(
                r.x + 4.0,
                r.y + 4.0 + i as f32 * 22.0,
                r.width - 8.0,
                22.0,
            );
            if row.bottom() > r.bottom() {
                break;
            }
            let selected = chosen == Some(i);
            if selected {
                cx.rec.layer(
                    "Selection",
                    row,
                    Shape::rounded(4.0),
                    Style::fill(c.accent_amalfi),
                );
            }
            let ink = if selected {
                c.text_on_accent
            } else {
                c.text_primary
            };
            let style = theme.text(&vetro_lib::amalfi::tokens::typography::BODY, ink);
            if self.labels.len() <= i {
                self.labels.resize_with(i + 1, Default::default);
            }
            if let Some(l) = self.labels.get_mut(i) {
                let lay = l.layout(
                    name,
                    &style,
                    vetro_lib::text::Align::Start,
                    row.width - 16.0,
                    Some(1),
                );
                let h = lay.size().height;
                cx.rec.text(
                    "Name",
                    vetro_lib::geom::Rect::new(
                        row.x + 8.0,
                        row.y + (22.0 - h) / 2.0,
                        row.width - 16.0,
                        h,
                    ),
                    lay,
                    None,
                    ink,
                );
            }
        }
    }

    fn event(
        &mut self,
        cx: &mut vetro_lib::core::EventCx<'_>,
        event: &vetro_lib::core::Event,
    ) -> vetro_lib::core::Handled {
        if let vetro_lib::core::Event::PointerDown { pos, .. } = event {
            let r = cx.bounds();
            let i = ((pos.y - r.y - 4.0) / 22.0).floor();
            if i >= 0.0 && (i as usize) < self.rows.len() {
                *self.chosen.borrow_mut() = Some(i as usize);
                cx.request_paint();
            }
            return vetro_lib::core::Handled::Yes;
        }
        vetro_lib::core::Handled::No
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_put_the_default_first() {
        let info = HashMap::new();
        let t = prompt_texts("confirm", &info);
        assert_eq!(t.buttons[0].0, "cancel");
        assert_eq!(t.detail, vec!["Developer: unknown".to_string()]);
        assert_eq!(
            prompt_texts("malware", &info).buttons[0].2,
            ButtonKind::Destructive
        );
        assert_eq!(prompt_texts("block", &info).buttons[0].1, "Done");
        assert_eq!(
            origin_line("https://user@lemon.example.org/a?b").as_deref(),
            Some("lemon.example.org")
        );
        assert_eq!(
            origin_line("file:///home/a/Lemon.sxv").as_deref(),
            Some("Lemon.sxv")
        );
    }
}
