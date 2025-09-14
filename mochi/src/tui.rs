use core::fmt::Write as _;
use shared::store::{COMP, NAME, PREFIX, VERSION};
use uefi::prelude::*;
use uefi::proto::console::text::{Input, Key, ScanCode};
use uefi::Identify;
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::SearchType;
use heapless::Vec;

struct ProgramEntry {
    name: &'static str,
    run: fn(st: &mut SystemTable<Boot>),
}

const PROGRAMS: &[ProgramEntry] = &[
    ProgramEntry {
        name: "echo",
        run: echo_program,
    },
    ProgramEntry {
        name: "keys",
        run: keys_program,
    },
];

pub fn run(st: &mut SystemTable<Boot>) -> ! {
    {
        let stdout = st.stdout();
        let _ = stdout.reset(false);
        let _ = stdout.clear();
        shared::console_println!(st, "Welcome to {COMP} {NAME}!");
        shared::console_println!(st, "You are using v{VERSION}");
        shared::console_println!(st, "Run 'help' to get started!");
    }

    {
        let stdin = st.stdin();
        let _ = stdin.reset(false);
    }

    let mut line = heapless::String::<256>::new();

    struct CommandEntry {
        name: &'static str,
        help: &'static str,
        run: fn(st: &mut SystemTable<Boot>, args: &str),
    }

    fn cmd_clear(st: &mut SystemTable<Boot>, _args: &str) {
        let _ = st.stdout().clear();
    }

    fn cmd_programs(st: &mut SystemTable<Boot>, _args: &str) {
        shared::console_println!(st, "Programs: {}", list_programs());
    }

    fn cmd_run(st: &mut SystemTable<Boot>, args: &str) {
        let name = args.trim();
        if name.is_empty() {
            shared::console_println!(st, "Usage: run <name>");
            return;
        }
        if let Some(p) = find_program(name) {
            shared::console_println!(st, "Launching '{}'...", p.name);
            (p.run)(st);
            shared::console_println!(st, "Program '{}' exited.", p.name);
        } else {
            shared::console_println!(st, "No such program: {}", name);
        }
    }

    fn cmd_ls(st: &mut SystemTable<Boot>, _args: &str) {
        let mut entries: Vec<heapless::String<64>, 128> = Vec::new();
        nori::list_root(st, |name| {
            let mut s = heapless::String::<64>::new();
            let _ = core::fmt::Write::write_fmt(&mut s, core::format_args!("{}", name));
            let _ = entries.push(s);
        });
        for s in entries.iter() {
            shared::console_println!(st, "{}", s);
        }
    }

    fn cmd_fs_handles(st: &mut SystemTable<Boot>, _args: &str) {
        let count = {
            let bt = st.boot_services();
            match bt.locate_handle_buffer(SearchType::ByProtocol(&SimpleFileSystem::GUID)) {
                Ok(handles) => handles.len(),
                Err(_) => 0,
            }
        };
        shared::console_println!(st, "Filesystems found: {}", count);
    }

    fn cmd_pwd(st: &mut SystemTable<Boot>, _args: &str) {
        // Only root supported currently
        shared::console_println!(st, "/");
    }

    static COMMANDS: &[CommandEntry] = &[
        CommandEntry { name: "help", help: "Show this help", run: |_st, _args| {} },
        CommandEntry { name: "clear", help: "Clear screen", run: cmd_clear },
        CommandEntry { name: "programs", help: "List programs", run: cmd_programs },
        CommandEntry { name: "run", help: "Run a program: run <name>", run: cmd_run },
        CommandEntry { name: "ls", help: "List root directory", run: cmd_ls },
        CommandEntry { name: "pwd", help: "Print current directory", run: cmd_pwd },
        CommandEntry { name: "fs-handles", help: "Count available filesystems", run: cmd_fs_handles },
    ];
    loop {
        {
            let _ = write!(st.stdout(), "{}", PREFIX);
        }
        line.clear();
        read_line(st, &mut line);

        let s = line.trim();
        if s.is_empty() {
            continue;
        }
        let (cmd_name, args) = match s.split_once(' ') {
            Some((c, rest)) => (c, rest),
            None => (s, ""),
        };

        if cmd_name == "help" {
            shared::console_println!(st, "Commands:");
            for c in COMMANDS {
                shared::console_println!(st, "  {:<12} {}", c.name, c.help);
            }
            continue;
        }

        match COMMANDS.iter().find(|c| c.name == cmd_name) {
            Some(c) => (c.run)(st, args),
            None => shared::console_println!(st, "Unknown: {} (try 'help')", cmd_name),
        }
    }
}

