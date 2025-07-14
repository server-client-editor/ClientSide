use crate::domain::ConversationId;
use crate::protocol::network::{worker::*, ws_message::*, *};
use dashmap::DashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::{Mutex, Notify};
use tokio::task::{AbortHandle, JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, debug_span, error, info, info_span, trace, warn, Instrument, Span};
use uuid::Uuid;

static INSTANCE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TaskRecord {
    pub abort_handle: AbortHandle,
    pub callback: Box<dyn FnOnce(WithGeneration<NetworkResult>) + Send + Sync>,
}

struct SessionRecord {
    pub ws_worker: Arc<Box<dyn WsWorker>>,
    pub task_handle: JoinHandle<()>,
    pub callback: Arc<Box<dyn Fn(StreamMessage) + Send + Sync>>,
}

pub struct NetworkImpl {
    span: Span,

    generation: AtomicU64,
    task_records: Arc<DashMap<u64, TaskRecord>>,
    cancellation_token: CancellationToken,
    runtime_handle: tokio::runtime::Handle,
    join_set: tokio::task::JoinSet<()>,

    result_tx: UnboundedSender<WithGeneration<NetworkResult>>,
    runtime_thread_handle: std::thread::JoinHandle<()>,

    http_worker: Box<dyn HttpWorker>,

    session_record: Arc<Mutex<Option<SessionRecord>>>,
    message_id: AtomicU64,
    message_buffer: Arc<DashMap<u64, Arc<Notify>>>,
}

impl NetworkImpl {
    pub fn try_new() -> anyhow::Result<Self> {
        let id = INSTANCE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let span = debug_span!("NetworkImpl", instance_id = id);


        let generation = AtomicU64::new(0);
        let task_records = Arc::new(DashMap::new());
        let cancellation_token = CancellationToken::new();

        let (result_tx, result_rx) = unbounded_channel::<WithGeneration<NetworkResult>>();
        let tokio_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let runtime_handle = tokio_runtime.handle().clone();

        let span_clone = span.clone();
        let records_clone = task_records.clone();
        let cancellation_token_clone = cancellation_token.clone();
        let runtime_thread_handle = std::thread::spawn(move || {
            tokio_runtime.block_on(Self::send_result_back(
                records_clone,
                cancellation_token_clone,
                result_rx,
            ).instrument(span_clone))
        });

        let join_set = tokio::task::JoinSet::new();

        let http_worker = Box::new(RealHttpWorker::new());
        let session_record = Arc::new(Mutex::new(None));
        let message_id = AtomicU64::new(0);
        let message_buffer = Arc::new(DashMap::new());

        Ok(Self {
            span,
            generation,
            task_records,
            cancellation_token,
            runtime_handle,
            join_set,
            result_tx,
            runtime_thread_handle,
            http_worker,
            session_record,
            message_id,
            message_buffer,
        })
    }

    async fn send_result_back(
        task_records: Arc<DashMap<u64, TaskRecord>>,
        cancellation_token: CancellationToken,
        mut result_rx: UnboundedReceiver<WithGeneration<NetworkResult>>,
    ) {
        loop {
            tokio::select! {
                biased;
                _ = cancellation_token.cancelled() => {
                    let undone = result_rx.len();
                    warn!("Unhandled messages when shutting down: {}", undone);
                }
                result = result_rx.recv() => match result {
                    None => break,
                    Some(with_generation) => {
                        let generation = with_generation.generation;
                        trace!("Retrieving task callback: {}", generation);
                        if let Some((_, TaskRecord {abort_handle, callback})) = task_records.remove(&generation) {
                            trace!("Executing task callback: {}", generation);
                            abort_handle.abort();
                            let callback = std::panic::AssertUnwindSafe(move || callback(with_generation));
                            if let Err(e) = std::panic::catch_unwind(callback) {
                                error!("Map function for {} panicked: {:?}", generation, e);
                            }
                        }
                    }
                }
            }
        }
    }

