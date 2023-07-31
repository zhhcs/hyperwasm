use std::{thread, time::Duration};

use crate::runwasm::Config;

pub struct Client {
    client: reqwest::Client,
}

impl Client {
    pub fn new() -> Client {
        Client {
            client: reqwest::Client::new(),
        }
    }

    pub async fn start(&self, config: &Config) -> Result<(), reqwest::Error> {
        let json = serde_json::to_string(config).unwrap();

        //
        for _ in 0..2 {
            let resp = self
                .client
                .get("http://127.0.0.1:3000")
                .header("Content-Type", "application/json")
                .body(json.clone())
                .send()
                .await?;

            let body = resp.text().await?;

            tracing::info!("Response: {}", body);
            thread::sleep(Duration::from_millis(10));
        }

        //
        let uname = serde_json::to_string("task1").unwrap();
        let resp = self
            .client
            .get("http://127.0.0.1:3000/uname")
            .header("Content-Type", "application/json")
            .body(uname.clone())
            .send()
            .await?;

        let body = resp.text().await?;

        tracing::info!("Response: {}", body);
        thread::sleep(Duration::from_millis(10));

        //
        let config2 = Config::new(
            "task2",
            "/home/ubuntu/dev/hyper-scheduler/examples/add.wat",
            12,
            20,
            "add",
        );
        let json = serde_json::to_string(&config2).unwrap();
        let resp = self
            .client
            .get("http://127.0.0.1:3000")
            .header("Content-Type", "application/json")
            .body(json.clone())
            .send()
            .await?;

        let body = resp.text().await?;

        tracing::info!("Response: {}", body);

        //
        let uname = serde_json::to_string("task2").unwrap();
        let resp = self
            .client
            .get("http://127.0.0.1:3000/uname")
            .header("Content-Type", "application/json")
            .body(uname.clone())
            .send()
            .await?;

        let body = resp.text().await?;

        tracing::info!("Response: {}", body);

        //
        let resp = self
            .client
            .get("http://127.0.0.1:3000/status")
            .send()
            .await?;

        let body = resp.text().await?;

        tracing::info!("Response: {}", body);
        thread::sleep(Duration::from_millis(10));

        Ok(())
    }
}
