pub mod runtime;
pub mod scheduler;
pub mod task;

use std::thread;

use runtime::Runtime;
pub use task::stack::StackSize;

// use crate::scheduler::worker::get_worker;

fn main() {
    let rt = Runtime::new();
    let rt1 = rt.clone();
    rt1.start();

    for i in 1..30 {
        rt.spawn(Box::new(move || {
            let i = i.clone();
            println!("Task {} start", i);
            let mut cnt = 36;
            for _ in 0..i % 5 {
                cnt += 1;
                if fib(cnt) > fib(35) {
                    cnt -= 2;
                    println!("this is task {}", i);
                }
            }
            println!("Task {} end", i);
        }));
        println!("Task {} spawned", i);
    }

    thread::sleep(std::time::Duration::from_secs(100));
    // rt.spawn(Box::new(move || {
    //     let i = 10;
    //     println!("Task Task {} start", i);
    //     // let w = unsafe { get_worker().as_mut() };
    //     // w.spawn(Box::new(|| {
    //     //     println!("Task spawn a coroutine");
    //     //     loop {}
    //     // }));
    //     // FIXME: 不支持生成子任务
    //     let mut cnt = 36;
    //     loop {
    //         cnt += 1;
    //         if fib(cnt) > fib(35) {
    //             cnt -= 2;
    //             println!("this this is task {}", i);
    //         }
    //     }
    // }));
}

fn fib(num: i32) -> i32 {
    if num == 0 {
        return 1;
    }
    if num == 1 {
        return 1;
    }
    fib(num - 1) + fib(num - 2)
}
