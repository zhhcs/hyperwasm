use super::{CallConfigRequest, CallFuncResponse, RegisterResponse, StatusQuery};
use crate::{
    axum::get_port,
    result::{FuncResult, ResultFuture},
    runtime::Runtime,
    runwasm::{call_func, get_status_by_name, Environment, FuncConfig, RegisterConfig},
};
use axum::{
    extract::{Multipart, Query},
    routing::{get, post},
    Json, Router,
};
use std::{cell::RefCell, collections::HashMap, net::SocketAddr, sync::Arc};

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
            .route("/call", post(Self::call_func))
            .route("/status", get(Self::get_status))
            .route("/uname", get(Self::get_status_by_name));

        let addr = SocketAddr::from(([0, 0, 0, 0], get_port()));
        tracing::info!("listening on {}", addr);

        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    }

    /// route: /register
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

            let config = RegisterConfig::new(&path, &name);
            if let Ok(env) = Environment::new(&config) {
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

    /// route: /init
    async fn init(Json(config): Json<RegisterConfig>) -> Json<RegisterResponse> {
        let mut reponse = RegisterResponse {
            status: "Error".to_owned(),
            url: "null".to_owned(),
        };

        let name = config.get_wasm_name();
        if ENV_MAP.with(|map| map.borrow().contains_key(name)) {
            reponse.status.push_str("_Invalid_wasm_name");
            return Json(reponse);
        }

        let path = config.get_path();
        let config = RegisterConfig::new(&path, &name);
        if let Ok(env) = Environment::new(&config) {
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

    /// route: /call
    async fn call_func(Json(call_config): Json<CallConfigRequest>) -> Json<CallFuncResponse> {
        let mut response = CallFuncResponse {
            status: "Error".to_owned(),
            result: "null".to_owned(),
        };
        let name = call_config.wasm_name.to_owned();
        let func_config = FuncConfig::new(call_config);
        let mut status = false;
        let func_result = Arc::new(FuncResult::new());
        ENV_MAP.with(|map| {
            if let Some(env) = map.borrow().get(&name) {
                let env = env.clone();
                match call_func(&RUNTIME, env, func_config, &func_result) {
                    Ok(_) => {
                        status = true;
                    }
                    Err(err) => response.status = format!("Error_{}", err),
                };
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

    async fn get_result(func_result: &Arc<FuncResult>) -> String {
        ResultFuture {
            result: func_result.clone(),
        }
        .await
    }

    /// route: /status
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
}
