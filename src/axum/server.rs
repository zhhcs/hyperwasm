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
use crossbeam::queue::ArrayQueue;
use once_cell::sync::OnceCell;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex, RwLock},
    time::Instant,
};
use tokio::task::JoinHandle;

thread_local! {
    /// 第一个请求的启动的时间戳
    static START: Cell<Option<Instant>> = Cell::new(None);
    /// 吞吐量统计 (时间戳，请求数量，准入数量)
    static THROUGHPUT: RefCell<Vec<(std::time::Duration,u32,u32)>> = RefCell::new(Vec::new());
}

/**
 * 初始化第一个请求的启动时间
 */
pub fn init_start() {
    START.with(|cell| {
        if cell.get().is_none() {
            cell.set(Some(Instant::now()));
        }
    });
}

/**
 * 获得Server启动时间
 */
pub fn get_start() -> Instant {
    START.with(|cell| cell.get()).expect("no start")
}

// 单例模式
static RUNTIME: OnceCell<Runtime> = OnceCell::new();
/**
 * 获取runtime
 */
fn runtime() -> &'static Runtime {
    RUNTIME.get().unwrap()
}

lazy_static::lazy_static! {
    /// 用于存放编译后的wasm环境
    static ref ENV_MAP: Arc<RwLock<HashMap<String, Environment>>> = Arc::new(RwLock::new(HashMap::new()));
    /// 函数调用的请求队列
    static ref REQUEST_QUEUE: ArrayQueue<SchedRequest> = ArrayQueue::new(10000);
    /// 执行的延迟统计，不含响应时间
    pub static ref LATENCY: Arc<Mutex<HashMap<i32, i32>>> = Arc::new(Mutex::new(HashMap::new()));
}

// 一些计数
/// 准入计数
static CNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// fib27计数，根据ddl判断
static CNT_27: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// fib30计数，根据ddl判断
static CNT_30: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// 总连接数
static CONNECTION: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
/// 累计热启动时延
static mut WARM_START_TIME: std::time::Duration = std::time::Duration::from_micros(0);
/// 累计成功调度时延
static mut SCHED_TIME: std::time::Duration = std::time::Duration::from_micros(0);

/// 调度请求
struct SchedRequest {
    name: String, // wasm的名字
    func_config: FuncConfig,
    func_result: Arc<FuncResult>,
    start_time: Instant, // 请求到达时间
}

/**
 * 创建全局调度器线程
 */
pub fn spawn_scheduler() -> JoinHandle<()> {
    tokio::task::spawn_blocking(|| {
        let cg_scheduler = crate::cgroupv2::Controllerv2::new(
            std::path::PathBuf::from("/sys/fs/cgroup/hypersched"),
            String::from("scheduler"),
        );
        cg_scheduler.set_threaded();
        cg_scheduler.set_cpuset(1, None);
        cg_scheduler.set_cgroup_threads(nix::unistd::gettid());
        loop {
            while let Some(sched) = REQUEST_QUEUE.pop() {
                let ddl = sched.func_config.get_relative_deadline();
                let start = std::time::Instant::now();
                if let Ok(map) = ENV_MAP.read() {
                    if let Some(env) = map.get(&sched.name) {
                        match call_func(
                            runtime(),
                            env.clone(),
                            sched.func_config,
                            &sched.func_result,
                        ) {
                            Ok(_) => {
                                let end = std::time::Instant::now();
                                unsafe {
                                    SCHED_TIME += end - start;
                                    WARM_START_TIME += end - sched.start_time;
                                }
                                // 测试用统计
                                CNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                if ddl == 20 {
                                    CNT_27.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                } else if ddl == 100 {
                                    CNT_30.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                }
                            }
                            Err(err) => {
                                sched.func_result.set_result(&err.to_string());
                                sched.func_result.set_completed();
                            }
                        }
                    }
                };
            }
        }
    })
}

pub struct Server {}

