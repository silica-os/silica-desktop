//! `silica-quarantine`: marks, shows or clears the quarantine attribute of files (silica-os
//! `docs/app-bundles.md` § 6).

use std::path::Path;
use std::process::ExitCode;

use silica_bundle::quarantine::{self, BY_HAND, Quarantine};

const USAGE: &str = "usage: silica-quarantine FILE [ORIGIN]   mark (origin: a URL)
       silica-quarantine --show FILE...
       silica-quarantine --clear FILE...";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--show") => {
            let mut code = ExitCode::SUCCESS;
            for file in args.get(1..).unwrap_or_default() {
                match quarantine::read(Path::new(file)) {
                    Some(q) => println!(
                        "{file}: flags {:04x}, time {}, agent {:?}, origin {:?}",
                        q.flags, q.time, q.agent, q.origin
                    ),
                    None => {
                        println!("{file}: not in quarantine");
                        code = ExitCode::FAILURE;
                    }
                }
            }
            code
        }
        Some("--clear") => {
            for file in args.get(1..).unwrap_or_default() {
                if let Err(e) = quarantine::clear(Path::new(file)) {
                    eprintln!("silica-quarantine: {file}: {e}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Some("-h" | "--help") | None => {
            println!("{USAGE}");
            ExitCode::from(2)
        }
        Some(file) => {
            let origin = args.get(1).map(String::as_str).unwrap_or_default();
            match quarantine::write(Path::new(file), &Quarantine::now(BY_HAND, "", origin)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("silica-quarantine: {file}: {e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
