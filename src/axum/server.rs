use crate::{
    result::{FuncResult, ResultFuture},
    runtime::Runtime,
    runwasm::{
        call, call_func, call_func_sync, get_status_by_name, Config, Environment, FuncConfig,
    },
};
use axum::{
    body::Body,
    extract::{Multipart, Query},
    http::Request,
    routing::{get, post},
    Json, Router,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
};

static mut PORT: AtomicU16 = AtomicU16::new(3001);

fn get_port() -> u16 {
    unsafe { PORT.fetch_add(1, Ordering::SeqCst) }
}

thread_local! {
    static ENV_MAP: RefCell<HashMap<String, Environment>> = RefCell::new(HashMap::new());
}

lazy_static::lazy_static! {
    static ref RUNTIME: Arc<Runtime> = Arc::new(Runtime::new());
}
pub struct Server {}

impl Server {
    pub async fn start() {
        RUNTIME.as_ref();
        let app = Router::new()
            .route("/register", post(Self::register))
            .route("/init", get(Self::init))
            .route("/status", get(Self::get_status))
            .route("/uname", get(Self::get_status_by_name))
            .route("/call", post(Self::call_func));
        // .route("/completed", get(Self::get_completed_status));

        let addr = SocketAddr::from(([0, 0, 0, 0], get_port()));
        tracing::info!("listening on {}", addr);

        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    }

