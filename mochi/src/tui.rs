use core::fmt::Write as _;
use heapless::Vec;
use shared::store::{COMP, NAME, PREFIX, VERSION};
use uefi::prelude::*;
use uefi::proto::console::text::{Input, Key, ScanCode};
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::SearchType;
use uefi::Identify;

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
    ProgramEntry {
        name: "glow",
        run: glow_program,
    },
];

const COMMAND_NAMES: &[&str] = &[
    "help",
    "clear",
    "programs",
    "run",
    "ls",
    "pwd",
    "fs-handles",
    "cat",
];

pub fn run(st: &mut SystemTable<Boot>) -> ! {
    {
        let stdout = st.stdout();
        let _ = stdout.reset(false);
        let _ = stdout.enable_cursor(true);
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
    const HISTORY_CAP: usize = 32;
    let mut history: heapless::Vec<heapless::String<256>, HISTORY_CAP> = heapless::Vec::new();
    let mut hist_nav: Option<usize> = None;
    let cwd = "~";

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
        shared::console_println!(st, "/");
    }

    fn cmd_cat(st: &mut SystemTable<Boot>, args: &str) {
        let name = args.trim();
        if name.is_empty() {
            shared::console_println!(st, "Usage: cat <filename>");
            return;
        }

        let mut wbuf = [0u16; 260];
        let c16 = match uefi::CStr16::from_str_with_buf(name, &mut wbuf) {
            Ok(s) => s,
            Err(_) => {
                shared::console_println!(st, "Invalid filename");
                return;
            }
        };

        enum CatOutcome {
            Data(alloc::vec::Vec<u8>),
            IsDir,
            NoFs,
            OpenVolumeErr,
            OpenErr,
            ReadErr,
            StatErr,
        }

        //todo: its obvious what needs to be done
        let outcome = {
            match nori::get_sfs(st.boot_services()) {
                Ok(mut sfs) => match sfs.open_volume() {
                    Ok(mut root) => match root.open(c16, FileMode::Read, FileAttribute::empty()) {
                        Ok(file) => match file.into_type() {
                            Ok(FileType::Regular(mut reg)) => {
                                let mut buf = [0u8; 1024];
                                let mut collected: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
                                let mut err: Option<CatOutcome> = None;
                                loop {
                                    match reg.read(&mut buf) {
                                        Ok(read) => {
                                            if read == 0 {
                                                break;
                                            }
                                            collected.extend_from_slice(&buf[..read]);
                                        }
                                        Err(_) => {
                                            err = Some(CatOutcome::ReadErr);
                                            break;
                                        }
                                    }
                                }
                                match err {
                                    Some(e) => e,
                                    None => CatOutcome::Data(collected),
                                }
                            }
                            Ok(FileType::Dir(_)) => CatOutcome::IsDir,
                            Err(_) => CatOutcome::StatErr,
                        },
                        Err(_) => CatOutcome::OpenErr,
                    },
                    Err(_) => CatOutcome::OpenVolumeErr,
                },
                Err(_) => CatOutcome::NoFs,
            }
        };

        match outcome {
            CatOutcome::Data(bytes) => {
                for &b in bytes.iter() {
                    let ch = b as char;
                    match ch {
                        '\r' => {}
                        '\n' => {
                            let _ = writeln!(st.stdout(), "");
                        }
                        _ if ch.is_ascii_graphic() || ch == ' ' => {
                            let _ = write!(st.stdout(), "{}", ch);
                        }
                        _ => {}
                    }
                }
                shared::console_println!(st, "");
            }
            CatOutcome::IsDir => {
                shared::console_println!(st, "{}: is a directory", name);
            }
            CatOutcome::NoFs => {
                shared::console_println!(st, "No filesystem available");
            }
            CatOutcome::OpenVolumeErr => {
                shared::console_println!(st, "Failed to open root volume");
            }
            CatOutcome::OpenErr => {
                shared::console_println!(st, "Cannot open: {}", name);
            }
            CatOutcome::ReadErr => {
                shared::console_println!(st, "Read error");
            }
            CatOutcome::StatErr => {
                shared::console_println!(st, "Failed to stat file");
            }
        }
    }

    static COMMANDS: &[CommandEntry] = &[
        CommandEntry {
            name: "help",
            help: "Show this help",
            run: |_st, _args| {},
        },
        CommandEntry {
            name: "clear",
            help: "Clear screen",
            run: cmd_clear,
        },
        CommandEntry {
            name: "programs",
            help: "List programs",
            run: cmd_programs,
        },
        CommandEntry {
            name: "run",
            help: "Run a program: run <name>",
            run: cmd_run,
        },
        CommandEntry {
            name: "ls",
            help: "List root directory",
            run: cmd_ls,
        },
        CommandEntry {
            name: "pwd",
            help: "Print current directory",
            run: cmd_pwd,
        },
        CommandEntry {
            name: "fs-handles",
            help: "Count available filesystems",
            run: cmd_fs_handles,
        },
        CommandEntry {
            name: "cat",
            help: "Show file contents: cat <name>",
            run: cmd_cat,
        },
    ];
    loop {
        {
            let _ = write!(st.stdout(), "root@mochi:{}{}", cwd, PREFIX);
        }
        line.clear();
        read_line_shell(st, &mut line, &history, &mut hist_nav, cwd);

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

        if history.last().map(|h| h.as_str()) != Some(s) {
            let mut item = heapless::String::<256>::new();
            let _ = item.push_str(s);
            if history.len() == HISTORY_CAP {
                let _ = history.remove(0);
            }
            let _ = history.push(item);
        }
        hist_nav = None;
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

fn read_line_simple(st: &mut SystemTable<Boot>, buf: &mut heapless::String<256>) {
    let _ = st.stdout().enable_cursor(false);
    let _ = write!(st.stdout(), "");
    let _ = write!(st.stdout(), "█");
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
                            let _ = write!(st.stdout(), "\u{8}");
                            shared::console_println!(st, "");
                            let _ = st.stdout().enable_cursor(true);
                            let _ = write!(st.stdout(), "\u{1b}");
                            return;
                        }
                        '\u{8}' => {
                            if !buf.is_empty() {
                                buf.pop();
                                let _ = write!(st.stdout(), "\u{8}\u{8} \u{8}█");
                            }
                        }
                        _ => {
                            if buf.push(c).is_ok() {
                                let _ = write!(st.stdout(), "\u{8}{}█", c);
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

fn read_line_shell(
    st: &mut SystemTable<Boot>,
    buf: &mut heapless::String<256>,
    history: &heapless::Vec<heapless::String<256>, 32>,
    hist_nav: &mut Option<usize>,
    cwd: &str,
) {
    let _ = st.stdout().enable_cursor(false);
    let _ = write!(st.stdout(), "\u{1b}");
    let _ = write!(st.stdout(), "█");
    loop {
        let read_result = { st.stdin().read_key() };
        match read_result {
            Ok(Some(key)) => match key {
                Key::Printable(c16) => {
                    let c: char = c16.into();
                    match c {
                        '\r' | '\n' => {
                            let _ = write!(st.stdout(), "\u{8}");
                            shared::console_println!(st, "");
                            let _ = st.stdout().enable_cursor(true);
                            let _ = write!(st.stdout(), "\u{1b}");
                            return;
                        }
                        '\u{8}' => {
                            if !buf.is_empty() {
                                buf.pop();
                                let _ = write!(st.stdout(), "\u{8}\u{8} \u{8}█");
                            }
                        }
                        '\t' => {
                            let _ = write!(st.stdout(), "\u{8}");
                            autocomplete_line(st, buf, cwd);
                            let _ = write!(st.stdout(), "█");
                        }
                        _ => {
                            if buf.push(c).is_ok() {
                                let _ = write!(st.stdout(), "\u{8}{}█", c);
                            }
                        }
                    }
                }
                Key::Special(sc) => match sc {
                    ScanCode::ESCAPE => {}
                    ScanCode::UP => {
                        if history.is_empty() {
                            continue;
                        }
                        let idx = match *hist_nav {
                            Some(i) => i.saturating_add(1),
                            None => 0,
                        };
                        if idx >= history.len() {
                            continue;
                        }
                        *hist_nav = Some(idx);
                        let s = &history[history.len() - 1 - idx];
                        let _ = write!(st.stdout(), "\u{8} ");
                        for _ in 0..buf.len() {
                            let _ = write!(st.stdout(), "\u{8}\u{8}");
                        }
                        buf.clear();
                        let _ = buf.push_str(s);
                        let _ = write!(st.stdout(), "{}█", s);
                    }
                    ScanCode::DOWN => {
                        if history.is_empty() {
                            continue;
                        }
                        match *hist_nav {
                            None => {}
                            Some(0) => {
                                *hist_nav = None;
                                let _ = write!(st.stdout(), "\u{8} ");
                                for _ in 0..buf.len() {
                                    let _ = write!(st.stdout(), "\u{8}\u{8}");
                                }
                                buf.clear();
                                let _ = write!(st.stdout(), "█");
                            }
                            Some(i) => {
                                let ni = i - 1;
                                *hist_nav = Some(ni);
                                let s = &history[history.len() - 1 - ni];
                                let _ = write!(st.stdout(), "\u{8} ");
                                for _ in 0..buf.len() {
                                    let _ = write!(st.stdout(), "\u{8}\u{8}");
                                }
                                buf.clear();
                                let _ = buf.push_str(s);
                                let _ = write!(st.stdout(), "{}█", s);
                            }
                        }
                    }
                    ScanCode::LEFT | ScanCode::RIGHT => {}
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

fn autocomplete_line(st: &mut SystemTable<Boot>, buf: &mut heapless::String<256>, cwd: &str) {
    let mut snapshot = heapless::String::<256>::new();
    let _ = snapshot.push_str(buf.as_str());
    let s = snapshot.as_str();

    let mut candidates: heapless::Vec<&'static str, 32> = heapless::Vec::new();
    let (head, tail) = match s.split_once(' ') {
        Some((h, t)) => (h, Some(t)),
        None => (s, None),
    };

    if tail.is_none() {
        for &name in COMMAND_NAMES {
            let _ = candidates.push(name);
        }
        for p in PROGRAMS {
            let _ = candidates.push(p.name);
        }
        complete_from_set(st, buf, head, candidates.as_slice(), None, cwd);
    } else {
        if head == "run" {
            for p in PROGRAMS {
                let _ = candidates.push(p.name);
            }
            complete_from_set(
                st,
                buf,
                tail.unwrap(),
                candidates.as_slice(),
                Some("run "),
                cwd,
            );
        }
    }
}

fn complete_from_set(
    st: &mut SystemTable<Boot>,
    buf: &mut heapless::String<256>,
    fragment: &str,
    set: &[&'static str],
    prefix: Option<&'static str>,
    cwd: &str,
) {
    let mut matches: heapless::Vec<&'static str, 32> = heapless::Vec::new();
    for name in set.iter().copied() {
        if name.starts_with(fragment) {
            let _ = matches.push(name);
        }
    }

    if matches.is_empty() {
        return;
    }
    if matches.len() == 1 {
        for _ in 0..buf.len() {
            let _ = write!(st.stdout(), "\u{8} \u{8}");
        }
        buf.clear();
        if let Some(p) = prefix {
            let _ = buf.push_str(p);
        }
        let _ = buf.push_str(matches[0]);
        let _ = write!(st.stdout(), "{}", buf.as_str());
        return;
    }

    shared::console_println!(st, "");
    for (i, m) in matches.iter().enumerate() {
        if i > 0 {
            let _ = write!(st.stdout(), " ");
        }
        let _ = write!(st.stdout(), "{}", m);
    }
    shared::console_println!(st, "");
    let _ = write!(st.stdout(), "{}{}", cwd, PREFIX);
    let _ = write!(st.stdout(), "{}", buf.as_str());
}

fn echo_program(st: &mut SystemTable<Boot>) {
    let out = st.stdout();
    let _ = out.clear();
    shared::console_println!(st, "Echo program. Type 'exit' to return.");
    let mut line = heapless::String::<256>::new();
    loop {
        let _ = write!(st.stdout(), "echo {} ", PREFIX);
        line.clear();
        read_line_simple(st, &mut line);
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

fn glow_program(st: &mut SystemTable<Boot>) {
    let out = st.stdout();
    let _ = out.clear();
    shared::console_println!(st, "glow — neovim real no clickbait");
    shared::console_println!(st, "Type text. Commands: :q to quit.");
    {
        let stdin = st.stdin();
        let _ = stdin.reset(false);
    }

    let mut line = heapless::String::<256>::new();
    loop {
        line.clear();
        let _ = write!(st.stdout(), "> ");
        read_line_simple(st, &mut line);
        let s = line.as_str();
        if s.starts_with(':') {
            let cmd = &s[1..].trim();
            match *cmd {
                "q" | "quit" => break,
                _ => {
                    shared::console_println!(st, "Unknown command: :{}", cmd);
                }
            }
        } else {
            shared::console_println!(st, "{}", s);
        }
    }
}
