#![windows_subsystem = "windows"]

//! Corsair headset battery level in the Windows tray.
//!
//! Single thread, single process: one hidden window receives tray, device
//! and theme notifications, and the message loop also waits on the HID read
//! event, so the app sleeps (0% CPU) until something actually happens.

mod draw;
mod hid;
mod icon;
mod tray;

use std::{
    cell::RefCell,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{CreateMutexW, INFINITE},
    },
    UI::{HiDpi::*, WindowsAndMessaging::*},
};

const ID_REFRESH: usize = 1;
const ID_EXIT: usize = 2;
const TIMER_RESCAN: usize = 1;
/// Debounce: a headset exposes several HID interfaces that arrive one by one.
const RESCAN_DELAY_MS: u32 = 1000;
const NIN_SELECT: u32 = WM_USER;
const NIN_KEYSELECT: u32 = WM_USER + 1;

#[derive(Clone, Copy, PartialEq)]
enum Status {
    NoDevice,
    Waiting,
    Report(hid::Report),
}

fn state_text(state: u8) -> &'static str {
    match state {
        1 => "Connected",
        2 => "Low battery",
        4 => "Fully charged",
        5 => "Charging",
        _ => "Disconnected",
    }
}

struct App {
    hwnd: HWND,
    taskbar_created: u32,
    device: Option<hid::Device>,
    status: Status,
    icon: HICON,
    light: bool,
}

impl App {
    fn tooltip(&self) -> String {
        let Some(dev) = &self.device else {
            return "No device found".into();
        };
        match self.status {
            Status::Report(r) if matches!(r.state, 1 | 2 | 4 | 5) => {
                format!("{}: {} ({}%)", dev.name, state_text(r.state), r.battery)
            }
            Status::Report(r) => format!("{}: {}", dev.name, state_text(r.state)),
            _ => dev.name.into(),
        }
    }

    fn glyph(&self) -> draw::Glyph {
        match self.status {
            Status::Report(r) if matches!(r.state, 1 | 2 | 4 | 5) => draw::Glyph::Battery {
                level: r.battery,
                low: r.state == 2 || r.battery <= 15,
                charging: r.state == 5,
            },
            _ => draw::Glyph::Headset,
        }
    }

    /// Re-renders the icon at the current DPI/theme and pushes it to the shell.
    fn redraw(&mut self, add: bool) {
        let icon = unsafe {
            let size = GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForWindow(self.hwnd));
            icon::render(self.glyph(), size, self.light)
        };
        let tip = self.tooltip();
        if add {
            tray::add(self.hwnd, icon, &tip);
        } else {
            tray::update(self.hwnd, icon, &tip);
        }
        if !self.icon.is_null() {
            unsafe { DestroyIcon(self.icon) };
        }
        self.icon = icon;
    }

    fn set_status(&mut self, status: Status) {
        if status != self.status {
            self.status = status;
            self.redraw(false);
        }
    }

    fn connect(&mut self) {
        self.device = None; // release the old handle before reopening
        self.device = hid::Device::open();
        self.status = if self.device.is_some() { Status::Waiting } else { Status::NoDevice };
        self.redraw(false);
    }

    fn on_device_event(&mut self) {
        let Some(dev) = &mut self.device else { return };
        match dev.read() {
            Ok(Some(report)) => self.set_status(Status::Report(report)),
            Ok(None) => {}
            Err(()) => {
                self.device = None;
                self.set_status(Status::NoDevice);
                schedule_rescan(self.hwnd);
            }
        }
    }
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the app state. Skips (instead of panicking) if the state is
/// already borrowed, which can only happen on a re-entrant window message.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(f))
}

