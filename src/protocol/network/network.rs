use uuid::Uuid;
use crate::domain::{ConversationId, UserId};

pub trait NetworkInterface {
    fn fetch_captcha(
        &mut self,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<CaptchaEvent>)>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>)>,
    ) -> anyhow::Result<u64>;
    fn signup(
        &mut self,
        username: String,
        password: String,
        captcha_id: Uuid,
        captcha_answer: String,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<SignupEvent>)>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>)>,
    ) -> anyhow::Result<u64>;
    fn login(
        &mut self,
        username: String,
        password: String,
        captcha_id: Uuid,
        captcha_answer: String,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<LoginEvent>)>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>)>,
    ) -> anyhow::Result<u64>;
    fn cancel(&mut self, generation: u64) -> anyhow::Result<()>;
    fn connect_chat(
        &mut self,
        address: String,
        jwt: String,
        timeout: u32,
        map_function: Box<dyn FnOnce(NetworkEvent)>,
    ) -> anyhow::Result<u64>;
    fn send_chat_message(
        &mut self,
        chat_generation: u64,
        conversation_id: ConversationId,
        message: String,
        timeout: u32,
        map_function: Box<dyn FnOnce(NetworkEvent)>,
    ) -> anyhow::Result<u64>;
}

pub type NetworkResult = Result<NetworkEvent, NetworkError>;

pub struct WithGeneration<T> {
    pub generation: u64,
    pub result: T,
}

pub enum NetworkError {
    Aborted,
    SysCancelled,
    UsrCancelled,
    Timeout,
}

pub enum NetworkEvent {
    Captcha(CaptchaEvent),
    Signup(SignupEvent),
    Login(LoginEvent),
}

#[derive(Debug)]
pub struct CaptchaEvent {
    pub result: Result<CaptchaData, CaptchaError>
}

#[derive(Debug)]
pub struct CaptchaData {
    pub id: Uuid,
    pub image_base64: String,
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
