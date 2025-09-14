use core::fmt;
use core::ptr;

const BUFFER_WIDTH: usize = 80;
const BUFFER_HEIGHT: usize = 25;
const VGA_BUFFER_ADDR: usize = 0xb8000;
static mut CURSOR_ROW: usize = 0;
static mut CURSOR_COL: usize = 0;
const DEFAULT_ATTR: u8 = 0x07;

#[inline]
fn write_cell(row: usize, col: usize, byte: u8) {
    let idx = row * BUFFER_WIDTH + col;
    let val: u16 = ((DEFAULT_ATTR as u16) << 8) | (byte as u16);
    let ptr_u16 = (VGA_BUFFER_ADDR as *mut u16).wrapping_add(idx);
    unsafe { ptr::write_volatile(ptr_u16, val) };
}

pub fn clear_screen() {
    for row in 0..BUFFER_HEIGHT {
        clear_row(row);
    }
    set_cursor_position(0, 0);
}

pub fn set_cursor_position(row: usize, col: usize) {
    unsafe {
        CURSOR_ROW = row;
        CURSOR_COL = col;
    }
}

fn clear_row(row: usize) {
    for col in 0..BUFFER_WIDTH {
        write_cell(row, col, b' ');
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
                write_cell(CURSOR_ROW, CURSOR_COL, b);
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

pub fn write_fmt(args: fmt::Arguments) {
    let mut w = Writer;
    let _ = fmt::Write::write_fmt(&mut w, args);
}

pub fn writeln_fmt(args: fmt::Arguments) {
    write_fmt(args);
    write_byte(b'\n');
}
