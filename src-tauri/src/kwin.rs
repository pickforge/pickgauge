//! KWin window-rule helper for the float capsule.
//!
//! On Wayland, GTK cannot set keep-above / no-focus / skip-taskbar, so on KDE
//! we install a KWin window rule that forces them for the float window
//! (matched by window class substring + exact title). Best-effort: silently
//! does nothing on other desktops or when the KDE config tools are missing.
//!
//! This rule also covers the KDE session's XWayland windows (the
//! `PICKGAUGE_X11=1` fallback, `GDK_BACKEND=x11`): X11's EWMH has no
//! `_NET_WM_STATE_SKIP_SWITCHER` hint, so GTK's `skip_taskbar`/`skip_pager`
//! calls (`gdk_window_set_skip_taskbar_hint`) cannot hide the window from
//! KWin's Alt+Tab switcher there either — only the `skipswitcher` KWin rule
//! can. The rule matches by window class + title regardless of whether the
//! client is a native Wayland surface or an XWayland one, so gating rule
//! installation on `GDK_BACKEND` was the bug (pickforge/pickgauge#49): it
//! left the XWayland fallback with no switcher exclusion at all.

use std::path::{Path, PathBuf};
use std::process::Command;

const RULE_GROUP: &str = "pickgauge-float-keep-above";
const FLOAT_TITLE: &str = "PickGauge Float";
const WM_CLASS: &str = "pickgauge";

fn find_command(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn group_has_key(contents: &str, group: &str, key: &str) -> bool {
    let header = format!("[{group}]");
    let mut in_group = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_group = trimmed == header;
            continue;
        }
        if in_group && trimmed.starts_with(key) {
            return true;
        }
    }
    false
}

/// Whether the current session is a KDE Wayland compositor session — native
/// Wayland or the KDE `GDK_BACKEND=x11` XWayland fallback both qualify, since
/// KWin manages window rules for both kinds of client. Any other desktop or
/// display server (GNOME, other Wayland compositors, plain X11 sessions) is
/// out of scope: those only get the standards-based `skip_taskbar`/
/// `skip_pager` X11 hints Tauri already sets, with no app-managed desktop
/// configuration.
fn is_kde_wayland_session(
    xdg_session_type: Option<&str>,
    xdg_current_desktop: Option<&str>,
) -> bool {
    if xdg_session_type != Some("wayland") {
        return false;
    }
    xdg_current_desktop
        .unwrap_or_default()
        .to_uppercase()
        .contains("KDE")
}

