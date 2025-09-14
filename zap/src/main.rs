#![no_main]
#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;
use log::info;
use shared::store::NAME;
use uefi::prelude::*;
use uefi::table::boot::MemoryType;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[entry]
fn main(handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut system_table).unwrap();

    let heap_size = 1024 * 1024;
    let heap_start = system_table
        .boot_services()
        .allocate_pool(MemoryType::LOADER_DATA, heap_size)
        .unwrap();
    unsafe {
        ALLOCATOR.lock().init(heap_start, heap_size);
    }

    info!(">>> {NAME} Stage 0 - Firmware initialization <<<");

    mochi::kmain(handle, system_table);

    #[allow(unreachable_code)]
    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}
