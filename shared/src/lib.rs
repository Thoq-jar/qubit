#![no_std]

pub mod store;
pub mod vga;

#[macro_export]
macro_rules! vga_println {
    ($($arg:tt)*) => {{
        $crate::vga::writeln_fmt(core::format_args!($($arg)*), None);
    }};
}

#[macro_export]
macro_rules! vga_println_color {
    ($fg:expr, $bg:expr, $($arg:tt)*) => {{
        $crate::vga::writeln_fmt(core::format_args!($($arg)*), Some(($fg, $bg)));
    }};
}

#[macro_export]
macro_rules! console_println {
    ($st:expr, $($arg:tt)*) => {{
        use core::fmt::Write as _;
        let _ = writeln!($st.stdout(), $($arg)*);
    }};
}

#[macro_export]
macro_rules! console_println_color {
    ($st:expr, $fg:expr, $bg:expr, $($arg:tt)*) => {{
        use core::fmt::Write as _;
        let _ = write!($st.stdout(), "{}{}", $fg, $bg);
        let _ = writeln!($st.stdout(), $($arg)*);
        let _ = write!($st.stdout(), "\x1b[0m");
    }};
}
