// use std::time::Duration;

use hyper_scheduler::{
    axum::{client::Client, CallConfigRequest},
    runwasm::RegisterConfig,
};
// use tokio::time::sleep;
use rand::Rng;

// cargo build --release --package hyper-scheduler --example client
// sudo ./target/release/examples/client
#[tokio::main(flavor = "multi_thread", worker_threads = 5)]
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
        let rand = vec![rng.gen_range(0..100); 500];
        let client = Client::new();
        let (num, t1, t2) = (27, 3, 20);
        let (num2, t1_2, t2_2) = (30, 9, 100);

        // if i % 9 == 0 {
        //     (30, 5, 100)
        // } else {
        //     (27, 1, 20)
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
        let call_config2 = CallConfigRequest {
            wasm_name: "fib.wasm".to_owned(),
            task_unique_name: format!("fib_abcd{}", i),
            export_func: "fib_r".to_owned(),
            param_type: "i32".to_owned(),
            params: vec![num2.to_string()],
            results_length: "1".to_owned(),
            expected_execution_time: t1_2.to_string(),
            expected_deadline: t2_2.to_string(),
            //expected_deadline: rng.gen_range(2100..20000).to_string(),
        };
        cfgs.push((client, call_config, call_config2, rand));
    }
    let client = Client::new();
    let config = RegisterConfig::new(
        "/home/zhanghao/dev/hyper-scheduler/examples/fib.wasm",
        "fib.wasm",
    );
    let _ = client.init(&config).await;

    let mut tasks = Vec::new();
    for cfg in cfgs {
        let task = tokio::spawn(req(cfg.0, cfg.1, cfg.2, cfg.3));
        tasks.push(task);
    }

    tracing::info!("spawn task");

    for task in tasks {
        let _ = task.await;
    }

    let client = Client::new();
    let _ = client.get_latency().await;
}

async fn req(
    client: Client,
    mut cfg1: CallConfigRequest,
    mut cfg2: CallConfigRequest,
    rand: Vec<i32>,
) {
    for i in 0..200 {
        cfg1.task_unique_name.push_str(&format!("_{}", i));
        let _ = client.call(&cfg1).await;
        // if rand[i % 500] > 49 {
        //     cfg1.task_unique_name.push_str(&format!("_{}", i));
        //     let _ = client.call(&cfg1).await;
        // } else {
        //     cfg2.task_unique_name.push_str(&format!("_{}", i));
        //     let _ = client.call(&cfg2).await;
        // }
    }
}
