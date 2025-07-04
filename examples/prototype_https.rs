use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cert = tokio::fs::read("certs/dev_cert.pem").await?;
    let cert = reqwest::Certificate::from_pem(&cert)?;

    let client = reqwest::Client::builder()
        .add_root_certificate(cert)
        .no_proxy()
        .build()?;

    let res = client.get("https://localhost:8443").send().await;
    match res {
        Ok(r) => println!("Response: {}", r.text().await?),
        Err(e) => {
            eprintln!("Request failed: {}", e);
            if let Some(source) = e.source() {
                eprintln!("Caused by: {}", source);
            }
        }
    }


    Ok(())
}