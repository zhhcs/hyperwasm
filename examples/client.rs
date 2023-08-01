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
    for i in 0..5 {
        let mut name = String::from("task");
        name.push_str(&i.to_string());
        let config = Config::new(
            &name,
            "/home/ubuntu/dev/hyper-scheduler/examples/add.wat",
            12,
            36,
            "add",
        );
        client.spawn_wasm(&config).await.unwrap();
    }
    client.get_status().await.unwrap();
    for i in 5..10 {
        let mut name = String::from("task");
        name.push_str(&i.to_string());
        let config = Config::new(
            &name,
            "/home/ubuntu/dev/hyper-scheduler/examples/add.wat",
            12,
            20,
            "add",
        );
        client.spawn_wasm(&config).await.unwrap();
    }
    client.get_status().await.unwrap();
}
