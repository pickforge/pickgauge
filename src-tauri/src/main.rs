// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Native Wayland scrolls and renders much smoother in WebKitGTK; the
    // float capsule stays on top through a KWin window rule instead of the
    // X11-only GTK keep-above. Set PICKGAUGE_X11=1 on compositors without
    // window rules to fall back to XWayland keep-above.
    if std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland")
        && std::env::var_os("PICKGAUGE_X11").is_some()
        && std::env::var_os("GDK_BACKEND").is_none()
    {
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }

    pickgauge_lib::run();
}
