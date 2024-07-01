use crate::{
    axum::{CallConfigRequest, TestRequest},
    result::FuncResult,
    runtime::{AdmissionControl, Runtime, SchedulabilityResult},
    task::SchedulerStatus,
};
use anyhow::Error;
use once_cell::sync::Lazy;
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

static TEST_QUEUE: Lazy<Mutex<VecDeque<Tester>>> = Lazy::new(|| Mutex::new(VecDeque::new()));
// pub static ref MODEL: Arc<ort::Session> = infer::detect::prepare_model();

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
    is_infer: bool,
}

impl RegisterConfig {
    pub fn new(path: &str, wasm_name: &str) -> RegisterConfig {
        RegisterConfig {
            path: path.to_string(),
            wasm_name: wasm_name.to_string(),
            is_infer: false,
        }
    }

    pub fn set_infer(&mut self) {
        self.is_infer = true;
    }

    pub fn is_infer(&self) -> bool {
        self.is_infer
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

/// wasm环境配置
#[derive(Clone)]
pub struct Environment {
    wasm_name: String,
    engine: Engine,
    module: Module,
    linker: Arc<Linker<WasiCtx>>,
    func_config: Option<FuncConfig>,
}

impl Environment {
    pub async fn new(config: &RegisterConfig) -> Result<Self, Error> {
        // let start = std::time::Instant::now();
        let wasm_name = config.get_wasm_name().to_owned();
        let engine = Engine::default();
        let module = Module::from_file(&engine, config.get_path())?;

        let mut linker = Linker::new(&engine);
        wasmtime_wasi::add_to_linker(&mut linker, |cx| cx)?;
        // if config.is_infer {
        //     linker.func_wrap("env", "llama2", move |temperature: f32| {
        //         llama2rs::llama2(temperature);
        //         Ok(())
        //     })?;
        /*// let model = infer::detect::prepare_model();
        let model = MODEL.clone();
        linker.func_wrap(
            "env",
            "infer",
            move |mut caller: wasmtime::Caller<'_, _>,
                  input_ptr: i32,
                  input_len: i32,
                  result_ptr: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|ext| ext.into_memory())
                    .ok_or_else(|| wasmtime::Trap::MemoryOutOfBounds)?;

                let mem = memory.data_mut(&mut caller);
                let string_data =
                    &mem[(input_ptr as usize)..((input_ptr + input_len) as usize)];

                let string_value = String::from_utf8_lossy(string_data).to_string();

                let buf = std::fs::read(string_value).unwrap_or(vec![]);
                let boxes = infer::detect::detect_objects_on_image(&model, buf);

                mem[result_ptr as usize..(result_ptr as usize + 8)]
                    .copy_from_slice(&(boxes.len()).to_le_bytes());
                Ok(())
            },
        )?;*/
        // }
        // let end = std::time::Instant::now();
        // tracing::info!("init latency: {:?}", end - start);
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

/// 函数配置
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
                    results = vec![wasmtime::Val::from(1.2); results_len];
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

/**
 * 参数解析，或许有更好的写法
 */
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

/**
 * 测试线程用的
 */
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

/**
 * wasm实例化
 */
fn instantiate(
    rt: &Runtime,
    env: Environment,
    mut conf: FuncConfig,
    func_result: &Arc<FuncResult>,
    schedulability_result: SchedulabilityResult,
) -> Result<u64, Error> {
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()?
        .build();
    let mut store = Store::new(&env.engine, wasi);
    let instance = env.linker.instantiate(&mut store, &env.module)?;

    let func_result_1 = func_result.clone();

    // 获取导出函数
    if let Some(caller) = instance.get_func(&mut store, &conf.export_func) {
        // 函数调用
        let func = move || match caller.call(&mut store, &conf.params, &mut conf.results) {
            Ok(_) => {
                func_result_1.set_result(&format!("{:?}", conf.results));
                func_result_1.set_completed();
                Ok(conf.results)
            }
            Err(err) => {
                tracing::warn!("run_wasm_error: {}", err);
                func_result_1.set_result(&format!("{:?}", err));
                func_result_1.set_completed();
                Err(err)
            }
        };
        // 打包成microprocess
        let id = rt.micro_process(func, schedulability_result);
        match id {
            Ok(id) => {
                // microprocess成功生成,名字唯一
                NAME_ID.with(|map| map.borrow_mut().insert(conf.task_unique_name.clone(), id));
                return Ok(id);
            }
            Err(err) => {
                func_result.set_result(&format!("{:?}", err));
                func_result.set_completed();
                return Err(err.into());
            }
        }
    } else {
        // 导出函数错误
        return Err(wasmtime::Error::msg("Invalid_export_func").context("Invalid_export_func"));
    }
}

/**
 * WASM函数调用
 */
pub fn call_func(
    rt: &Runtime,
    env: Environment,
    mut conf: FuncConfig,
    func_result: &Arc<FuncResult>,
) -> Result<u64, Error> {
    if conf.relative_deadline == 0 || conf.expected_execution_time == 0 {
    } else if conf.relative_deadline <= conf.expected_execution_time {
        return Err(wasmtime::Error::msg("Invalid_deadline").context("Invalid_deadline"));
    }
    if conf.task_unique_name.eq("anon") {
        conf.task_unique_name = format!("anon{:?}", std::time::Instant::now());
    }
    if NAME_ID.with(|map| map.borrow().contains_key(conf.task_unique_name.as_str())) {
        return Err(wasmtime::Error::msg("Invalid_unique_name").context("Invalid_unique_name"));
    }

    let expected_execution_time =
        if conf.relative_deadline == 0 || conf.expected_execution_time == 0 {
            None
        } else {
            Some(std::time::Duration::from_millis(
                conf.expected_execution_time,
            ))
        };
    let relative_deadline = if conf.relative_deadline == 0 || conf.expected_execution_time == 0 {
        None
    } else {
        Some(std::time::Duration::from_millis(conf.relative_deadline))
    };

    let res = rt.admission_control_result(expected_execution_time, relative_deadline);

    if res.get_ac() == AdmissionControl::UNSCHEDULABLE {
        // 不可调度
        let msg = "spawn failed, cause: UNSCHEDULABLE";
        func_result.set_result(msg);
        func_result.set_completed();
        Err(Error::msg(msg))
    } else {
        // 实例化
        instantiate(rt, env, conf, func_result, res)
    }
}

/**
 * 通过任务名字获取状态
 */
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
