//! Remote application enumeration and quick-launch (UU feature: remote_app_list).
//! Scans Start Menu `.lnk` shortcuts + Windows Uninstall registry keys on the host,
//! and launches selected entries cleanly via ShellExecuteW / Command.

use crate::msg::AppEntry;
use anyhow::{bail, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Enumerate Start Menu shortcuts and common installed programs (deduplicated, sorted).
pub fn enumerate_apps(limit: usize) -> Vec<AppEntry> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();

    // 1. Always include handy built-in Windows utilities
    for (name, path) in [
        ("记事本 (Notepad)", "notepad.exe"),
        ("计算器 (Calc)", "calc.exe"),
        ("命令提示符 (CMD)", "cmd.exe"),
        ("Windows Terminal", "wt.exe"),
        ("任务管理器 (Taskmgr)", "taskmgr.exe"),
        ("文件资源管理器", "explorer.exe"),
    ] {
        if out.len() < limit && seen.insert(name.to_lowercase()) {
            out.push(AppEntry {
                name: name.to_owned(),
                path: path.to_owned(),
                source: "系统内置".to_owned(),
            });
        }
    }

    // 2. Start Menu .lnk shortcuts (ProgramData + APPDATA)
    let mut roots = Vec::new();
    if let Some(pd) = std::env::var_os("ProgramData") {
        roots.push(PathBuf::from(pd).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Some(ad) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(ad).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    for root in roots {
        collect_shortcuts(&root, 0, &mut seen, &mut out, limit);
        if out.len() >= limit {
            break;
        }
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.truncate(limit);
    out
}

fn collect_shortcuts(
    dir: &Path,
    depth: usize,
    seen: &mut BTreeSet<String>,
    out: &mut Vec<AppEntry>,
    limit: usize,
) {
    if depth > 3 || out.len() >= limit {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        if out.len() >= limit {
            break;
        }
        let path = ent.path();
        if path.is_dir() {
            collect_shortcuts(&path, depth + 1, seen, out, limit);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("lnk"))
            .unwrap_or(false)
        {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let low = stem.to_lowercase();
                if low.contains("uninstall") || low.contains("卸载") || low.contains("help") {
                    continue;
                }
                if seen.insert(low) {
                    out.push(AppEntry {
                        name: stem.to_owned(),
                        path: path.to_string_lossy().into_owned(),
                        source: "开始菜单".to_owned(),
                    });
                }
            }
        }
    }
}

/// Safety gate for launch targets: allows `.lnk`, `.exe`, or allowlisted system binaries.
pub fn validate_launch_target(target: &str) -> Result<()> {
    let t = target.trim();
    if t.is_empty() || t.contains('&') || t.contains('|') || t.contains(';') {
        bail!("rejected unsafe app path {t:?}");
    }
    let lower = t.to_ascii_lowercase();
    let allowed_sys = matches!(
        lower.as_str(),
        "notepad.exe" | "calc.exe" | "cmd.exe" | "wt.exe" | "taskmgr.exe" | "explorer.exe"
    );
    if allowed_sys {
        return Ok(());
    }
    let p = Path::new(t);
    let ext_ok = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("lnk") || e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false);
    if !ext_ok || !p.exists() {
        bail!("invalid or missing app target {t:?}");
    }
    Ok(())
}

/// Launch a validated shortcut or executable via `ShellExecuteW`.
pub fn launch_app(target: &str) -> Result<String> {
    validate_launch_target(target)?;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let op: Vec<u16> = "open\0".encode_utf16().collect();
    let file: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    let res = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if (res.0 as isize) <= 32 {
        bail!("ShellExecuteW failed (code {})", res.0 as isize);
    }
    let label = Path::new(target)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(target)
        .to_owned();
    Ok(format!("已启动 {label}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_returns_system_and_start_menu_apps() {
        let apps = enumerate_apps(64);
        assert!(!apps.is_empty());
        assert!(apps.iter().any(|a| a.path == "notepad.exe"));
    }

    #[test]
    fn validate_blocks_shell_metacharacters_and_missing_files() {
        assert!(validate_launch_target("notepad.exe").is_ok());
        assert!(validate_launch_target("notepad.exe & calc.exe").is_err());
        assert!(validate_launch_target("").is_err());
        assert!(validate_launch_target("C:\\nonexistent_app_12345.exe").is_err());
    }
}
