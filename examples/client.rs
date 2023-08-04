use hyper_scheduler::axum::client::Client;
use hyper_scheduler::runwasm::Config;

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
    for i in 0..2 {
        let mut name = String::from("task32");
        name.push_str(&i.to_string());
        let config = Config::new(
            &name,
            "/home/ubuntu/dev/hyper-scheduler/examples/fib32.wasm",
            10,
            60 - i * 2,
            "fib32",
        );
        client
            .call(&config, "http://127.0.0.1:3001/fib32")
            .await
            .unwrap();
    }

    for i in 0..2 {
        let mut name = String::from("task33");
        name.push_str(&i.to_string());
        let config = Config::new(
            &name,
            "/home/ubuntu/dev/hyper-scheduler/examples/fib33.wasm",
            15,
            55 - i * 5,
            "fib33",
        );
        client
            .call(&config, "http://127.0.0.1:3002/fib33")
            .await
            .unwrap();
    }

    for i in 2..5 {
        let mut name = String::from("task32");
        name.push_str(&i.to_string());
        let config = Config::new(
            &name,
            "/home/ubuntu/dev/hyper-scheduler/examples/fib32.wasm",
            10,
            40 - i * 2,
            "fib32",
        );
        client
            .call(&config, "http://127.0.0.1:3001/fib32")
            .await
            .unwrap();
    }

    for i in 2..5 {
        let mut name = String::from("task33");
        name.push_str(&i.to_string());
        let config = Config::new(
            &name,
            "/home/ubuntu/dev/hyper-scheduler/examples/fib33.wasm",
            15,
            75 - i * 3,
            "fib33",
        );
        client
            .call(&config, "http://127.0.0.1:3002/fib33")
            .await
            .unwrap();
    }
}
