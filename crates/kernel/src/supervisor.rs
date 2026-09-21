use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{error, warn};

#[derive(Clone, Debug, Default)]
pub struct ShutdownState {
    requested: Arc<AtomicBool>,
}

impl ShutdownState {
    pub fn request(&self) {
        self.requested.store(true, Ordering::Release);
    }

    pub fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, Default)]
pub struct TaskHealth {
    running: Arc<AtomicBool>,
    operational: Arc<AtomicBool>,
}

impl TaskHealth {
    pub fn available(&self) -> bool {
        self.running.load(Ordering::Acquire) && self.operational.load(Ordering::Acquire)
    }

    pub fn running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn start(&self) -> TaskHealthGuard {
        self.running.store(true, Ordering::Release);
        self.operational.store(true, Ordering::Release);
        TaskHealthGuard {
            health: self.clone(),
        }
    }

    fn stop(&self) {
        self.operational.store(false, Ordering::Release);
        self.running.store(false, Ordering::Release);
    }
}

pub struct TaskHealthGuard {
    health: TaskHealth,
}

impl Drop for TaskHealthGuard {
    fn drop(&mut self) {
        self.health.stop();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownReason {
    Coordinated,
    CtrlC,
    Sigterm,
}

pub async fn wait_for_shutdown(mut coordinated: watch::Receiver<bool>) -> ShutdownReason {
    if *coordinated.borrow() {
        return ShutdownReason::Coordinated;
    }

    #[cfg(unix)]
    let mut sigterm = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        Ok(signal) => Some(signal),
        Err(error) => {
            warn!(error = %error, "SIGTERM listener unavailable; retaining Ctrl-C shutdown");
            None
        }
    };

    loop {
        #[cfg(unix)]
        {
            if let Some(signal) = sigterm.as_mut() {
                tokio::select! {
                    changed = coordinated.changed() => {
                        if changed.is_err() || *coordinated.borrow() {
                            return ShutdownReason::Coordinated;
                        }
                    }
                    result = tokio::signal::ctrl_c() => {
                        if let Err(error) = result {
                            warn!(error = %error, "Ctrl-C listener failed; shutting down");
                        }
                        return ShutdownReason::CtrlC;
                    }
                    signal = signal.recv() => {
                        if signal.is_none() {
                            warn!("SIGTERM listener closed; retaining Ctrl-C shutdown");
                            sigterm = None;
                        } else {
                            return ShutdownReason::Sigterm;
                        }
                    }
                }
                continue;
            }
        }

        tokio::select! {
            changed = coordinated.changed() => {
                if changed.is_err() || *coordinated.borrow() {
                    return ShutdownReason::Coordinated;
                }
            }
            result = tokio::signal::ctrl_c() => {
                if let Err(error) = result {
                    warn!(error = %error, "Ctrl-C listener failed; shutting down");
                }
                return ShutdownReason::CtrlC;
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("{task} task exited before coordinated shutdown: {detail}")]
    TaskExited { task: &'static str, detail: String },
    #[error("{task} task failed during coordinated shutdown: {detail}")]
    TaskJoin { task: &'static str, detail: String },
    #[error("{task} task did not stop within {timeout:?}")]
    ShutdownTimeout {
        task: &'static str,
        timeout: Duration,
    },
}

pub async fn supervise_background_tasks(
    mut relay_handle: JoinHandle<()>,
    mut executor_handle: JoinHandle<()>,
    mut scheduler_handle: JoinHandle<()>,
    shutdown_tx: watch::Sender<bool>,
    mut shutdown: watch::Receiver<bool>,
    shutdown_state: ShutdownState,
    shutdown_timeout: Duration,
) -> Result<(), SupervisorError> {
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return join_background_tasks(
                        &mut relay_handle,
                        &mut executor_handle,
                        &mut scheduler_handle,
                        shutdown_timeout,
                    ).await;
                }
            }
            result = &mut relay_handle => {
                if *shutdown.borrow() {
                    let executor_result =
                        join_with_timeout("executor", &mut executor_handle, shutdown_timeout)
                            .await;
                    let scheduler_result =
                        join_with_timeout("scheduler", &mut scheduler_handle, shutdown_timeout)
                            .await;
                    return executor_result.and(scheduler_result);
                }
                return handle_unexpected_exit(
                    "relay",
                    result,
                    [
                        ("executor", &mut executor_handle),
                        ("scheduler", &mut scheduler_handle),
                    ],
                    &shutdown_tx,
                    &shutdown_state,
                    shutdown_timeout,
                ).await;
            }
            result = &mut executor_handle => {
                if *shutdown.borrow() {
                    let relay_result =
                        join_with_timeout("relay", &mut relay_handle, shutdown_timeout).await;
                    let scheduler_result =
                        join_with_timeout("scheduler", &mut scheduler_handle, shutdown_timeout)
                            .await;
                    return relay_result.and(scheduler_result);
                }
                return handle_unexpected_exit(
                    "executor",
                    result,
                    [("relay", &mut relay_handle), ("scheduler", &mut scheduler_handle)],
                    &shutdown_tx,
                    &shutdown_state,
                    shutdown_timeout,
                ).await;
            }
            result = &mut scheduler_handle => {
                if *shutdown.borrow() {
                    let relay_result =
                        join_with_timeout("relay", &mut relay_handle, shutdown_timeout).await;
                    let executor_result =
                        join_with_timeout("executor", &mut executor_handle, shutdown_timeout)
                            .await;
                    return relay_result.and(executor_result);
                }
                return handle_unexpected_exit(
                    "scheduler",
                    result,
                    [("relay", &mut relay_handle), ("executor", &mut executor_handle)],
                    &shutdown_tx,
                    &shutdown_state,
                    shutdown_timeout,
                ).await;
            }
        }
    }
}

async fn handle_unexpected_exit(
    task: &'static str,
    result: Result<(), tokio::task::JoinError>,
    survivors: [(&'static str, &mut JoinHandle<()>); 2],
    shutdown_tx: &watch::Sender<bool>,
    shutdown_state: &ShutdownState,
    shutdown_timeout: Duration,
) -> Result<(), SupervisorError> {
    let detail = match result {
        Ok(()) => "task returned before coordinated shutdown".to_owned(),
        Err(error) => error.to_string(),
    };
    shutdown_state.request();
    let _ = shutdown_tx.send(true);
    for (survivor_name, survivor) in survivors {
        if let Err(error) = join_with_timeout(survivor_name, survivor, shutdown_timeout).await {
            error!(task, survivor = survivor_name, error = %error, "surviving background task did not stop cleanly");
        }
    }
    Err(SupervisorError::TaskExited { task, detail })
}

async fn join_background_tasks(
    relay_handle: &mut JoinHandle<()>,
    executor_handle: &mut JoinHandle<()>,
    scheduler_handle: &mut JoinHandle<()>,
    shutdown_timeout: Duration,
) -> Result<(), SupervisorError> {
    let relay_result = join_with_timeout("relay", relay_handle, shutdown_timeout).await;
    let executor_result = join_with_timeout("executor", executor_handle, shutdown_timeout).await;
    let scheduler_result = join_with_timeout("scheduler", scheduler_handle, shutdown_timeout).await;
    relay_result.and(executor_result).and(scheduler_result)
}

async fn join_with_timeout(
    task: &'static str,
    handle: &mut JoinHandle<()>,
    shutdown_timeout: Duration,
) -> Result<(), SupervisorError> {
    match timeout(shutdown_timeout, &mut *handle).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(SupervisorError::TaskJoin {
            task,
            detail: error.to_string(),
        }),
        Err(_) => {
            handle.abort();
            let _ = handle.await;
            Err(SupervisorError::ShutdownTimeout {
                task,
                timeout: shutdown_timeout,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_guard_marks_task_unavailable_after_drop() {
        let health = TaskHealth::default();
        assert!(!health.running());
        {
            let _guard = health.start();
            assert!(health.available());
        }
        assert!(!health.available());
    }

    #[test]
    fn shutdown_state_is_idempotent() {
        let state = ShutdownState::default();
        assert!(!state.is_requested());
        state.request();
        state.request();
        assert!(state.is_requested());
    }

    #[tokio::test]
    async fn supervisor_stops_workers_after_coordinated_shutdown() {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let relay_rx = shutdown_tx.subscribe();
        let executor_rx = shutdown_tx.subscribe();
        let relay = tokio::spawn(async move {
            let mut receiver = relay_rx;
            let _ = receiver.changed().await;
        });
        let executor = tokio::spawn(async move {
            let mut receiver = executor_rx;
            let _ = receiver.changed().await;
        });
        let scheduler_rx = shutdown_tx.subscribe();
        let scheduler = tokio::spawn(async move {
            let mut receiver = scheduler_rx;
            let _ = receiver.changed().await;
        });
        let state = ShutdownState::default();
        let supervisor = tokio::spawn(supervise_background_tasks(
            relay,
            executor,
            scheduler,
            shutdown_tx.clone(),
            shutdown_rx,
            state.clone(),
            Duration::from_secs(1),
        ));
        assert!(shutdown_tx.send(true).is_ok());
        let result = supervisor.await;
        assert!(matches!(result, Ok(Ok(()))));
        assert!(!state.is_requested());
    }

    #[tokio::test]
    async fn supervisor_requests_shutdown_when_worker_exits() {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let relay = tokio::spawn(async {});
        let executor_rx = shutdown_tx.subscribe();
        let executor = tokio::spawn(async move {
            let mut receiver = executor_rx;
            let _ = receiver.changed().await;
        });
        let scheduler_rx = shutdown_tx.subscribe();
        let scheduler = tokio::spawn(async move {
            let mut receiver = scheduler_rx;
            let _ = receiver.changed().await;
        });
        let state = ShutdownState::default();
        let result = supervise_background_tasks(
            relay,
            executor,
            scheduler,
            shutdown_tx,
            shutdown_rx,
            state.clone(),
            Duration::from_secs(1),
        )
        .await;
        assert!(matches!(
            result,
            Err(SupervisorError::TaskExited { task: "relay", .. })
        ));
        assert!(state.is_requested());
    }
}
