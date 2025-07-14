use once_cell::sync::Lazy;
use crossbeam_channel::{Sender, Receiver};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;
use client_side::domain::ConversationId;
use client_side::protocol::network::*;

static SHUTDOWN_CHANNEL: Lazy<(Sender<()>, Receiver<()>)> = Lazy::new(|| crossbeam_channel::unbounded());

fn print_error(network_error: WithGeneration<NetworkError>) {
    println!("{:?}", network_error.result);
    let _ = SHUTDOWN_CHANNEL.0.send(());
}

fn print_captcha(captcha_event: WithGeneration<CaptchaEvent>) {
    println!("{:?}", captcha_event.result);
    let _ = SHUTDOWN_CHANNEL.0.send(());
}

fn print_signup(signup_event: WithGeneration<SignupEvent>) {
    println!("{:?}", signup_event.result);
    let _ = SHUTDOWN_CHANNEL.0.send(());
}

fn print_login(login_event: WithGeneration<LoginEvent>) {
    println!("{:?}", login_event.result);
    let _ = SHUTDOWN_CHANNEL.0.send(());
}

fn print_session(session_event: WithGeneration<SessionEvent>) {
    println!("{:?}", session_event.result);
    let _ = SHUTDOWN_CHANNEL.0.send(());
}

fn print_stream(stream_message: StreamMessage) {
    println!("{:?}", stream_message);
    let _ = SHUTDOWN_CHANNEL.0.send(());
}

fn print_send(message_event: WithGeneration<MessageEvent>) {
    println!("{:?}", message_event.result);
    let _ = SHUTDOWN_CHANNEL.0.send(());
}

fn main() {
    // tracing_subscriber::fmt()
    //     .with_env_filter(EnvFilter::new("client_side=trace,client_side::protocol::network::worker=off"))
    //     .init();

    let mut network0 = NetworkImpl::try_new().unwrap();
    if let Err(e) = network0.cancel(0) {
        println!("{}", e);
    }
    let generation = network0.fetch_captcha(1000, Box::new(print_captcha), Box::new(print_error));
    println!("{}", network0.cancel(generation.unwrap()).is_ok());

    let _ = network0.fetch_captcha(1000, Box::new(print_captcha), Box::new(print_error));
    let _ = network0.signup("testuser".to_string(), "testpass".to_string(), Uuid::nil(), "123456".to_string(), 1000, Box::new(print_signup), Box::new(print_error));
    let _ = network0.login("testuser".to_string(), "testpass".to_string(), Uuid::nil(), "123456".to_string(), 1000, Box::new(print_login), Box::new(print_error));

    let _ = network0.connect_chat("".to_string(), "fake-access-token:testuser0".to_string(), Box::new(print_stream), 1000, Box::new(print_session), Box::new(print_error));

    let mut network1 = NetworkImpl::try_new().unwrap();
    let _ = network1.connect_chat("".to_string(), "fake-access-token:testuser1".to_string(), Box::new(print_stream), 1000, Box::new(print_session), Box::new(print_error));

    let mut network2 = NetworkImpl::try_new().unwrap();
    let _ = network2.connect_chat("".to_string(), "fake-access-token:testuser2".to_string(), Box::new(print_stream), 1000, Box::new(print_session), Box::new(print_error));

    std::thread::sleep(std::time::Duration::from_millis(2000));

    let _ = network0.send_chat_message(ConversationId(Uuid::nil()), "Hello".to_string(), 1000, Box::new(print_send), Box::new(print_error));
    let _ = network1.send_chat_message(ConversationId(Uuid::nil()), "Hi".to_string(), 1000, Box::new(print_send), Box::new(print_error));

    let _ = network0.send_chat_message(ConversationId(Uuid::nil()), "Hello".to_string(), 1000, Box::new(print_send), Box::new(print_error));
    let _ = network1.send_chat_message(ConversationId(Uuid::nil()), "Hi".to_string(), 1000, Box::new(print_send), Box::new(print_error));

    let _ = network2.send_chat_message(ConversationId(Uuid::nil()), "Hello from group".to_string(), 1000, Box::new(print_send), Box::new(print_error));

    // 3 REST + 3 Connection + 5 ACK + 6 Distribute
    for _i in 0..(3 + 3 + 5 + 6) {
        let _result = SHUTDOWN_CHANNEL.1.recv();
    }
}
