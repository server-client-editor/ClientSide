//! This module uses a tightly coupled implementation to enable fast iteration.
//! For a more ergonomic and decoupled approach, see the example in `prototype_mixed_dispatch.rs`.

use crate::shell::AppMessage;
use anyhow::Result;

pub enum NetworkEvent {
    Placeholder,
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

pub struct NetworkImpl {}

impl NetworkImpl {
    pub fn new() -> Self {
        Self {}
    }
}

impl Network for NetworkImpl {
    fn fetch_captcha(
        &mut self,
        timeout: u32,
        map_function: Box<dyn FnOnce(NetworkEvent) -> AppMessage>,
    ) -> Result<u64> {
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
