use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

pub struct ResultFuture {
    pub result: Arc<FuncResult>,
}
pub struct FuncResult {
    completed: Mutex<bool>,
    result: Mutex<Option<String>>,
    waker: Mutex<Option<Waker>>,
}

unsafe impl Send for FuncResult {}
unsafe impl Sync for FuncResult {}

impl FuncResult {
    pub fn new() -> FuncResult {
        FuncResult {
            completed: false.into(),
            result: Mutex::new(None),
            waker: Mutex::new(None),
        }
    }

    pub fn set_completed(&self) {
        if let Ok(mut completed) = self.completed.lock() {
            *completed = true;
            if let Ok(mut waker) = self.waker.lock() {
                if let Some(waker) = waker.take() {
                    waker.wake();
                }
            }
        };
    }

    pub fn set_result(&self, str: &str) {
        if let Ok(mut result) = self.result.lock() {
            result.replace(str.to_owned());
        }
    }
}

impl Future for ResultFuture {
    type Output = String;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if *self.result.completed.lock().unwrap() {
            Poll::Ready(self.result.result.lock().unwrap().clone().unwrap())
        } else {
            if let Ok(mut waker) = self.result.waker.lock() {
                waker.replace(cx.waker().clone());
            };
            Poll::Pending
        }
    }
}
