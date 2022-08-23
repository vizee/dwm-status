use std::sync::Once;
use std::time::Duration;
use std::{env, process, ptr, thread};

use sysinfo::{CpuExt, System, SystemExt};
use x11::*;

mod x11 {
    use core::ffi::{c_char, c_int, c_ulong, c_void};

    pub type Display = c_void;
    type XID = c_ulong;
    pub type Window = XID;

    #[link(name = "X11")]
    extern "C" {
        pub fn XOpenDisplay(display_name: *const c_char) -> *mut Display;
        // pub fn XCloseDisplay(display: *mut Display) -> c_int;
        pub fn XDefaultScreen(display: *mut Display) -> c_int;
        pub fn XRootWindow(display: *mut Display, screen_number: c_int) -> Window;
        // pub fn XDisplayName(string: *const c_char) -> *const c_char;
        pub fn XStoreName(display: *mut Display, w: Window, window_name: *const c_char) -> c_int;
        pub fn XFlush(display: *mut Display) -> c_int;
    }
}

static ONCE_INIT: Once = Once::new();
static mut PRI_DISPLAY: *mut Display = ptr::null_mut();
static mut ROOT_WID: Window = 0;

fn set_root_name(name: &str) {
    ONCE_INIT.call_once(|| unsafe {
        let dpy = XOpenDisplay(ptr::null());
        if dpy.is_null() {
            eprintln!("unable to open display");
            process::exit(2);
        }
        let scr = XDefaultScreen(dpy);
        let root = XRootWindow(dpy, scr);
        PRI_DISPLAY = dpy;
        ROOT_WID = root;
    });
    unsafe {
        XStoreName(PRI_DISPLAY, ROOT_WID, name.as_ptr().cast());
        XFlush(PRI_DISPLAY);
    }
}

struct Status {
    time: String,
    cpu: String,
    mem: String,
    sys_stat: System,
    should_update: bool,
}

fn load_sys_stat(status: &mut Status) {
    status.sys_stat.refresh_system();
    status.cpu = format!("{:2.0}%", status.sys_stat.global_cpu_info().cpu_usage());

    status.mem = || -> String {
        let mut used = status.sys_stat.used_memory();
        let mut unit = "KB";
        if used > 1024 {
            used /= 1024;
            unit = "MB";
            if used > 1024 {
                return format!("{:.1}GB", used as f64 / 1024f64);
            }
        }
        format!("{:3}{}", used, unit)
    }();
}

fn refresh_status(status: &mut Status, force: bool) {
    let now = chrono::Local::now();
    let now_uts = now.timestamp();
    if force || now_uts % 60 == 0 {
        status.time = now.format("%m/%d %H:%M").to_string();
        status.should_update = true;
    }
    if force || now_uts % 3 == 0 {
        load_sys_stat(status);
        status.should_update = true;
    }
}

fn update_status_text(mut s: String) {
    #[cfg(debug_assertions)]
    println!("refresh {}", s);
    s.push('\0');
    set_root_name(s.as_str());
}

fn main() {
    if let Some(arg) = env::args().skip(1).next() {
        update_status_text(arg);
        return;
    }

    let mut status = Status {
        time: String::default(),
        cpu: String::default(),
        mem: String::default(),
        sys_stat: System::new(),
        should_update: false,
    };
    refresh_status(&mut status, true);
    loop {
        if status.should_update {
            update_status_text(format!("[{}|{}] {}", status.cpu, status.mem, status.time));
            status.should_update = false;
        }
        thread::sleep(Duration::from_secs(1));
        refresh_status(&mut status, false);
    }
}
