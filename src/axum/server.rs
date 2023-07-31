use std::sync::Arc;

use crate::{
    runtime::Runtime,
    runwasm::{run_wasm, Config},
};
use axum::{routing::get, Json, Router};

lazy_static::lazy_static! {
    static ref RUNTIME: Arc<Runtime> = Arc::new(Runtime::new());
}
pub struct Server {}

impl Server {
    pub async fn start() {
        RUNTIME.as_ref();
        let app = Router::new().route("/", get(Self::handler));

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

    pub fn get_status() {
        RUNTIME.print_completed_status();
    }
}

//  name + id hash
