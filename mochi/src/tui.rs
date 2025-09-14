use core::fmt::Write as _;
use heapless::Vec;
use shared::kprintln;
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
    ProgramEntry {
        name: "zam",
        run: zam_program,
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
    "x:debug-panic",
];

pub fn run(st: &mut SystemTable<Boot>) -> ! {
    {
        let stdout = st.stdout();
        let _ = stdout.reset(false);
        let _ = stdout.enable_cursor(true);
        let _ = stdout.clear();
        kprintln!(st, "{COMP} {NAME} {VERSION} tty0");
        kprintln!(st, "Run 'help' to get started!");
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
        kprintln!(st, "Programs: {}", list_programs());
    }

    fn cmd_run(st: &mut SystemTable<Boot>, args: &str) {
        let name = args.trim();
        if name.is_empty() {
            kprintln!(st, "Usage: run <name>");
            return;
        }
        if let Some(p) = find_program(name) {
            kprintln!(st, "Launching '{}'...", p.name);
            (p.run)(st);
            kprintln!(st, "Program '{}' exited.", p.name);
        } else {
            kprintln!(st, "No such program: {}", name);
        }
    }

    fn cmd_ls(st: &mut SystemTable<Boot>, _args: &str) {
        let mut entries: Vec<heapless::String<64>, 128> = Vec::new();
        nori::list_root(st, |name| {
            let mut s = heapless::String::<64>::new();
            let _ = core::fmt::Write::write_fmt(&mut s, format_args!("{}", name));
            let _ = entries.push(s);
        });
        for s in entries.iter() {
            kprintln!(st, "{}", s);
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
        kprintln!(st, "Filesystems found: {}", count);
    }

    fn cmd_pwd(st: &mut SystemTable<Boot>, _args: &str) {
        kprintln!(st, "/");
    }

    fn cmd_cat(st: &mut SystemTable<Boot>, args: &str) {
        let name = args.trim();
        if name.is_empty() {
            kprintln!(st, "Usage: cat <filename>");
            return;
        }

        let mut wbuf = [0u16; 260];
        let c16 = match uefi::CStr16::from_str_with_buf(name, &mut wbuf) {
            Ok(s) => s,
            Err(_) => {
                kprintln!(st, "Invalid filename");
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
                kprintln!(st, "");
            }
            CatOutcome::IsDir => {
                kprintln!(st, "{}: is a directory", name);
            }
            CatOutcome::NoFs => {
                kprintln!(st, "No filesystem available");
            }
            CatOutcome::OpenVolumeErr => {
                kprintln!(st, "Failed to open root volume");
            }
            CatOutcome::OpenErr => {
                kprintln!(st, "Cannot open: {}", name);
            }
            CatOutcome::ReadErr => {
                kprintln!(st, "Read error");
            }
            CatOutcome::StatErr => {
                kprintln!(st, "Failed to stat file");
            }
        }
    }

    fn x_debug_panic(_st: &mut SystemTable<Boot>, _args: &str) {
        panic!("Test panic");
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
        CommandEntry {
            name: "x:debug-panic",
            help: "For debugging: test panics",
            run: x_debug_panic,
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
            kprintln!(st, "Commands:");
            for c in COMMANDS {
                kprintln!(st, "  {:<12} {}", c.name, c.help);
            }
            continue;
        }

        match COMMANDS.iter().find(|c| c.name == cmd_name) {
            Some(c) => (c.run)(st, args),
            None => kprintln!(st, "Unknown: {} (try 'help')", cmd_name),
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
    let _ = st.stdout().enable_cursor(true);
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
                            kprintln!(st, "");
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
                let _ = st.boot_services().stall(1000);
            }
            Err(_) => {
                let _ = st.boot_services().stall(2000);
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
    let _ = st.stdout().enable_cursor(true);
    loop {
        let read_result = { st.stdin().read_key() };
        match read_result {
            Ok(Some(key)) => match key {
                Key::Printable(c16) => {
                    let c: char = c16.into();
                    match c {
                        '\r' | '\n' => {
                            kprintln!(st, "");
                            return;
                        }
                        '\u{8}' => {
                            if !buf.is_empty() {
                                buf.pop();
                                let _ = write!(st.stdout(), "\u{8} \u{8}");
                            }
                        }
                        '\t' => {
                            autocomplete_line(st, buf, cwd);
                        }
                        _ => {
                            if buf.push(c).is_ok() {
                                let _ = write!(st.stdout(), "{}", c);
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
                        for _ in 0..buf.len() {
                            let _ = write!(st.stdout(), "\u{8} \u{8}");
                        }
                        buf.clear();
                        let _ = buf.push_str(s);
                        let _ = write!(st.stdout(), "{}", s);
                    }
                    ScanCode::DOWN => {
                        if history.is_empty() {
                            continue;
                        }
                        match *hist_nav {
                            None => {}
                            Some(0) => {
                                *hist_nav = None;
                                for _ in 0..buf.len() {
                                    let _ = write!(st.stdout(), "\u{8} \u{8}");
                                }
                                buf.clear();
                            }
                            Some(i) => {
                                let ni = i - 1;
                                *hist_nav = Some(ni);
                                let s = &history[history.len() - 1 - ni];
                                for _ in 0..buf.len() {
                                    let _ = write!(st.stdout(), "\u{8} \u{8}");
                                }
                                buf.clear();
                                let _ = buf.push_str(s);
                                let _ = write!(st.stdout(), "{}", s);
                            }
                        }
                    }
                    ScanCode::LEFT | ScanCode::RIGHT => {}
                    _ => {}
                },
            },
            Ok(None) => {
                let _ = st.boot_services().stall(1000);
            }
            Err(_) => {
                let _ = st.boot_services().stall(2000);
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

    kprintln!(st, "");
    for (i, m) in matches.iter().enumerate() {
        if i > 0 {
            let _ = write!(st.stdout(), " ");
        }
        let _ = write!(st.stdout(), "{}", m);
    }
    kprintln!(st, "");
    let _ = write!(st.stdout(), "{}{}", cwd, PREFIX);
    let _ = write!(st.stdout(), "{}", buf.as_str());
}

fn echo_program(st: &mut SystemTable<Boot>) {
    let out = st.stdout();
    let _ = out.clear();
    kprintln!(st, "Echo program. Type 'exit' to return.");
    let mut line = heapless::String::<256>::new();
    loop {
        let _ = write!(st.stdout(), "echo {} ", PREFIX);
        line.clear();
        read_line_simple(st, &mut line);
        let s = line.trim();
        if s == "exit" {
            break;
        }
        kprintln!(st, "{}", s);
    }
}

fn keys_program(st: &mut SystemTable<Boot>) {
    let out = st.stdout();
    let _ = out.clear();
    kprintln!(st, "Keys demo. Press ESC to return.");

    {
        let stdin = st.stdin();
        let _ = stdin.reset(false);
    }
    loop {
        let read_result = { st.stdin().read_key() };
        match read_result {
            Ok(Some(Key::Printable(c16))) => {
                let c: char = c16.into();
                kprintln!(st, "Printable: {:?}", c);
                if c == '\u{1b}' {
                    break;
                }
            }
            Ok(Some(Key::Special(sc))) => {
                kprintln!(st, "Special: {:?}", sc);
                if sc == ScanCode::ESCAPE {
                    break;
                }
            }
            Ok(None) => {
                let _ = st.boot_services().stall(1_000);
            }
            Err(_) => {
                let _ = st.boot_services().stall(2_000);
            }
        }
    }
}

fn glow_program(st: &mut SystemTable<Boot>) {
    let out = st.stdout();
    let _ = out.clear();
    kprintln!(st, "glow — neovim real no clickbait");
    kprintln!(st, "Type text. Commands: :q to quit.");
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
                    kprintln!(st, "Unknown command: :{}", cmd);
                }
            }
        } else {
            kprintln!(st, "{}", s);
        }
    }
}

fn zam_program(st: &mut SystemTable<Boot>) {
    let _ = st.stdout().clear();
    let _ = st.stdin().reset(false);
    let _ = st.stdout().enable_cursor(true);

    let mut screen_w = 0usize;
    let mut screen_h = 0usize;
    let mut cell_w = 8usize;
    let mut cell_h = 16usize;
    let _ = wasabi::with_gop(st.boot_services(), |gop| {
        screen_w = wasabi::width(gop);
        screen_h = wasabi::height(gop);
        cell_w = screen_w / 80usize;
        cell_h = screen_h / 25usize;
    });

    let mut win_w = (screen_w * 3) / 5;
    let mut win_h = (screen_h * 3) / 5;
    if win_w < cell_w * 20 {
        win_w = cell_w * 20;
    }
    if win_h < cell_h * 10 {
        win_h = cell_h * 10;
    }
    let mut win_x = screen_w / 6;
    let mut win_y = screen_h / 6;
    let title_h = cell_h;

    let mut line = heapless::String::<256>::new();
    let mut cur_row = 0usize;
    let mut mouse_x = (screen_w / 2) as i32;
    let mut mouse_y = (screen_h / 2) as i32;
    let mut last_px: Option<(usize, usize)> = None;
    let mut dragging = false;
    let mut prev_left = false;

    let mut pointer_handle: Option<uefi::Handle> = None;
    if let Ok(handles) =
        st.boot_services()
            .locate_handle_buffer(uefi::table::boot::SearchType::ByProtocol(
                &uefi::proto::console::pointer::Pointer::GUID,
            ))
    {
        if let Some(&ph) = handles.first() {
            pointer_handle = Some(ph);
        }
    }

    let mut redraw_window = true;
    loop {
        if redraw_window {
            let _ = wasabi::with_gop(st.boot_services(), |gop| {
                wasabi::fill_rect(gop, 0, 0, screen_w, screen_h, wasabi::to_color(48, 25, 52));
                wasabi::fill_rect(
                    gop,
                    win_x,
                    win_y,
                    win_w,
                    win_h,
                    wasabi::to_color(30, 20, 32),
                );
                wasabi::fill_rect(
                    gop,
                    win_x,
                    win_y,
                    win_w,
                    title_h,
                    wasabi::to_color(40, 30, 42),
                );
                wasabi::fill_rect(
                    gop,
                    win_x + 2,
                    win_y + title_h,
                    win_w - 4,
                    win_h - title_h,
                    wasabi::to_color(30, 20, 32),
                );
            });
            let term_col0 = win_x / cell_w + 1;
            let term_row0 = win_y / cell_h + 2;
            let _ = st
                .stdout()
                .set_cursor_position(term_col0 as usize, term_row0 as usize);
            kprintln!(st, "zam terminal");
            last_px = None;
            redraw_window = false;
        }

        if let Some(ph) = pointer_handle {
            unsafe {
                if let Ok(mut p) = st
                    .boot_services()
                    .open_protocol::<uefi::proto::console::pointer::Pointer>(
                        uefi::table::boot::OpenProtocolParams {
                            handle: ph,
                            agent: st.boot_services().image_handle(),
                            controller: None,
                        },
                        uefi::table::boot::OpenProtocolAttributes::Exclusive,
                    )
                {
                    if let Ok(Some(state)) = p.read_state() {
                        let dx = state.relative_movement[0] as i32;
                        let dy = state.relative_movement[1] as i32;
                        mouse_x = (mouse_x + dx).max(0).min(screen_w.saturating_sub(1) as i32);
                        mouse_y = (mouse_y + dy).max(0).min(screen_h.saturating_sub(1) as i32);
                        let left = state.button[0];
                        let px = (mouse_x as usize).min(screen_w.saturating_sub(1));
                        let py = (mouse_y as usize).min(screen_h.saturating_sub(1));

                        if left && !prev_left {
                            if py >= win_y
                                && py < win_y + title_h
                                && px >= win_x
                                && px < win_x + win_w
                            {
                                dragging = true;
                            }
                        }
                        if prev_left && !left {
                            dragging = false;
                        }
                        if dragging {
                            let mut nx_i = win_x as i32 + dx * 50;
                            let mut ny_i = win_y as i32 + dy * 50;

                            if nx_i < 0 {
                                nx_i = 0;
                            }
                            if ny_i < 0 {
                                ny_i = 0;
                            }

                            if (nx_i as usize) + win_w > screen_w {
                                nx_i = (screen_w - win_w) as i32;
                            }
                            if (ny_i as usize) + win_h > screen_h {
                                ny_i = (screen_h - win_h) as i32;
                            }

                            let nx = nx_i as usize;
                            let ny = ny_i as usize;

                            if nx != win_x || ny != win_y {
                                win_x = nx;
                                win_y = ny;
                                redraw_window = true;
                            }
                        }

                        if let Some((opx, opy)) = last_px {
                            let obg = if opx >= win_x
                                && opx < win_x + win_w
                                && opy >= win_y
                                && opy < win_y + win_h
                            {
                                if opy < win_y + title_h {
                                    wasabi::to_color(40, 30, 42)
                                } else if opx >= win_x + 2
                                    && opx < win_x + win_w - 2
                                    && opy >= win_y + title_h
                                {
                                    wasabi::to_color(30, 20, 32)
                                } else {
                                    wasabi::to_color(50, 40, 52)
                                }
                            } else {
                                wasabi::to_color(48, 25, 52)
                            };
                            let _ = wasabi::with_gop(st.boot_services(), |gop| {
                                wasabi::fill_rect(gop, opx, opy, 5, 5, obg);
                            });
                        }
                        let _ = wasabi::with_gop(st.boot_services(), |gop| {
                            wasabi::fill_rect(gop, px, py, 5, 5, wasabi::to_color(255, 255, 255));
                        });
                        last_px = Some((px, py));
                        prev_left = left;
                    }
                }
            }
        }

        let read_result = { st.stdin().read_key() };
        match read_result {
            Ok(Some(Key::Printable(c16))) => {
                let c: char = c16.into();
                match c {
                    '\u{1b}' => break,
                    '\r' | '\n' => {
                        cur_row += 1;
                        let max_rows = (win_h - title_h - 4) / cell_h;
                        if cur_row >= max_rows {
                            cur_row = max_rows.saturating_sub(1);
                        }
                        let _ = st
                            .stdout()
                            .set_cursor_position(win_x / cell_w + 1, win_y / cell_h + 2 + cur_row);
                        kprintln!(st, "");
                        line.clear();
                    }
                    '\u{8}' => {
                        if !line.is_empty() {
                            line.pop();
                            let _ = st.stdout().set_cursor_position(
                                win_x / cell_w + 1,
                                win_y / cell_h + 2 + cur_row,
                            );
                            let mut s = heapless::String::<256>::new();
                            let _ = s.push_str(line.as_str());
                            let mut rem = ((win_w - 4) / cell_w).saturating_sub(s.len());
                            while rem > 0 {
                                let _ = s.push(' ');
                                rem -= 1;
                            }
                            let _ = write!(st.stdout(), "{}", s.as_str());
                            let _ = st.stdout().set_cursor_position(
                                win_x / cell_w + 1 + line.len(),
                                win_y / cell_h + 2 + cur_row,
                            );
                        }
                    }
                    _ => {
                        if line.len() + 1 >= ((win_w - 4) / cell_w) {
                            continue;
                        }
                        if line.push(c).is_ok() {
                            let _ = write!(st.stdout(), "{}", c);
                        }
                    }
                }
            }
            Ok(Some(Key::Special(sc))) => {
                if sc == ScanCode::ESCAPE {
                    break;
                }
            }
            Ok(None) => {
                let _ = st.boot_services().stall(500);
            }
            Err(_) => {
                let _ = st.boot_services().stall(1_000);
            }
        }
    }
}
