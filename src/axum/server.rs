use crate::{
    runtime::Runtime,
    runwasm::{call, get_status_by_name, Config, Environment},
};
use axum::{body::Body, extract::Query, http::Request, routing::get, Json, Router};
use std::{
    cell::RefCell,
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc,
    },
};

static mut PORT: AtomicU16 = AtomicU16::new(3000);

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
            .route("/init", get(Self::init))
            .route("/status", get(Self::get_status))
            .route("/uname", get(Self::get_status_by_name));
        // .route("/completed", get(Self::get_completed_status));

        let addr = SocketAddr::from(([0, 0, 0, 0], get_port()));
        tracing::info!("listening on {}", addr);

        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
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
        let start = std::time::Instant::now();
        let mut response = String::new();
        ENV_MAP.with(|map| {
            if let Some(env) = map.borrow().get(config.get_wasm_name()) {
                let env = env.clone();
                match call(&RUNTIME, env, Some(config)) {
                    Ok((id, name)) => {
                        response.push_str(&format!("new task spawned id: {}, name: {}", id, name))
                    }
                    Err(err) => response.push_str(&err.to_string()),
                };
            } else {
                response.push_str("Invalid wasm name");
            }
        });
        let end = std::time::Instant::now();
        tracing::info!("call with {:?}", end - start);
        response
    }

    async fn call(req: Request<Body>) -> String {
        let start = std::time::Instant::now();
        let mut response = String::new();
        let mut uri = req.uri().to_string();
        uri.remove(0);
        // tracing::info!("uri = {}", uri);
        ENV_MAP.with(|map| {
            if let Some(env) = map.borrow().get(&uri) {
                let env = env.clone();
                match call(&RUNTIME, env, None) {
                    Ok((id, name)) => {
                        response.push_str(&format!("new task spawned id: {}, name: {}", id, name))
                    }
                    Err(err) => response.push_str(&err.to_string()),
                };
            } else {
                response.push_str("Invalid wasm name");
            }
        });
        let end = std::time::Instant::now();
        tracing::info!("call {:?}", end - start);
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
}

#[derive(serde::Deserialize)]
struct Params {
    uname: String,
}