impl Server {
    /**
     * 启动Server,HyperWasm的入口
     * 这里配置工作核心的数量
     */
    pub async fn start(worker_threads: u8) {
        // 初始化runtime
        let _ = RUNTIME.set(Runtime::new(Some(worker_threads)));
        // 创建全局调度器线程
        let sched = spawn_scheduler();
        // crate::runwasm::MODEL.as_ref();

        // 在这里添加路由
        let app = Router::new()
            .route("/register", post(Self::register))
            .route("/test", post(Self::test))
            .route("/call_with_name", post(Self::call_with_name))
            .route("/init", get(Self::init))
            .route("/call", post(Self::call_func))
            .route("/status", get(Self::get_status))
            .route("/uname", get(Self::get_status_by_name))
            .route("/warm-start-latency", get(Self::get_warm_start_latency));

        let addr = SocketAddr::from(([0, 0, 0, 0], get_port()));
        tracing::info!("listening on {}", addr);

        // 测试用线程，实验室联调用
        // let handle = spawn_tester();

        // 启动服务器
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
        // 全局调度器线程在这里阻塞
        sched.await.unwrap();
        // 测试线程在这里阻塞
        // handle.await.unwrap();
    }

    /**
     * route: /init
     * 部署服务
     */
    async fn init(Json(config): Json<RegisterConfig>) -> Json<RegisterResponse> {
        // 记录第一个请求的启动时间
        init_start();
        let mut reponse = RegisterResponse {
            status: "Error".to_owned(),
            url: "null".to_owned(),
        };

        // wasm文件名
        let name = config.get_wasm_name();
        // 判断是否已经初始化过
        if let Ok(map) = ENV_MAP.read() {
            if map.contains_key(name) {
                reponse.status.push_str("_Invalid_wasm_name");
                return Json(reponse);
            }
        } else {
            return Json(reponse);
        }

        let start = std::time::Instant::now();

        // 编译(冷启动的一部分)
        if let Ok(env) = Environment::new(&config).await {
            let end = std::time::Instant::now();
            let cold_start = end - start;
            tracing::info!("cold-start: {:?}", cold_start);
            // 将编译后的环境配置保存到全局变量
            if let Ok(map) = ENV_MAP.write().as_mut() {
                map.insert((&env.get_wasm_name()).to_string(), env);
            }
            // 部署成功
            reponse.status = "Success".to_owned();
            reponse.url = format!("http://127.0.0.1:{}/call", get_port());
            return Json(reponse);
        }
        Json(reponse)
    }

    /**
     * route: /call
     * 函数调用
     */
    async fn call_func(Json(call_config): Json<CallConfigRequest>) -> Json<CallFuncResponse> {
        // 每次请求+1
        let connection = CONNECTION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        //请求数量统计，每10000个请求更新一次
        if connection % 10000 == 0 {
            let cnt = CNT.load(std::sync::atomic::Ordering::Relaxed);
            let time = std::time::Instant::now() - get_start();
            // 吞吐量统计
            THROUGHPUT.with(|throughput| throughput.borrow_mut().push((time, connection, cnt)));
        }
        let mut response = CallFuncResponse {
            status: "Error".to_owned(),
            result: "null".to_owned(),
        };
        let name = call_config.wasm_name.clone();
        // 这个status的flag感觉没什么用
        let mut status = false;
        // 开始计时
        let start_time = std::time::Instant::now();
        // 解析函数调用的参数
        match FuncConfig::new(call_config.clone()) {
            Ok(func_config) => {
                let func_result = Arc::new(FuncResult::new());
                // 检查是否已部署服务
                if let Ok(map) = ENV_MAP.read() {
                    if map.contains_key(&name) {
                        // 发送到全局调度器队列等待准入控制
                        let _ = REQUEST_QUEUE.push(SchedRequest {
                            name,
                            func_config,
                            func_result: func_result.clone(),
                            start_time,
                        });
                        status = true;
                    } else {
                        response.status = "Error_Invalid_wasm_name".to_owned();
                    }
                };
                if status {
                    // 取得函数计算结果
                    let res = Self::get_result(&func_result).await;
                    response.status = "Success".to_owned();
                    response.result = res;
                }
            }
            Err(err) => response.status = format!("Error_{}", err),
        };
        Json(response)
    }

