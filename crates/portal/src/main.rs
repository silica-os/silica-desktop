//! `silica-portal`: the xdg-desktop-portal backend of Silica on the session bus.

use std::collections::HashMap;
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use silica_portal::{
    BUS_NAME, DialogKind, DialogRequest, OBJECT_PATH, RESPONSE_CANCELLED, RESPONSE_OK,
    RESPONSE_OTHER, dialog_command, find_mustache_exec, parent_handle, parse_uris, path_from_bytes,
};
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

type Results = HashMap<String, OwnedValue>;

struct FileChooser;

fn option_bool(options: &HashMap<String, OwnedValue>, key: &str) -> bool {
    options
        .get(key)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}

fn option_string(options: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    options
        .get(key)
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .filter(|s| !s.is_empty())
}

fn option_path(options: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let value = options.get(key)?.try_clone().ok()?;
    let bytes = Vec::<u8>::try_from(value).ok()?;
    path_from_bytes(&bytes)
}

fn request(
    parent_window: &str,
    title: &str,
    options: &HashMap<String, OwnedValue>,
) -> DialogRequest {
    DialogRequest {
        title: title.to_owned(),
        accept_label: option_string(options, "accept_label"),
        multiple: option_bool(options, "multiple"),
        directory: option_bool(options, "directory"),
        current_name: option_string(options, "current_name"),
        current_folder: option_path(options, "current_folder"),
        parent: parent_handle(parent_window),
    }
}

/// Opens Mustache's dialog and waits for the choice, without blocking the bus.
async fn run_dialog(kind: DialogKind, request: DialogRequest) -> (u32, Results) {
    let env: HashMap<String, String> = std::env::vars().collect();
    let Some(exec) = find_mustache_exec(&env) else {
        eprintln!(
            "silica-portal: Mustache is not installed ({})",
            silica_portal::MUSTACHE_ID
        );
        return (RESPONSE_OTHER, Results::new());
    };
    let words = dialog_command(&exec, kind, &request);
    let Some((program, args)) = words.split_first() else {
        return (RESPONSE_OTHER, Results::new());
    };
    let child = std::process::Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(err) => {
            eprintln!("silica-portal: {program}: {err}");
            return (RESPONSE_OTHER, Results::new());
        }
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => async_io::Timer::after(Duration::from_millis(50)).await,
            Err(err) => {
                eprintln!("silica-portal: {err}");
                return (RESPONSE_OTHER, Results::new());
            }
        };
    };
    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read;
        if out.read_to_string(&mut stdout).is_err() {
            stdout.clear();
        }
    }
    let uris = parse_uris(&stdout);
    if !status.success() || uris.is_empty() {
        return (RESPONSE_CANCELLED, Results::new());
    }
    let mut results = Results::new();
    if let Ok(value) = OwnedValue::try_from(Value::from(uris)) {
        results.insert("uris".to_owned(), value);
    }
    (RESPONSE_OK, results)
}

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooser {
    async fn open_file(
        &self,
        _handle: ObjectPath<'_>,
        _app_id: &str,
        parent_window: &str,
        title: &str,
        options: HashMap<String, OwnedValue>,
    ) -> (u32, Results) {
        run_dialog(DialogKind::Open, request(parent_window, title, &options)).await
    }

    async fn save_file(
        &self,
        _handle: ObjectPath<'_>,
        app_id: &str,
        parent_window: &str,
        title: &str,
        options: HashMap<String, OwnedValue>,
    ) -> (u32, Results) {
        let (response, results) =
            run_dialog(DialogKind::Save, request(parent_window, title, &options)).await;
        // What an app saves arrives from outside: the quarantine attribute (docs/spec/trust.md).
        if response == RESPONSE_OK
            && let Some(uris) = results
                .get("uris")
                .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
        {
            for uri in uris {
                if let Some(path) = uri
                    .strip_prefix("file://")
                    .and_then(|p| silica_portal::path_from_bytes(&percent_decode(p)))
                    && let Err(err) = silica_portal::mark_saved(std::path::Path::new(&path), app_id)
                {
                    eprintln!("silica-portal: quarantine {path}: {err}");
                }
            }
        }
        (response, results)
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        4
    }
}

/// Decodes `%XX` escapes of a URI path.
fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%'
            && let Some(hex) = bytes.get(i + 1..i + 3)
            && let Ok(text) = std::str::from_utf8(hex)
            && let Ok(v) = u8::from_str_radix(text, 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b);
        i += 1;
    }
    out
}

const USAGE: &str = "usage: silica-portal [--address ADDRESS]

  --address ADDRESS    bus to connect to (default: DBUS_SESSION_BUS_ADDRESS)";

fn main() -> ExitCode {
    let mut address = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--address" => match args.next() {
                Some(value) => address = Some(value),
                None => {
                    eprintln!("silica-portal: --address needs an address\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("silica-portal: unknown option {other}\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let outcome: zbus::Result<()> = zbus::block_on(async move {
        let builder = match &address {
            Some(address) => zbus::connection::Builder::address(address.as_str())?,
            None => zbus::connection::Builder::session()?,
        };
        // Appearance: from the file until the daemon answers, then from the daemon.
        let settings = silica_portal::settings::Settings {
            dark: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                silica_portal::settings::appearance_file()
                    .is_some_and(|f| silica_portal::settings::read_file(&f)),
            )),
            accent: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(
                silica_portal::settings::NO_ACCENT,
            )),
        };
        let connection = builder
            .name(BUS_NAME)?
            .serve_at(OBJECT_PATH, FileChooser)?
            .serve_at(OBJECT_PATH, settings.clone())?
            .build()
            .await?;
        eprintln!(
            "silica-portal: FileChooser e Settings su {}",
            connection
                .unique_name()
                .map_or("?".to_owned(), |n| n.to_string())
        );
        // The daemon may not be there yet: retry every 2 s. If the bus closes (Ossido's
        // private bus dies with Ossido) the portal exits.
        let follow = async {
            loop {
                if let Err(err) =
                    silica_portal::settings::follow_daemon(&connection, &settings, OBJECT_PATH)
                        .await
                {
                    eprintln!("silica-portal: appearance from the daemon: {err}");
                }
                async_io::Timer::after(Duration::from_secs(2)).await;
            }
        };
        let lost = async {
            use futures_lite::StreamExt;
            let mut messages = zbus::MessageStream::from(&connection);
            while let Some(Ok(_)) = messages.next().await {}
            Err(zbus::Error::Failure("session bus closed".to_owned()))
        };
        futures_lite::future::or(follow, lost).await
    });
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("silica-portal: {err}");
            ExitCode::FAILURE
        }
    }
}
