#![no_std]
#![no_main]

extern crate alloc;
extern crate user_lib;

use user_lib::{println,print, sys_wait, sys_yield};
use user_lib::syscall::sys_clone;
use user_lib::syscall::sys_exit;

const SIGCHLD: usize = 17;

#[inline]
fn wexitstatus(status: isize) -> usize {
    ((status as usize) >> 8) & 0xff
}

#[no_mangle]
pub fn main() -> usize {
    const N: usize = 32;

    println!("========== START pc_rel ==========");

    let mut pids: [isize; N] = [0; N];

    for i in 0..N {
        // clone(child_func, NULL, stack, 1024, SIGCHLD) in the test suite
        // Here we only require fork-like semantics: parent gets pid>0, child gets 0.
        let pid = sys_clone(SIGCHLD, 0, 0, 0, 0);
        if pid == 0 {
            // Child: yield a lot to stress scheduler and parent/child bookkeeping.
            for _ in 0..5000 {
                sys_yield();
            }
            sys_exit((i & 0xff) as usize);
        }
        if pid < 0 {
            println!("pc_rel: clone failed at i={} pid={}", i, pid);
            return 1;
        }
        pids[i] = pid;
    }

    let mut reaped = 0usize;
    let mut seen: [bool; N] = [false; N];

    while reaped < N {
        let mut st: isize = 0;
        let waited = sys_wait(&mut st as *mut isize);
        if waited < 0 {
            println!("pc_rel: wait returned {} before reaping all children (reaped={}/{})", waited, reaped, N);
            return 1;
        }

        // Validate pid belongs to our set.
        let mut idx_opt: Option<usize> = None;
        for i in 0..N {
            if pids[i] == waited {
                idx_opt = Some(i);
                break;
            }
        }
        if idx_opt.is_none() {
            println!("pc_rel: unexpected waited pid={} status={}", waited, st);
            return 1;
        }
        let idx = idx_opt.unwrap();
        if seen[idx] {
            println!("pc_rel: duplicate reap pid={} idx={}", waited, idx);
            return 1;
        }
        seen[idx] = true;

        let code = wexitstatus(st);
        if code != (idx & 0xff) {
            println!("pc_rel: bad exit code for pid={} idx={} expect={} got={} raw_status={}", waited, idx, idx & 0xff, code, st);
            return 1;
        }

        reaped += 1;
    }

    // After all children are reaped, wait should return -1 (ECHILD in Linux).
    let mut st: isize = 0;
    let waited = sys_wait(&mut st as *mut isize);
    if waited != -1 {
        println!("pc_rel: expected wait=-1 after reaping all children, got waited={} status={}", waited, st);
        return 1;
    }

    println!("pc_rel: PASS");
    println!("========== END pc_rel ==========");
    0
}
