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
    for y in 0..height {
        for x in 0..width {
            draw_pixel(gop, x, y, color);
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

pub fn width(gop: &GraphicsOutput) -> usize {
    gop.current_mode_info().resolution().0
}

pub fn height(gop: &GraphicsOutput) -> usize {
    gop.current_mode_info().resolution().1
}

pub fn to_color(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}
