use std::any::Any;
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;

trait NetData: Any + Display {}
impl<T: Any + Display> NetData for T {}

struct Network<Type: NetData> {
    inner: NetworkInternal,
    _marker: PhantomData<Type>,
}

impl<Type: NetData> Network<Type> {
    fn new() -> Self {
        Self {
            inner: NetworkInternal::new(),
            _marker: Default::default(),
        }
    }

    fn send(&mut self, data: Type) {
        self.inner.send(Box::new(data));
    }

    fn recv(&mut self) -> Option<Type> {
        self.inner.recv().and_then(|boxed| {
            (boxed as Box<dyn Any>)
                .downcast::<Type>()
                .ok()
                .map(|typed| *typed)
        })
    }
}

struct NetworkInternal {
    data: Option<Box<dyn NetData>>,
}

impl NetworkInternal {
    fn new() -> Self {
        Self { data: None }
    }

    fn send(&mut self, data: Box<dyn NetData>) {
        println!("Sending {}", data);
        self.data = Some(data);
    }

    fn recv(&mut self) -> Option<Box<dyn NetData>> {
        self.data.take()
    }
}

enum Message {
    Hello,
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::Hello => {
                write!(f, "Hello")
            }
        }
    }
}

struct App {
    network: Network<Message>,
}

impl App {
    fn new() -> Self {
        Self {
            network: Network::new(),
        }
    }
}

fn main() {
    let mut app = App::new();
    app.network.send(Message::Hello);
    let message = app.network.recv().unwrap();
    println!("Claimed data: {}", message);
}
