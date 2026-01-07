#![no_std]
#![no_main]

use core::usize;
extern crate alloc;
use user_lib::{String, getchar, print, println};
use crate::alloc::string::ToString;
extern crate user_lib;


mod ui {
    use user_lib::{print, println};

    pub fn banner() {
        println!("BlueStarOS---------------------------------------------");
        println!("CopyRight -> Dirinkbottle 2025");
        println!("");
    }

    pub fn prompt() {
        print!("BlueStarOS> ");
    }
}

mod console {
    use alloc::{string::ToString, vec::Vec};
    use user_lib::{String, getchar, print};

    static mut COMMAND_BUFFER: Option<Vec<String>> = None;
    static mut HISTORY_CURSOR: usize = 0;
    static mut TAB_TRIGGERED: bool = false;

    fn ensure_history() {
        unsafe {
            if COMMAND_BUFFER.is_none() {
                COMMAND_BUFFER = Some(Vec::new());
                HISTORY_CURSOR = 0;
            }
        }
    }

    pub fn history_push(cmd: String) {
        ensure_history();
        let cmd = cmd.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        unsafe {
            if let Some(buf) = COMMAND_BUFFER.as_mut() {
                buf.push(cmd);
                HISTORY_CURSOR = buf.len();
            }
        }
    }

    fn history_set_cursor_to_end() {
        ensure_history();
        unsafe {
            if let Some(buf) = COMMAND_BUFFER.as_ref() {
                HISTORY_CURSOR = buf.len();
            }
        }
    }

    fn history_select_prev() -> Option<String> {
        ensure_history();
        unsafe {
            let buf = COMMAND_BUFFER.as_ref()?;
            if buf.is_empty() {
                return None;
            }
            if HISTORY_CURSOR == 0 {
                return Some(buf[0].clone());
            }
            HISTORY_CURSOR -= 1;
            Some(buf[HISTORY_CURSOR].clone())
        }
    }

    fn history_select_next() -> Option<String> {
        ensure_history();
        unsafe {
            let buf = COMMAND_BUFFER.as_ref()?;
            if buf.is_empty() {
                return None;
            }
            if HISTORY_CURSOR >= buf.len() {
                return Some(String::new());
            }
            HISTORY_CURSOR += 1;
            if HISTORY_CURSOR >= buf.len() {
                Some(String::new())
            } else {
                Some(buf[HISTORY_CURSOR].clone())
            }
        }
    }

    fn is_eol(c: u8) -> bool {
        c == b'\r' || c == b'\n'
    }

    fn is_backspace(c: u8) -> bool {
        c == 8u8 || c == 127u8
    }

    fn is_tab(c: u8) -> bool {
        c == b'\t'
    }

    pub fn take_tab_triggered() -> bool {
        unsafe {
            let v = TAB_TRIGGERED;
            TAB_TRIGGERED = false;
            v
        }
    }

    fn clear_current_input(len: usize) {
        for _ in 0..len {
            print!("\x08 \x08");
        }
    }

    fn try_read_arrow(c: u8) -> Option<u8> {
        if c != 0x1b {
            return None;
        }
        let c1 = getchar() as u8;
        if c1 != b'[' {
            return None;
        }
        let c2 = getchar() as u8;
        Some(c2)
    }

    pub fn read_line() -> String {
        history_set_cursor_to_end();
        let mut line = String::new();
        loop {
            let c = getchar() as u8;
            if is_tab(c) {
                unsafe {
                    TAB_TRIGGERED = true;
                }
                break;
            }
            if let Some(code) = try_read_arrow(c) {
                if code == b'A' {
                    if let Some(sel) = history_select_prev() {
                        clear_current_input(line.len());
                        line = sel;
                        print!("{}", line);
                    }
                    continue;
                }
                if code == b'B' {
                    if let Some(sel) = history_select_next() {
                        clear_current_input(line.len());
                        line = sel;
                        print!("{}", line);
                    }
                    continue;
                }
            }
            if is_eol(c) {
                print!("\n");
                break;
            }

            if is_backspace(c) {
                if !line.is_empty() {
                    line.pop();
                    print!("\x08 \x08");
                }
                continue;
            }

            if c.is_ascii_graphic() || c == b' ' {
                line.push(c as char);
                print!("{}", c as char);
            }
        }
        line
    }
}

