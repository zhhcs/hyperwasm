use super::{
    CallConfigRequest, CallFuncResponse, CallWithName, RegisterResponse, StatusQuery, TestRequest,
};
use crate::{
    axum::get_port,
    result::{FuncResult, ResultFuture},
    runtime::Runtime,
    runwasm::{
        call_func, call_func_sync, get_status_by_name, get_test_env, set_test_env, Environment,
        FuncConfig, RegisterConfig, Tester,
    },
};
use axum::{
    extract::{Multipart, Query},
    routing::{get, post},
    Json, Router,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Instant,
};

thread_local! {
    static ENV_MAP: RefCell<HashMap<String, Environment>> = RefCell::new(HashMap::new());
    static START: Cell<Option<Instant>> = Cell::new(None);
    static THROUGHPUT: RefCell<Vec<(std::time::Duration,u32,u32)>> = RefCell::new(Vec::new());
}

pub fn init_start() {
    START.with(|cell| {
        assert!(cell.get().is_none());
        cell.set(Some(Instant::now()));
    });
}

pub fn get_start() -> Instant {
    START.with(|cell| cell.get()).expect("no start")
}

lazy_static::lazy_static! {
    // static ref RUNTIME: Arc<Runtime> = Arc::new(Runtime::new(Some(2)));
    static ref RUNTIME: Arc<Runtime> = Arc::new(Runtime::default());
    pub static ref LATENCY: Arc<Mutex<HashMap<i32, i32>>> = Arc::new(Mutex::new(HashMap::new()));
}

static CNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static CNT_27: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static CNT_30: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

static CONNECTION: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

static mut T: std::time::Duration = std::time::Duration::from_micros(0);

pub struct Server {}

impl Server {
    pub async fn start() {
        RUNTIME.as_ref();
        // crate::runwasm::MODEL.as_ref();
        let app = Router::new()
            .route("/register", post(Self::register))
            .route("/test", post(Self::test))
            .route("/call_with_name", post(Self::call_with_name))
            .route("/init", get(Self::init))
            .route("/call", post(Self::call_func))
            .route("/status", get(Self::get_status))
            .route("/uname", get(Self::get_status_by_name))
            .route("/warm-start-latency", get(Self::get_warm_start_latency))
            .route("/fib27", get(Self::fib27));

        let addr = SocketAddr::from(([0, 0, 0, 0], get_port()));
        tracing::info!("listening on {}", addr);

        let handle = tokio::task::spawn_blocking(|| {
            let cg_tester = crate::cgroupv2::Controllerv2::new(
                std::path::PathBuf::from("/sys/fs/cgroup/hypersched"),
                String::from("tester"),
            );
            cg_tester.set_threaded();
            cg_tester.set_cpuset(1, None);
            cg_tester.set_cgroup_threads(nix::unistd::gettid());
            loop {
                // RUNTIME.as_ref().drop_co();
                std::thread::sleep(std::time::Duration::from_millis(1));
                if let Some(tester) = get_test_env() {
                    // tracing::info!("call_func_sync tid {}", gettid());
                    match call_func_sync(tester.env) {
                        Ok(time) => {
                            let res = format!("{:?}", time.as_millis() + 1);
                            tester.result.set_result(&res);
                            tester.result.set_completed();
                        }
                        Err(err) => {
                            tester.result.set_result(&err.to_string());
                            tester.result.set_completed();
                        }
                    };
                }
            }
        });

        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
        handle.await.unwrap();
    }

    /// route: /register
    async fn register(mut multipart: Multipart) -> Json<RegisterResponse> {
        // tracing::info!("register tid = {}", nix::unistd::gettid());
        let mut reponse = RegisterResponse {
            status: "Error".to_owned(),
            url: "null".to_owned(),
        };
        if let Some(field) = multipart.next_field().await.unwrap() {
            let name = field.name().unwrap().to_string();
            if ENV_MAP.with(|map| map.borrow().contains_key(&name)) {
                reponse.status.push_str("_Invalid_wasm_name");
                return Json(reponse);
            }
            let data = field.bytes().await.unwrap();
            let path = format!("/tmp/{}", name);
            tokio::fs::write(&path, &data).await.unwrap();
            tracing::info!("saved to: {}", path);

            let config = RegisterConfig::new(&path, &name);
            if let Ok(env) = Environment::new(&config).await {
                ENV_MAP.with(|map| {
                    map.borrow_mut()
                        .insert((&env.get_wasm_name()).to_string(), env)
                });

                reponse.status = "Success".to_owned();
                reponse.url = format!("http://127.0.0.1:{}/call", get_port());
                return Json(reponse);
            };
        }
        Json(reponse)
    }

