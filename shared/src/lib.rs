#![no_std]

pub mod store;
pub mod vga;

#[macro_export]
macro_rules! kprintln {
    ($st:expr, $($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(&mut $st.stdout(), core::format_args!($($arg)*));
        let _ = core::fmt::Write::write_str(&mut $st.stdout(), "\n");
    }};
}