    async fn send_message_back(
        notify: Arc<Notify>,
        session_record: Arc<Mutex<Option<SessionRecord>>>,
        message_buffer: Arc<DashMap<u64, Arc<Notify>>>,
        cancellation_token: CancellationToken,
        mut message_rx: UnboundedReceiver<WithGeneration<ServerToClient>>,
    ) {
        notify.notified().await; // Wait until session_record is initialized

        loop {
            tokio::select! {
                biased;
                _ = cancellation_token.cancelled() => {
                    let undone = message_rx.len();
                    warn!("Unhandled WebSocket messages when shutting down: {}", undone);
                }
                message = message_rx.recv() => match message {
                    None => break,
                    Some(with_generation) => {
                        let generation = with_generation.generation;
                        match with_generation.result {
                            ServerToClient::Distribute(message) => {
                                trace!("Receiving message: {}", message.content.content);
                                let stream_message = StreamMessage::Distribute(ChatMessage {
                                    sender: message.sender,
                                    conversation_id: message.content.conversation_id,
                                    content: message.content.content,
                                });

                                debug!("Before get the lock");
                                if let Some(record) = &*session_record.lock().await {
                                    debug!("After get the lock");
                                    let callback = record.callback.clone();
                                    let callback = std::panic::AssertUnwindSafe(move || callback(stream_message));
                                    if let Err(e) = std::panic::catch_unwind(callback) {
                                        error!("Map function for WebSocket stream {} panicked: {:?}", generation, e);
                                    }
                                }
                            }
                            ServerToClient::ACK(ACK {message_seq}) => {
                                trace!("Receiving ACK: {:?}", message_seq);
                                let (_, notify) = match message_buffer.remove(&message_seq) {
                                    Some(inner) => inner,
                                    None => {
                                        trace!("Got None when ACK is received: {:?}", message_seq);
                                        break;
                                    }
                                };
                                notify.notify_one();
                                trace!("Notify one: {:?}", message_seq);
                            }
                        };
                    }
                }
            }
        }
    }

    pub fn create_task(
        &mut self,
        task: Pin<Box<dyn Future<Output = NetworkEvent> + Send>>,
        timeout: Duration,
        callback: Box<dyn FnOnce(WithGeneration<NetworkResult>) + Send + Sync>,
    ) -> anyhow::Result<u64> {
        let generation = self.generation.fetch_add(1, Ordering::Relaxed);
        let cancellation_token = self.cancellation_token.clone();
        let result_tx = self.result_tx.clone();

        let notify = Arc::new(Notify::new());
        let notify_clone = notify.clone();
        let cancellation_wrapped = async move {
            notify_clone.notified().await;
            let timeout_wrapped = async {
                match tokio::time::timeout(timeout, task).await {
                    Ok(e) => {
                        debug!("Task finished: {}", generation);
                        let message = WithGeneration {
                            generation,
                            result: Ok(e),
                        };
                        let _ = result_tx.send(message);
                    }
                    Err(_) => {
                        debug!("Task {} timed out", generation);
                        let message = WithGeneration {
                            generation,
                            result: Err(NetworkError::Timeout),
                        };
                        let _ = result_tx.send(message);
                    }
                }
            };

            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    debug!("Task {} was cancelled by global shutdown", generation);
                    let message = WithGeneration {
                        generation,
                        result: Err(NetworkError::SysCancelled),
                    };
                    let _ = result_tx.send(message);
                }
                _ = timeout_wrapped => {}
            }
        }.instrument(self.span.clone());

        let abort_handle = self
            .runtime_handle
            .block_on(async { self.join_set.spawn(cancellation_wrapped) });

        let record = TaskRecord {
            abort_handle,
            callback,
        };
        self.task_records.insert(generation, record);
        notify.notify_one();

        Ok(generation)
    }
}

