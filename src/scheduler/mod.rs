use crate::scheduler::worker::get_worker;

pub mod worker;

pub(crate) fn signal_handler() {
    // println!("signal_handler");
    let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigfillset(&mut mask);
        libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
    }
    // println!("get local queue in signal handler");

    let worker = unsafe { get_worker().as_mut() };
    // println!("suspend and resume");
    worker.suspend();
    worker.get_task();

    worker.set_curr();

    // println!("########### end of signal handler ##########");
    unsafe {
        libc::sigemptyset(&mut mask);
        libc::sigprocmask(libc::SIG_UNBLOCK, &mask, std::ptr::null_mut());
    }
}
