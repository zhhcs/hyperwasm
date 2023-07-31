use std::sync::Arc;

use crate::{
    runtime::Runtime,
    runwasm::{get_status_by_name, run_wasm, Config},
};
use axum::{routing::get, Json, Router};

lazy_static::lazy_static! {
    static ref RUNTIME: Arc<Runtime> = Arc::new(Runtime::new());
}
pub struct Server {}

impl Server {
    pub async fn start() {
        RUNTIME.as_ref();
        let app = Router::new()
            .route("/", get(Self::handler))
            .route("/status", get(Self::get_status))
            .route("/uname", get(Self::get_status_by_name));

        tracing::info!("listening on 0.0.0.0:3000");
        axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
            .serve(app.into_make_service())
            .await
            .unwrap();
    }

    async fn handler(Json(config): Json<Config>) -> &'static str {
        tracing::info!("Received a request");
        tracing::info!("{}", config);
        match run_wasm(&RUNTIME, config) {
            Ok(_) => "new task spawned",
            Err(err) => {
                if err.to_string() == "need unique name" {
                    "need unique name"
                } else {
                    "failed to spawn"
                }
            }
        }
    }

    async fn get_status() -> &'static str {
        if let Some(status) = RUNTIME.get_status() {
            status.iter().for_each(|(id, stat)| {
                tracing::info!("\nid: {}, status: \n{}", id, stat);
            });
        };
        "000"
    }

    async fn get_status_by_name(Json(uname): Json<String>) -> &'static str {
        if let Some(status) = get_status_by_name(&RUNTIME, &uname) {
            tracing::info!("\nuname: {} \n{}", uname, status)
        };
        "111"
    }
}

//  name + id hash
