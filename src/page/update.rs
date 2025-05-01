pub trait Update<MessageType> {
    fn update_one(&mut self, message: MessageType);
}