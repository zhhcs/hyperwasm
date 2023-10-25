use crate::{
    axum::{CallConfigRequest, TestRequest},
    result::FuncResult,
    runtime::Runtime,
    task::SchedulerStatus,
};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::{sync::WasiCtxBuilder, WasiCtx};

thread_local! {
    static NAME_ID: RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
}

lazy_static::lazy_static! {
    static ref TEST_QUEUE: Arc<Mutex<VecDeque<Tester>>> = Arc::new(Mutex::new(VecDeque::new()));
}

pub fn set_test_env(tester: Tester) {
    if let Ok(queue) = TEST_QUEUE.lock().as_mut() {
        queue.push_back(tester);
    }
}

pub fn get_test_env() -> Option<Tester> {
    if let Ok(queue) = TEST_QUEUE.lock().as_mut() {
        queue.pop_front()
    } else {
        None
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RegisterConfig {
    path: String,
    wasm_name: String,
}

impl RegisterConfig {
    pub fn new(path: &str, wasm_name: &str) -> RegisterConfig {
        RegisterConfig {
            path: path.to_string(),
            wasm_name: wasm_name.to_string(),
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
pub struct Tester {
    pub env: Environment,
    pub result: Arc<FuncResult>,
}

#[derive(Clone)]
pub struct Environment {
    wasm_name: String,
    engine: Engine,
    module: Module,
    linker: Arc<Linker<WasiCtx>>,
    func_config: Option<FuncConfig>,
}

impl Environment {
    pub fn new(config: &RegisterConfig) -> Result<Self, Error> {
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
            func_config: None,
        })
    }

    pub fn set_func_config(&mut self, config: FuncConfig) {
        self.func_config = Some(config);
    }

    pub fn get_func_config(&self) -> Option<FuncConfig> {
        self.func_config.clone()
    }

    pub fn get_wasm_name(&self) -> &str {
        &self.wasm_name
    }

    pub fn set_test_time(&mut self, test_time: u64) {
        if let Some(func_config) = &mut self.func_config {
            func_config.set_expected_execution_time(test_time);
        }
    }

    pub fn get_test_time(&self) -> u64 {
        if let Some(func_config) = &self.func_config {
            func_config.get_expected_execution_time()
        } else {
            0
        }
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
    pub fn new(call_config: CallConfigRequest) -> Result<FuncConfig, Error> {
        match cvt_params(call_config.param_type, call_config.params) {
            Ok(params) => {
                let results_len = call_config.results_length.parse().unwrap_or(0);
                let results;
                if params.len() == 0 {
                    results = vec![wasmtime::Val::from(0); results_len];
                } else {
                    results = vec![params[0].clone(); results_len];
                }
                // tracing::info!("params: {:?}", params);
                let fc = FuncConfig {
                    task_unique_name: call_config.task_unique_name,
                    export_func: call_config.export_func,
                    params,
                    results,
                    expected_execution_time: call_config
                        .expected_execution_time
                        .parse::<u64>()
                        .unwrap_or(0),
                    relative_deadline: call_config.expected_deadline.parse::<u64>().unwrap_or(0),
                };
                Ok(fc)
            }
            Err(err) => Err(err),
        }
    }

    pub fn from(test_config: TestRequest) -> Result<FuncConfig, Error> {
        match cvt_params(test_config.param_type, test_config.params) {
            Ok(params) => {
                let results_len = test_config.results_length.parse().unwrap_or(0);
                let results;
                if params.len() == 0 {
                    results = vec![wasmtime::Val::from(0); results_len];
                } else {
                    results = vec![params[0].clone(); results_len];
                }
                // tracing::info!("params: {:?}", params);
                let fc = FuncConfig {
                    task_unique_name: "anon".to_owned(),
                    export_func: test_config.export_func,
                    params,
                    results,
                    expected_execution_time: 0,
                    relative_deadline: test_config.expected_deadline.parse().unwrap_or(0),
                };
                Ok(fc)
            }
            Err(err) => Err(err),
        }
    }

    pub fn set_relative_deadline(&mut self, relative_deadline: u64) {
        self.relative_deadline = relative_deadline;
    }

    pub fn get_relative_deadline(&self) -> u64 {
        self.relative_deadline
    }

    pub fn set_expected_execution_time(&mut self, expected_execution_time: u64) {
        self.expected_execution_time = expected_execution_time;
    }

    pub fn get_expected_execution_time(&self) -> u64 {
        self.expected_execution_time
    }
}

fn cvt_params(param_type: String, params: Vec<String>) -> Result<Vec<wasmtime::Val>, Error> {
    let mut res = Vec::new();
    let mut ok = true;
    if param_type.to_ascii_lowercase() == "void" {
    } else if param_type.to_ascii_lowercase() == "i32" {
        params.iter().for_each(|param| {
            match param.parse::<i32>() {
                Ok(val) => res.push(wasmtime::Val::from(val)),
                Err(_) => ok = false,
            };
        })
    } else if param_type.to_ascii_lowercase() == "i64" {
        params.iter().for_each(|param| {
            match param.parse::<i64>() {
                Ok(val) => res.push(wasmtime::Val::from(val)),
                Err(_) => ok = false,
            };
        })
    } else if param_type.to_ascii_lowercase() == "f32" {
        params.iter().for_each(|param| {
            match param.parse::<f32>() {
                Ok(val) => res.push(wasmtime::Val::from(val)),
                Err(_) => ok = false,
            };
        })
    } else if param_type.to_ascii_lowercase() == "f64" {
        params.iter().for_each(|param| {
            match param.parse::<f64>() {
                Ok(val) => res.push(wasmtime::Val::from(val)),
                Err(_) => ok = false,
            };
        })
    } else if param_type.to_ascii_lowercase() == "u128" {
        params.iter().for_each(|param| {
            match param.parse::<u128>() {
                Ok(val) => res.push(wasmtime::Val::from(val)),
                Err(_) => ok = false,
            };
        })
    } else {
        ok = false;
    }
    if ok {
        Ok(res)
    } else {
        Err(wasmtime::Error::msg("Invalid_params").context("Invalid_params"))
    }
}

pub fn call_func_sync(env: Environment) -> Result<Duration, Error> {
    let start = std::time::Instant::now();
    let mut conf = env.get_func_config().unwrap();
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()?
        .build();

    let mut store = Store::new(&env.engine, wasi);
    let instance = env.linker.instantiate(&mut store, &env.module)?;

    if let Some(caller) = instance.get_func(&mut store, &conf.export_func) {
        match caller.call(&mut store, &conf.params, &mut conf.results) {
            Ok(_) => {
                let end = std::time::Instant::now();
                let time = end - start;
                return Ok(time);
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
    func_result: &Arc<FuncResult>,
) -> Result<(u64, String), Error> {
    if conf.relative_deadline <= conf.expected_execution_time {
        return Err(wasmtime::Error::msg("Invalid_deadline").context("Invalid_deadline"));
    }
    if conf.task_unique_name.eq("anon") {
        conf.task_unique_name = format!("anon{:?}", std::time::Instant::now());
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
    // let task_unique_name = conf.task_unique_name.clone();
    let func_result = func_result.clone();
    if let Some(caller) = instance.get_func(&mut store, &conf.export_func) {
        let func = move || match caller.call(&mut store, &conf.params, &mut conf.results) {
            Ok(_) => {
                // tracing::info!("{}: results = {:?}", task_unique_name, conf.results);
                func_result.set_result(&format!("{:?}", conf.results));
                func_result.set_completed();
                Ok(conf.results)
            }
            Err(err) => {
                tracing::warn!("run_wasm_error: {}", err);
                func_result.set_result(&format!("{:?}", err));
                func_result.set_completed();
                Err(err)
            }
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

impl fmt::Display for FuncConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "expected_execution_time: {:?}, relative_deadline: {:?}",
            self.expected_execution_time, self.relative_deadline
        )
    }
}