mod command {
    use user_lib::{String, println, print, sys_exec_args, sys_exit, sys_fork, sys_wait, chdir, getcwd};
    use alloc::vec::Vec;
    fn clear_screen() {
        // ANSI: clear screen + move cursor to home
        print!("\x1b[2J\x1b[H");
    }

    fn help() {
        println!("Built-in commands:");
        println!("  help        Show this help");
        println!("  clear       Clear the screen");
        println!("  echo <msg>  Print <msg>");
        println!("  cd <path>   Change current directory (init built-in)");
        println!("  pwd         Print current directory");
        println!("  ls          Run /test/ls");
        println!("  mkdir       Run /test/mkdir");
        println!("  rm          Run /test/rm");
        println!("  cat         Run /test/cat");
        println!("  exit        Exit init");
    }

    fn run_bin(path: &str, argv0: &str, args: &[&str]) {
        let mut path = String::from(path);
        path.push('\0');

        let mut arg_strings: Vec<String> = Vec::new();
        arg_strings.push(String::from(argv0));
        for a in args.iter() {
            arg_strings.push(String::from(*a));
        }
        for s in arg_strings.iter_mut() {
            s.push('\0');
        }
        let mut argv_ptrs: Vec<usize> = Vec::new();
        for s in arg_strings.iter() {
            argv_ptrs.push(s.as_ptr() as usize);
        }
        argv_ptrs.push(0);

        let pid = sys_fork();
        if pid == 0 {
            let ret = sys_exec_args(&path, argv_ptrs.as_ptr());
            println!("exec failed, ret={};", ret);
            sys_exit(1);
        }
        if pid < 0 {
            println!("fork failed, ret={}", pid);
            return;
        }
        let mut code: isize = 0;
        let waited = sys_wait(&mut code as *mut isize);
        if waited < 0 {
            println!("wait failed, ret={}", waited);
        }
    }

    fn run_test_bin(name: &str, args: &[&str]) {
        let mut path = String::from("/test/");
        path.push_str(name);
        run_bin(&path, name, args);
    }

    pub fn handle_line(line: String) {
        let line = line;
        if line.is_empty() {
            return;
        }

        let mut parts = line.split_whitespace();
        let cmd = match parts.next() {
            Some(c) => c,
            None => return,
        };
        let rest: Vec<&str> = parts.collect();

        if cmd == "ls" {
            run_test_bin("ls", &rest);
            return;
        }
        if cmd == "mkdir" {
            run_test_bin("mkdir", &rest);
            return;
        }
        if cmd == "rm" {
            run_test_bin("rm", &rest);
            return;
        }
        if cmd == "cat" {
            run_test_bin("cat", &rest);
            return;
        }

        if cmd == "help" {
            help();
            return;
        }

        if cmd == "clear" {
            clear_screen();
            return;
        }

        if cmd == "echo" {
            for (i, s) in rest.iter().enumerate() {
                if i != 0 {
                    print!(" ");
                }
                print!("{}", s);
            }
            return;
        }

        if cmd == "cd" {
            if rest.len() != 1 {
                println!("usage: cd <path>");
                return;
            }
            let ret = chdir(rest[0]);
            if ret < 0 {
                println!("cd failed, ret={}", ret);
            }
            return;
        }

        if cmd == "pwd" {
            match getcwd() {
                Some(s) => println!("{}", s),
                None => println!("pwd failed"),
            }
            return;
        }

        if cmd == "exit" {
            sys_exit(0);
            panic!("_start UnReachBle!");
        }

        if let Some(prog) = cmd.strip_prefix("./") {
            if prog.is_empty() {
                println!("Invalid command: {}", line);
                return;
            }
            let cwd = match getcwd() {
                Some(s) => s,
                None => {
                    println!("getcwd failed");
                    return;
                }
            };
            let mut path = cwd;
            if !path.ends_with('/') {
                path.push('/');
            }
            path.push_str(prog);
            run_bin(&path, prog, &rest);
            return;
        }

        run_test_bin(cmd, &rest);
    }

    pub fn handle_tab(_line: String) {
        //ls获取当前目录获取所有条目然后分割成vec
        //分割当前已经输入的命令，以最后一个为准，使用withprefix匹配条目并且补全。如果有多个匹配就选取最后一个
    }
}


#[no_mangle]
pub fn main(){
    ui::banner();
    loop {
        ui::prompt();
        let line = console::read_line();
        if console::take_tab_triggered() {
            command::handle_tab(line);
            continue;
        }
        console::history_push(line.clone());
        command::handle_line(line);
    }
}