pub fn ensure_float_rule() {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    if !is_kde_wayland_session(session_type.as_deref(), desktop.as_deref()) {
        return;
    }
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    let rules_path = Path::new(&home).join(".config/kwinrulesrc");
    let existing_rules = std::fs::read_to_string(&rules_path).unwrap_or_default();
    let already_registered = existing_rules.contains(RULE_GROUP);
    // "positionrule" inside our group marks the current revision; rewrite older ones.
    if already_registered && group_has_key(&existing_rules, RULE_GROUP, "positionrule") {
        return;
    }

    let Some(write_tool) = ["kwriteconfig6", "kwriteconfig5"]
        .into_iter()
        .find(|t| find_command(t).is_some())
    else {
        return;
    };
    let read_tool = if write_tool == "kwriteconfig6" {
        "kreadconfig6"
    } else {
        "kreadconfig5"
    };

    let write = |key: &str, value: &str| {
        let _ = Command::new(write_tool)
            .args([
                "--file",
                "kwinrulesrc",
                "--group",
                RULE_GROUP,
                "--key",
                key,
                value,
            ])
            .status();
    };

    write("Description", "PickGauge float capsule (managed by PickGauge)");
    // Match: window class contains "pickgauge" AND title is exactly the
    // float window title, so the main window is unaffected.
    write("wmclass", WM_CLASS);
    write("wmclassmatch", "2"); // substring
    write("title", FLOAT_TITLE);
    write("titlematch", "1"); // exact
    // Force (rule value 2): keep above, never take focus, stay out of the
    // taskbar, pager, and window switcher.
    write("above", "true");
    write("aboverule", "2");
    write("acceptfocus", "false");
    write("acceptfocusrule", "2");
    write("skiptaskbar", "true");
    write("skiptaskbarrule", "2");
    write("skippager", "true");
    write("skippagerrule", "2");
    write("skipswitcher", "true");
    write("skipswitcherrule", "2");
    // Initial spot only (rule 3 = apply initially) — one row below the
    // PickScribe capsule so the Pickforge floats never stack.
    write("position", "64,140");
    write("positionrule", "3");

    // Register the rule group in the [General] rules list (Plasma 6 format).
    if !already_registered && find_command(read_tool).is_some() {
        let existing = Command::new(read_tool)
            .args(["--file", "kwinrulesrc", "--group", "General", "--key", "rules"])
            .output()
            .ok()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_default();
        let rules = if existing.is_empty() {
            RULE_GROUP.to_string()
        } else {
            format!("{existing},{RULE_GROUP}")
        };
        let _ = Command::new(write_tool)
            .args([
                "--file",
                "kwinrulesrc",
                "--group",
                "General",
                "--key",
                "rules",
                &rules,
            ])
            .status();
    }

    // Ask KWin to reload its rules.
    let _ = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.kde.KWin",
            "--object-path",
            "/KWin",
            "--method",
            "org.kde.KWin.reconfigure",
        ])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_wayland_kde_session_installs_the_rule() {
        // Also covers pickforge/pickgauge#49: the predicate takes no
        // GDK_BACKEND input at all, so a KDE Wayland session running the
        // float capsule through XWayland (GDK_BACKEND=x11, e.g.
        // PICKGAUGE_X11=1) installs the rule identically to native Wayland —
        // X11 has no standards-based Alt+Tab exclusion hint, so the KWin
        // rule is the only way to keep the capsule out of the switcher
        // there.
        assert!(is_kde_wayland_session(Some("wayland"), Some("KDE")));
    }

    #[test]
    fn plasma_reports_a_compound_desktop_string() {
        // XDG_CURRENT_DESKTOP can list multiple desktops, e.g. "KDE" alone on
        // Plasma 5 or "KDE:Plasma" style values on some distros.
        assert!(is_kde_wayland_session(Some("wayland"), Some("KDE:Plasma")));
    }

    #[test]
    fn plain_x11_session_is_out_of_scope() {
        assert!(!is_kde_wayland_session(Some("x11"), Some("KDE")));
    }

    #[test]
    fn missing_session_type_is_out_of_scope() {
        assert!(!is_kde_wayland_session(None, Some("KDE")));
    }

    #[test]
    fn non_kde_wayland_compositor_is_out_of_scope() {
        assert!(!is_kde_wayland_session(Some("wayland"), Some("GNOME")));
        assert!(!is_kde_wayland_session(Some("wayland"), Some("sway")));
        assert!(!is_kde_wayland_session(Some("wayland"), None));
    }

    #[test]
    fn group_has_key_scopes_matches_to_the_named_group() {
        let contents =
            "[other-group]\npositionrule=3\n\n[pickgauge-float-keep-above]\nskiptaskbar=true\n";
        assert!(!group_has_key(
            contents,
            "pickgauge-float-keep-above",
            "positionrule"
        ));
        assert!(group_has_key(
            contents,
            "pickgauge-float-keep-above",
            "skiptaskbar"
        ));
    }

    #[test]
    fn group_has_key_finds_a_key_in_the_current_revision() {
        let contents = "[pickgauge-float-keep-above]\nskiptaskbar=true\npositionrule=3\n";
        assert!(group_has_key(
            contents,
            "pickgauge-float-keep-above",
            "positionrule"
        ));
    }
}
