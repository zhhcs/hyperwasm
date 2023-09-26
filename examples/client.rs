use hyper_scheduler::{
    axum::{client::Client, CallConfigRequest, TestRequest},
    runwasm::RegisterConfig,
};

// cargo build --release --package hyper-scheduler --example client
// sudo ./target/release/examples/client
#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");

    let client = Client::new();
    let config = RegisterConfig::new(
        "/home/zhanghao/dev/hyper-scheduler/examples/fib.wasm",
        "fib.wasm",
    );
    let _ = client.init(&config).await;

    let test_config = TestRequest {
        wasm_name: "fib.wasm".to_owned(),
        export_func: "fib".to_owned(),
        param_type: "i32".to_owned(),
        params: vec!["34".to_owned()],
        results_length: "1".to_owned(),
    };

    let _ = client.test(test_config).await;

    let call_config = CallConfigRequest {
        wasm_name: "fib.wasm".to_owned(),
        task_unique_name: "fib_abcd".to_owned(),
        export_func: "fib".to_owned(),
        param_type: "i32".to_owned(),
        params: vec!["34".to_owned()],
        results_length: "1".to_owned(),
        expected_execution_time: "0".to_owned(),
        relative_deadline: "1".to_owned(),
    };
    let _ = client.call(&call_config).await;
}
