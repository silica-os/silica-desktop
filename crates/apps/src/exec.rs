//! The `Exec` key: splitting it into arguments, its field codes, and the command line a
//! launcher runs (Desktop Entry specification, "The Exec key").

/// Splits an `Exec` value into words: double quotes group, and inside them `\"`, `` \` ``,
/// `\$` and `\\` are escapes. Unbalanced quotes: the rest is one word.
pub fn split(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let (mut quoted, mut started) = (false, false);
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' => quoted = false,
                '\\' => match chars.next() {
                    Some(n @ ('"' | '`' | '$' | '\\')) => word.push(n),
                    Some(n) => {
                        word.push('\\');
                        word.push(n);
                    }
                    None => word.push('\\'),
                },
                c => word.push(c),
            }
        } else {
            match c {
                '"' => {
                    quoted = true;
                    started = true;
                }
                ' ' | '\t' | '\n' => {
                    if started {
                        words.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                c => {
                    word.push(c);
                    started = true;
                }
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// Removes the field codes (`%f %F %u %U %d %D %n %N %i %c %k %v %m`); `%%` is a `%`. Words
/// that become empty are dropped. The result is a plain command line for `sh -c` (the words
/// are joined with spaces as written: this is what launchers that ignore field codes run).
pub fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' && chars.peek().is_some() {
            if chars.next() == Some('%') {
                out.push('%');
            }
            continue;
        }
        out.push(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a launch passes to the field codes.
#[derive(Clone, Debug, Default)]
pub struct Context<'a> {
    /// The files or URIs to open. A `file://` URI is turned into a path for `%f`/`%F`; a path
    /// into a `file://` URI for `%u`/`%U`.
    pub targets: &'a [String],
    /// `%i`: the entry's `Icon`.
    pub icon: Option<&'a str>,
    /// `%c`: the translated name.
    pub name: Option<&'a str>,
    /// `%k`: the entry's file.
    pub desktop_file: Option<&'a str>,
}

/// The path of a target: a `file://` URI decoded, a path as it is; None for other URIs.
pub fn target_path(target: &str) -> Option<String> {
    if target.starts_with('/') {
        return Some(target.to_owned());
    }
    let rest = target.strip_prefix("file://")?;
    // `file://host/path`: only the local host.
    let path = if rest.starts_with('/') {
        rest
    } else {
        let slash = rest.find('/')?;
        let host = rest.get(..slash)?;
        if host != "localhost" {
            return None;
        }
        rest.get(slash..)?
    };
    percent_decode(path)
}

/// The URI of a target: a path as a `file://` URI, a URI as it is.
pub fn target_uri(target: &str) -> String {
    if target.starts_with('/') {
        format!("file://{}", percent_encode_path(target))
    } else {
        target.to_owned()
    }
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let s = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(s, 16).ok()?);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Percent-encodes a path for a `file://` URI (RFC 3986: unreserved characters and `/` stay).
pub fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The arguments of a launch: the words of `exec` with the field codes expanded. `%f`/`%u` take
/// the first target (an app that takes one file at a time; the caller starts it once per target,
/// see [`takes_many`]), `%F`/`%U` all of them. An `exec` without file codes gets no targets.
pub fn expand(exec: &str, ctx: &Context<'_>) -> Vec<String> {
    let mut out = Vec::new();
    for word in split(exec) {
        match word.as_str() {
            "%f" => out.extend(ctx.targets.first().and_then(|t| target_path(t))),
            "%F" => out.extend(ctx.targets.iter().filter_map(|t| target_path(t))),
            "%u" => out.extend(ctx.targets.first().map(|t| target_uri(t))),
            "%U" => out.extend(ctx.targets.iter().map(|t| target_uri(t))),
            "%i" => {
                if let Some(icon) = ctx.icon.filter(|i| !i.is_empty()) {
                    out.push("--icon".to_owned());
                    out.push(icon.to_owned());
                }
            }
            "%c" => out.extend(ctx.name.map(str::to_owned)),
            "%k" => out.extend(ctx.desktop_file.map(str::to_owned)),
            _ => {
                // Codes inside a word: `%%` is `%`, the deprecated and unknown ones go.
                let mut text = String::with_capacity(word.len());
                let mut chars = word.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '%' && chars.peek().is_some() {
                        if chars.next() == Some('%') {
                            text.push('%');
                        }
                        continue;
                    }
                    text.push(c);
                }
                if !text.is_empty() {
                    out.push(text);
                }
            }
        }
    }
    out
}

/// The command lines that hand the targets to `exec`, as launchers do: `%F`/`%U` take every
/// target in one run, `%f`/`%u` one run per target, and an `exec` without file codes gets the
/// targets at the end as paths (what the Share menu and Open With always did for entries that
/// forget their codes). No targets: one run.
pub fn launch_lines(exec: &str, ctx: &Context<'_>) -> Vec<Vec<String>> {
    if ctx.targets.is_empty() {
        return vec![expand(exec, ctx)];
    }
    if takes_targets(exec) && !takes_many(exec) {
        return ctx
            .targets
            .iter()
            .map(|t| {
                expand(
                    exec,
                    &Context {
                        targets: std::slice::from_ref(t),
                        ..ctx.clone()
                    },
                )
            })
            .collect();
    }
    let mut line = expand(exec, ctx);
    if !takes_targets(exec) {
        line.extend(
            ctx.targets
                .iter()
                .map(|t| target_path(t).unwrap_or_else(|| t.clone())),
        );
    }
    vec![line]
}

/// Whether `exec` takes several targets at once (`%F`, `%U`).
pub fn takes_many(exec: &str) -> bool {
    split(exec).iter().any(|w| w == "%F" || w == "%U")
}

/// Whether `exec` takes targets at all.
pub fn takes_targets(exec: &str) -> bool {
    split(exec)
        .iter()
        .any(|w| matches!(w.as_str(), "%f" | "%F" | "%u" | "%U"))
}

/// Quotes a word for an `Exec` value (double quotes when needed, with the reserved characters
/// escaped).
pub fn quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | ',' | ':' | '=')
        });
    if plain {
        return word.to_owned();
    }
    let mut out = String::from("\"");
    for c in word.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Quotes a word for `sh -c`.
