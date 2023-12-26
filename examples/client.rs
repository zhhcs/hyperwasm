// use std::time::Duration;

use hyper_scheduler::{
    axum::{client::Client, CallConfigRequest},
    runwasm::RegisterConfig,
};
// use tokio::time::sleep;
use rand::Rng;

// cargo build --release --package hyper-scheduler --example client
// sudo ./target/release/examples/client
#[tokio::main(flavor = "multi_thread", worker_threads = 20)]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");

    // let test_config = TestRequest {
    //     wasm_name: "fib.wasm".to_owned(),
    //     export_func: "fib".to_owned(),
    //     param_type: "i32".to_owned(),
    //     params: vec!["34".to_owned()],
    //     results_length: "1".to_owned(),
    //     expected_deadline: "10".to_owned(),
    // };

    // let _ = client.test(test_config).await;
    let mut rng = rand::thread_rng();

    let mut cfgs = Vec::new();
    for i in 0..100 {
        let client = Client::new();
        let (num, t1, t2) = (27, 1, rng.gen_range(2..20));
        // if i % 2 == 0 {
        //     (20, 1, rng.gen_range(2..10))
        // } else {
        //     (40, 2000, rng.gen_range(2100..20000))
        // };
        let call_config = CallConfigRequest {
            wasm_name: "fib.wasm".to_owned(),
            task_unique_name: format!("fib_abcd{}", i),
            export_func: "fib_r".to_owned(),
            param_type: "i32".to_owned(),
            params: vec![num.to_string()],
            results_length: "1".to_owned(),
            expected_execution_time: t1.to_string(),
            expected_deadline: t2.to_string(),
            //expected_deadline: rng.gen_range(2100..20000).to_string(),
        };
        cfgs.push((client, call_config));
    }
    let client = Client::new();
    let config = RegisterConfig::new(
        "/home/zhanghao/dev/hyper-scheduler/examples/fib.wasm",
        "fib.wasm",
    );
    let _ = client.init(&config).await;

    let mut tasks = Vec::new();
    for cfg in cfgs {
        let task = tokio::spawn(req(cfg.0, cfg.1));
        tasks.push(task);
    }

    tracing::info!("spawn task");

    for task in tasks {
        let _ = task.await;
    }

    let client = Client::new();
    let _ = client.get_latency().await;
}

async fn req(client: Client, mut cfg: CallConfigRequest) {
    for i in 0..200 {
        cfg.task_unique_name.push_str(&format!("_{}", i));
        let _ = client.call(&cfg).await;
    }
}
