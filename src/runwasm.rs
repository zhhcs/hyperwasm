use crate::{axum::CallConfigRequest, result::FuncResult, runtime::Runtime, task::SchedulerStatus};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap, fmt, sync::Arc};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::{sync::WasiCtxBuilder, WasiCtx};

thread_local! {
    static NAME_ID: RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
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
pub struct Environment {
    wasm_name: String,
    engine: Engine,
    module: Module,
    linker: Arc<Linker<WasiCtx>>,
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
                    relative_deadline: call_config.relative_deadline.parse::<u64>().unwrap_or(0),
                };
                Ok(fc)
            }
            Err(err) => Err(err),
        }
    }
}

fn cvt_params(param_type: String, params: Vec<String>) -> Result<Vec<wasmtime::Val>, Error> {
    let mut res = Vec::new();
    let mut ok = true;
    if param_type.to_ascii_lowercase() == "i32" {
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

#[deprecated]
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
    func_result: &Arc<FuncResult>,
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
    let func_result = func_result.clone();
    if let Some(caller) = instance.get_func(&mut store, &conf.export_func) {
        let func = move || match caller.call(&mut store, &conf.params, &mut conf.results) {
            Ok(_) => {
                tracing::info!("{}: results = {:?}", task_unique_name, conf.results);

                func_result.set_completed();
                func_result.set_result(&format!("{:?}", conf.results));
                Ok(conf.results)
            }
            Err(err) => {
                tracing::warn!("run_wasm_error: {}", err);
                func_result.set_completed();
                func_result.set_result(&format!("{:?}", err));
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
