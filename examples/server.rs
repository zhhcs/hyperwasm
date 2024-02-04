use hyper_scheduler::axum::server::Server;

// cargo build --release --package hyper-scheduler --example server
// sudo ./target/release/examples/server
// http://127.0.0.1:3000/status
fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");

    let rt = tokio::runtime::Builder::new_current_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        Server::start(2).await;
    });
}
