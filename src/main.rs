use std::ffi::c_int;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::time::Duration;
use std::{process, ptr, thread};

use alsa::mixer::{SelemChannelId, SelemId};
use alsa::Mixer;
use sys::*;
use sysinfo::{CpuExt, System, SystemExt};

mod sys {
    use core::ffi::{c_char, c_int, c_ulong, c_void};

    pub type Display = c_void;
    type XID = c_ulong;
    pub type Window = XID;

    pub const SIGUSR1: c_int = 10;

    #[allow(non_camel_case_types)]
    pub type sighandler_t = *mut c_void;

    #[link(name = "X11")]
    extern "C" {
        pub fn XOpenDisplay(display_name: *const c_char) -> *mut Display;
        // pub fn XCloseDisplay(display: *mut Display) -> c_int;
        pub fn XDefaultScreen(display: *mut Display) -> c_int;
        pub fn XRootWindow(display: *mut Display, screen_number: c_int) -> Window;
        // pub fn XDisplayName(string: *const c_char) -> *const c_char;
        pub fn XStoreName(display: *mut Display, w: Window, window_name: *const c_char) -> c_int;
        pub fn XFlush(display: *mut Display) -> c_int;
        pub fn signal(signum: c_int, hadnler: sighandler_t) -> sighandler_t;
    }
}

static ONCE_INIT: Once = Once::new();
static mut PRI_DISPLAY: *mut Display = ptr::null_mut();
static mut ROOT_WID: Window = 0;
static FULL_STATUS: AtomicBool = AtomicBool::new(false);

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
    sys_stat: System,
    cpu: String,
    mem: String,
    vol: String,
    should_update: bool,
}

fn load_snd_vol(status: &mut Status) {
    status.vol = if let Ok(mixer) = Mixer::new("default", false) {
        mixer
            .find_selem(&SelemId::new("Master", 0))
            .and_then(|master| {
                let (vol_min, vol_max) = master.get_playback_volume_range();
                let mut total = 0;
                let mut cur = 0;
                for c in SelemChannelId::all().iter() {
                    if master.has_playback_channel(*c) {
                        total += vol_max - vol_min;
                        let sw = master.get_playback_switch(*c).ok()?;
                        if sw == 0 {
                            continue;
                        }
                        let vol = master.get_playback_volume(*c).ok()?;
                        cur += vol - vol_min;
                    }
                }
                Some(if cur == 0 {
                    "-/-".to_string()
                } else {
                    format!("{:.0}%", (cur * 100) as f64 / total as f64)
                })
            })
            .unwrap_or("E".to_string())
    } else {
        "E".to_string()
    };
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
            if used >= 1000 {
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
    if FULL_STATUS.load(Ordering::SeqCst) {
        load_snd_vol(status);
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

extern "C" fn sig_user(_sig: c_int) {
    FULL_STATUS.store(!FULL_STATUS.load(Ordering::SeqCst), Ordering::SeqCst);
}

fn main() {
    unsafe {
        sys::signal(SIGUSR1, sig_user as sighandler_t);
    }

    let mut status = Status {
        time: String::default(),
        cpu: String::default(),
        mem: String::default(),
        sys_stat: System::new(),
        vol: String::default(),
        should_update: false,
    };
    refresh_status(&mut status, true);
    loop {
        if status.should_update {
            if FULL_STATUS.load(Ordering::SeqCst) {
                update_status_text(format!(
                    "[{}|{}] ({}) {}",
                    status.cpu, status.mem, status.vol, status.time
                ));
            } else {
                update_status_text(format!("[{}|{}] {}", status.cpu, status.mem, status.time));
            }
            status.should_update = false;
        }
        thread::sleep(Duration::from_secs(1));
        refresh_status(&mut status, false);
    }
}
