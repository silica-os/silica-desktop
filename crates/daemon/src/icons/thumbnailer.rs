//! Freedesktop thumbnailers: `$XDG_DATA_DIRS/thumbnailers/*.thumbnailer`
//! (Thumbnail Managing Standard, "Thumbnailer Entry"). An app that opens a type can ship one to
//! draw the preview of its documents. The service runs it in a subprocess that is bounded in
//! time, in output size and in what it is given (`Exec` words only, no shell), and caches the
//! picture like every other preview (docs/thumbnails.md).
//!
//! Keys: `TryExec` (a program that must exist), `Exec` (`%i` input path, `%u` input URI, `%o`
//! output PNG path, `%s` size in pixels, `%%`), `MimeType` (the types it can draw).

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::apps::Dirs;

/// How long a thumbnailer may run before it is killed.
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// The largest picture a thumbnailer may write.
const MAX_OUTPUT: u64 = 64 * 1024 * 1024;

/// A `.thumbnailer` file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Thumbnailer {
    pub try_exec: Option<String>,
    pub exec: String,
    pub mime_types: Vec<String>,
}

/// Parses a `.thumbnailer` text; None without `Exec` or `MimeType`.
pub fn parse(text: &str) -> Option<Thumbnailer> {
    let mut out = Thumbnailer::default();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[Thumbnailer Entry]";
        } else if inside && let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            match key.trim() {
                "TryExec" if !value.is_empty() => out.try_exec = Some(value.to_owned()),
                "Exec" => out.exec = value.to_owned(),
                "MimeType" => {
                    out.mime_types = value
                        .split(';')
                        .map(str::trim)
                        .filter(|m| !m.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
                _ => {}
            }
        }
    }
    (!out.exec.is_empty() && !out.mime_types.is_empty()).then_some(out)
}

/// Splits an `Exec` value into words: spaces separate, double quotes group, a backslash escapes
/// inside quotes.
pub fn split_exec(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(current);
    }
    words
}

/// What the field codes become.
pub struct Fields<'a> {
    pub input: &'a Path,
    pub uri: &'a str,
    pub output: &'a Path,
    pub size: u32,
}

/// Replaces the field codes of one word.
fn expand(word: &str, fields: &Fields<'_>) -> String {
    let mut out = String::new();
    let mut chars = word.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('i') => out.push_str(&fields.input.to_string_lossy()),
            Some('u') => out.push_str(fields.uri),
            Some('o') => out.push_str(&fields.output.to_string_lossy()),
            Some('s') => out.push_str(&fields.size.to_string()),
            Some('%') => out.push('%'),
            _ => {}
        }
    }
    out
}

/// The program and arguments of a run; None when `Exec` has no program or never says where the
/// output goes.
pub fn command_line(
    thumbnailer: &Thumbnailer,
    fields: &Fields<'_>,
) -> Option<(String, Vec<String>)> {
    if !thumbnailer.exec.contains("%o") {
        return None;
    }
    let mut words = split_exec(&thumbnailer.exec)
        .into_iter()
        .map(|w| expand(&w, fields));
    let program = words.next().filter(|p| !p.is_empty())?;
    Some((program, words.collect()))
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Finds a program: an absolute path as it is, else in `path_var` (`PATH`).
pub fn find_program(program: &str, path_var: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let path = PathBuf::from(program);
        return (path.is_absolute() && is_executable(&path)).then_some(path);
    }
    path_var
        .split(':')
        .filter(|d| d.starts_with('/'))
        .map(|d| Path::new(d).join(program))
        .find(|p| is_executable(p))
}

/// Every thumbnailer of the data directories, the first with a given file name winning.
#[derive(Clone, Debug, Default)]
pub struct Thumbnailers {
    list: Vec<Thumbnailer>,
}

impl Thumbnailers {
    pub fn from_list(list: Vec<Thumbnailer>) -> Thumbnailers {
        Thumbnailers { list }
    }

