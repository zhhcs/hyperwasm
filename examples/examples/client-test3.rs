use clap::Parser;
use hyper_scheduler::{
    axum::{client::Client, ClientArgs, TestRequest},
    runwasm::RegisterConfig,
};

// cargo build --release --package hyper-scheduler --example client-test3
// sudo ./target/release/examples/client-test3
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
    let (num, t2) = (30, 100);
    let test_cfg = TestRequest {
        wasm_name: "fib.wasm".to_owned(),
        export_func: "fib_r".to_owned(),
        param_type: "i32".to_owned(),
        params: vec![num.to_string()],
        results_length: "1".to_owned(),
        expected_deadline: t2.to_string(),
    };

    // 部署服务
    let config = RegisterConfig::new(
        "/home/zhanghao/dev/hyper-scheduler/examples/fib.wasm",
        "fib.wasm",
    );
    let _ = client.init(&config).await;
    let _ = client.test(test_cfg).await;
}
