//! This module uses a tightly coupled implementation to enable fast iteration.
//! For a more ergonomic and decoupled approach, see the example in `prototype_mixed_dispatch.rs`.

use crate::shell::AppMessage;
use anyhow::Result;
use tracing::trace;

pub enum NetworkEvent {
    Placeholder,
    CaptchaFetched(u64, String),
    CaptchaFailed(u64),
}

pub trait Network {
    fn fetch_captcha(
        &mut self,
        timeout: u32,
        map_function: Box<dyn FnOnce(NetworkEvent) -> AppMessage>,
    ) -> Result<u64>;
    fn login(
        &mut self,
        timeout: u32,
        map_function: Box<dyn FnOnce(NetworkEvent) -> AppMessage>,
    ) -> Result<u64>;
    fn cancel(&mut self, generation: u64) -> Result<()>;
}

pub struct FakeNetwork {
    message_tx: crossbeam_channel::Sender<AppMessage>,
}

impl FakeNetwork {
    pub fn new(message_tx: crossbeam_channel::Sender<AppMessage>) -> Self {
        Self {
            message_tx,
        }
    }
}

impl Network for FakeNetwork {
    fn fetch_captcha(
        &mut self,
        timeout: u32,
        map_function: Box<dyn FnOnce(NetworkEvent) -> AppMessage>,
    ) -> Result<u64> {
        trace!("Fetching captcha");
        Ok(0)
    }

    fn login(
        &mut self,
        timeout: u32,
        map_function: Box<dyn FnOnce(NetworkEvent) -> AppMessage>,
    ) -> Result<u64> {
        Ok(0)
    }

    fn cancel(&mut self, generation: u64) -> Result<()> {
        Ok(())
    }
}
