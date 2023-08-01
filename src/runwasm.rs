use std::{cell::RefCell, collections::HashMap, fmt};

use serde::{Deserialize, Serialize};
use wasmtime::{Instance, Module, Store};

use crate::{runtime::Runtime, task::SchedulerStatus};

thread_local! {
    static MAP : RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    unique_name: String,
    path: String,
    expected_execution_time: u64,
    relative_deadline: u64,
    name: String,
    // TODO: params & results
}

impl Config {
    pub fn new(
        unique_name: &str,
        path: &str,
        expected_execution_time: u64,
        relative_deadline: u64,
        name: &str,
    ) -> Config {
        Config {
            unique_name: unique_name.to_string(),
            path: path.to_string(),
            expected_execution_time,
            relative_deadline,
            name: name.to_string(),
        }
    }
}

pub fn run_wasm(rt: &Runtime, config: Config) -> wasmtime::Result<()> {
    if MAP.with(|map| map.borrow().contains_key(config.unique_name.as_str())) {
        return Err(wasmtime::Error::msg("need unique name").context("need unique name"));
    }
    let mut store = Store::<()>::default();
    let module = Module::from_file(store.engine(), config.path)?;
    let instance = Instance::new(&mut store, &module, &[])?;

    let name = instance.get_typed_func::<i32, i32>(&mut store, &config.name)?;

    let func = move || {
        if let Ok(_) = name.call(&mut store, 10000000) {
        } else {
            tracing::warn!("run wasm error");
        }
    };

    let mut expected_execution_time = None;
    let mut relative_deadline = None;
    if config.expected_execution_time != 0 && config.relative_deadline != 0 {
        expected_execution_time = Some(std::time::Duration::from_millis(
            config.expected_execution_time,
        ));
        relative_deadline = Some(std::time::Duration::from_millis(config.relative_deadline));
    }

    let id = rt.spawn(func, expected_execution_time, relative_deadline);
    match id {
        Ok(id) => {
            MAP.with(|map| map.borrow_mut().insert(config.unique_name, id));
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

pub fn get_status_by_name(rt: &Runtime, unique_name: &str) -> Option<SchedulerStatus> {
    if let Some(id) = MAP.with(|map| map.borrow().get(unique_name).cloned()) {
        if let Some(mut status) = rt.get_status_by_id(id) {
            if let Some(start) = status.curr_start_time {
                status.running_time += std::time::Instant::now() - start;
            }
            return Some(status);
        }
    }
    None
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "expected_execution_time: {:?}, relative_deadline: {:?}",
            self.expected_execution_time, self.relative_deadline
        )
    }
}
