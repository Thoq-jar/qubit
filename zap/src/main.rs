#![no_main]
#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;
use shared::store::FIRMWARE_NAME;
use shared::{kprintln, vga};
use uefi::prelude::*;
use uefi::table::boot::MemoryType;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[entry]
fn main(handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    kprintln!(
        &mut system_table,
        ">>> {FIRMWARE_NAME} Stage 0 - Firmware initialization <<<"
    );

    uefi::helpers::init(&mut system_table).unwrap();

    let heap_size = 1024 * 1024;
    let heap_start = system_table
        .boot_services()
        .allocate_pool(MemoryType::LOADER_DATA, heap_size)
        .unwrap();
    unsafe {
        ALLOCATOR.lock().init(heap_start, heap_size);
    }

    kprintln!(&mut system_table, "Welcome to Zap!");

    let _ = core::fmt::Write::write_str(&mut system_table.stdout(), "Booting in ");
    for i in (1..=3).rev() {
        let _ = core::fmt::Write::write_fmt(&mut system_table.stdout(), format_args!("{i}... "));
        system_table.boot_services().stall(1_000_000);
    }
    let _ = system_table.stdout().clear();
    kprintln!(&mut system_table, "Booting...");

    mochi::kmain(handle, system_table);

    #[allow(unreachable_code)]
    Status::SUCCESS
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Safety: `uefi::helpers::init` called in `main` before potential panics
    if let Some(mut st) = unsafe { uefi_console() } {
        let _ = st.stdout().clear();
        let _ =
            core::fmt::Write::write_str(&mut st.stdout(), "========== KERNEL PANIC ==========\r\n");
        let _ = core::fmt::Write::write_str(&mut st.stdout(), "A FATAL ERROR OCCURRED\r\n");

        let _ =
            core::fmt::Write::write_fmt(&mut st.stdout(), format_args!("MESSAGE: {}\r\n", info));
    } else {
        vga::clear_screen();
        vga::set_cursor_position(0, 0);
        vga::writeln_fmt(format_args!("========== KERNEL PANIC =========="));
        vga::writeln_fmt(format_args!(" FATAL ERROR OCURRED!"));
        vga::writeln_fmt(format_args!(" MESSAGE: {}", info));
    }
    loop {}
}

unsafe fn uefi_console() -> Option<SystemTable<Boot>> {
    #[allow(unused_unsafe)]
    {
        // SAFETY: accessing global set by helpers::init.
        // if not initialized, this may panic, if it does: the panic handler will double panic and abort
        let st: SystemTable<Boot> = unsafe { uefi::helpers::system_table() };
        Some(st)
    }
}

#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}
