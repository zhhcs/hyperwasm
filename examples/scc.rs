use std::time::Duration;

use clap::Parser;
use hyper_scheduler::{
    axum::{client::Client, CallConfigRequest, ClientArgs},
    runwasm::RegisterConfig,
};
use tokio::time::sleep;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
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
    let config = RegisterConfig::new(
        "/home/zhanghao/dev/hyper-scheduler/examples/fib.wasm",
        "fib.wasm",
    );
    let _ = client.init(&config).await;

    sleep(Duration::from_millis(100)).await;
    let mut cfgs = Vec::new();

    let client1 = Client::new(local_ip, port);
    let scc1 = CallConfigRequest {
        wasm_name: "fib.wasm".to_owned(),
        task_unique_name: "scc1".to_owned(),
        export_func: "fib_r".to_owned(),
        param_type: "i32".to_owned(),
        params: vec!["35".to_string()],
        results_length: "1".to_owned(),
        expected_execution_time: "35".to_owned(),
        expected_deadline: "50".to_owned(),
    };
    cfgs.push((client1, scc1));

    let client2 = Client::new(local_ip, port);
    let scc2 = CallConfigRequest {
        wasm_name: "fib.wasm".to_owned(),
        task_unique_name: "scc2".to_owned(),
        export_func: "fib_r".to_owned(),
        param_type: "i32".to_owned(),
        params: vec!["30".to_string()],
        results_length: "1".to_owned(),
        expected_execution_time: "4".to_owned(),
        expected_deadline: "15".to_owned(),
    };
    cfgs.push((client2, scc2));

    let client3 = Client::new(local_ip, port);
    let scc3 = CallConfigRequest {
        wasm_name: "fib.wasm".to_owned(),
        task_unique_name: "scc3".to_owned(),
        export_func: "fib_r".to_owned(),
        param_type: "i32".to_owned(),
        params: vec!["27".to_string()],
        results_length: "1".to_owned(),
        expected_execution_time: "1".to_owned(),
        expected_deadline: "5".to_owned(),
    };
    cfgs.push((client3, scc3));

    let client4 = Client::new(local_ip, port);
    let scc4 = CallConfigRequest {
        wasm_name: "fib.wasm".to_owned(),
        task_unique_name: "scc3".to_owned(),
        export_func: "fib_r".to_owned(),
        param_type: "i32".to_owned(),
        params: vec!["35".to_string()],
        results_length: "1".to_owned(),
        expected_execution_time: "35".to_owned(),
        expected_deadline: "50".to_owned(),
    };
    cfgs.push((client4, scc4));

    let mut tasks = Vec::new();
    for cfg in cfgs {
        let task = tokio::spawn(req(cfg.0, cfg.1));
        tasks.push(task);
    }

    tracing::info!("spawn task");

    for task in tasks {
        let _ = task.await;
    }

    let client = Client::new(local_ip, port);
    let _ = client.get_latency().await;
    let _ = client.get_status_by_name("scc1").await;
    let _ = client.get_status_by_name("scc2").await;
    let _ = client.get_status_by_name("scc3").await;
    let _ = client.get_status_by_name("scc4").await;
}

async fn req(client: Client, cfg: CallConfigRequest) {
    let _ = client.call_latency(&cfg).await;
}
