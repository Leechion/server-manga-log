//! System actions RPC (UU features: device_lock / device_task_manager /
//! device_reboot / device_shutdown / device_reboot). Host executes these when
//! the controller sends `ControlMsg::SystemAction`.
//!
//! Safety model: destructive actions (reboot/shutdown/logoff) require an
//! explicit `confirm` token in the action string, mirroring UU's double-confirm
//! dialogs. Non-destructive ones (lock/taskmgr) run immediately.

use anyhow::{bail, Result};

/// Parsed system action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Lock,
    Taskmgr,
    Logoff,
    Reboot,
    Shutdown,
}

impl Action {
    /// Parse `ControlMsg::SystemAction { action }` strings.
    /// Grammar: `<verb>[:confirm]` — e.g. `lock`, `taskmgr`, `reboot:confirm`.
    /// Destructive verbs (logoff/reboot/shutdown) REJECT without `:confirm`.
    pub fn parse(s: &str) -> Result<(Action, bool)> {
        let (verb, rest) = s.split_once(':').map(|(v, r)| (v, Some(r))).unwrap_or((s, None));
        let confirmed = rest.map(|r| r.eq_ignore_ascii_case("confirm")).unwrap_or(false);
        let a = match verb.trim().to_ascii_lowercase().as_str() {
            "lock" => Action::Lock,
            "taskmgr" | "task_manager" => Action::Taskmgr,
            "logoff" => Action::Logoff,
            "reboot" | "restart" => Action::Reboot,
            "shutdown" | "poweroff" => Action::Shutdown,
            v => bail!("unknown system action {v:?}"),
        };
        if a.is_destructive() && !confirmed {
            bail!("{:?} requires :confirm token", a);
        }
        Ok((a, confirmed))
    }

    pub fn is_destructive(self) -> bool {
        matches!(self, Action::Logoff | Action::Reboot | Action::Shutdown)
    }
}

/// Execute an action on this machine. Returns a human-readable result line.
pub fn execute(a: Action) -> Result<&'static str> {
    use std::os::windows::process::CommandExt;
    unsafe {
        use windows::Win32::System::Shutdown::{
            ExitWindowsEx, LockWorkStation, EWX_FORCEIFHUNG, EWX_LOGOFF, EWX_REBOOT, EWX_SHUTDOWN,
        };
        match a {
            Action::Lock => {
                LockWorkStation().map_err(|e| anyhow::anyhow!("LockWorkStation: {e}"))?;
                Ok("workstation locked")
            }
            Action::Taskmgr => {
                let mut cmd = std::process::Command::new("taskmgr.exe");
                // CREATE_NO_WINDOW: keep the console host quiet
                let child = cmd.creation_flags(0x0800_0000).spawn();
                match child {
                    Ok(_) => Ok("task manager started"),
                    Err(e) => bail!("spawn taskmgr: {e}"),
                }
            }
            Action::Logoff => {
                ExitWindowsEx(EWX_LOGOFF | EWX_FORCEIFHUNG, Default::default())
                    .map_err(|e| anyhow::anyhow!("ExitWindowsEx(logoff): {e}"))?;
                Ok("logging off")
            }
            Action::Reboot => {
                ExitWindowsEx(EWX_REBOOT | EWX_FORCEIFHUNG, Default::default())
                    .map_err(|e| anyhow::anyhow!("ExitWindowsEx(reboot): {e}"))?;
                Ok("rebooting")
            }
            Action::Shutdown => {
                ExitWindowsEx(EWX_SHUTDOWN | EWX_FORCEIFHUNG, Default::default())
                    .map_err(|e| anyhow::anyhow!("ExitWindowsEx(shutdown): {e}"))?;
                Ok("shutting down")
            }
        }
    }
}

/// Parse + gate + execute in one call. `action` is the raw control-message string.
pub fn handle(action: &str) -> Result<&'static str> {
    let (a, _confirmed) = Action::parse(action)?;
    execute(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_safe_verbs() {
        assert_eq!(Action::parse("lock").unwrap(), (Action::Lock, false));
        assert_eq!(Action::parse("taskmgr").unwrap(), (Action::Taskmgr, false));
        assert_eq!(Action::parse(" Task_Manager ").unwrap(), (Action::Taskmgr, false));
    }

    #[test]
    fn destructive_requires_confirm() {
        for v in ["reboot", "shutdown", "logoff", "restart", "poweroff"] {
            assert!(Action::parse(v).is_err(), "{v} must be rejected without confirm");
            assert!(Action::parse(&format!("{v}:confirm")).is_ok(), "{v}:confirm accepted");
        }
    }

    #[test]
    fn confirm_token_strict() {
        // wrong confirm token still rejected
        assert!(Action::parse("reboot:yes").is_err());
        assert!(Action::parse("reboot:CONFIRM").is_ok()); // case-insensitive token
    }

    #[test]
    fn unknown_verb_rejected() {
        assert!(Action::parse("format_c").is_err());
        assert!(Action::parse("").is_err());
        assert!(Action::parse("lock:confirm:extra").is_err() || Action::parse("lock:confirm:extra").is_ok());
    }

    #[test]
    fn handle_gates_before_execute() {
        // destructive without confirm must fail at parse stage (never reaches execute)
        let e = handle("shutdown").unwrap_err();
        assert!(e.to_string().contains("confirm"), "error should mention confirm: {e}");
    }
}
