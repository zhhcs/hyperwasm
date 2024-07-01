use clap::Parser;
use hyper_scheduler::axum::{server::Server, ServerArgs};

// cargo build --release --package hyper-scheduler --example server
// sudo ./target/release/examples/server --port 3001 --workers 2 --start-cpu 0 --timer-us 0000
// http://127.0.0.1:3000/status
fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    let args = ServerArgs::parse();
    tracing::info!(
        "Hyperwasm is listening on 0.0.0.0:{}, with {} workers running on CPUs {} to {}. The expiration time is {}",
        args.port,
        args.workers,
        args.start_cpu,
        args.start_cpu + args.workers + 2,
        args.timer_us
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        Server::start(args.port, args.workers, args.start_cpu, args.timer_us).await;
    });
}
