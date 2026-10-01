//! Notification-area icon (Shell_NotifyIconW, NOTIFYICON_VERSION_4).

use std::mem::{size_of, zeroed};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{Shell::*, WindowsAndMessaging::*},
};

pub const WM_TRAY: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;

fn data(hwnd: HWND, icon: HICON, tip: &str) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { zeroed() };
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ICON_ID;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    let max = nid.szTip.len() - 1;
    for (dst, src) in nid.szTip.iter_mut().zip(tip.encode_utf16().take(max)) {
        *dst = src;
    }
    nid
}

/// Also used after Explorer restarts (TaskbarCreated).
pub fn add(hwnd: HWND, icon: HICON, tip: &str) {
    let mut nid = data(hwnd, icon, tip);
    unsafe {
        Shell_NotifyIconW(NIM_ADD, &nid);
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &nid);
    }
}

pub fn update(hwnd: HWND, icon: HICON, tip: &str) {
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &data(hwnd, icon, tip)) };
}

pub fn remove(hwnd: HWND) {
    let mut nid: NOTIFYICONDATAW = unsafe { zeroed() };
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ICON_ID;
    unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}
