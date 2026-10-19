//! `silica-daemon`: Silica's system services on the session bus.

use std::process::ExitCode;

use futures_lite::{StreamExt, future};
use silica_daemon::actions::{ACTIONS_PATH, Actions};
use silica_daemon::apps::{AppsConfig, AppsService};
use silica_daemon::settings::{SETTINGS_PATH, Settings};
use silica_daemon::share::{SHARE_PATH, Share};
use silica_daemon::sound::{SOUND_PATH, Sound, SoundConfig};
use silica_daemon::thumbnails::{THUMBNAILS_PATH, Thumbnails};
use silica_daemon::{
    AuthConfig, AuthService, ClipboardService, Daemon, SystemConfig, SystemService,
};

const USAGE: &str = "usage: silica-daemon [--address ADDRESS] [--replace]

  --address ADDRESS    bus to connect to (default: DBUS_SESSION_BUS_ADDRESS)
  --replace            takes the names even if another server holds them";

fn main() -> ExitCode {
    let mut address = None;
    let mut replace = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--address" => match args.next() {
                Some(value) => address = Some(value),
                None => {
                    eprintln!("silica-daemon: --address needs an address\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--replace" => replace = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("silica-daemon: unknown option {other}\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    match zbus::block_on(run(address, replace)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("silica-daemon: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn run(address: Option<String>, replace: bool) -> zbus::Result<()> {
    let builder = match address {
        Some(address) => zbus::connection::Builder::address(address.as_str())?,
        None => zbus::connection::Builder::session()?,
    };
    let connection = builder.build().await?;
    // The system bus is for the Control Center; without it, those sources are absent.
    let system_bus = match zbus::Connection::system().await {
        Ok(bus) => Some(bus),
        Err(err) => {
            eprintln!("silica-daemon: system bus unavailable: {err}");
            None
        }
    };
    // The polkit agent: the password dialog for privileged settings.
    let auth = AuthService::serve(
        connection.clone(),
        AuthConfig {
            system_bus: system_bus.clone(),
            ..AuthConfig::default()
        },
    )
    .await?;
    let system = SystemService::serve(
        connection.clone(),
        SystemConfig {
            system_bus,
            ..SystemConfig::default()
        },
    )
    .await?;
    // Launch Services: the app registry, kept current, for the shell, the apps and the rest of
    // the daemon (app icons, the Share menu).
    let apps = AppsService::serve(
        connection.clone(),
        AppsConfig::from_env(&|k| std::env::var(k).ok()),
    )
    .await?;
    // Faro's clipboard and actions scopes.
    let clipboard = ClipboardService::serve(connection.clone()).await?;
    connection.object_server().at(ACTIONS_PATH, Actions).await?;
    // The Share menu of apps: its targets and the sharing.
    connection.object_server().at(SHARE_PATH, Share).await?;
    // Thumbnails of files for apps and the shell, cached per the freedesktop specification.
    connection
        .object_server()
        .at(THUMBNAILS_PATH, Thumbnails::from_env())
        .await?;
    // The typed settings the shell applies live and Settings edits.
    connection
        .object_server()
        .at(SETTINGS_PATH, Settings::from_env())
        .await?;
    // The interface sound effects (the Trash, copies), for apps and the shell.
    connection
        .object_server()
        .at(SOUND_PATH, Sound::new(SoundConfig::default()))
        .await?;
    let daemon = Daemon::serve(connection, replace).await?;
    eprintln!(
        "silica-daemon: notifiche su {}",
        daemon
            .connection()
            .unique_name()
            .map_or("?".to_owned(), |n| n.to_string())
    );
    // Ossido's private bus closes with Ossido: then the daemon exits.
    let lost = async {
        let mut messages = zbus::MessageStream::from(daemon.connection());
        while let Some(Ok(_)) = messages.next().await {}
        Err(zbus::Error::Failure("session bus closed".to_owned()))
    };
    // Lost one of its names to another daemon (`--replace`, or the D-Bus-activated instance
    // racing the spawned one): this one goes away instead of running next to it.
    let replaced = async {
        let name = daemon.name_lost().await?;
        eprintln!("silica-daemon: lost {name} to another daemon, exiting");
        Ok(())
    };
    future::or(
        future::or(
            future::or(
                future::or(
                    future::or(future::or(daemon.run(), system.run()), clipboard.run()),
                    apps.run(),
                ),
                auth.run(),
            ),
            lost,
        ),
        replaced,
    )
    .await
}
