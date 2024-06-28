use hyper_scheduler::{
    axum::{client::Client, CallConfigRequest},
    runwasm::RegisterConfig,
};

// cargo build --release --package hyper-scheduler --example client-test4-1c
// sudo ./target/release/examples/client
#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");

    let client = Client::new();
    let (num, t1, t2) = (30, 4, 3);
    let call_config = CallConfigRequest {
        wasm_name: "fib.wasm".to_owned(),
        task_unique_name: format!("fib_abcd"),
        export_func: "fib_r".to_owned(),
        param_type: "i32".to_owned(),
        params: vec![num.to_string()],
        results_length: "1".to_owned(),
        expected_execution_time: t1.to_string(),
        expected_deadline: t2.to_string(),
        //expected_deadline: rng.gen_range(2100..20000).to_string(),
    };

    // 部署服务
    let config = RegisterConfig::new(
        "/home/user/lmxia/hyperwasm-multi_thread/hyper-scheduler/fib.wasm",
        "fib.wasm",
    );
    let _ = client.init(&config).await;

    // 函数调用

    tracing::info!("spawn task");
    let _ = client.call(&call_config).await;
    let _ = client.get_latency().await;
}
