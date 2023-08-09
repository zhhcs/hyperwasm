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

    pub async fn init(&self, config: &Config) -> Result<(), reqwest::Error> {
        let json = serde_json::to_string(config).unwrap();
        let resp = self
            .client
            .get("http://127.0.0.1:3000/init")
            .header("Content-Type", "application/json")
            .body(json.clone())
            .send()
            .await?;

        let body = resp.text().await?;
        tracing::info!("Response: {}", body);
        Ok(())
    }

    pub async fn call(&self, config: &Config, url: &str) -> Result<(), reqwest::Error> {
        let json = serde_json::to_string(config).unwrap();
        let resp = self
            .client
            .get(url)
            .header("Content-Type", "application/json")
            .body(json.clone())
            .send()
            .await?;

        let body = resp.text().await?;
        tracing::info!("Response: {}", body);
        Ok(())
    }

    pub async fn get_status_by_name(&self, uname: &str) -> Result<(), reqwest::Error> {
        let resp = self
            .client
            .get("http://127.0.0.1:3000/uname")
            .query(&[("uname", uname)])
            .send()
            .await?;

        let body = resp.text().await?;
        tracing::info!("Response: {}", body);
        Ok(())
    }

    pub async fn get_status(&self) -> Result<(), reqwest::Error> {
        let resp = self
            .client
            .get("http://127.0.0.1:3000/status")
            .send()
            .await?;

        let body = resp.text().await?;
        tracing::info!("Response: {}", body);
        Ok(())
    }

    // pub async fn get_completed_status(&self) -> Result<(), reqwest::Error> {
    //     let resp = self
    //         .client
    //         .get("http://127.0.0.1:3000/completed")
    //         .send()
    //         .await?;

    //     let body = resp.text().await?;
    //     tracing::info!("Response: {}", body);
    //     Ok(())
    // }
}
