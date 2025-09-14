use core::fmt;
use core::fmt::Write;
use core::ptr;

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum Color {
    Black = 0x0,
    Blue = 0x1,
    Green = 0x2,
    Cyan = 0x3,
    Red = 0x4,
    Magenta = 0x5,
    Brown = 0x6,
    LightGray = 0x7,
    DarkGray = 0x8,
    LightBlue = 0x9,
    LightGreen = 0xA,
    LightCyan = 0xB,
    LightRed = 0xC,
    Pink = 0xD,
    Yellow = 0xE,
    White = 0xF,
}

#[derive(Copy, Clone)]
struct ColorCode(u8);

impl ColorCode {
    const fn new(fg: Color, bg: Color) -> Self {
        Self((bg as u8) << 4 | (fg as u8))
    }
}

const BUFFER_WIDTH: usize = 80;
const BUFFER_HEIGHT: usize = 25;
const VGA_BUFFER_ADDR: usize = 0xb8000;
static mut CURSOR_ROW: usize = 0;
static mut CURSOR_COL: usize = 0;
static mut CURRENT_COLOR: ColorCode =
    ColorCode((Color::Black as u8) << 4 | (Color::LightGray as u8));

#[inline]
fn write_cell(row: usize, col: usize, byte: u8, color: ColorCode) {
    let idx = row * BUFFER_WIDTH + col;
    let val: u16 = (color.0 as u16) << 8 | (byte as u16);
    let ptr_u16 = (VGA_BUFFER_ADDR as *mut u16).wrapping_add(idx);
    unsafe { ptr::write_volatile(ptr_u16, val) };
}

fn clear_row(row: usize) {
    for col in 0..BUFFER_WIDTH {
        write_cell(row, col, b' ', unsafe { CURRENT_COLOR });
    }
}

fn newline() {
    unsafe {
        if CURSOR_ROW < BUFFER_HEIGHT - 1 {
            CURSOR_ROW += 1;
            CURSOR_COL = 0;
        } else {
            for row in 1..BUFFER_HEIGHT {
                for col in 0..BUFFER_WIDTH {
                    let from_idx = row * BUFFER_WIDTH + col;
                    let to_idx = (row - 1) * BUFFER_WIDTH + col;
                    let from_ptr = (VGA_BUFFER_ADDR as *const u16).wrapping_add(from_idx);
                    let to_ptr = (VGA_BUFFER_ADDR as *mut u16).wrapping_add(to_idx);
                    let val = ptr::read_volatile(from_ptr);
                    ptr::write_volatile(to_ptr, val);
                }
            }
            clear_row(BUFFER_HEIGHT - 1);
            CURSOR_COL = 0;
        }
    }
}

fn write_byte(byte: u8) {
    unsafe {
        match byte {
            b'\n' => {
                newline();
            }
            b => {
                if CURSOR_COL >= BUFFER_WIDTH {
                    newline();
                }
                write_cell(CURSOR_ROW, CURSOR_COL, b, CURRENT_COLOR);
                CURSOR_COL += 1;
            }
        }
    }
}

struct Writer;

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            write_byte(byte);
        }
        Ok(())
    }
}

pub fn set_color(fg: Color, bg: Color) {
    unsafe {
        CURRENT_COLOR = ColorCode::new(fg, bg);
    }
}

pub fn writeln_fmt(args: fmt::Arguments, color: Option<(Color, Color)>) {
    let old_color = unsafe { CURRENT_COLOR };
    if let Some((fg, bg)) = color {
        set_color(fg, bg);
    }
    let _ = Writer.write_fmt(args);
    write_byte(b'\n');

    unsafe {
        CURRENT_COLOR = old_color;
    }
}
