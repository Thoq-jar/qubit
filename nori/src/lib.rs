#![no_std]

use log::info;
use uefi::prelude::*;
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::{BootServices, ScopedProtocol, SearchType};
use uefi::{cstr16, Identify};
use uefi::CStr16;

pub fn list_root_directory(system_table: &mut SystemTable<Boot>) {
    let bt = system_table.boot_services();
    let mut sfs = get_sfs(bt).expect("Failed to get SimpleFileSystem protocol");
    let mut root = sfs.open_volume().expect("Failed to open volume");

    let mut buffer = [0u8; 1024];
    loop {
        let info = match root
            .read_entry(&mut buffer)
            .expect("Failed to read directory entry")
        {
            Some(info) => info,
            None => break,
        };

        let name = info.file_name();
        if name == cstr16!(".") || name == cstr16!("..") {
            continue;
        }

        info!("{}", name);
    }
}

pub fn get_sfs<'a>(bt: &'a BootServices) -> uefi::Result<ScopedProtocol<'a, SimpleFileSystem>> {
    let handle = bt.locate_handle_buffer(SearchType::ByProtocol(&SimpleFileSystem::GUID))?[0];
    bt.open_protocol_exclusive::<SimpleFileSystem>(handle)
}

pub fn list_root<'a, F>(system_table: &mut SystemTable<Boot>, mut f: F)
where
    F: FnMut(&CStr16),
{
    let bt = system_table.boot_services();
    let mut sfs = get_sfs(bt).expect("Failed to get SimpleFileSystem protocol");
    let mut root = sfs.open_volume().expect("Failed to open volume");

    let mut buffer = [0u8; 1024];
    loop {
        let info = match root
            .read_entry(&mut buffer)
            .expect("Failed to read directory entry")
        {
            Some(info) => info,
            None => break,
        };

        let name = info.file_name();
        if name == cstr16!(".") || name == cstr16!("..") {
            continue;
        }

        f(name);
    }
}
