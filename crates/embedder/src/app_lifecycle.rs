//! The app's lifecycle, as on the reference (silica-desktop#142, measured in APP-LC).
//!
//! Closing an app's last window does not end the app: the window goes away (Ossido hides it,
//! `silica-window-v1` › `set_hidden`) and the app keeps running with no window, frontmost,
//! with its Dock dot. The app learns it (`lastWindowClosed`) and drops the closed window's
//! content. A reopen (a click in the Dock, launching the app again) is an event the app
//! answers by building a new window (`reopen`, then `openWindow`), as the reference does: a
//! document app shows its Open panel, Mail its main window; the closed window never comes
//! back as it was (that would be minimizing). Single-window apps (Calculator, Clock,
//! Settings) end with their last window instead. Only Quit ends an app; the file manager
//! has none. The reference never ends a windowless app by itself.
//!
//! Every app process registers itself in the app registry (silica-control `apps`): a unix
//! socket answering `status`, `reopen` and `quit`. A second launch of a running app hands
//! over to it (`reopen`) and exits.
//!
//! The `silica/app` channel (JSONMethodCodec):
//! - from the app: `setQuitWhenLastWindowCloses {value}`, `setHandlesReopen {value}`,
//!   `setQuitEnabled {value}`, `openWindow`, `closeWindow`, `quit`, `isHidden`, `appInfo`
//!   (`{appId, bundle}`: the app id and the Flutter bundle it runs, for its About panel);
//! - to the app: `lastWindowClosed`, `reopen {windowless}`, `open {args}` (the files of a
//!   second launch, `open <JSON array>` on the socket).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use silica_control::apps;

pub const CHANNEL: &str = "silica/app";

/// How often a windowless app checks the debug idle limit.
pub const CHECK_PERIOD: Duration = Duration::from_secs(30);

/// How long a registry client may take to send its request.
const CLIENT_TIMEOUT: Duration = Duration::from_millis(200);

/// A request on the registry socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Status,
    Reopen,
    Quit,
    /// A second launch with arguments (files): they go to this process.
    Open(Vec<String>),
}

pub fn parse_request(line: &str) -> Option<Request> {
    let line = line.trim();
    if let Some(list) = line.strip_prefix("open ") {
        return serde_json::from_str::<Vec<String>>(list)
            .ok()
            .map(Request::Open);
    }
    match line {
        "status" => Some(Request::Status),
        "reopen" => Some(Request::Reopen),
        "quit" => Some(Request::Quit),
        _ => None,
    }
}

/// This process in the app registry: the socket file goes away with it.
pub struct Registration {
    path: PathBuf,
    parked: PathBuf,
}

impl Registration {
    /// Writes the `.parked` marker: the system ends this windowless app.
    pub fn park(&self) {
        if let Err(err) = std::fs::write(&self.parked, b"") {
            tracing::warn!("app registry: {}: {err}", self.parked.display());
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Registers this process of `app_id`; the listener is non-blocking, for the event loop.
pub fn register(app_id: &str) -> std::io::Result<(Registration, UnixListener)> {
    if !apps::valid_app_id(app_id) {
        return Err(std::io::Error::other(format!(
            "app id not usable: {app_id}"
        )));
    }
    let dir = apps::dir().ok_or_else(|| std::io::Error::other("XDG_RUNTIME_DIR is not set"))?;
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    // Started again: whatever the system ended is running now.
    let parked = apps::parked_path(&dir, app_id);
    let _ = std::fs::remove_file(&parked);
    let path = apps::socket_path(&dir, app_id, std::process::id());
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    Ok((Registration { path, parked }, listener))
}

/// A second launch: if another process of `app_id` runs (with or without a window), it gets
/// the reopen and this process has nothing to do (one instance per app).
pub fn forward(app_id: &str) -> bool {
    apps::valid_app_id(app_id) && apps::reopen(app_id, true)
}

/// A second launch with arguments (files): a running process of `app_id` gets them
/// (`open`) and this process has nothing to do.
pub fn forward_open(app_id: &str, args: &[String]) -> bool {
    apps::valid_app_id(app_id) && apps::open(app_id, args)
}

/// Reads one request from a registry client (bounded, with a short timeout).
pub fn read_request(stream: &UnixStream) -> Option<Request> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(CLIENT_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(CLIENT_TIMEOUT)).ok()?;
    let mut line = String::new();
    BufReader::new(std::io::Read::take(stream, apps::MAX_OPEN_LINE as u64))
        .read_line(&mut line)
        .ok()?;
    parse_request(&line)
}

/// Answers a registry client with one line.
pub fn answer(mut stream: &UnixStream, text: &str) {
    let _ = stream
        .write_all(text.as_bytes())
        .and_then(|()| stream.write_all(b"\n"));
}

/// The debug idle limit: `SILICA_APP_IDLE_LIMIT` seconds ends a windowless app after that
/// long. Off by default: the reference keeps windowless apps running (APP-LC).
pub fn idle_limit() -> Option<Duration> {
    std::env::var("SILICA_APP_IDLE_LIMIT")
        .ok()
        .and_then(|secs| secs.parse::<u64>().ok())
        .map(Duration::from_secs)
}

/// How often to check: [`CHECK_PERIOD`], sooner with a short idle limit.
pub fn check_period(limit: Duration) -> Duration {
    CHECK_PERIOD.min(limit.max(Duration::from_secs(1)))
}

/// The answer to `appInfo` on `silica/app`: the app id and the Flutter bundle it runs (the
/// About panel finds the app's `icon/` folder next to the bundle, silica-ui#99).
pub fn app_info(app_id: &str, bundle: &std::path::Path) -> serde_json::Value {
    serde_json::json!({
        "appId": app_id,
        "bundle": bundle.to_string_lossy(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_names_the_app_and_its_bundle() {
        let info = app_info(
            "dev.frascella.Calculator",
            std::path::Path::new(
                "/var/lib/silica-dev/apps/dev.frascella.Calculator/flutter_assets",
            ),
        );
        assert_eq!(info["appId"], "dev.frascella.Calculator");
        assert_eq!(
            info["bundle"],
            "/var/lib/silica-dev/apps/dev.frascella.Calculator/flutter_assets"
        );
    }

    #[test]
    fn requests_are_parsed() {
        assert_eq!(parse_request("status\n"), Some(Request::Status));
        assert_eq!(parse_request(" reopen "), Some(Request::Reopen));
        assert_eq!(parse_request("quit"), Some(Request::Quit));
        assert_eq!(parse_request("rm -rf"), None);
        assert_eq!(
            parse_request("open [\"/home/a/b c.png\"]\n"),
            Some(Request::Open(vec!["/home/a/b c.png".to_owned()]))
        );
        assert_eq!(parse_request("open not-json"), None);
    }

    #[test]
    fn no_idle_limit_by_default() {
        assert_eq!(check_period(Duration::from_secs(5)), Duration::from_secs(5));
        assert_eq!(check_period(Duration::from_secs(3600)), CHECK_PERIOD);
    }
}
