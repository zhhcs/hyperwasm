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

        Ok(())
    }
}
