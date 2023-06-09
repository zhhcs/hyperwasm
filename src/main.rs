mod task;
use std::{mem::MaybeUninit, ptr::null_mut, sync::Arc};
use task::CoSchedule;
use tokio::task as tokio_task;
use wasmtime::Linker;
fn main() {

    // main_thread_setup();
}

pub fn _co_main_exit() {
    unsafe { libc::exit(0) };
}

fn signal_handler() {
    // TODO!
}

fn main_thread_setup() {
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
    unsafe { libc::sched_yield() };
    // unsafe { libc::setcontext() };
    unsafe { libc::malloc(10) };
}
