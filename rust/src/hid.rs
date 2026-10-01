//! Raw Win32 HID access: device discovery, the battery request and
//! overlapped (event-driven) reads. No polling: the main loop waits on
//! `Device::event()` and calls `Device::read()` when it is signaled.

use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::{
    core::GUID,
    Win32::{
        Devices::{DeviceAndDriverInstallation::*, HumanInterfaceDevice::*},
        Foundation::*,
        Storage::FileSystem::*,
        System::{Threading::CreateEventW, IO::*},
        UI::WindowsAndMessaging::*,
    },
};

const CORSAIR_VID: u16 = 0x1B1C;
const DATA_REQ: [u8; 2] = [0xC9, 0x64];
const MIC_UP_FLAG: u8 = 128;
const WRITE_TIMEOUT_MS: u32 = 1000;
const DEVICE_NOTIFY_WINDOW_HANDLE: u32 = 0;

fn model_name(pid: u16) -> Option<&'static str> {
    Some(match pid {
        0x0A38 => "Corsair HS70 Wireless",
        0x0A4F => "Corsair HS70 PRO Wireless",
        0x1B27 | 0x0A2B => "Corsair VOID Wireless",
        0x0A14 | 0x0A16 | 0x0A1A => "Corsair VOID PRO Wireless",
        0x0A55 | 0x0A51 => "Corsair VOID ELITE Wireless",
        0x0A3E | 0x0A40 | 0x0A42 | 0x0A44 | 0x0A5C | 0x0A64 => "Corsair Virtuoso RGB Wireless",
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq)]
pub struct Report {
    pub battery: u8,
    pub state: u8,
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.0) };
        }
    }
}

pub struct Device {
    pub name: &'static str,
    // Field order matters: the file must close before the event it signals.
    file: Handle,
    event: Handle,
    ov: Box<OVERLAPPED>,
    buf: Vec<u8>,
    out_len: usize,
    pending: bool,
}

impl Device {
    /// Opens the first known Corsair headset that accepts the battery request.
    pub fn open() -> Option<Device> {
        let list = interface_list(&hid_guid())?;
        list.split(|&c| c == 0)
            .filter(|path| is_corsair_path(path))
            .find_map(|path| {
                let mut path = path.to_vec();
                path.push(0);
                unsafe { Self::try_open(&path) }
            })
    }

    unsafe fn try_open(path: &[u16]) -> Option<Device> {
        let file = Handle(CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            null_mut(),
        ));
        if file.0 == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut attrs: HIDD_ATTRIBUTES = zeroed();
        attrs.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
        if HidD_GetAttributes(file.0, &mut attrs) == 0 || attrs.VendorID != CORSAIR_VID {
            return None;
        }
        let name = model_name(attrs.ProductID)?;

        let (in_len, out_len) = report_lengths(file.0)?;
        if in_len < 5 || out_len < DATA_REQ.len() {
            return None;
        }

        let event = Handle(CreateEventW(null(), TRUE, FALSE, null()));
        if event.0.is_null() {
            return None;
        }

