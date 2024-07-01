use super::{CallConfigRequest, TestRequest};
use crate::runwasm::RegisterConfig;
/**
 * Client
 * 一些客户端的接口
 * 与Server中的几乎对应
 */
pub struct Client {
    client: reqwest::Client,
    local_ip: String,
    port: u16,
}

impl Client {
    pub fn new(local_ip: &String, port: u16) -> Client {
        Client {
            client: reqwest::Client::new(),
            local_ip: local_ip.to_string(),
            port,
        }
    }

    pub async fn init(&self, config: &RegisterConfig) -> Result<(), reqwest::Error> {
        let url = format!("http://{}:{}/init", self.local_ip, self.port);
        let json = serde_json::to_string(config).unwrap();
        let start = std::time::Instant::now();
        let resp = self
            .client
            .get(url)
            .header("Content-Type", "application/json")
            .body(json.clone())
            .send()
            .await?;
        let end = std::time::Instant::now();
        tracing::info!("{:?}", end - start);
        let body = resp.text().await?;
        tracing::info!("Response: {}", body);
        Ok(())
    }

    pub async fn test(&self, test_request: TestRequest) -> Result<(), reqwest::Error> {
        let url = format!("http://{}:{}/test", self.local_ip, self.port);
        let json = serde_json::to_string(&test_request).unwrap();
        let resp = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .body(json.clone())
            .send()
            .await?;

        let body = resp.text().await?;
        tracing::info!("Response: {}", body);
        Ok(())
    }

    pub async fn call(&self, call_config: &CallConfigRequest) -> Result<(), reqwest::Error> {
        // tracing::info!("call !");
        // let ddl = call_config.expected_deadline.clone();
        let url = format!("http://{}:{}/call", self.local_ip, self.port);
        let json = serde_json::to_string(call_config).unwrap();
        // let start = std::time::Instant::now();
        let resp = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .body(json.clone())
            .send()
            .await?;

        // let end = std::time::Instant::now();
        // tracing::info!("call response: {:?}", end - start);
        let body = resp.text().await?;
        tracing::info!("res: {}", body);
        Ok(())
    }

    pub async fn call_latency(
        &self,
        call_config: &CallConfigRequest,
    ) -> Result<std::time::Duration, reqwest::Error> {
        // tracing::info!("call !");
        // let ddl = call_config.expected_deadline.clone();
        let url = format!("http://{}:{}/call", self.local_ip, self.port);
        let json = serde_json::to_string(call_config).unwrap();
        let start = std::time::Instant::now();
        let resp = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .body(json.clone())
            .send()
            .await?;

        let end = std::time::Instant::now();
        tracing::info!("call response: {:?}", end - start);
        let _body = resp.text().await?;
        // tracing::info!("res: {}", body);
        Ok(end - start)
    }

    pub async fn get_status_by_name(&self, uname: &str) -> Result<(), reqwest::Error> {
        let url = format!("http://{}:{}/uname", self.local_ip, self.port);

        let resp = self
            .client
            .get(url)
            .query(&[("uname", uname)])
            .send()
            .await?;

        let body = resp.text().await?;
        tracing::info!("Response: {}", body);
        Ok(())
    }

    pub async fn get_status(&self) -> Result<(), reqwest::Error> {
        let url = format!("http://{}:{}/status", self.local_ip, self.port);

        let resp = self.client.get(url).send().await?;

        let body = resp.text().await?;
        tracing::info!("Response: {}", body);
        Ok(())
    }

    pub async fn get_latency(&self) -> Result<(), reqwest::Error> {
        let url = format!("http://{}:{}/warm-start-latency", self.local_ip, self.port);
        let resp = self.client.get(url).send().await?;

        let body = resp.text().await?;
        tracing::info!("Response: {}", body);
        Ok(())
    }
}