    pub fn load(dirs: &Dirs) -> Thumbnailers {
        let mut seen = HashSet::new();
        let mut list = Vec::new();
        let roots = std::iter::once(&dirs.data_home).chain(&dirs.data_dirs);
        for root in roots {
            let Ok(read) = fs::read_dir(root.join("thumbnailers")) else {
                continue;
            };
            let mut files: Vec<_> = read
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "thumbnailer"))
                .collect();
            files.sort_by_key(|e| e.file_name());
            for file in files {
                if !seen.insert(file.file_name()) {
                    continue;
                }
                if let Some(t) = fs::read_to_string(file.path())
                    .ok()
                    .and_then(|text| parse(&text))
                {
                    list.push(t);
                }
            }
        }
        Thumbnailers { list }
    }

    /// The thumbnailers able to draw one of `chain` (the type and its parents), those of the
    /// nearest type first.
    pub fn find(&self, chain: &[String]) -> Vec<&Thumbnailer> {
        let mut out: Vec<&Thumbnailer> = Vec::new();
        for mime in chain {
            for t in &self.list {
                if t.mime_types.iter().any(|m| m == mime) && !out.contains(&t) {
                    out.push(t);
                }
            }
        }
        out
    }
}

/// Waits for a child at most `timeout`; kills it after that. True when it exited successfully.
fn wait_bounded(child: &mut std::process::Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// Runs `thumbnailer` on `input` for a picture of `size` pixels. None when its program is
/// missing (`TryExec`, `Exec`), it fails, runs past `timeout`, or writes no usable PNG.
pub fn run(
    thumbnailer: &Thumbnailer,
    input: &Path,
    size: u32,
    timeout: Duration,
    path_var: &str,
) -> Option<image::DynamicImage> {
    if let Some(try_exec) = &thumbnailer.try_exec {
        find_program(try_exec, path_var)?;
    }
    let uri = format!("file://{}", input.to_str()?);
    let dir = std::env::temp_dir().join(format!(
        "silica-thumbnailer-{}-{:x}",
        std::process::id(),
        md5::compute(format!("{uri}|{size}|{}", thumbnailer.exec).as_bytes())
    ));
    crate::thumbnails::ensure_dir(&dir)?;
    let output = dir.join("out.png");
    let result = (|| {
        let fields = Fields {
            input,
            uri: &uri,
            output: &output,
            size,
        };
        let (program, args) = command_line(thumbnailer, &fields)?;
        let program = find_program(&program, path_var)?;
        let mut child = Command::new(program)
            .args(args)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        if !wait_bounded(&mut child, timeout) {
            return None;
        }
        if fs::metadata(&output).ok()?.len() > MAX_OUTPUT {
            return None;
        }
        image::open(&output).ok()
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "[Thumbnailer Entry]\nTryExec=maker\n\
        Exec=maker -s %s \"%i\" %o\nMimeType=application/x-doc;text/x-doc;\n";

    #[test]
    fn a_thumbnailer_entry_is_parsed() {
        let t = parse(ENTRY).unwrap();
        assert_eq!(t.try_exec.as_deref(), Some("maker"));
        assert_eq!(t.mime_types, ["application/x-doc", "text/x-doc"]);
        assert!(parse("[Thumbnailer Entry]\nExec=x %o\n").is_none());
        assert!(parse("[Other]\nExec=x %o\nMimeType=a/b;\n").is_none());
    }

    #[test]
    fn exec_words_and_field_codes() {
        assert_eq!(
            split_exec("a  \"b c\" d\\e \"q\\\"r\" \"\""),
            ["a", "b c", "d\\e", "q\"r", ""]
        );
        let t = parse(ENTRY).unwrap();
        let fields = Fields {
            input: Path::new("/d/my file.doc"),
            uri: "file:///d/my%20file.doc",
            output: Path::new("/o/out.png"),
            size: 256,
        };
        let (program, args) = command_line(&t, &fields).unwrap();
        assert_eq!(program, "maker");
        assert_eq!(args, ["-s", "256", "/d/my file.doc", "/o/out.png"]);
        // %u, %% and unknown codes; no %o means no output: refused.
        let t = Thumbnailer {
            exec: "p %u 100%% %o %z".into(),
            mime_types: vec!["a/b".into()],
            ..Thumbnailer::default()
        };
        let (_, args) = command_line(&t, &fields).unwrap();
        assert_eq!(args, ["file:///d/my%20file.doc", "100%", "/o/out.png", ""]);
        let t = Thumbnailer {
            exec: "p %i".into(),
            ..Thumbnailer::default()
        };
        assert!(command_line(&t, &fields).is_none());
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("thumbnailer-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn script(dir: &Path, name: &str, body: &str) {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn thumbnailers_are_selected_by_type_and_chain_with_the_first_file_winning() {
        let root = tmp("select");
        let (user, system) = (root.join("user"), root.join("system"));
        for dir in [&user, &system] {
            fs::create_dir_all(dir.join("thumbnailers")).unwrap();
        }
        let entry = |exec: &str, mime: &str| {
            format!("[Thumbnailer Entry]\nExec={exec} %i %o\nMimeType={mime};\n")
        };
        fs::write(
            user.join("thumbnailers/a.thumbnailer"),
            entry("user-a", "x/doc"),
        )
        .unwrap();
        fs::write(
            system.join("thumbnailers/a.thumbnailer"),
            entry("system-a", "x/doc"),
        )
        .unwrap();
        fs::write(
            system.join("thumbnailers/b.thumbnailer"),
            entry("system-b", "x/base"),
        )
        .unwrap();
        fs::write(system.join("thumbnailers/c.txt"), entry("ignored", "x/doc")).unwrap();
        let all = Thumbnailers::load(&Dirs {
            data_home: user,
            data_dirs: vec![system],
            ..Dirs::default()
        });
        let chain = ["x/doc".to_owned(), "x/base".to_owned()];
        let found: Vec<_> = all.find(&chain).iter().map(|t| t.exec.as_str()).collect();
        // The user's a.thumbnailer shadows the system's; the nearest type comes first.
        assert_eq!(found, ["user-a %i %o", "system-b %i %o"]);
        assert!(all.find(&["x/other".to_owned()]).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_thumbnailer_draws_a_picture_and_is_killed_past_the_time_limit() {
        let root = tmp("run");
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        // Copies a prepared PNG to the output path.
        let ready = root.join("ready.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
            .save(&ready)
            .unwrap();
        script(&bin, "maker", &format!("cp {} \"$2\"", ready.display()));
        script(&bin, "slow", "sleep 30");
        script(&bin, "broken", "echo nothing > \"$2\"");
        let path_var = bin.display().to_string();
        let input = root.join("in.doc");
        fs::write(&input, "x").unwrap();
        let make = |exec: &str, try_exec: Option<&str>| Thumbnailer {
            try_exec: try_exec.map(str::to_owned),
            exec: exec.to_owned(),
            mime_types: vec!["x/doc".into()],
        };
        let img = run(
            &make("maker %s %o", Some("maker")),
            &input,
            64,
            Duration::from_secs(10),
            &path_var,
        )
        .unwrap();
        assert_eq!((img.width(), img.height()), (8, 8));
        // The time limit: the slow program is killed well before it ends.
        let started = Instant::now();
        let out = run(
            &make("slow %s %o", None),
            &input,
            64,
            Duration::from_millis(200),
            &path_var,
        );
        assert!(out.is_none());
        assert!(started.elapsed() < Duration::from_secs(10));
        // Not a picture, a missing TryExec, a missing program.
        for t in [
            make("broken %s %o", None),
            make("maker %s %o", Some("not-installed")),
            make("not-installed %s %o", None),
        ] {
            assert!(run(&t, &input, 64, Duration::from_secs(10), &path_var).is_none());
        }
        let _ = fs::remove_dir_all(&root);
    }
}
