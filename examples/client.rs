use hyper_scheduler::axum::client::Client;
use hyper_scheduler::runwasm::Config;

// cargo build --package hyper-scheduler --example client
//
#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");
    let client = Client::new();
    for i in 1..6 {
        let mut name = String::from("task");
        name.push_str(&i.to_string());
        let config = Config::new(
            &name,
            "/home/ubuntu/dev/hyper-scheduler/examples/add.wat",
            12,
            20,
            "add",
        );
        client.spawn(&config).await.unwrap();
    }
    client.get_status().await.unwrap();
    client.get_status_by_name("task1").await.unwrap();
    client.get_completed_status().await.unwrap();
}
