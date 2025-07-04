use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cert = tokio::fs::read("certs/dev_cert.pem").await?;
    let cert = reqwest::Certificate::from_pem(&cert)?;

    let client = reqwest::Client::builder()
        .add_root_certificate(cert)
        .no_proxy()
        .build()?;

    let worker: Arc<dyn HttpWorker> = Arc::new(RealHttpWorker);

    let worker_clone = worker.clone();
    let client_clone = client.clone();
    let task1 = tokio::spawn(async move {
        let content = worker_clone.fetch_captcha(client_clone.clone()).await.unwrap();
        println!("{}", content);
    });

    let worker_clone = worker.clone();
    let client_clone = client.clone();
    let task2 = tokio::spawn(async move {
        let content = worker_clone.fetch_captcha(client_clone).await.unwrap();
        println!("{}", content);
    });

    let x = tokio::join!(task1, task2);

    Ok(())
}

const BASE_URL: &str = "https:/localhost:8443/api/v1";
const CAPTCHA_SUFFIX: &str = "captcha";

fn endpoint_url(suffix: &str) -> String {
    format!("{}/{}", BASE_URL.trim_end_matches('/'), suffix.trim_start_matches('/'))
}

#[async_trait::async_trait]
trait HttpWorker: Send + Sync {
    async fn fetch_captcha(&self, client: reqwest::Client) -> anyhow::Result<String>;
}

struct RealHttpWorker;

#[async_trait::async_trait]
impl HttpWorker for RealHttpWorker {
    async fn fetch_captcha(&self, client: reqwest::Client) -> anyhow::Result<String> {
        let resp = client.get(endpoint_url(CAPTCHA_SUFFIX)).send().await?;
        Ok(format!("{:?}", resp))
    }
}

struct FakeHttpWorker;

#[async_trait::async_trait]
impl HttpWorker for FakeHttpWorker {
    async fn fetch_captcha(&self, _client: reqwest::Client) -> anyhow::Result<String> {
        Ok("text".to_string())
    }
}
