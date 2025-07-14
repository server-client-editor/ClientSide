use futures_util::{StreamExt};
use crate::protocol::network::{CaptchaData, TokenInfo, WithGeneration};
use crate::domain;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::BufReader;
use std::sync::Arc;
use futures_util::SinkExt;
use futures_util::stream::{SplitSink, SplitStream};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async_tls_with_config, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http, Error, Message};
use tracing::{trace, warn};
use uuid::Uuid;
use crate::domain::ConversationId;
use crate::protocol::network::ws_message::{ClientToServer, ServerToClient, ChatContent, SendMessage};

const API_BASE_URL: &str = "https://127.0.0.1:8443/api/v1";
const CAPTCHA_SUFFIX: &str = "captcha";
const SIGNUP_SUFFIX: &str = "signup";
const LOGIN_SUFFIX: &str = "login";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptchaResponse {
    pub id: Uuid,
    pub image_base64: String,
    pub expire_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct SignupRequest {
    pub username: String,
    pub password: String,
    pub captcha_id: Uuid,
    pub captcha_answer: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignupResponse;

#[derive(Debug, Serialize)]
struct LoginRequest {
    pub username: String,
    pub password: String,
    pub captcha_id: Uuid,
    pub captcha_answer: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginResponse {
    pub user_id: domain::UserId,
    pub auth_tokens: domain::AuthTokens,
}

#[async_trait::async_trait]
pub trait HttpWorker: Send + Sync {
    async fn fetch_captcha(&self) -> anyhow::Result<CaptchaData>;
    async fn signup(
        &self,
        username: String,
        password: String,
        captcha_id: Uuid,
        captcha_answer: String,
    ) -> anyhow::Result<()>;
    async fn login(
        &self,
        username: String,
        password: String,
        captcha_id: Uuid,
        captcha_answer: String,
    ) -> anyhow::Result<TokenInfo>;

    fn clone_box(&self) -> Box<dyn HttpWorker>;
}

fn endpoint_url(suffix: &str) -> String {
    format!(
        "{}/{}",
        API_BASE_URL.trim_end_matches('/'),
        suffix.trim_start_matches('/')
    )
}

#[derive(Clone)]
pub struct RealHttpWorker {
    client: Client,
}

impl RealHttpWorker {
    pub fn new() -> Self {
        let cert = fs::read("certs/dev_cert.pem").expect("Failed to read certificate");
        let cert = reqwest::Certificate::from_pem(&cert).expect("Failed to parse cert");

        let client = Client::builder()
            .add_root_certificate(cert)
            .no_proxy()
            .build()
            .expect("Failed to build http client");
        Self { client }
    }
}

#[async_trait::async_trait]
impl HttpWorker for RealHttpWorker {
    async fn fetch_captcha(&self) -> anyhow::Result<CaptchaData> {
        let response = self.client.get(endpoint_url(CAPTCHA_SUFFIX)).send().await?;
        let response: CaptchaResponse = response.json().await?;
        let captcha_data = CaptchaData {
            id: response.id,
            image_base64: response.image_base64,
        };

        Ok(captcha_data)
    }

    async fn signup(
        &self,
        username: String,
        password: String,
        captcha_id: Uuid,
        captcha_answer: String,
    ) -> anyhow::Result<()> {
        let request = SignupRequest {
            username,
            password,
            captcha_id,
            captcha_answer,
        };

        let response = self
            .client
            .post(endpoint_url(SIGNUP_SUFFIX))
            .json(&request)
            .send()
            .await?;

        let _response: SignupResponse = response.json().await?;

        Ok(())
    }

    async fn login(
        &self,
        username: String,
        password: String,
        captcha_id: Uuid,
        captcha_answer: String,
    ) -> anyhow::Result<TokenInfo> {
        let request = LoginRequest {
            username,
            password,
            captcha_id,
            captcha_answer,
        };

        let response = self
            .client
            .post(endpoint_url(LOGIN_SUFFIX))
            .json(&request)
            .send()
            .await?;

        let response: LoginResponse = response.json().await?;

        let token_info = TokenInfo {
            user_id: response.user_id,
            access_token: response.auth_tokens.access_token,
        };

        Ok(token_info)
    }

    fn clone_box(&self) -> Box<dyn HttpWorker> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn HttpWorker> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

const WS_CHAT_URL: &str = "wss://127.0.0.1:8443/api/v1/chat";

#[async_trait::async_trait]
pub trait WsWorker: Send + Sync {
    async fn send_message(&self, message_seq: u64, conversation_id: ConversationId, content: String) -> anyhow::Result<()>;
}

pub struct RealWsWorker {
    pub generation: u64,
    pub to_sender: UnboundedSender<ClientToServer>,
    pub watcher_handle: JoinHandle<()>,
}

impl RealWsWorker {
    pub async fn try_new(generation: u64, access_token: String, from_receiver: UnboundedSender<WithGeneration<ServerToClient>>) -> anyhow::Result<Self> {
        // region Create connection
        let cert_file = &mut BufReader::new(fs::File::open("certs/dev_cert.pem")?);
        let certs = rustls_pemfile::certs(cert_file).collect::<Result<Vec<_>, _>>()?;

        let mut root_store = rustls::RootCertStore::empty();
        for cert in certs {
            root_store.add(cert)?
        }

        let _ = rustls::crypto::ring::default_provider().install_default();

        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let connector = tokio_tungstenite::Connector::Rustls(Arc::new(config));

        let url = url::Url::parse(WS_CHAT_URL)?;
        let mut request = url.into_client_request()?;
        request.headers_mut().insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_str(format!("Bearer {}", access_token).clone().as_str())?,
        );

        let (ws_stream, _) = connect_async_tls_with_config(request, None, false, Some(connector)).await?;
        let (mut to_server, mut from_server) = ws_stream.split();
        // endregion

        // region Create sender and receiver
        let (to_sender, from_app) = unbounded_channel();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let sender_handle = tokio::spawn(sender(from_app, to_server, shutdown_rx.clone()));
        let receiver_handle = tokio::spawn(receiver(generation, from_server, from_receiver, shutdown_rx));
        let watcher_handle = tokio::spawn(watcher(sender_handle, receiver_handle, shutdown_tx));
        // endregion

        Ok(Self { generation, to_sender, watcher_handle })
    }
}

// region helpers
async fn sender(
    mut from_app: UnboundedReceiver<ClientToServer>,
    mut to_server: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            Some(message) = from_app.recv() => {
                let _ = to_server.send(Message::Text(serde_json::to_string(&message).unwrap().into())).await;
            }
            _ = shutdown.changed() => break,
        }
    }
}

async fn receiver(
    generation: u64,
    mut from_server: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    mut from_receiver: UnboundedSender<WithGeneration<ServerToClient>>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            Some(message) = from_server.next() => {
                let message = match message {
                    Ok(Message::Text(body)) => body,
                    Ok(Message::Close(_)) => break,
                    Ok(_) => continue,
                    Err(_) => break,
                };

                match serde_json::from_str(&message) {
                    Ok(message) => {
                        let message = WithGeneration {
                            generation,
                            result: message,
                        };
                        trace!("Received message: {:?}", message);
                        let _ = from_receiver.send(message);
                    }
                    Err(_) => break,
                }
            }
            _ = shutdown.changed() => break,
        }
    }
}

async fn watcher(sender_handle: JoinHandle<()>, receiver_handle: JoinHandle<()>, shutdown: watch::Sender<bool>) {
    let _ = tokio::select! {
        result = sender_handle => {
            warn!("Sender task ended");
            let _ = shutdown.send(true);
        },
        result = receiver_handle => {
            warn!("Receiver task ended");
            let _ = shutdown.send(true);
        }
    };
}
// endregion

#[async_trait::async_trait]
impl WsWorker for RealWsWorker {
    async fn send_message(&self, message_seq: u64, conversation_id: ConversationId, content: String) -> anyhow::Result<()> {
        let message = ClientToServer::Send(SendMessage {
            message_seq,
            content: ChatContent { conversation_id, content },
        });
        self.to_sender.send(message)?;
        Ok(())
    }
}