impl NetworkInterface for NetworkImpl {
    fn fetch_captcha(
        &mut self,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<CaptchaEvent>) + Send + Sync>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>) + Send + Sync>,
    ) -> anyhow::Result<u64> {
        let worker = self.http_worker.clone();
        let callback = Box::new(|result: WithGeneration<NetworkResult>| {
            let generation = result.generation;
            match result.result {
                Ok(event) => match event {
                    NetworkEvent::Captcha(event) => map_function(WithGeneration {
                        generation,
                        result: event,
                    }),
                    _ => error!("Unexpected network event: {:?}", event),
                },
                Err(error) => err_function(WithGeneration {
                    generation,
                    result: error,
                }),
            }
        });

        let task = Box::pin(async move {
            let result = match worker.fetch_captcha().await {
                Ok(inner) => Ok(inner),
                Err(error) => {
                    error!("Failed to fetch captcha: {:?}", error);
                    Err(CaptchaError::FallbackError)
                }
            };

            NetworkEvent::Captcha(CaptchaEvent { result })
        });

        Ok(self.create_task(task, Duration::from_millis(timeout), Box::new(callback))?)
    }

    fn signup(
        &mut self,
        username: String,
        password: String,
        captcha_id: Uuid,
        captcha_answer: String,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<SignupEvent>) + Send + Sync>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>) + Send + Sync>,
    ) -> anyhow::Result<u64> {
        let worker = self.http_worker.clone();
        let callback = Box::new(|result: WithGeneration<NetworkResult>| {
            let generation = result.generation;
            match result.result {
                Ok(event) => match event {
                    NetworkEvent::Signup(event) => map_function(WithGeneration {
                        generation,
                        result: event,
                    }),
                    _ => error!("Unexpected network event: {:?}", event),
                },
                Err(error) => err_function(WithGeneration {
                    generation,
                    result: error,
                }),
            }
        });

        let task = Box::pin(async move {
            let result = match worker
                .signup(username, password, captcha_id, captcha_answer)
                .await
            {
                Ok(inner) => Ok(inner),
                Err(error) => {
                    error!("Failed to signup: {:?}", error);
                    Err(SignupError::FallbackError)
                }
            };

            NetworkEvent::Signup(SignupEvent { result })
        });

        Ok(self.create_task(task, Duration::from_millis(timeout), callback)?)
    }

    fn login(
        &mut self,
        username: String,
        password: String,
        captcha_id: Uuid,
        captcha_answer: String,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<LoginEvent>) + Send + Sync>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>) + Send + Sync>,
    ) -> anyhow::Result<u64> {
        let worker = self.http_worker.clone();
        let callback = Box::new(move |result: WithGeneration<NetworkResult>| {
            let generation = result.generation;
            match result.result {
                Ok(event) => match event {
                    NetworkEvent::Login(event) => map_function(WithGeneration {
                        generation,
                        result: event,
                    }),
                    _ => error!("Unexpected network event: {:?}", event),
                },
                Err(error) => err_function(WithGeneration {
                    generation,
                    result: error,
                }),
            }
        });

        let task = Box::pin(async move {
            let result = match worker
                .login(username, password, captcha_id, captcha_answer)
                .await
            {
                Ok(inner) => Ok(inner),
                Err(error) => {
                    error!("Failed to login: {:?}", error);
                    Err(LoginError::FallbackError)
                }
            };

            NetworkEvent::Login(LoginEvent { result })
        });

        Ok(self.create_task(task, Duration::from_millis(timeout), callback)?)
    }

    fn cancel(&mut self, generation: u64) -> anyhow::Result<()> {
        if let Some((_, TaskRecord { abort_handle, .. })) = self.task_records.remove(&generation) {
            abort_handle.abort();
            Ok(())
        } else {
            Err(anyhow::anyhow!("No such task: {:?}", generation))
        }
    }

    fn connect_chat(
        &mut self,
        address: String,
        jwt: String,
        msg_function: Box<dyn Fn(StreamMessage) + Send + Sync>,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<SessionEvent>) + Send + Sync>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>) + Send + Sync>,
    ) -> anyhow::Result<u64> {
        let stream_generation = self.generation.fetch_add(1, Ordering::Relaxed);
        let callback = Box::new(move |result: WithGeneration<NetworkResult>| {
            let generation = result.generation;
            match result.result {
                Ok(event) => match event {
                    NetworkEvent::Session(event) => map_function(WithGeneration {
                        generation,
                        result: event,
                    }),
                    _ => error!("Unexpected network event: {:?}", event),
                },
                Err(error) => err_function(WithGeneration {
                    generation,
                    result: error,
                }),
            }
        });

        let span = self.span.clone();
        let runtime_handle = self.runtime_handle.clone();
        let cancellation_token = self.cancellation_token.clone();
        let session_record = self.session_record.clone();
        let message_buffer = self.message_buffer.clone();
        let (message_tx, message_rx) = unbounded_channel();
        let task = Box::pin(async move {
            let result = match RealWsWorker::try_new(stream_generation, jwt, message_tx).await {
                Ok(worker) => {
                    let notify = Arc::new(Notify::new());
                    let task_handle = runtime_handle.spawn(Self::send_message_back(
                        notify.clone(),
                        session_record.clone(),
                        message_buffer,
                        cancellation_token,
                        message_rx,
                    ).instrument(span));

                    *session_record.lock().await = Some(SessionRecord {
                        ws_worker: Arc::new(Box::new(worker)),
                        task_handle,
                        callback: Arc::new(msg_function),
                    });
                    notify.notify_one();
                    Ok(ChatMetaData)
                }
                Err(error) => {
                    warn!("Failed to connect to chat server: {:?}", error);
                    Err(ChatConnError::FallbackError)
                }
            };

            NetworkEvent::Session(SessionEvent { result })
        });

        Ok(self.create_task(task, Duration::from_millis(timeout), Box::new(callback))?)
    }

    fn send_chat_message(
        &mut self,
        conversation_id: ConversationId,
        content: String,
        timeout: u64,
        map_function: Box<dyn FnOnce(WithGeneration<MessageEvent>) + Send + Sync>,
        err_function: Box<dyn FnOnce(WithGeneration<NetworkError>) + Send + Sync>,
    ) -> anyhow::Result<u64> {
        let span = self.span.clone();
        let _enter = span.enter();

        let message_id = self.message_id.fetch_add(1, Ordering::Relaxed);

        let span = self.span.clone();
        let message_buffer = self.message_buffer.clone();
        let content_clone = content.clone();
        let callback = Box::new(move |result: WithGeneration<NetworkResult>| {
            let _enter = span.enter();
            let generation = result.generation;
            match result.result {
                Ok(event) => match event {
                    NetworkEvent::Chat(event) => map_function(WithGeneration {
                        generation,
                        result: event,
                    }),
                    _ => error!("Unexpected network event: {:?}", event),
                },
                Err(error) => err_function(WithGeneration {
                    generation,
                    result: error,
                }),
            }
            message_buffer.remove(&message_id);
            trace!("Remove message in callback wrapper: {:?} {}", message_id, content_clone);
        });

        let session_record = self.session_record.clone();
        let message_buffer = self.message_buffer.clone();
        let task = Box::pin(async move {
            let worker = match &*session_record.lock().await {
                None => {
                    return NetworkEvent::Chat(MessageEvent {
                        result: Err(MessageError::MissingSession),
                    })
                }
                Some(record) => record.ws_worker.clone(),
            };

            let notify = Arc::new(Notify::new());
            message_buffer.insert(message_id, notify.clone());
            trace!("Insert message in task: {:?} {}", message_id, content);

            if let Err(error) = worker.send_message(message_id, conversation_id.clone(), content.clone()).await {
                error!("Failed to send message: {:?}", error);
                return NetworkEvent::Chat(MessageEvent {
                    result: Err(MessageError::FallbackError),
                })
            }

            trace!("Waiting for notify");
            notify.notified().await;

            message_buffer.remove(&message_id);
            trace!("Remove message after notify: {:?} {}", message_id, content);
            NetworkEvent::Chat(MessageEvent {
                result: Ok(MessageSent),
            })
        }.instrument(self.span.clone()));

        Ok(self.create_task(task, Duration::from_millis(timeout), callback)?)
    }
}
