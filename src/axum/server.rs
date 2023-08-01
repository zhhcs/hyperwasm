use std::sync::Arc;

use crate::{
    runtime::Runtime,
    runwasm::{get_status_by_name, run_wasm, Config},
};
use axum::{extract::Query, routing::get, Json, Router};

lazy_static::lazy_static! {
    static ref RUNTIME: Arc<Runtime> = Arc::new(Runtime::new());
}
pub struct Server {}

impl Server {
    pub async fn start() {
        RUNTIME.as_ref();
        let app = Router::new()
            .route("/runwasm", get(Self::handler))
            .route("/status", get(Self::get_status))
            .route("/uname", get(Self::get_status_by_name))
            .route("/completed", get(Self::get_completed_status));

        tracing::info!("listening on 0.0.0.0:3000");
        axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
            .serve(app.into_make_service())
            .await
            .unwrap();
    }

    async fn handler(Json(config): Json<Config>) -> String {
        tracing::info!("Received a request");
        // tracing::info!("{}", config);
        match run_wasm(&RUNTIME, config) {
            Ok(_) => "new task spawned".to_string(),
            Err(err) => err.to_string(),
        }
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

    async fn get_completed_status() -> String {
        if let Some(status) = RUNTIME.get_completed_status() {
            if status.len() == 0 {
                return "no task completed".to_string();
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
}

#[derive(serde::Deserialize)]
struct Params {
    uname: String,
}
