use hyper_scheduler::{
    axum::{client::Client, CallConfigRequest},
    runwasm::RegisterConfig,
};

// cargo build --release --package hyper-scheduler --example llama2
// sudo ./target/release/examples/llama2
#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");

    let mut cfgs = Vec::new();
    for i in 0..5 {
        let client = Client::new();
        let call_cfg = CallConfigRequest {
            wasm_name: "chat.wasm".to_string(),
            task_unique_name: format!("chat{}", i),
            export_func: "chat".to_owned(),
            param_type: "f32".to_owned(),
            params: vec![1.0.to_string()],
            results_length: "0".to_owned(),
            expected_execution_time: "80".to_owned(),
            expected_deadline: "4000".to_owned(),
        };
        cfgs.push((client, call_cfg));
        // let _ = client.call(&call_cfg).await;
    }

    let client = Client::new();
    let mut config = RegisterConfig::new(
        "/home/zhanghao/dev/hyper-scheduler/examples/chat.wasm",
        "chat.wasm",
    );
    config.set_infer();
    let _ = client.init(&config).await;
    tracing::info!("spawn task");

    let mut tasks = Vec::new();
    for cfg in cfgs {
        let task = tokio::spawn(req(cfg.0, cfg.1));
        tasks.push(task);
    }

    for task in tasks {
        let _ = task.await;
    }
    let _ = client.get_latency().await;
}

async fn req(client: Client, mut cfg: CallConfigRequest) {
    for i in 0..1 {
        cfg.task_unique_name.push_str(&format!("_{}", i));
        let latency = client.call_latency(&cfg).await.unwrap();
        tracing::info!("{}-{:?}", cfg.task_unique_name, latency);
    }
}
