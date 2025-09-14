#![no_std]
extern crate alloc;

mod tui;

use log::info;
use shared::store::{COMP, NAME};
use uefi::prelude::*;

#[no_mangle]
pub extern "C" fn kmain(_image_handle: Handle, mut system_table: SystemTable<Boot>) -> ! {
    uefi::helpers::init(&mut system_table).unwrap();

    info!(">>> {NAME} Stage 1 - Initializing <<<");

    info!(">>> {NAME} Stage 2 - Loading userland <<<");

    info!("Welcome to {COMP} {NAME}!");

    tui::run(&mut system_table);
}
