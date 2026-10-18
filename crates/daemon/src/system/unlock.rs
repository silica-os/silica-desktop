//! Checking the lock screen's password: the lock screen sends it to `System1.Unlock`, the daemon
//! checks it for the user it runs as with PAM's own helper, `unix_chkpwd` (what screen lockers
//! use for the password of the logged-in user) and answers. The lock screen, which holds the
//! session lock (ext-session-lock-v1), unlocks itself on a true answer. The password never
//! reaches the compositor.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_process::{Command, Stdio};
use futures_lite::io::AsyncWriteExt;

/// Where `unix_chkpwd` lives on the distributions we run on; `SILICA_UNIX_CHKPWD` replaces it
/// (tests).
const CHKPWD: &[&str] = &[
    "/usr/sbin/unix_chkpwd",
    "/sbin/unix_chkpwd",
    "/usr/bin/unix_chkpwd",
];

/// How long a wrong password makes the next try wait.
pub const WRONG_PASSWORD_DELAY: Duration = Duration::from_millis(1500);

fn helper() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SILICA_UNIX_CHKPWD") {
        return Some(PathBuf::from(path));
    }
    CHKPWD
        .iter()
        .map(Path::new)
        .find(|p| p.is_file())
        .map(Path::to_path_buf)
}

/// The login name of this process's user.
pub fn user_name() -> Option<String> {
    ["USER", "LOGNAME"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
}

/// Whether `password` is the password of `user`. `Err` when the check cannot be made at all
/// (no helper): then nobody is let in by it.
pub async fn check_password(user: &str, password: &str) -> Result<bool, String> {
    let helper = helper().ok_or("no unix_chkpwd on this system")?;
    check_password_with(&helper, user, password).await
}

/// [`check_password`] with this helper program.
pub async fn check_password_with(
    helper: &Path,
    user: &str,
    password: &str,
) -> Result<bool, String> {
    let mut child = Command::new(helper)
        .arg(user)
        .arg("nullok")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("unix_chkpwd: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        // The helper reads the password up to a NUL.
        let mut line = password.as_bytes().to_vec();
        line.push(0);
        // A helper that quits before reading (wrong user) closes the pipe: that is a no.
        let _ = stdin.write_all(&line).await;
        let _ = stdin.close().await;
    }
    let status = child
        .status()
        .await
        .map_err(|e| format!("unix_chkpwd: {e}"))?;
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake(dir: &Path, script: &str) -> PathBuf {
        let path = dir.join("unix_chkpwd");
        std::fs::write(&path, format!("#!/usr/bin/env bash\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn the_helper_decides() {
        let dir = std::env::temp_dir().join(format!("silica-chkpwd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Accepts "right" for the user "ada" only.
        let path = fake(
            &dir,
            "[ \"$1\" = ada ] || exit 7\nread -r -d '' pass\n[ \"$pass\" = right ]",
        );
        futures_lite::future::block_on(async {
            assert_eq!(check_password_with(&path, "ada", "right").await, Ok(true));
            assert_eq!(check_password_with(&path, "ada", "wrong").await, Ok(false));
            assert_eq!(check_password_with(&path, "bob", "right").await, Ok(false));
            assert_eq!(check_password_with(&path, "ada", "").await, Ok(false));
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
