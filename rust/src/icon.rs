//! Tray icons rendered at runtime: crisp at any DPI, theme-aware, and no
//! image assets in the binary. The drawing itself lives in `draw.rs`.

use crate::draw::{self, Glyph};
use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::TRUE,
    Graphics::Gdi::*,
    System::Registry::*,
    UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO},
};

/// True when the taskbar uses the light theme (so glyphs must be dark).
pub fn light_taskbar() -> bool {
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0".encode_utf16().collect();
    let value: Vec<u16> = "SystemUsesLightTheme\0".encode_utf16().collect();
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut data as *mut u32 as *mut _,
            &mut size,
        ) == 0
            && data != 0
    }
}

pub fn render(glyph: Glyph, size: i32, light: bool) -> HICON {
    to_icon(&draw::draw(glyph, size, light))
}

fn to_icon(canvas: &draw::Canvas) -> HICON {
    let n = canvas.size as i32;
    unsafe {
        let mut bmi: BITMAPINFO = zeroed();
        bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = n;
        bmi.bmiHeader.biHeight = -n; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut bits = null_mut();
        let color = CreateDIBSection(null_mut(), &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if color.is_null() {
            return null_mut();
        }
        let pixels = canvas.bgra();
        std::slice::from_raw_parts_mut(bits as *mut u32, pixels.len()).copy_from_slice(&pixels);

        // All-zero AND mask: transparency comes from the alpha channel.
        let stride = canvas.size.div_ceil(16) * 2;
        let mask_bits = vec![0u8; stride * canvas.size];
        let mask = CreateBitmap(n, n, 1, 1, mask_bits.as_ptr() as *const _);

        let info = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}
