use hyper_scheduler::axum::client::Client;
use hyper_scheduler::runwasm::Config;

// cargo build --package hyper-scheduler --example client
//
#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");
    let client = Client::new();
    let config = Config::new(
        "/home/ubuntu/dev/hyper-scheduler/examples/add.wat",
        12,
        20,
        "add",
    );
    client.start(&config).await.unwrap();
}
