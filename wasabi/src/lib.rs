#![no_std]

use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::{BootServices, OpenProtocolAttributes, OpenProtocolParams};
use uefi::Result;

pub fn with_gop<F, R>(boot_services: &BootServices, mut f: F) -> Result<R>
where
    F: FnMut(&mut GraphicsOutput) -> R,
{
    let gop_handle = boot_services.get_handle_for_protocol::<GraphicsOutput>()?;
    let mut gop = unsafe {
        boot_services.open_protocol::<GraphicsOutput>(
            OpenProtocolParams {
                handle: gop_handle,
                agent: boot_services.image_handle(),
                controller: None,
            },
            OpenProtocolAttributes::Exclusive,
        )?
    };
    Ok(f(&mut gop))
}

pub fn clear(gop: &mut GraphicsOutput, color: u32) {
    let (width, height) = gop.current_mode_info().resolution();
    let stride = gop.current_mode_info().stride();
    let mut framebuffer = gop.frame_buffer();
    for y in 0..height {
        let row_off = y * stride * 4;
        for x in 0..width {
            let off = row_off + x * 4;
            unsafe { framebuffer.write_value(off, color) };
        }
    }
}

pub fn draw_pixel(gop: &mut GraphicsOutput, x: usize, y: usize, color: u32) {
    let (width, height) = gop.current_mode_info().resolution();
    if x >= width || y >= height {
        return;
    }

    let stride = gop.current_mode_info().stride();
    let mut framebuffer = gop.frame_buffer();
    let offset = (y * stride + x) * 4;
    unsafe {
        framebuffer.write_value(offset, color);
    }
}

pub fn fill_rect(gop: &mut GraphicsOutput, x: usize, y: usize, w: usize, h: usize, color: u32) {
    let (sw, sh) = gop.current_mode_info().resolution();
    if x >= sw || y >= sh || w == 0 || h == 0 { return; }
    let max_w = sw - x;
    let max_h = sh - y;
    let w = w.min(max_w);
    let h = h.min(max_h);
    let stride = gop.current_mode_info().stride();
    let mut fb = gop.frame_buffer();
    let start = y * stride + x;
    for row in 0..h {
        let base = (start + row * stride) * 4;
        for col in 0..w {
            unsafe { fb.write_value(base + col * 4, color) };
        }
    }
}

pub fn width(gop: &GraphicsOutput) -> usize {
    gop.current_mode_info().resolution().0
}

pub fn height(gop: &GraphicsOutput) -> usize {
    gop.current_mode_info().resolution().1
}

pub fn to_color(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}
