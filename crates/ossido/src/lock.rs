//! The lock screen and the screen saver.
//!
//! The lock screen is a shell component (the `lock` layer, an overlay with the keyboard); Ossido
//! owns the state: while the session is locked the shortcuts, the virtual terminal switch, the
//! gestures and the hot corners are off, and a click or the wheel reaches only the lock layer.
//! The password is checked by silica-daemon (`Authenticate`), which then asks Ossido to unlock
//! (`session.unlock`): the password never reaches the compositor.
//!
//! Locking is automatic after the idle time of `lockscreen.json` counted from the moment the
//! screen saver starts or the display turns off, and before sleeping. The screen saver starts
//! after `screensaver.json` `minutes` without input; Ossido tells the shell with the state
//! `screensaver.active` (the id of the saver, empty when none) and `session.locked`.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use smithay::desktop::layer_map_for_output;
use tracing::info;

use crate::state::State;

/// The namespace of the lock screen's layer.
pub const LOCK_NAMESPACE: &str = "lock";

/// The savers the shell draws; `ScreenSaverConfig` accepts only these.
pub const SAVERS: &[&str] = &["drift", "clock", "colors"];

/// The saver used when the file names none (the first).
pub const SAVER_DEFAULT: &str = "drift";

impl State {
    /// Whether the lock screen's layer exists.
    fn has_lock_layer(&self) -> bool {
        layer_map_for_output(&self.output)
            .layers()
            .any(|layer| layer.namespace() == LOCK_NAMESPACE)
    }

    /// Locks the session: the lock screen takes the keyboard and the pointer, nothing else
    /// reaches the windows. Refused when no lock screen is running (it could not be left).
    pub fn lock_session(&mut self) -> Result<(), String> {
        if self.locked {
            return Ok(());
        }
        if !self.has_lock_layer() {
            return Err("no lock screen is running".to_owned());
        }
        info!("session locked");
        self.locked = true;
        self.locked_focus = self.focused_window();
        self.gesture = None;
        self.dismiss_transients();
        self.broadcast_to_shell("session.locked".to_owned(), "1".to_owned());
        // The lock layer asks for the keyboard exclusively as soon as it sees the state; until
        // then nobody has it.
        self.focus_window(None);
        let layer = layer_map_for_output(&self.output)
            .layers()
            .find(|layer| layer.namespace() == LOCK_NAMESPACE)
            .cloned();
        if let Some(layer) = layer {
            self.focus_layer(&layer);
        }
        self.wake();
        Ok(())
    }

    /// Unlocks the session (the daemon checked the password).
    pub fn unlock_session(&mut self) {
        if !self.locked {
            return;
        }
        info!("session unlocked");
        self.locked = false;
        self.lock_started = None;
        self.broadcast_to_shell("session.locked".to_owned(), "0".to_owned());
        let window = self
            .locked_focus
            .take()
            .filter(|w| self.space.elements().any(|e| e == w))
            .or_else(|| self.space.elements().last().cloned());
        self.focus_window(window);
        self.wake();
    }

    /// A click or the wheel at `location` while locked may only go to the lock layer.
    pub fn locked_blocks(
        &self,
        location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) -> bool {
        self.locked
            && !self
                .layer_under(
                    &[smithay::wayland::shell::wlr_layer::Layer::Overlay],
                    location,
                )
                .is_some_and(|(layer, _)| layer.namespace() == LOCK_NAMESPACE)
    }

    /// `lockscreen.json` and `screensaver.json` changed.
    pub fn apply_lock_config(&mut self) {
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        self.lock_config = crate::config::LockConfig::parse(
            crate::config::read_json(&dir, crate::config::LOCK_FILE).as_ref(),
        );
        self.saver_config = crate::config::SaverConfig::parse(
            crate::config::read_json(&dir, crate::config::SAVER_FILE).as_ref(),
        );
    }

    /// Every idle check: starts the screen saver, locks when the time has come.
    pub fn lock_check(&mut self, idle: Duration) {
        let saver_due = self
            .saver_config
            .minutes
            .is_some_and(|m| idle.as_secs_f64() >= m * 60.0);
        if saver_due && !self.saver_active {
            self.saver_active = true;
            let saver = self.saver_config.saver.clone();
            info!("screen saver {saver} after {} s idle", idle.as_secs());
            self.broadcast_to_shell("screensaver.active".to_owned(), saver);
        }
        // The password is asked for a while after the saver or the display off began.
        let began = self.saver_active || self.display_off;
        if began && self.lock_started.is_none() {
            self.lock_started = Some(Instant::now());
        }
        if self.locked {
            return;
        }
        if let (Some(delay), Some(since)) = (self.lock_config.password_delay, self.lock_started)
            && since.elapsed() >= Duration::from_secs(u64::from(delay))
            && let Err(err) = self.lock_session()
        {
            tracing::warn!("lock: {err}");
            // Not again at every check.
            self.lock_started = None;
        }
    }

    /// Any input: the saver goes away, and the count for the lock starts again.
    pub fn lock_activity(&mut self) {
        if self.saver_active {
            self.saver_active = false;
            self.broadcast_to_shell("screensaver.active".to_owned(), String::new());
        }
        if !self.locked {
            self.lock_started = None;
        }
    }

    /// Before the computer sleeps: lock if a password is asked for.
    pub fn lock_before_sleep(&mut self) {
        if self.lock_config.password_delay.is_some()
            && let Err(err) = self.lock_session()
        {
            tracing::warn!("lock before sleep: {err}");
        }
    }

    /// The lock state for `idle.state`.
    pub fn lock_state(&self) -> Value {
        json!({
            "locked": self.locked,
            "screenSaver": self.saver_active,
            "passwordDelaySeconds": self.lock_config.password_delay,
            "screenSaverMinutes": self.saver_config.minutes,
        })
    }
}
