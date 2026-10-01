//! Generates the .exe icon from the same drawing code as the tray icons and
//! embeds it. The .res file is written by hand (no rc.exe needed): link.exe
//! accepts it directly.

#[allow(dead_code)]
#[path = "src/draw.rs"]
mod draw;

use std::{env, fs, path::PathBuf};

const SIZES: [i32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const LANG_NEUTRAL: u16 = 0;

fn main() {
    println!("cargo:rerun-if-changed=src/draw.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let images: Vec<(i32, Vec<u8>)> = SIZES.iter().map(|&s| (s, png(&draw::draw_app_icon(s)))).collect();

    fs::write(out.join("app.ico"), ico(&images)).unwrap();
    let res = out.join("app.res");
    fs::write(&res, res_file(&images)).unwrap();

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins={}", res.display());
    }
}

/// Icon directory entry fields shared by .ico files and RT_GROUP_ICON.
fn dir_entry(size: i32, bytes: usize) -> Vec<u8> {
    let dim = if size >= 256 { 0 } else { size as u8 }; // 0 means 256
    let mut e = vec![dim, dim, 0, 0];
    e.extend(1u16.to_le_bytes()); // planes
    e.extend(32u16.to_le_bytes()); // bits per pixel
    e.extend((bytes as u32).to_le_bytes());
    e
}

fn dir_header(count: usize) -> Vec<u8> {
    [0u16, 1, count as u16].iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Standalone .ico (handy for inspection; the build embeds the .res).
fn ico(images: &[(i32, Vec<u8>)]) -> Vec<u8> {
    let mut out = dir_header(images.len());
    let mut offset = 6 + 16 * images.len();
    for (size, data) in images {
        out.extend(dir_entry(*size, data.len()));
        out.extend((offset as u32).to_le_bytes());
        offset += data.len();
    }
    images.iter().for_each(|(_, data)| out.extend(data));
    out
}

/// Compiled resource file: one RT_ICON per image plus the RT_GROUP_ICON that
/// Explorer uses (lowest ID = the app icon).
fn res_file(images: &[(i32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    res_entry(&mut out, 0, 0, 0, &[]); // mandatory empty first entry
    let mut group = dir_header(images.len());
    for (i, (size, data)) in images.iter().enumerate() {
        let id = i as u16 + 1;
        res_entry(&mut out, RT_ICON, id, 0x1010, data);
        group.extend(dir_entry(*size, data.len()));
        group.extend(id.to_le_bytes());
    }
    res_entry(&mut out, RT_GROUP_ICON, 1, 0x1030, &group);
    out
}

fn res_entry(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    const HEADER_SIZE: u32 = 32;
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(HEADER_SIZE.to_le_bytes());
    for v in [0xFFFF, kind, 0xFFFF, name] {
        out.extend(v.to_le_bytes()); // ordinal type and name
    }
    out.extend(0u32.to_le_bytes()); // data version
    out.extend(flags.to_le_bytes());
    out.extend(LANG_NEUTRAL.to_le_bytes());
    out.extend(0u32.to_le_bytes()); // version
    out.extend(0u32.to_le_bytes()); // characteristics
    out.extend(data);
    out.resize(out.len().next_multiple_of(4), 0);
}

fn png(canvas: &draw::Canvas) -> Vec<u8> {
    let n = canvas.size as u32;
    let mut raw = Vec::with_capacity((n * (n * 4 + 1)) as usize);
    for row in canvas.rgba().chunks(n as usize) {
        raw.push(0); // filter: none
        row.iter().for_each(|px| raw.extend(px));
    }
    let mut ihdr = Vec::new();
    ihdr.extend(n.to_be_bytes());
    ihdr.extend(n.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]); // 8-bit RGBA

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    png_chunk(&mut out, b"IHDR", &ihdr);
    png_chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 10));
    png_chunk(&mut out, b"IEND", &[]);
    out
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend(kind);
    out.extend(data);
    let crc = crc32(&out[start..]);
    out.extend(crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
