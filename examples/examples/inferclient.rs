use clap::Parser;
use hyper_scheduler::{
    axum::{client::Client, CallConfigRequest, ClientArgs},
    runwasm::RegisterConfig,
};

// cargo build --release --package hyper-scheduler --example inferclient
// sudo ./target/release/examples/inferclient
#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");

    let args = ClientArgs::parse();
    let local_ip = &args.local_ip;
    let port = args.port;
    let client = Client::new(local_ip, port);
    let mut config = RegisterConfig::new(
        "/home/zhanghao/dev/hyper-scheduler/examples/detect.wasm",
        "detect.wasm",
    );
    config.set_infer();
    let _ = client.init(&config).await;

    for i in 0..10 {
        let call_cfg = CallConfigRequest {
            wasm_name: "detect.wasm".to_string(),
            task_unique_name: format!("detect{}", i),
            export_func: "detect".to_owned(),
            param_type: "void".to_owned(),
            params: vec![],
            results_length: "1".to_owned(),
            expected_execution_time: "1500".to_owned(),
            expected_deadline: "2000".to_owned(),
        };
        let _ = client.call(&call_cfg).await;
    }
    let _ = client.get_latency().await;
    // let test_config = TestRequest {
    //     wasm_name: "detect.wasm".to_owned(),
    //     export_func: "detect".to_owned(),
    //     param_type: "void".to_owned(),
    //     params: vec![],
    //     results_length: "1".to_owned(),
    //     expected_deadline: "1500".to_owned(),
    // };

    // let _ = client.test(test_config).await;
}