fn schedule_rescan(hwnd: HWND) {
    unsafe { SetTimer(hwnd, TIMER_RESCAN, RESCAN_DELAY_MS, None) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

unsafe fn show_menu(hwnd: HWND, x: i32, y: i32) {
    let Some(tip) = with_app(|a| a.tooltip()) else { return };
    let (tip, refresh, exit) = (wide(&tip), wide("Refresh device"), wide("Exit"));

    let menu = CreatePopupMenu();
    AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, tip.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, ID_REFRESH, refresh.as_ptr());
    AppendMenuW(menu, MF_STRING, ID_EXIT, exit.as_ptr());

    let align = if GetSystemMetrics(SM_MENUDROPALIGNMENT) != 0 { TPM_RIGHTALIGN } else { TPM_LEFTALIGN };
    // Required so the menu closes when the user clicks elsewhere.
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenuEx(menu, align | TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY, x, y, hwnd, null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);

    match cmd as usize {
        ID_REFRESH => {
            with_app(|a| a.connect());
        }
        ID_EXIT => {
            DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        tray::WM_TRAY => {
            let event = (lparam & 0xFFFF) as u32;
            if matches!(event, WM_CONTEXTMENU | NIN_SELECT | NIN_KEYSELECT) {
                let x = (wparam & 0xFFFF) as i16 as i32;
                let y = ((wparam >> 16) & 0xFFFF) as i16 as i32;
                show_menu(hwnd, x, y);
            }
            0
        }
        WM_DEVICECHANGE => {
            if matches!(wparam as u32, DBT_DEVICEARRIVAL | DBT_DEVICEREMOVECOMPLETE) {
                schedule_rescan(hwnd);
            }
            TRUE as LRESULT
        }
        WM_TIMER if wparam == TIMER_RESCAN => {
            KillTimer(hwnd, TIMER_RESCAN);
            with_app(|a| {
                if a.device.is_none() {
                    a.connect();
                }
            });
            0
        }
        WM_SETTINGCHANGE => {
            with_app(|a| {
                let light = icon::light_taskbar();
                if light != a.light {
                    a.light = light;
                    a.redraw(false);
                }
            });
            0
        }
        WM_DPICHANGED | WM_DISPLAYCHANGE => {
            with_app(|a| a.redraw(false));
            0
        }
        WM_DESTROY => {
            tray::remove(hwnd);
            PostQuitMessage(0);
            0
        }
        _ => {
            if with_app(|a| msg == a.taskbar_created) == Some(true) {
                with_app(|a| a.redraw(true));
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

/// Message loop that also wakes up when the HID read completes.
unsafe fn run() {
    let mut msg: MSG = zeroed();
    loop {
        let event = with_app(|a| a.device.as_ref().map(|d| d.event())).flatten();
        let (count, handles) = match &event {
            Some(h) => (1, h as *const HANDLE),
            None => (0, null()),
        };
        let r = MsgWaitForMultipleObjectsEx(count, handles, INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
        if count == 1 && r == WAIT_OBJECT_0 {
            with_app(|a| a.on_device_event());
        }
        while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                return;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn main() {
    unsafe {
        let mutex_name = wide("Local\\corsair_battery_level");
        let mutex = CreateMutexW(null(), FALSE, mutex_name.as_ptr());
        if mutex.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
            return; // already running
        }

        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(null());
        let class = wide("CorsairBatteryLevel");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassExW(&wc);
        // Hidden top-level window (not message-only: those miss the
        // TaskbarCreated broadcast).
        let hwnd = CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, null_mut(), null_mut(), hinstance, null());
        if hwnd.is_null() {
            return;
        }

        hid::register_notifications(hwnd);
        let taskbar_created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        APP.with(|cell| {
            *cell.borrow_mut() = Some(App {
                hwnd,
                taskbar_created,
                device: None,
                status: Status::NoDevice,
                icon: null_mut(),
                light: icon::light_taskbar(),
            })
        });
        with_app(|a| {
            a.redraw(true);
            a.connect();
        });

        run();

        // Close the device explicitly; TLS destructors may not run at exit.
        APP.with(|cell| {
            if let Some(app) = cell.borrow_mut().take() {
                if !app.icon.is_null() {
                    DestroyIcon(app.icon);
                }
            }
        });
        CloseHandle(mutex);
    }
}
