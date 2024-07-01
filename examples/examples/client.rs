use clap::Parser;
use hyper_scheduler::{
    axum::{client::Client, CallConfigRequest, ClientArgs},
    runwasm::RegisterConfig,
};

// cargo build --release --package hyper-scheduler --example client
// sudo ./target/release/examples/client --local-ip 127.0.0.1 --port 3001
#[tokio::main(flavor = "multi_thread", worker_threads = 15)]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");
    let args = ClientArgs::parse();
    let local_ip = &args.local_ip;
    let port = args.port;
    let mut cfgs = Vec::new();

    for i in 0..1000 {
        let client = Client::new(local_ip, port);
        let (num, t1, t2) = if i % 2 == 0 {
            (30, 5, 100)
        } else {
            (27, 2, 20)
        };
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

    // 部署服务
    let client = Client::new(local_ip, port);
    let config = RegisterConfig::new(
        "/home/zhanghao/dev/hyper-scheduler/examples/fib.wasm",
        "fib.wasm",
    );
    let _ = client.init(&config).await;

    // 函数调用
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
}

async fn req(client: Client, mut cfg1: CallConfigRequest) {
    for i in 0..2000 {
        cfg1.task_unique_name.push_str(&format!("_{}", i));
        let _ = client.call(&cfg1).await;
    }
}
