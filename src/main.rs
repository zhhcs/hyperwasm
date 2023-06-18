#![feature(naked_functions)]
pub mod ctx;
mod task;
use may::coroutine;
use std::{mem::MaybeUninit, ptr::null_mut, sync::Arc};
use task::CoSchedule;
use tokio::task as tokio_task;
use wasmtime::Linker;
fn main() {
    let co_sche = CoSchedule::sched_init();
    main_thread_setup(&co_sche);
}

pub fn _co_main_exit() {
    unsafe { libc::exit(0) };
}

fn signal_handler() {
    // TODO!
}

fn main_thread_setup(co_sche: &CoSchedule) {
    let mut action: libc::sigaction = unsafe { MaybeUninit::zeroed().assume_init() };
    action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
    unsafe {
        libc::sigfillset(&mut action.sa_mask as *mut libc::sigset_t);
    }
    action.sa_sigaction = signal_handler as usize;
    unsafe {
        libc::sigaction(
            libc::SIGURG,
            &mut action as *mut libc::sigaction,
            null_mut(),
        )
    };

    unsafe {
        libc::setcontext(&co_sche.task_main.lock().unwrap().co_ctx as *const libc::ucontext_t)
    };
}

fn co_run<F: FnOnce()>(mut co_sche: &CoSchedule, f: F) {
    // unsafe {
    //     libc::makecontext(
    //         &mut co_sche.task_main.lock().unwrap().co_ctx,
    //         f as extern "C" fn(),
    //         1,
    //     )
    // };
}
