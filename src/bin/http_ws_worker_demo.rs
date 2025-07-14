use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;
use client_side::domain::ConversationId;
use client_side::protocol::network::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    // let worker = RealHttpWorker::new();
    //
    // match worker.fetch_captcha().await {
    //     Ok(captcha) => println!("{}\n{}", captcha.id, captcha.image_base64.chars().take(64).collect::<String>()),
    //     Err(e) => println!("{}", e),
    // }
    //
    // match worker.signup("testuser".to_string(), "testpass".to_string(), Uuid::nil(), "123456".to_string()).await {
    //     Ok(_) => println!("Success"),
    //     Err(_) => println!("Failed"),
    // }
    //
    // match worker.login("testuser".to_string(), "testpass".to_string(), Uuid::nil(), "123456".to_string()).await {
    //     Ok(token_info) => println!("{:?}", token_info),
    //     Err(e) => println!("{}", e),
    // }

    let (tx0, mut rx0) = unbounded_channel();
    let worker0 = RealWsWorker::try_new(0u64, "fake-access-token:testuser0".to_string(), tx0.clone()).await?;
    let message0 = ClientToServer::Send(SendMessage {
        message_seq: 0,
        content: ChatContent { conversation_id: ConversationId(Uuid::nil()), content: "Hello".to_string() },
    });

    let (tx1, mut rx1) = unbounded_channel();
    let worker1 = RealWsWorker::try_new(0u64, "fake-access-token:testuser1".to_string(), tx1.clone()).await?;
    let message1 = ClientToServer::Send(SendMessage {
        message_seq: 0,
        content: ChatContent { conversation_id: ConversationId(Uuid::nil()), content: "Hi".to_string() },
    });

    let _ = worker0.to_sender.send(message0)?;
    let _ = worker1.to_sender.send(message1)?;

    let recv0 = tokio::spawn(async move {
        if let Some(r) = rx0.recv().await {
            println!("{:?}", r);
        }
    });

    let recv1 = tokio::spawn(async move {
        if let Some(r) = rx1.recv().await {
            println!("{:?}", r);
        }
    });

    let _ = tokio::join!(recv0, recv1);

    Ok(())
}
