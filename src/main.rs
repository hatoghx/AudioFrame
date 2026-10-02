#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod ae;
mod audio;
mod core;
mod export;
mod mmd;
mod model;
mod ui;

use core::loc::{self, Lang};
use eframe::egui;

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let bytes = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let bytes = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    let bytes = bytes.get(offset..offset.checked_add(4)?)?;
    Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn application_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("img/icon.ico");
    if read_u16(bytes, 0)? != 0 || read_u16(bytes, 2)? != 1 {
        return None;
    }

    let count = read_u16(bytes, 4)? as usize;
    let mut best: Option<(u32, u32, Vec<u8>)> = None;

    for index in 0..count {
        let entry = 6usize.checked_add(index.checked_mul(16)?)?;
        let entry_bytes = bytes.get(entry..entry.checked_add(16)?)?;
        let size = read_u32(entry_bytes, 8)? as usize;
        let offset = read_u32(entry_bytes, 12)? as usize;
        let image = bytes.get(offset..offset.checked_add(size)?)?;
        if image.len() < 40 || read_u32(image, 0)? != 40 {
            continue;
        }

        let width = read_i32(image, 4)?;
        let total_height = read_i32(image, 8)?;
        let bit_count = read_u16(image, 14)?;
        let compression = read_u32(image, 16)?;
        if width <= 0 || total_height == 0 || bit_count != 32 || compression != 0 {
            continue;
        }

        let height = total_height.unsigned_abs() / 2;
        let width = width as u32;
        if height == 0 {
            continue;
        }
        let pixel_count = width.checked_mul(height)? as usize;
        let pixel_bytes = pixel_count.checked_mul(4)?;
        let pixel_end = 40usize.checked_add(pixel_bytes)?;
        if pixel_end > image.len() {
            continue;
        }

        let row_bytes = width.checked_mul(4)? as usize;
        let mut rgba = vec![0; pixel_bytes];
        let mut has_alpha = false;
        for y in 0..height as usize {
            let source_y = if total_height > 0 {
                height as usize - 1 - y
            } else {
                y
            };
            for x in 0..width as usize {
                let source = 40 + source_y * row_bytes + x * 4;
                let target = (y * width as usize + x) * 4;
                rgba[target] = image[source + 2];
                rgba[target + 1] = image[source + 1];
                rgba[target + 2] = image[source];
                rgba[target + 3] = image[source + 3];
                has_alpha |= image[source + 3] != 0;
            }
        }

        if !has_alpha {
            let mask_row_bytes = width.checked_add(31)?.checked_div(32)?.checked_mul(4)? as usize;
            let mask_end = pixel_end.checked_add(mask_row_bytes.checked_mul(height as usize)?)?;
            if mask_end <= image.len() {
                for y in 0..height as usize {
                    let source_y = if total_height > 0 {
                        height as usize - 1 - y
                    } else {
                        y
                    };
                    for x in 0..width as usize {
                        let mask = image[pixel_end + source_y * mask_row_bytes + x / 8];
                        if mask & (0x80 >> (x % 8)) != 0 {
                            rgba[(y * width as usize + x) * 4 + 3] = 0;
                        } else {
                            rgba[(y * width as usize + x) * 4 + 3] = 255;
                        }
                    }
                }
            }
        }

        let replace = best
            .as_ref()
            .map(|(best_width, best_height, _)| width * height > *best_width * *best_height)
            .unwrap_or(true);
        if replace {
            best = Some((width, height, rgba));
        }
    }

    let (width, height, rgba) = best?;
    Some(egui::IconData {
        rgba,
        width,
        height,
    })
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    loc::init();
    loc::set_lang(Lang::detect_system());

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("AudioFrame")
        .with_inner_size([800.0, 400.0])
        .with_min_inner_size([600.0, 250.0]);
    if let Some(icon) = application_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "AudioFrame",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
