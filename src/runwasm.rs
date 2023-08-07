use crate::{runtime::Runtime, task::SchedulerStatus};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap, fmt, sync::Arc};
use wasmtime::{Engine, Instance, Linker, Module, Store};
use wasmtime_wasi::{sync::WasiCtxBuilder, WasiCtx};

thread_local! {
    static NAME_ID: RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    task_unique_name: String,
    path: String,
    expected_execution_time: u64,
    relative_deadline: u64,
    wasm_name: String,
    func: Option<String>,
    param: Option<i32>,
    // TODO: params & results
}

impl Config {
    pub fn new(
        task_unique_name: &str,
        path: &str,
        expected_execution_time: u64,
        relative_deadline: u64,
        wasm_name: &str,
        func: Option<String>,
        param: Option<i32>,
    ) -> Config {
        Config {
            task_unique_name: task_unique_name.to_string(),
            path: path.to_string(),
            expected_execution_time,
            relative_deadline,
            wasm_name: wasm_name.to_string(),
            func,
            param,
        }
    }

    pub fn get_wasm_name(&self) -> &str {
        &self.wasm_name
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }
}

#[derive(Clone)]
pub struct Environment {
    wasm_name: String,
    engine: Engine,
    module: Module,
    linker: Arc<Linker<WasiCtx>>,
}

impl Environment {
    pub fn new(config: &Config) -> Result<Self, Error> {
        let wasm_name = config.get_wasm_name().to_owned();
        let engine = Engine::default();
        let module = Module::from_file(&engine, config.get_path())?;

        let mut linker = Linker::new(&engine);
        wasmtime_wasi::add_to_linker(&mut linker, |cx| cx)?;

        Ok(Self {
            wasm_name,
            engine,
            module,
            linker: Arc::new(linker),
        })
    }

    pub fn get_wasm_name(&self) -> &str {
        &self.wasm_name
    }
}

pub fn call(rt: &Runtime, env: Environment, config: Config) -> Result<(), Error> {
    if config.relative_deadline < config.expected_execution_time {
        return Err(wasmtime::Error::msg("Invalid deadline").context("Invalid deadline"));
    }
    if NAME_ID.with(|map| map.borrow().contains_key(config.task_unique_name.as_str())) {
        return Err(wasmtime::Error::msg("Invalid unique name").context("Invalid unique name"));
    }
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()?
        .build();

    let mut store = Store::new(&env.engine, wasi);

    // Instantiate into our own unique store using the shared linker, afterwards
    // acquiring the `_start` function for the module and executing it.
    let instance = env.linker.instantiate(&mut store, &env.module)?;

    let mut expected_execution_time = None;
    let mut relative_deadline = None;
    if config.expected_execution_time != 0 {
        expected_execution_time = Some(std::time::Duration::from_millis(
            config.expected_execution_time,
        ));
        relative_deadline = Some(std::time::Duration::from_millis(config.relative_deadline));
    }

    let mut name = String::new();
    match config.func {
        Some(func) => name.push_str(&func),
        None => name.push_str("_start"),
    }
    let task_unique_name = config.task_unique_name.clone();
    match config.param {
        Some(param) => {
            let caller = instance.get_typed_func::<i32, i32>(&mut store, &name)?;
            let func = move || {
                if let Ok(res) = caller.call(&mut store, param) {
                    tracing::info!("{}, res = {}", task_unique_name, res);
                } else {
                    tracing::warn!("run wasm error");
                };
            };
            let id = rt.spawn(func, expected_execution_time, relative_deadline);
            match id {
                Ok(id) => {
                    NAME_ID.with(|map| map.borrow_mut().insert(config.task_unique_name, id));
                    Ok(())
                }
                Err(err) => Err(err.into()),
            }
        }
        None => {
            let caller = instance.get_typed_func::<(), i32>(&mut store, &name)?;
            let func = move || {
                if let Ok(res) = caller.call(&mut store, ()) {
                    tracing::info!("{}, res = {}", task_unique_name, res);
                } else {
                    tracing::warn!("run wasm error");
                };
            };
            let id = rt.spawn(func, expected_execution_time, relative_deadline);
            match id {
                Ok(id) => {
                    NAME_ID.with(|map| map.borrow_mut().insert(config.task_unique_name, id));
                    Ok(())
                }
                Err(err) => Err(err.into()),
            }
        }
    }
}

#[deprecated]
pub fn run_wasm(rt: &Runtime, config: Config) -> wasmtime::Result<()> {
    if config.relative_deadline < config.expected_execution_time {
        return Err(wasmtime::Error::msg("Invalid deadline").context("Invalid deadline"));
    }
    if NAME_ID.with(|map| map.borrow().contains_key(config.task_unique_name.as_str())) {
        return Err(wasmtime::Error::msg("Invalid unique name").context("Invalid unique name"));
    }
    let mut store = Store::<()>::default();
    let module = Module::from_file(store.engine(), config.path)?;
    let instance = Instance::new(&mut store, &module, &[])?;

    let name = instance.get_typed_func::<i32, i32>(&mut store, &config.wasm_name)?;

    let func = move || {
        if let Ok(_) = name.call(&mut store, 10000000) {
        } else {
            tracing::warn!("run wasm error");
        }
    };

    let mut expected_execution_time = None;
    let mut relative_deadline = None;
    if config.expected_execution_time != 0 {
        expected_execution_time = Some(std::time::Duration::from_millis(
            config.expected_execution_time,
        ));
        relative_deadline = Some(std::time::Duration::from_millis(config.relative_deadline));
    }

    let id = rt.spawn(func, expected_execution_time, relative_deadline);
    match id {
        Ok(id) => {
            NAME_ID.with(|map| map.borrow_mut().insert(config.task_unique_name, id));
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

pub fn get_status_by_name(rt: &Runtime, unique_name: &str) -> Option<SchedulerStatus> {
    if let Some(id) = NAME_ID.with(|map| map.borrow().get(unique_name).cloned()) {
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
