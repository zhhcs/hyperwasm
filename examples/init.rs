use std::thread;
use std::time::Duration;

use hyper_scheduler::axum::client::Client;
use hyper_scheduler::runwasm::Config;

// cargo build --release --package hyper-scheduler --example init
// ./target/release/examples/init
#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");
    let client = Client::new();
    let config = Config::new(
        "hello",
        "/home/ubuntu/dev/hyper-scheduler/examples/fib32.wasm",
        0,
        0,
        "fib32",
    );
    client.init(&config).await.unwrap();
    thread::sleep(Duration::from_millis(2000));
    let config = Config::new(
        "hello",
        "/home/ubuntu/dev/hyper-scheduler/examples/fib33.wasm",
        0,
        0,
        "fib33",
    );
    client.init(&config).await.unwrap();
}
