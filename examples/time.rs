use hyper_scheduler::{
    axum::{client::Client, CallConfigRequest},
    runwasm::RegisterConfig,
};

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");

    let client = Client::new();
    let config = RegisterConfig::new(
        "/home/zhanghao/dev/fannkuchen-master/target/wasm32-wasi/release/nop.wasm",
        "nop.wasm",
    );
    let _ = client.init(&config).await;

    let mut cfgs = Vec::new();
    for i in 0..10 {
        let client = Client::new();
        let call_cfg = CallConfigRequest {
            wasm_name: "nop.wasm".to_string(),
            task_unique_name: format!("nop{}", i),
            export_func: "nop".to_owned(),
            param_type: "void".to_owned(),
            params: vec![],
            results_length: "0".to_owned(),
            expected_execution_time: "100".to_owned(),
            expected_deadline: "2000".to_owned(),
        };
        cfgs.push((client, call_cfg));
        // let _ = client.call(&call_cfg).await;
    }

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
    for i in 0..10 {
        cfg.task_unique_name.push_str(&format!("_{}", i));
        let _ = client.call(&cfg).await;
    }
}