    async fn test(Json(test_config): Json<TestRequest>) -> Json<CallFuncResponse> {
        // tracing::info!("test tid = {}", nix::unistd::gettid());
        let mut response = CallFuncResponse {
            status: "Error".to_owned(),
            result: "null".to_owned(),
        };
        let name = test_config.wasm_name.to_owned();
        let mut status = false;

        match FuncConfig::from(test_config) {
            Ok(func_config) => {
                let func_result = Arc::new(FuncResult::new());
                ENV_MAP.with(|map| {
                    if let Some(env) = map.borrow_mut().get_mut(&name) {
                        env.set_func_config(func_config);
                        let env = env.clone();
                        set_test_env(Tester {
                            env,
                            result: func_result.clone(),
                        });
                        status = true;
                    } else {
                        response.status = "Error_Invalid_wasm_name".to_owned();
                    }
                });
                if status {
                    let res = Self::get_result(&func_result).await;
                    if let Ok(time) = res.parse::<u64>() {
                        ENV_MAP.with(|map| {
                            map.borrow_mut().get_mut(&name).unwrap().set_test_time(time)
                        });
                        response.result = time.to_string();
                    } else {
                        response.result = res;
                    }
                    response.status = "Success".to_owned();
                }
            }
            Err(err) => response.status = format!("Error_{}", err),
        };

        Json(response)
    }

    /// route: /init
    async fn init(Json(config): Json<RegisterConfig>) -> Json<RegisterResponse> {
        init_start();
        let mut reponse = RegisterResponse {
            status: "Error".to_owned(),
            url: "null".to_owned(),
        };

        let name = config.get_wasm_name();
        if ENV_MAP.with(|map| map.borrow().contains_key(name)) {
            reponse.status.push_str("_Invalid_wasm_name");
            return Json(reponse);
        }
        let start = std::time::Instant::now();
        // let path = config.get_path();
        // let config = RegisterConfig::new(&path, &name);
        if let Ok(env) = Environment::new(&config).await {
            let end = std::time::Instant::now();
            let cold_start = end - start;
            tracing::info!("cold-start: {:?}", cold_start);
            ENV_MAP.with(|map| {
                map.borrow_mut()
                    .insert((&env.get_wasm_name()).to_string(), env)
            });

            reponse.status = "Success".to_owned();
            reponse.url = format!("http://127.0.0.1:{}/call", get_port());
            return Json(reponse);
        }
        Json(reponse)
    }

    async fn call_with_name(Json(name): Json<CallWithName>) -> Json<CallFuncResponse> {
        let mut response = CallFuncResponse {
            status: "Error".to_owned(),
            result: "null".to_owned(),
        };
        let mut status = false;
        let func_result = Arc::new(FuncResult::new());
        ENV_MAP.with(|map| {
            if let Some(env) = map.borrow().get(&name.wasm_name) {
                if let Some(func_config) = env.get_func_config() {
                    let env = env.clone();
                    match call_func(&RUNTIME, env, func_config, &func_result) {
                        Ok(_) => {
                            status = true;
                        }
                        Err(err) => response.status = format!("Error_{}", err),
                    };
                }
            } else {
                response.status = "Error_Invalid_wasm_name".to_owned();
            }
        });
        if status {
            let res = Self::get_result(&func_result).await;
            response.status = "Success".to_owned();
            response.result = res;
        }
        Json(response)
    }

    /// route: /call
    async fn call_func(Json(call_config): Json<CallConfigRequest>) -> Json<CallFuncResponse> {
        // let ddl = call_config.expected_deadline.clone();
        let connection = CONNECTION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if connection % 10000 == 0 {
            //请求数量统计
            let cnt = CNT.load(std::sync::atomic::Ordering::Relaxed);
            let time = std::time::Instant::now() - get_start();
            THROUGHPUT.with(|throughput| throughput.borrow_mut().push((time, connection, cnt)));
        }
        // tracing::info!("call tid = {}", nix::unistd::gettid());
        let mut response = CallFuncResponse {
            status: "Error".to_owned(),
            result: "null".to_owned(),
        };
        let name = call_config.wasm_name.to_owned();
        let ddl = call_config.expected_deadline.clone();
        let mut status = false;

        let start = std::time::Instant::now();
        let mut warm_start = std::time::Duration::from_millis(0);
        match FuncConfig::new(call_config) {
            Ok(func_config) => {
                let func_result = Arc::new(FuncResult::new());
                ENV_MAP.with(|map| {
                    if let Some(env) = map.borrow().get(&name) {
                        // tracing::info!("{:?}", env.get_func_config());
                        // let test_time = env.get_test_time();
                        // if func_config.get_relative_deadline() >= test_time {
                        let env = env.clone();
                        match call_func(&RUNTIME, env, func_config, &func_result) {
                            Ok(_) => {
                                let end = std::time::Instant::now();
                                warm_start = end - start;
                                // tracing::info!("{:?}", warm_start);
                                unsafe {
                                    T += warm_start;
                                }
                                CNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                if ddl == "20" {
                                    CNT_27.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                } else if ddl == "100" {
                                    CNT_30.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                }
                                // tracing::info!("warm-start: {:?}", end - start);
                                status = true;
                            }
                            Err(err) => {
                                response.status = format!("Error_{}", err);
                                // tracing::info!("{}", err);
                            }
                        };
                        // }
                    } else {
                        response.status = "Error_Invalid_wasm_name".to_owned();
                    }
                });
                if status {
                    let res = Self::get_result(&func_result).await;
                    response.status = "Success".to_owned();
                    response.result = res;
                    response.result = format!("{:?}", warm_start);
                }
            }
            Err(err) => response.status = format!("Error_{}", err),
        };
        // let end_res = std::time::Instant::now();
        // tracing::info!("ddl: {}, response latency: {:?}", ddl, end_res - start);
        Json(response)
    }

