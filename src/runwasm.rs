use crate::{axum::server::CallConfigRequest, runtime::Runtime, task::SchedulerStatus};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap, fmt, sync::Arc};
use wasmtime::{Engine, Instance, Linker, Module, Store};
use wasmtime_wasi::{sync::WasiCtxBuilder, WasiCtx};

thread_local! {
    static NAME_ID: RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Serialize, Deserialize, Debug)]
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
    config: Config,
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
            config: config.clone(),
            engine,
            module,
            linker: Arc::new(linker),
        })
    }

    pub fn get_wasm_name(&self) -> &str {
        &self.wasm_name
    }
}

#[derive(Clone, Debug)]
pub struct FuncConfig {
    task_unique_name: String,   //实例名称,必须唯一
    export_func: String,        //调用的导出函数名称
    params: Vec<wasmtime::Val>, //数组
    results: Vec<wasmtime::Val>,
    expected_execution_time: u64, //预期执行时长(必须小于相对截止时间，单位毫秒)
    relative_deadline: u64,       //相对截止时间(单位毫秒)
}

impl FuncConfig {
    pub fn new(call_config: CallConfigRequest) -> FuncConfig {
        let params = cvt_params(call_config.param_type, call_config.params);
        let results_len = call_config.results_length.parse().unwrap();
        let results = vec![params[0].clone(); results_len];
        // tracing::info!("params: {:?}", params);
        FuncConfig {
            task_unique_name: "anon".to_owned(), //call_config.task_unique_name,
            export_func: call_config.export_func,
            params,
            results,
            expected_execution_time: 0, //call_config.expected_execution_time.parse::<u64>().unwrap(),
            relative_deadline: 0,       //call_config.relative_deadline.parse::<u64>().unwrap(),
        }
    }
}

fn cvt_params(param_type: String, params: Vec<String>) -> Vec<wasmtime::Val> {
    let mut res = Vec::new();
    if param_type == "i32" {
        params.iter().for_each(|param| {
            let val = wasmtime::Val::from(param.parse::<i32>().unwrap());
            res.push(val);
        })
    } else if param_type == "i64" {
        params.iter().for_each(|param| {
            let val = wasmtime::Val::from(param.parse::<i64>().unwrap());
            res.push(val);
        })
    } else if param_type == "f32" {
        params.iter().for_each(|param| {
            let val = wasmtime::Val::from(param.parse::<f32>().unwrap());
            res.push(val);
        })
    } else if param_type == "f64" {
        params.iter().for_each(|param| {
            let val = wasmtime::Val::from(param.parse::<f64>().unwrap());
            res.push(val);
        })
    } else if param_type == "u128" {
        params.iter().for_each(|param| {
            let val = wasmtime::Val::from(param.parse::<u128>().unwrap());
            res.push(val);
        })
    }
    res
}

pub fn call_func_sync(env: Environment, mut conf: FuncConfig) -> Result<Vec<wasmtime::Val>, Error> {
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()?
        .build();

    let mut store = Store::new(&env.engine, wasi);
    let instance = env.linker.instantiate(&mut store, &env.module)?;

    if let Some(caller) = instance.get_func(&mut store, &conf.export_func) {
        match caller.call(&mut store, &conf.params, &mut conf.results) {
            Ok(_) => {
                return Ok(conf.results);
            }
            Err(err) => return Err(err),
        };
    } else {
        return Err(wasmtime::Error::msg("Invalid_export_func").context("Invalid_export_func"));
    }
}

pub fn call_func(
    rt: &Runtime,
    env: Environment,
    mut conf: FuncConfig,
) -> Result<(u64, String), Error> {
    if conf.relative_deadline <= conf.expected_execution_time {
        return Err(wasmtime::Error::msg("Invalid_deadline").context("Invalid_deadline"));
    }
    if NAME_ID.with(|map| map.borrow().contains_key(conf.task_unique_name.as_str())) {
        return Err(wasmtime::Error::msg("Invalid_unique_name").context("Invalid_unique_name"));
    }
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()?
        .build();

    let mut store = Store::new(&env.engine, wasi);
    let instance = env.linker.instantiate(&mut store, &env.module)?;

    let expected_execution_time = Some(std::time::Duration::from_millis(
        conf.expected_execution_time,
    ));
    let relative_deadline = Some(std::time::Duration::from_millis(conf.relative_deadline));
    let task_unique_name = conf.task_unique_name.clone();
    if let Some(caller) = instance.get_func(&mut store, &conf.export_func) {
        let func = move || {
            if let Ok(_) = caller.call(&mut store, &conf.params, &mut conf.results) {
                tracing::info!("{}: results = {:?}", task_unique_name, conf.results);
            } else {
                tracing::warn!("run_wasm_error");
            };
        };
        let id = rt.spawn(func, expected_execution_time, relative_deadline);
        match id {
            Ok(id) => {
                NAME_ID.with(|map| map.borrow_mut().insert(conf.task_unique_name.clone(), id));
                return Ok((id, conf.task_unique_name));
            }
            Err(err) => return Err(err.into()),
        }
    } else {
        return Err(wasmtime::Error::msg("Invalid_export_func").context("Invalid_export_func"));
    }
}

pub fn call(
    rt: &Runtime,
    env: Environment,
    config: Option<Config>,
) -> Result<(u64, String), Error> {
    let conf = match config {
        Some(config) => config,
        None => {
            let mut env_config = env.config.clone();
            env_config.task_unique_name = env_config.task_unique_name
                + "-"
                + &rand::Rng::sample_iter(rand::thread_rng(), &rand::distributions::Alphanumeric)
                    .take(7)
                    .map(char::from)
                    .collect::<String>();

            env_config
        }
    };

    if conf.relative_deadline < conf.expected_execution_time {
        return Err(wasmtime::Error::msg("Invalid deadline").context("Invalid deadline"));
    }
    if NAME_ID.with(|map| map.borrow().contains_key(conf.task_unique_name.as_str())) {
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
    if conf.expected_execution_time != 0 {
        expected_execution_time = Some(std::time::Duration::from_millis(
            conf.expected_execution_time,
        ));
        relative_deadline = Some(std::time::Duration::from_millis(conf.relative_deadline));
    }

    let mut name = String::new();
    let task_unique_name = conf.task_unique_name.clone();
    match conf.func {
        Some(func) => name.push_str(&func),
        None => {
            name.push_str("_start");
            let caller = instance.get_typed_func::<(), ()>(&mut store, &name)?;
            let func = move || {
                if let Ok(_) = caller.call(&mut store, ()) {
                    tracing::info!("{} end", task_unique_name);
                } else {
                    tracing::warn!("run wasm error");
                };
            };
            let id = rt.spawn(func, expected_execution_time, relative_deadline);
            match id {
                Ok(id) => {
                    NAME_ID.with(|map| map.borrow_mut().insert(conf.task_unique_name.clone(), id));
                    return Ok((id, conf.task_unique_name));
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    match conf.param {
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
                    NAME_ID.with(|map| map.borrow_mut().insert(conf.task_unique_name.clone(), id));
                    Ok((id, conf.task_unique_name))
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
                    NAME_ID.with(|map| map.borrow_mut().insert(conf.task_unique_name.clone(), id));
                    Ok((id, conf.task_unique_name))
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
