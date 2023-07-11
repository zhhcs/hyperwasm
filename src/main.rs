pub mod runtime;
pub mod scheduler;
pub mod task;

use std::sync::Arc;

use runtime::Runtime;
pub use task::stack::StackSize;

fn main() {
    let rt = Arc::new(Runtime::new());
    for i in 1..30 {
        rt.spawn(Box::new(move || {
            let i = i.clone();
            println!("Task {} start", i);
            let mut num = 35;
            for index in 0..i % 5 {
                num += index;
                let res = fib(num);
                if res > fib(35) {
                    num -= 2;
                    println!("this is task {}, res = {}", i, res % fib(i));
                }
            }
            println!("Task {} end", i);
        }))
        .unwrap();
        println!("Task {} spawned", i);
    }
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
