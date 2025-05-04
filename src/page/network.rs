//! This module uses a tightly coupled implementation to enable fast iteration.
//! For a more ergonomic and decoupled approach, see the example in `prototype_mixed_dispatch.rs`.

use std::sync::atomic::{AtomicU64, Ordering};
use crate::shell::AppMessage;
use anyhow::Result;
use tracing::trace;

const IMG_STR_0: &str = "iVBORw0KGgoAAAANSUhEUgAAAGQAAAAyCAMAAACd646MAAAAP1BMVEUAAAAAOnJjndUIQnpRi8OBu/N8tu4qZJwMRn6Lxf0KRHxSjMRalMwoYppmoNhrpd01b6cgWpI8dq4tZ58WUIhMzA4eAAAAAXRSTlMAQObYZgAAAapJREFUeJzsmM1yhCAMgJN1RmU86Pj+D9vpAiHEwAaxTA+bQ9WK+fKLZuEfysuyaO1kvAyUde2lWBZxBvbxTII4hDKA8YfiqncRnwiic2UKBomXbZp53TmAs8QAKgdPmxo4ooPOU6VEDwIEAKZpskNkB+mepDBhPJ8IDI/kSoXEI0tWZ2VoEIqdT1fC3QWR8QpDGHLfFyquC0QylMfsECRCyoU/R10hxa0hgpiUstAjQsp6vh4yRkuuqIgAgTWmVIOpQLg/NaX6Jd8DGAxSJMOdgq4SJSslBLkTgPCQ940/Fl+diELxWw5Rryz+6ZD5+OEjwK9w3KnjODQIUBEiwJwZ+v5X7SPg98H4GggWlzxJV/M8i2iw3C8FjAPuff642FL8X8lgUVkWlZLMELVISZAk0BhQ84SZq9JZ0V4oBqZOCvVJHY+8waFCsFLCyitW06D3+QMvA4u2MmZvJldMLmxm+/6ZcghFrWZZPIk7QUBUGFs7PlGM67ath2KVEYyvfOWG1Ie1hxiVYY2ke86wMIbM72Mow38l4N8ULWNoE4NB2obdJspPAAAA//9aeATJZ1KZSAAAAABJRU5ErkJggg==";
const IMG_STR_1: &str = "iVBORw0KGgoAAAANSUhEUgAAAGQAAAAyCAMAAACd646MAAAAP1BMVEUAAAARfGBu2b0ahWkok3d+6c1NuJxBrJBl0LRPup4Qe19s17seiW0CbVFTvqIynYEch2t+6c0qlXk0n4NFsJTJ6I4rAAAAAXRSTlMAQObYZgAAAbtJREFUeJzsl93usyAMxtstmQkjWYj3f69vhiJtaeVDZt6Df0/GJuuvD31Ahbvicwfjc4HybKacXHtXGE+d4lvZkfGuUXSGr1Bwi0TpKYlQqgyglPnBU0fafAYgI+IPJIl83/xutphvM4CU/mW4nYIAU0ShaHn8dBt7v3ANgqTHVEm6lqqIn10bTeoQAwJhP5gb7dHAkEJALNEx8np/Ho8aJTYXRd081TF2wh2tSsjyI5WmCXHOYf5Djx+Q9/aotewUoOPT81SNyBY/nVqibCkn5S0qQ4UhiHkCE+DlUSmz00qMxcmOR9HK7Zv3XjKQVngGyV40HH84zUvBUCS3laSw9i7qxpEK6Bw500LybGUj0w/UgXyMuzfqhCzGBB1DQL5hiEHbKOUOyZy8O8KJZ9V465iSnhsdQlAadMYoHn+0v4hGh/2eVhzZjUpU/aXBYv8Wu6qWMJaAH3LLslyCgNFQZq6rSkjOvE8Q2YFoOnEcl89vcb788rETuu9eWjS8g1xjPO13kGkRAc2MMErpmBuCTXGDfIViM9w8ih13MP7iP4xXx9x1lPFqpqzrOky5gdERNyDG418AAAD//3/dBjfl+kg/AAAAAElFTkSuQmCC";

pub enum NetworkEvent {
    Placeholder,
    CaptchaFetched(u64, String),
    CaptchaFailed(u64),
    LoginSucceeded(u64, String, String),
    LoginFailed(u64),
    LoginTimeout(u64),
}

pub trait Network {
    fn fetch_captcha(
        &mut self,
        timeout: u32,
        map_function: Box<dyn FnOnce(NetworkEvent) -> AppMessage>,
    ) -> Result<u64>;
    fn login(
        &mut self,
        username: String,
        password: String,
        captcha: String,
        timeout: u32,
        map_function: Box<dyn FnOnce(NetworkEvent) -> AppMessage>,
    ) -> Result<u64>;
    fn cancel(&mut self, generation: u64) -> Result<()>;
}

pub struct FakeNetwork {
    generation: AtomicU64,
    message_tx: crossbeam_channel::Sender<AppMessage>,
}

impl FakeNetwork {
    pub fn new(message_tx: crossbeam_channel::Sender<AppMessage>) -> Self {
        Self {
            generation: AtomicU64::new(0),
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
        
        let generation = self.generation.fetch_add(1, Ordering::Relaxed);
        let message_tx = self.message_tx.clone();
        let message = match generation % 3 {
            0 => map_function(NetworkEvent::CaptchaFetched(generation, String::from(IMG_STR_0))),
            1 => map_function(NetworkEvent::CaptchaFetched(generation, String::from(IMG_STR_1))),
            2 => map_function(NetworkEvent::CaptchaFailed(generation)),
            _ => map_function(NetworkEvent::Placeholder),
        };
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(timeout as u64));
            message_tx.send(message).unwrap();
        });
        
        Ok(generation)
    }

    fn login(
        &mut self,
        username: String,
        password: String,
        captcha: String,
        timeout: u32,
        map_function: Box<dyn FnOnce(NetworkEvent) -> AppMessage>,
    ) -> Result<u64> {
        trace!("Logging in");
        
        let generation = self.generation.fetch_add(1, Ordering::Relaxed);
        let message_tx = self.message_tx.clone();
        let messages = match generation % 2 {
            0 => map_function(NetworkEvent::LoginFailed(generation)),
            1 => map_function(NetworkEvent::LoginSucceeded(generation, "addr".into(), "jwt".into())),
            _ => map_function(NetworkEvent::Placeholder)
        }; 
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(timeout as u64));
            message_tx.send(messages).unwrap();
        });
        
        Ok(generation)
    }

    fn cancel(&mut self, generation: u64) -> Result<()> {
        Ok(())
    }
}