    async fn get_result(func_result: &Arc<FuncResult>) -> String {
        // tracing::info!("get_result tid = {}", nix::unistd::gettid());
        ResultFuture {
            result: func_result.clone(),
        }
        .await
    }

    /// route: /status
    async fn get_status() -> String {
        if let Some(status) = RUNTIME.get_status() {
            if status.len() == 0 {
                if let Some(cs) = RUNTIME.get_completed_status() {
                    let mut s = String::from("no task running\n");
                    cs.iter().for_each(|(id, stat)| {
                        s.push_str(&format!(
                            "\nid: {}, status: {:?}\n{}",
                            id, stat.co_status, stat
                        ));
                    });
                    return s;
                }
                return "no task running".to_string();
            }
            let mut s = String::new();
            status.iter().for_each(|(id, stat)| {
                s.push_str(&format!(
                    "\nid: {}, status: {:?}\n{}",
                    id, stat.co_status, stat
                ));
            });
            return s;
        };
        "500".to_string()
    }

    /// route: /uname
    async fn get_status_by_name(Query(params): Query<StatusQuery>) -> String {
        if let Some(status) = get_status_by_name(&RUNTIME, &params.uname) {
            format!(
                "\nuname: {} {:?}\n{}",
                params.uname, status.co_status, status
            )
        } else {
            "not found".to_string()
        }
    }

    async fn get_warm_start_latency() -> String {
        THROUGHPUT.with(|throughput| {
            throughput.borrow().iter().for_each(|tuple| {
                tracing::info!("{:?},{},{}", tuple.0, tuple.1, tuple.2);
            })
        });
        let mut latency_res = String::new();
        if let Ok(latency) = LATENCY.lock() {
            latency.iter().for_each(|(time, cnt)| {
                latency_res.push_str(&format!("\nlatency: {}, cnt: {}", time, cnt));
            });
        }
        let cnt = CNT.load(std::sync::atomic::Ordering::Relaxed);
        let cnt27 = CNT_27.load(std::sync::atomic::Ordering::Relaxed);
        let cnt30 = CNT_30.load(std::sync::atomic::Ordering::Relaxed);

        if cnt == 0 {
            return "cnt: 0".to_owned();
        }
        let start_latency = unsafe { T } / cnt;
        format!(
            "cnt: {}, start_latency: {:?}, cnt27: {}, cnt30: {}, \nlatency: {}",
            cnt, start_latency, cnt27, cnt30, latency_res
        )
    }

    async fn fib27() -> Json<CallFuncResponse> {
        let call_config = CallConfigRequest {
            wasm_name: "fib.wasm".to_owned(),
            task_unique_name: format!("fib_abcd{}", CNT.load(std::sync::atomic::Ordering::Relaxed)),
            export_func: "fib_r".to_owned(),
            param_type: "i32".to_owned(),
            params: vec!["27".to_owned()],
            results_length: "1".to_owned(),
            expected_execution_time: "1".to_owned(),
            expected_deadline: 20.to_string(),
        };
        let mut response = CallFuncResponse {
            status: "Error".to_owned(),
            result: "null".to_owned(),
        };
        let name = call_config.wasm_name.to_owned();
        // let ddl = call_config.expected_deadline.clone();
        let mut status = false;

        let start = std::time::Instant::now();
        match FuncConfig::new(call_config) {
            Ok(func_config) => {
                let func_result = Arc::new(FuncResult::new());
                ENV_MAP.with(|map| {
                    if let Some(env) = map.borrow().get(&name) {
                        // tracing::info!("{:?}", env.get_func_config());
                        let test_time = env.get_test_time();
                        if func_config.get_relative_deadline() >= test_time {
                            let env = env.clone();
                            match call_func(&RUNTIME, env, func_config, &func_result) {
                                Ok(_) => {
                                    let end = std::time::Instant::now();
                                    let warm_start = end - start;
                                    unsafe {
                                        T += warm_start;
                                    }
                                    CNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                    // tracing::info!("warm-start: {:?}", end - start);
                                    status = true;
                                }
                                Err(err) => response.status = format!("Error_{}", err),
                            };
                        }
                    } else {
                        response.status = "Error_Invalid_wasm_name".to_owned();
                    }
                });
                if status {
                    let res = Self::get_result(&func_result).await;
                    response.status = "Success".to_owned();
                    response.result = res;
                }
            }
            Err(err) => response.status = format!("Error_{}", err),
        };
        // let end_res = std::time::Instant::now();
        // tracing::info!("ddl: {}, response latency: {:?}", ddl, end_res - start);
        Json(response)
    }
}