pub fn shell_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting_follows_the_quoting_rules() {
        assert_eq!(split("a  b\tc"), ["a", "b", "c"]);
        assert_eq!(
            split(r#"sh -c "echo \"hi\" \$HOME" x"#),
            ["sh", "-c", r#"echo "hi" $HOME"#, "x"]
        );
        assert_eq!(split(r#""""#), [""]);
        assert_eq!(split(r#"a "b c"#), ["a", "b c"]);
    }

    #[test]
    fn field_codes_are_stripped() {
        assert_eq!(
            strip_field_codes("silica-calculator --app-id x %U  %%done"),
            "silica-calculator --app-id x %done"
        );
        assert_eq!(
            strip_field_codes("flatpak run org.mozilla.firefox @@u %u @@"),
            "flatpak run org.mozilla.firefox @@u @@"
        );
    }

    #[test]
    fn expanding_turns_paths_and_uris_around() {
        let targets = vec![
            "/home/u/a b.txt".to_owned(),
            "file:///home/u/c%20d.txt".to_owned(),
            "https://example.org/".to_owned(),
        ];
        let ctx = Context {
            targets: &targets,
            icon: Some("calc"),
            name: Some("Calc"),
            desktop_file: Some("/x.desktop"),
        };
        assert_eq!(
            expand("app %F", &ctx),
            ["app", "/home/u/a b.txt", "/home/u/c d.txt"]
        );
        assert_eq!(
            expand("app %U", &ctx),
            [
                "app",
                "file:///home/u/a%20b.txt",
                "file:///home/u/c%20d.txt",
                "https://example.org/"
            ]
        );
        assert_eq!(expand("app %f", &ctx), ["app", "/home/u/a b.txt"]);
        assert_eq!(
            expand("app %i %c %k 100%% %d", &ctx),
            ["app", "--icon", "calc", "Calc", "/x.desktop", "100%"]
        );
        assert_eq!(expand("app", &ctx), ["app"]);
        assert!(takes_many("app %U") && !takes_many("app %f"));
        assert!(takes_targets("app %f") && !takes_targets("app %i"));
    }

    #[test]
    fn launch_lines_follow_the_codes() {
        let targets = vec!["/a".to_owned(), "file:///b".to_owned()];
        let ctx = Context {
            targets: &targets,
            ..Context::default()
        };
        assert_eq!(
            launch_lines("x %U", &ctx),
            [vec!["x", "file:///a", "file:///b"]]
        );
        assert_eq!(
            launch_lines("x %f", &ctx),
            [vec!["x", "/a"], vec!["x", "/b"]]
        );
        assert_eq!(
            launch_lines("x --new", &ctx),
            [vec!["x", "--new", "/a", "/b"]]
        );
        assert_eq!(
            launch_lines("x %f", &Context::default()),
            [vec!["x".to_owned()]]
        );
    }

    #[test]
    fn file_uris_of_other_hosts_are_not_paths() {
        assert_eq!(target_path("file://localhost/a").as_deref(), Some("/a"));
        assert_eq!(target_path("file://other/a"), None);
        assert_eq!(target_path("file:///a%2"), None);
    }

    #[test]
    fn quoting_round_trips() {
        for word in ["plain", "/A B/Hello.app", "a\"b$c`d\\e", ""] {
            assert_eq!(split(&quote(word)), [word]);
        }
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }
}