        let mut dev = Device {
            name,
            file,
            event,
            ov: Box::new(zeroed()),
            buf: vec![0; in_len],
            out_len,
            pending: false,
        };
        (dev.request() && dev.start_read()).then_some(dev)
    }

    pub fn event(&self) -> HANDLE {
        self.event.0
    }

    /// Completes the pending read and queues the next one.
    /// `Err` means the device is gone and must be dropped.
    pub fn read(&mut self) -> Result<Option<Report>, ()> {
        let mut n = 0u32;
        unsafe {
            if GetOverlappedResult(self.file.0, &*self.ov, &mut n, FALSE) == 0 {
                return if GetLastError() == ERROR_IO_INCOMPLETE { Ok(None) } else { Err(()) };
            }
        }
        self.pending = false;
        let report = parse(&self.buf[..n as usize]);
        if self.start_read() {
            Ok(report)
        } else {
            Err(())
        }
    }

    /// Sends the battery/state request; the answer arrives as an input report.
    unsafe fn request(&mut self) -> bool {
        let mut buf = vec![0u8; self.out_len];
        buf[..DATA_REQ.len()].copy_from_slice(&DATA_REQ);
        let mut ov: OVERLAPPED = zeroed();
        ov.hEvent = self.event.0;
        if WriteFile(self.file.0, buf.as_ptr(), buf.len() as u32, null_mut(), &mut ov) == 0
            && GetLastError() != ERROR_IO_PENDING
        {
            return false;
        }
        let mut n = 0u32;
        if GetOverlappedResultEx(self.file.0, &ov, &mut n, WRITE_TIMEOUT_MS, FALSE) == 0 {
            CancelIoEx(self.file.0, &ov);
            GetOverlappedResult(self.file.0, &ov, &mut n, TRUE);
            return false;
        }
        true
    }

    fn start_read(&mut self) -> bool {
        unsafe {
            *self.ov = zeroed();
            self.ov.hEvent = self.event.0;
            let ok = ReadFile(
                self.file.0,
                self.buf.as_mut_ptr(),
                self.buf.len() as u32,
                null_mut(),
                &mut *self.ov,
            );
            // Synchronous completion also signals the event, so both paths
            // are handled by the main loop.
            self.pending = ok != 0 || GetLastError() == ERROR_IO_PENDING;
        }
        self.pending
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        if self.pending {
            unsafe {
                CancelIoEx(self.file.0, &*self.ov);
                let mut n = 0u32;
                GetOverlappedResult(self.file.0, &*self.ov, &mut n, TRUE);
            }
        }
    }
}

fn parse(data: &[u8]) -> Option<Report> {
    // Devices without numbered reports prepend a 0 report ID; skip it.
    let data = match data.first() {
        Some(0) => &data[1..],
        _ => data,
    };
    if data.len() < 5 {
        return None;
    }
    let mut battery = data[2];
    if battery > MIC_UP_FLAG {
        battery -= MIC_UP_FLAG;
    }
    Some(Report { battery: battery.min(100), state: data[4] })
}

fn hid_guid() -> GUID {
    unsafe {
        let mut guid = zeroed();
        HidD_GetHidGuid(&mut guid);
        guid
    }
}

/// Present HID interface paths as a double-NUL-terminated UTF-16 list.
fn interface_list(guid: &GUID) -> Option<Vec<u16>> {
    unsafe {
        loop {
            let mut len = 0u32;
            if CM_Get_Device_Interface_List_SizeW(&mut len, guid, null(), CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
                != CR_SUCCESS
            {
                return None;
            }
            let mut buf = vec![0u16; len as usize];
            match CM_Get_Device_Interface_ListW(guid, null(), buf.as_mut_ptr(), len, CM_GET_DEVICE_INTERFACE_LIST_PRESENT) {
                CR_SUCCESS => return Some(buf),
                CR_BUFFER_SMALL => continue, // a device arrived in between
                _ => return None,
            }
        }
    }
}

/// Cheap pre-filter so we never open unrelated HID devices.
fn is_corsair_path(path: &[u16]) -> bool {
    String::from_utf16_lossy(path).to_ascii_lowercase().contains("vid_1b1c")
}

unsafe fn report_lengths(file: HANDLE) -> Option<(usize, usize)> {
    let mut pp: PHIDP_PREPARSED_DATA = 0;
    if HidD_GetPreparsedData(file, &mut pp) == 0 {
        return None;
    }
    let mut caps: HIDP_CAPS = zeroed();
    let status = HidP_GetCaps(pp, &mut caps);
    HidD_FreePreparsedData(pp);
    (status == HIDP_STATUS_SUCCESS)
        .then_some((caps.InputReportByteLength as usize, caps.OutputReportByteLength as usize))
}

/// Delivers WM_DEVICECHANGE to `hwnd` whenever a HID device arrives or leaves.
pub fn register_notifications(hwnd: HWND) {
    unsafe {
        let mut filter: DEV_BROADCAST_DEVICEINTERFACE_W = zeroed();
        filter.dbcc_size = size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32;
        filter.dbcc_devicetype = DBT_DEVTYP_DEVICEINTERFACE;
        filter.dbcc_classguid = hid_guid();
        RegisterDeviceNotificationW(hwnd, &filter as *const _ as *const _, DEVICE_NOTIFY_WINDOW_HANDLE);
    }
}
