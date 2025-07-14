use crate::domain::{ConversationId, UserId};
use std::fmt::Debug;
use uuid::Uuid;

pub trait NetworkInterface {
    fn fetch_captcha(
        &mut self,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<CaptchaEvent>) + Send + Sync>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>) + Send + Sync>,
    ) -> anyhow::Result<u64>;
    fn signup(
        &mut self,
        username: String,
        password: String,
        captcha_id: Uuid,
        captcha_answer: String,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<SignupEvent>) + Send + Sync>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>) + Send + Sync>,
    ) -> anyhow::Result<u64>;
    fn login(
        &mut self,
        username: String,
        password: String,
        captcha_id: Uuid,
        captcha_answer: String,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<LoginEvent>) + Send + Sync>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>) + Send + Sync>,
    ) -> anyhow::Result<u64>;
    fn cancel(&mut self, generation: u64) -> anyhow::Result<()>;
    fn connect_chat(
        &mut self,
        address: String,
        jwt: String,
        msg_function: Box<dyn Fn(StreamMessage) + Send + Sync>,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<SessionEvent>) + Send + Sync>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>) + Send + Sync>,
    ) -> anyhow::Result<u64>;
    fn send_chat_message(
        &mut self,
        conversation_id: ConversationId,
        message: String,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<MessageEvent>) + Send + Sync>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>) + Send + Sync>,
    ) -> anyhow::Result<u64>;
}

pub type NetworkResult = Result<NetworkEvent, NetworkError>;

#[derive(Debug)]
pub struct WithGeneration<T> {
    pub generation: u64,
    pub result: T,
}

#[derive(Debug)]
pub enum NetworkError {
    Aborted,
    SysCancelled,
    UsrCancelled,
    Timeout,
}

#[derive(Debug)]
pub enum NetworkEvent {
    Captcha(CaptchaEvent),
    Signup(SignupEvent),
    Login(LoginEvent),
    Session(SessionEvent),
    Chat(MessageEvent),
}

#[derive(Debug)]
pub struct CaptchaEvent {
    pub result: Result<CaptchaData, CaptchaError>,
}

pub struct CaptchaData {
    pub id: Uuid,
    pub image_base64: String,
}

impl Debug for CaptchaData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptchaData")
            .field("id", &self.id)
            .field(
                "image_base64",
                &self.image_base64.chars().take(64).collect::<String>(),
            )
            .finish()
    }
}

#[derive(Debug)]
pub enum CaptchaError {
    FallbackError,
}

#[derive(Debug)]
pub struct SignupEvent {
    pub result: Result<(), SignupError>,
}

#[derive(Debug)]
pub enum SignupError {
    DuplicateName,
    WeakPassword,
    WrongCaptcha,
    FallbackError,
}

#[derive(Debug)]
pub struct LoginEvent {
    pub result: Result<TokenInfo, LoginError>,
}

#[derive(Debug)]
pub struct TokenInfo {
    pub user_id: UserId,
    pub access_token: String,
}

#[derive(Debug)]
pub enum LoginError {
    Unauthorized,
    WrongCaptcha,
    FallbackError,
}

#[derive(Debug)]
pub struct SessionEvent {
    pub result: Result<ChatMetaData, ChatConnError>,
}

#[derive(Debug)]
pub struct ChatMetaData;

#[derive(Debug)]
pub enum ChatConnError {
    FallbackError,
}

#[derive(Debug)]
pub struct MessageEvent {
    pub result: Result<MessageSent, MessageError>,
}

#[derive(Debug)]
pub struct MessageSent;

#[derive(Debug)]
pub enum MessageError {
    MissingSession,
    FallbackError,
}

#[derive(Debug)]
pub enum StreamMessage {
    Distribute(ChatMessage),
}

#[derive(Debug)]
pub struct ChatMessage {
    pub sender: UserId,
    pub conversation_id: ConversationId,
    pub content: String,
}
