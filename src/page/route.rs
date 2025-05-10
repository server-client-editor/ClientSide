#[derive(Debug)]
pub enum Route {
    FatalPage,
    LobbyPage(String, String),
    ChatConnSuccess,
    ChatConnFailure,
    LoginPage,
    ShutdownPage,
    SignupPage,
}