fn list_programs() -> heapless::String<128> {
    let mut s = heapless::String::<128>::new();
    for (i, p) in PROGRAMS.iter().enumerate() {
        if i > 0 {
            let _ = s.push_str(", ");
        }
        let _ = s.push_str(p.name);
    }
    s
}

fn find_program(name: &str) -> Option<&'static ProgramEntry> {
    PROGRAMS.iter().find(|p| p.name == name)
}

// Replaced ad-hoc command parser with a registry above

fn read_line(st: &mut SystemTable<Boot>, buf: &mut heapless::String<256>) {
    loop {
        let read_result = {
            let stdin: &mut Input = st.stdin();
            stdin.read_key()
        };
        match read_result {
            Ok(Some(key)) => match key {
                Key::Printable(c16) => {
                    let c: char = c16.into();
                    match c {
                        '\r' | '\n' => {
                            shared::console_println!(st, "");
                            return;
                        }
                        '\u{8}' => {
                            if !buf.is_empty() {
                                buf.pop();
                                let _ = write!(st.stdout(), "\u{8} \u{8}");
                            }
                        }
                        _ => {
                            if buf.push(c).is_ok() {
                                let _ = write!(st.stdout(), "{}", c);
                            }
                        }
                    }
                }
                Key::Special(sc) => match sc {
                    ScanCode::ESCAPE => { /* ignore */ }
                    ScanCode::UP | ScanCode::DOWN | ScanCode::LEFT | ScanCode::RIGHT => { /* ignore */
                    }
                    _ => {}
                },
            },
            Ok(None) => {
                let _ = st.boot_services().stall(5000);
            }
            Err(_) => {
                let _ = st.boot_services().stall(10000);
            }
        }
    }
}

fn echo_program(st: &mut SystemTable<Boot>) {
    let out = st.stdout();
    let _ = out.clear();
    shared::console_println!(st, "Echo program. Type 'exit' to return.");
    let mut line = heapless::String::<256>::new();
    loop {
        let _ = write!(st.stdout(), "echo {} ", PREFIX);
        line.clear();
        read_line(st, &mut line);
        let s = line.trim();
        if s == "exit" {
            break;
        }
        shared::console_println!(st, "{}", s);
    }
}

fn keys_program(st: &mut SystemTable<Boot>) {
    let out = st.stdout();
    let _ = out.clear();
    shared::console_println!(st, "Keys demo. Press ESC to return.");

    {
        let stdin = st.stdin();
        let _ = stdin.reset(false);
    }
    loop {
        let read_result = { st.stdin().read_key() };
        match read_result {
            Ok(Some(Key::Printable(c16))) => {
                let c: char = c16.into();
                shared::console_println!(st, "Printable: {:?}", c);
                if c == '\u{1b}' {
                    break;
                }
            }
            Ok(Some(Key::Special(sc))) => {
                shared::console_println!(st, "Special: {:?}", sc);
                if sc == ScanCode::ESCAPE {
                    break;
                }
            }
            Ok(None) => {
                let _ = st.boot_services().stall(5_000);
            }
            Err(_) => {
                let _ = st.boot_services().stall(10_000);
            }
        }
    }
}
