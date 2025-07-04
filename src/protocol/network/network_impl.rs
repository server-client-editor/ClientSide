use crate::domain::ConversationId;
use crate::protocol::network::{NetworkError, NetworkEvent, NetworkResult, WithGeneration};
use dashmap::DashMap;
use std::future::Future;
use std::pin::Pin;
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::AbortHandle;
use tokio::time::error::Elapsed;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};
use uuid::Uuid;

struct TaskRecord {
    pub abort_handle: AbortHandle,
    pub callback: Box<dyn FnOnce(WithGeneration<NetworkResult>) + Send + Sync>,
}

pub struct NetworkImpl {
    generation: AtomicU64,
    task_records: Arc<DashMap<u64, TaskRecord>>,
    cancellation_token: CancellationToken,
    runtime_handle: tokio::runtime::Handle,
    join_set: tokio::task::JoinSet<()>,

    result_tx: UnboundedSender<WithGeneration<NetworkResult>>,
    runtime_thread_handle: std::thread::JoinHandle<()>,
}

impl NetworkImpl {
    pub fn try_new() -> anyhow::Result<Self> {
        let generation = AtomicU64::new(0);
        let task_records = Arc::new(DashMap::new());
        let cancellation_token = CancellationToken::new();

        let (result_tx, result_rx) = unbounded_channel::<WithGeneration<NetworkResult>>();
        let tokio_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let runtime_handle = tokio_runtime.handle().clone();

        let records_clone = task_records.clone();
        let cancellation_token_clone = cancellation_token.clone();
        let runtime_thread_handle = std::thread::spawn(move || {
            tokio_runtime.block_on(Self::send_result_back(
                records_clone,
                cancellation_token_clone,
                result_rx,
            ))
        });

        let join_set = tokio::task::JoinSet::new();

        Ok(Self {
            generation,
            task_records,
            cancellation_token,
            runtime_handle,
            join_set,
            result_tx,
            runtime_thread_handle,
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
                        if let Some((_, TaskRecord {callback, ..})) = task_records.remove(&generation) {
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

    pub fn create_task(
        &mut self,
        task: Pin<Box<dyn Future<Output = NetworkEvent> + Send + Sync>>,
        timeout: Duration,
        callback: Box<dyn FnOnce(WithGeneration<NetworkResult>) + Send + Sync>,
    ) -> anyhow::Result<u64> {
        let generation = self.generation.fetch_add(1, Ordering::Relaxed);
        let cancellation_token = self.cancellation_token.clone();
        let result_tx = self.result_tx.clone();

        let cancellation_wrapped = async move {
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
        };

        let abort_handle = self
            .runtime_handle
            .block_on(async { self.join_set.spawn(cancellation_wrapped) });

        let record = TaskRecord {
            abort_handle,
            callback,
        };
        self.task_records.insert(generation, record);

        Ok(generation)
    }
}