    async fn register(mut multipart: Multipart) -> Json<RegisterResponse> {
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

            let config = Config::new("", &path, 0, 0, &name, None, None);
            if let Ok(env) = Environment::new(&config) {
                ENV_MAP.with(|map| {
                    map.borrow_mut()
                        .insert((&env.get_wasm_name()).to_string(), env)
                });

                reponse.status = "Success".to_owned();
                reponse.url = format!("http://127.0.0.1:3001/call");
                return Json(reponse);
            };
        }
        Json(reponse)
    }

    async fn call_func_sync(Json(call_config): Json<CallConfigRequest>) -> Json<CallFuncResponse> {
        let mut response = CallFuncResponse {
            status: "Error".to_owned(),
            result: "null".to_owned(),
        };
        let name = call_config.wasm_name.to_owned();
        let func_config = FuncConfig::new(call_config);
        ENV_MAP.with(|map| {
            if let Some(env) = map.borrow().get(&name) {
                let env = env.clone();
                match call_func_sync(env, func_config) {
                    Ok(result) => {
                        response.status = "Success".to_owned();
                        response.result = format!("{:?}", result);
                    }
                    Err(err) => response.status = format!("Error_{}", err),
                };
            }
        });
        Json(response)
    }

    async fn call_func(Json(call_config): Json<CallConfigRequest>) -> Json<CallFuncResponse> {
        let mut response = CallFuncResponse {
            status: "Success".to_owned(),
            result: "null".to_owned(),
        };
        let name = call_config.wasm_name.to_owned();
        let func_config = FuncConfig::new(call_config);
        // tracing::info!("uri = {}", uri);
        let mut status = false;
        let func_result = Arc::new(FuncResult::new());
        ENV_MAP.with(|map| {
            if let Some(env) = map.borrow().get(&name) {
                let env = env.clone();
                match call_func(&RUNTIME, env, func_config, &func_result) {
                    Ok(_) => {
                        status = true;
                        // let end = std::time::Instant::now();
                        // tracing::info!("call {:?}", end - start);
                    }
                    Err(err) => response.status = format!("Error_{}", err),
                };
            } else {
                response.status = "Error_Invalid_wasm_name".to_owned();
            }
        });
        if status {
            let res = Self::get_result(&func_result).await;
            response.result = res;
        }
        Json(response)
    }

    async fn init(Json(config): Json<Config>) -> String {
        if ENV_MAP.with(|map| map.borrow().contains_key(config.get_wasm_name())) {
            return "Invalid wasm name".to_string();
        }
        if let Ok(env) = Environment::new(&config) {
            ENV_MAP.with(|map| {
                map.borrow_mut()
                    .insert((&env.get_wasm_name()).to_string(), env)
            });
            let port = get_port();
            let path = "/".to_owned() + &config.get_wasm_name();
            let path_with = "/".to_owned() + &config.get_wasm_name() + "/config";
            let app = Router::new()
                .route(&path, get(Self::call))
                .route(&path_with, get(Self::call_with));
            let addr = SocketAddr::from(([0, 0, 0, 0], port));
            tracing::info!("{} listening on {}", &config.get_wasm_name(), addr);
            tokio::spawn(async move {
                axum::Server::bind(&addr)
                    .serve(app.into_make_service())
                    .await
                    .unwrap();
            });
            return addr.to_string() + &path;
        };
        "unexpected error".to_string()
    }

    async fn call_with(Json(config): Json<Config>) -> String {
        // let start = std::time::Instant::now();
        let mut response = String::new();
        ENV_MAP.with(|map| {
            if let Some(env) = map.borrow().get(config.get_wasm_name()) {
                let env = env.clone();
                match call(&RUNTIME, env, Some(config)) {
                    Ok((id, name)) => {
                        // let end = std::time::Instant::now();
                        // tracing::info!("call with {:?}", end - start);
                        response.push_str(&format!("new task spawned id: {}, name: {}", id, name))
                    }
                    Err(err) => response.push_str(&err.to_string()),
                };
            } else {
                response.push_str("Invalid wasm name");
            }
        });
        response.push_str("\n");
        response
    }

    async fn call(req: Request<Body>) -> String {
        // let start = std::time::Instant::now();
        let mut response = String::new();
        let mut uri = req.uri().to_string();
        uri.remove(0);
        // tracing::info!("uri = {}", uri);
        ENV_MAP.with(|map| {
            if let Some(env) = map.borrow().get(&uri) {
                let env = env.clone();
                match call(&RUNTIME, env, None) {
                    Ok((id, name)) => {
                        // let end = std::time::Instant::now();
                        // tracing::info!("call {:?}", end - start);
                        response.push_str(&format!("new task spawned id: {}, name: {}", id, name))
                    }
                    Err(err) => response.push_str(&err.to_string()),
                };
            } else {
                response.push_str("Invalid wasm name");
            }
        });

        response
    }

    async fn get_status() -> String {
        if let Some(status) = RUNTIME.get_status() {
            if status.len() == 0 {
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

    async fn get_status_by_name(Query(params): Query<Params>) -> String {
        if let Some(status) = get_status_by_name(&RUNTIME, &params.uname) {
            format!(
                "\nuname: {} {:?}\n{}",
                params.uname, status.co_status, status
            )
        } else {
            "not found".to_string()
        }
    }

    // async fn get_completed_status() -> String {
    //     if let Some(status) = RUNTIME.get_completed_status() {
    //         if status.len() == 0 {
    //             return "no task completed".to_string();
    //         }
    //         let mut s = String::new();
    //         status.iter().for_each(|(id, stat)| {
    //             s.push_str(&format!(
    //                 "\nid: {}, status: {:?}\n{}",
    //                 id, stat.co_status, stat
    //             ));
    //         });
    //         return s;
    //     };
    //     "500".to_string()
    // }

    async fn get_result(func_result: &Arc<FuncResult>) -> String {
        ResultFuture {
            result: func_result.clone(),
        }
        .await
    }
}

#[derive(serde::Deserialize)]
struct Params {
    uname: String,
}

#[derive(serde::Deserialize)]
pub struct CallConfigRequest {
    pub wasm_name: String,
    // pub task_unique_name: String, //实例名称,必须唯一
    pub export_func: String, //调用的导出函数名称
    pub param_type: String,  //数据类型
    pub params: Vec<String>, //数组
    pub results_length: String, //结果长度
                             //     pub expected_execution_time: String, //预期执行时长(必须小于相对截止时间，单位毫秒)
                             //     pub relative_deadline: String,       //相对截止时间(单位毫秒)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RegisterResponse {
    status: String,
    url: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CallFuncResponse {
    status: String,
    result: String,
}