    /**
     * 取得函数计算结果
     */
    async fn get_result(func_result: &Arc<FuncResult>) -> String {
        ResultFuture {
            result: func_result.clone(),
        }
        .await
    }

    /**
     * route: /status
     * 获取函数状态
     */
    async fn get_status() -> String {
        if let Some(status) = runtime().get_status() {
            if status.len() == 0 {
                // 如果没有正在运行的，就返回已经完成的
                if let Some(cs) = runtime().get_completed_status() {
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
            // 正在运行或就绪的函数状态
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

    /**
     * route: /uname
     * 根据名字查状态
     */
    async fn get_status_by_name(Query(params): Query<StatusQuery>) -> String {
        if let Some(status) = get_status_by_name(runtime(), &params.uname) {
            format!(
                "\nuname: {} {:?}\n{}",
                params.uname, status.co_status, status
            )
        } else {
            "not found".to_string()
        }
    }

    /**
     * route: /warm-start-latency
     * 查看平均热启动延迟、平均准入控制延迟等等
     */
    async fn get_warm_start_latency() -> String {
        // 直接输出每个时间段的吞吐量统计
        THROUGHPUT.with(|throughput| {
            throughput.borrow().iter().for_each(|tuple| {
                tracing::info!("{:?},{},{}", tuple.0, tuple.1, tuple.2);
            })
        });
        // 所有任务的延迟分布
        let mut latency_res = String::new();
        if let Ok(latency) = LATENCY.lock() {
            latency.iter().for_each(|(time, cnt)| {
                latency_res.push_str(&format!("\nlatency: {}, cnt: {}", time, cnt));
            });
        }

        // 获取一些计数信息
        let cnt = CNT.load(std::sync::atomic::Ordering::Relaxed);
        let cnt27 = CNT_27.load(std::sync::atomic::Ordering::Relaxed);
        let cnt30 = CNT_30.load(std::sync::atomic::Ordering::Relaxed);

        // 没有计数就算了
        if cnt == 0 {
            return "cnt: 0".to_owned();
        }
        // 计算平均延迟
        let start_latency = unsafe { WARM_START_TIME } / cnt;
        let sched_latency = unsafe { SCHED_TIME } / cnt;
        format!(
            "cnt: {}, start_latency: {:?}, cnt27: {}, cnt30: {}, sched_latency: {:?},\nlatency: {}",
            cnt, start_latency, cnt27, cnt30, sched_latency, latency_res
        )
    }

    /**
     * route: /register
     * 不用这个，但是留着
     */
    async fn register(mut multipart: Multipart) -> Json<RegisterResponse> {
        // tracing::info!("register tid = {}", nix::unistd::gettid());
        let mut reponse = RegisterResponse {
            status: "Error".to_owned(),
            url: "null".to_owned(),
        };
        if let Some(field) = multipart.next_field().await.unwrap() {
            let name = field.name().unwrap().to_string();
            if let Ok(map) = ENV_MAP.read() {
                if map.contains_key(&name) {
                    reponse.status.push_str("_Invalid_wasm_name");
                    return Json(reponse);
                }
            } else {
                return Json(reponse);
            }
            let data = field.bytes().await.unwrap();
            let path = format!("/tmp/{}", name);
            tokio::fs::write(&path, &data).await.unwrap();
            tracing::info!("saved to: {}", path);

            let config = RegisterConfig::new(&path, &name);
            if let Ok(env) = Environment::new(&config).await {
                if let Ok(map) = ENV_MAP.write().as_mut() {
                    map.insert((&env.get_wasm_name()).to_string(), env);
                    reponse.status = "Success".to_owned();
                    reponse.url = format!("http://127.0.0.1:{}/call", get_port());
                    return Json(reponse);
                }
            };
        }
        Json(reponse)
    }

    /**
     * route: /call_with_name
     * 匿名调用
     * 现在没法用了，一般也不用
     */
    async fn call_with_name(Json(name): Json<CallWithName>) -> Json<CallFuncResponse> {
        let mut response = CallFuncResponse {
            status: "Error".to_owned(),
            result: "null".to_owned(),
        };
        let mut status = false;
        let func_result = Arc::new(FuncResult::new());
        if let Ok(map) = ENV_MAP.read() {
            if let Some(env) = map.get(&name.wasm_name) {
                if let Some(func_config) = env.get_func_config() {
                    let env = env.clone();
                    match call_func(runtime(), env, func_config, &func_result) {
                        Ok(_) => {
                            status = true;
                        }
                        Err(err) => response.status = format!("Error_{}", err),
                    };
                }
            } else {
                response.status = "Error_Invalid_wasm_name".to_owned();
            }
        };
        if status {
            let res = Self::get_result(&func_result).await;
            response.status = "Success".to_owned();
            response.result = res;
        }
        Json(response)
    }

    /**
     * route: /test
     * 实验室联调用
     */
    async fn test(Json(test_config): Json<TestRequest>) -> Json<CallFuncResponse> {
        let mut response = CallFuncResponse {
            status: "Error".to_owned(),
            result: "null".to_owned(),
        };
        let name = test_config.wasm_name.to_owned();
        let mut status = false;

        match FuncConfig::from(test_config) {
            Ok(func_config) => {
                let func_result = Arc::new(FuncResult::new());
                if let Ok(map) = ENV_MAP.write().as_mut() {
                    if let Some(env) = map.get_mut(&name) {
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
                }
                if status {
                    let res = Self::get_result(&func_result).await;
                    if let Ok(time) = res.parse::<u64>() {
                        if let Ok(map) = ENV_MAP.write().as_mut() {
                            map.get_mut(&name).unwrap().set_test_time(time);
                        };
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

    /**
     * route: /call
     * 函数调用
     * 已经不用了，但我想留着
     */
    #[deprecated]
    async fn _call_func(Json(call_config): Json<CallConfigRequest>) -> Json<CallFuncResponse> {
        // let ddl = call_config.expected_deadline.clone();
        let connection = CONNECTION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if connection % 10000 == 0 {
            //请求数量统计，每10000个请求更新一次
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

        // 调用结果判断
        match FuncConfig::new(call_config) {
            Ok(func_config) => {
                let func_result = Arc::new(FuncResult::new());
                if let Ok(map) = ENV_MAP.read() {
                    if let Some(env) = map.get(&name) {
                        // tracing::info!("{:?}", env.get_func_config());
                        // let test_time = env.get_test_time();
                        // if func_config.get_relative_deadline() >= test_time {
                        let env = env.clone();
                        match call_func(runtime(), env, func_config, &func_result) {
                            Ok(_) => {
                                // 准入成功，累计热启动时延
                                let end = std::time::Instant::now();
                                warm_start = end - start;
                                // tracing::info!("{:?}", warm_start);
                                unsafe {
                                    WARM_START_TIME += warm_start;
                                }
                                // 测试用统计
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
                };
                if status {
                    // 取得函数计算结果
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
}

/**
 * 创建测试用线程
 * 实验室联调用
 */
pub fn spawn_tester() -> JoinHandle<()> {
    tokio::task::spawn_blocking(|| {
        let cg_tester = crate::cgroupv2::Controllerv2::new(
            std::path::PathBuf::from("/sys/fs/cgroup/hypersched"),
            String::from("tester"),
        );
        cg_tester.set_threaded();
        cg_tester.set_cpuset(0, None);
        cg_tester.set_cgroup_threads(nix::unistd::gettid());
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1));
            if let Some(tester) = get_test_env() {
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
    })
}
