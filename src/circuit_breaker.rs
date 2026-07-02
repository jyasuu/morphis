use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::Instant;

#[derive(Clone, Debug)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u64,
    pub reset_timeout: Duration,
    pub half_open_max_requests: u64,
}

#[derive(Clone)]
pub struct CircuitBreaker {
    inner: Arc<RwLock<Inner>>,
    config: CircuitBreakerConfig,
}

struct Inner {
    state: State,
    failure_count: u64,
}

enum State {
    Closed,
    Open(Instant),
    HalfOpen(u64),
}

#[derive(Debug)]
pub enum Error<E> {
    CircuitOpen,
    Inner(E),
}

impl<E: fmt::Display> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::CircuitOpen => write!(f, "circuit breaker open"),
            Error::Inner(e) => write!(f, "{}", e),
        }
    }
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                state: State::Closed,
                failure_count: 0,
            })),
            config,
        }
    }

    pub async fn call<F, Fut, T, E>(&self, f: F) -> Result<T, Error<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        if !self.is_request_allowed().await {
            return Err(Error::CircuitOpen);
        }
        let result = f().await;
        match &result {
            Ok(_) => self.record_success().await,
            Err(_) => self.record_failure().await,
        }
        result.map_err(Error::Inner)
    }

    async fn is_request_allowed(&self) -> bool {
        let mut inner = self.inner.write().await;
        match inner.state {
            State::Closed => true,
            State::Open(opened_at) => {
                if opened_at.elapsed() >= self.config.reset_timeout {
                    let n = self.config.half_open_max_requests;
                    if n > 0 {
                        inner.state = State::HalfOpen(n - 1);
                    } else {
                        inner.state = State::Closed;
                        inner.failure_count = 0;
                    }
                    true
                } else {
                    false
                }
            }
            State::HalfOpen(ref mut remaining) => {
                if *remaining > 0 {
                    *remaining -= 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    async fn record_success(&self) {
        let mut inner = self.inner.write().await;
        match inner.state {
            State::HalfOpen(_) | State::Open(_) => {
                inner.state = State::Closed;
                inner.failure_count = 0;
            }
            State::Closed => {
                inner.failure_count = 0;
            }
        }
    }

    async fn record_failure(&self) {
        let mut inner = self.inner.write().await;
        match inner.state {
            State::Closed => {
                inner.failure_count += 1;
                if inner.failure_count >= self.config.failure_threshold {
                    inner.state = State::Open(Instant::now());
                }
            }
            State::HalfOpen(_) => {
                inner.state = State::Open(Instant::now());
            }
            State::Open(_) => {
                inner.state = State::Open(Instant::now());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(failure_threshold: u64, reset_timeout_secs: u64, half_open_max: u64) -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold,
            reset_timeout: Duration::from_secs(reset_timeout_secs),
            half_open_max_requests: half_open_max,
        }
    }

    fn ok() -> Result<(), String> { Ok(()) }
    fn err_msg() -> Result<(), String> { Err("err".to_string()) }

    #[tokio::test]
    async fn initial_state_allows_calls() {
        let cb = CircuitBreaker::new(cfg(3, 30, 1));
        assert!(cb.call(|| async { ok() }).await.is_ok());
    }

    #[tokio::test]
    async fn failures_open_circuit_after_threshold() {
        let cb = CircuitBreaker::new(cfg(3, 30, 1));

        assert!(cb.call(|| async { err_msg() }).await.is_err());
        assert!(cb.call(|| async { err_msg() }).await.is_err());
        let third = cb.call(|| async { err_msg() }).await;
        assert!(matches!(third, Err(Error::Inner(_))));

        match cb.call(|| async { ok() }).await {
            Err(Error::CircuitOpen) => {}
            other => panic!("expected CircuitOpen, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn success_resets_failure_count_in_closed() {
        let cb = CircuitBreaker::new(cfg(3, 30, 1));

        assert!(cb.call(|| async { err_msg() }).await.is_err());
        assert!(cb.call(|| async { err_msg() }).await.is_err());
        assert!(cb.call(|| async { ok() }).await.is_ok());

        assert!(cb.call(|| async { err_msg() }).await.is_err());
        assert!(cb.call(|| async { err_msg() }).await.is_err());
        assert!(cb.call(|| async { err_msg() }).await.is_err());

        match cb.call(|| async { ok() }).await {
            Err(Error::CircuitOpen) => {}
            other => panic!("expected CircuitOpen, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn half_open_probe_success_closes_circuit() {
        tokio::time::pause();
        let cb = CircuitBreaker::new(cfg(2, 10, 1));

        cb.call(|| async { err_msg() }).await.ok();
        cb.call(|| async { err_msg() }).await.ok();

        assert!(matches!(cb.call(|| async { ok() }).await, Err(Error::CircuitOpen)));

        tokio::time::advance(Duration::from_secs(11)).await;

        assert!(cb.call(|| async { ok() }).await.is_ok());
        assert!(cb.call(|| async { ok() }).await.is_ok());
    }

    #[tokio::test]
    async fn half_open_probe_failure_reopens_circuit() {
        tokio::time::pause();
        let cb = CircuitBreaker::new(cfg(2, 10, 1));

        cb.call(|| async { err_msg() }).await.ok();
        cb.call(|| async { err_msg() }).await.ok();

        tokio::time::advance(Duration::from_secs(11)).await;

        assert!(matches!(cb.call(|| async { err_msg() }).await, Err(Error::Inner(_))));
        assert!(matches!(cb.call(|| async { ok() }).await, Err(Error::CircuitOpen)));
    }

    #[tokio::test]
    async fn half_open_multiple_probes_allowed() {
        tokio::time::pause();
        let cb = CircuitBreaker::new(cfg(2, 10, 3));

        cb.call(|| async { err_msg() }).await.ok();
        cb.call(|| async { err_msg() }).await.ok();

        tokio::time::advance(Duration::from_secs(11)).await;

        assert!(cb.call(|| async { ok() }).await.is_ok());
        assert!(cb.call(|| async { ok() }).await.is_ok());
        assert!(cb.call(|| async { ok() }).await.is_ok());
    }

    #[tokio::test]
    async fn half_open_exhausted_slots_rejected() {
        tokio::time::pause();
        let cb = Arc::new(CircuitBreaker::new(cfg(2, 10, 1)));

        cb.call(|| async { err_msg() }).await.ok();
        cb.call(|| async { err_msg() }).await.ok();

        tokio::time::advance(Duration::from_secs(11)).await;

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();
        let (hold_tx, hold_rx) = tokio::sync::oneshot::channel::<()>();

        let cb2 = cb.clone();
        let _probe = tokio::spawn(async move {
            let _ = cb2
                .call(|| async {
                    let _ = ready_tx.send(());
                    let _ = hold_rx.await;
                    ok()
                })
                .await;
        });

        ready_rx.await.unwrap();

        assert!(matches!(cb.call(|| async { ok() }).await, Err(Error::CircuitOpen)));

        drop(hold_tx);
    }

    #[tokio::test]
    async fn half_open_max_requests_zero_closes_directly() {
        tokio::time::pause();
        let cb = CircuitBreaker::new(cfg(2, 10, 0));

        cb.call(|| async { err_msg() }).await.ok();
        cb.call(|| async { err_msg() }).await.ok();

        assert!(matches!(cb.call(|| async { ok() }).await, Err(Error::CircuitOpen)));

        tokio::time::advance(Duration::from_secs(11)).await;

        assert!(cb.call(|| async { ok() }).await.is_ok());
    }

    #[tokio::test]
    async fn threshold_one_opens_on_first_failure() {
        let cb = CircuitBreaker::new(cfg(1, 30, 1));

        assert!(matches!(cb.call(|| async { err_msg() }).await, Err(Error::Inner(_))));
        assert!(matches!(cb.call(|| async { ok() }).await, Err(Error::CircuitOpen)));
    }

    #[tokio::test]
    async fn half_open_failure_restarts_timer() {
        tokio::time::pause();
        let cb = CircuitBreaker::new(cfg(2, 10, 1));

        cb.call(|| async { err_msg() }).await.ok();
        cb.call(|| async { err_msg() }).await.ok();

        tokio::time::advance(Duration::from_secs(11)).await;

        assert!(matches!(cb.call(|| async { err_msg() }).await, Err(Error::Inner(_))));

        assert!(matches!(cb.call(|| async { ok() }).await, Err(Error::CircuitOpen)));

        tokio::time::advance(Duration::from_secs(9)).await;

        assert!(matches!(cb.call(|| async { ok() }).await, Err(Error::CircuitOpen)));

        tokio::time::advance(Duration::from_secs(2)).await;

        assert!(cb.call(|| async { ok() }).await.is_ok());
    }

    #[tokio::test]
    async fn concurrent_calls_are_safe() {
        let cb = Arc::new(CircuitBreaker::new(cfg(10, 30, 5)));

        let mut handles = Vec::new();
        for _ in 0..20 {
            let cb = cb.clone();
            handles.push(tokio::spawn(async move {
                cb.call(|| async { ok() }).await
            }));
        }

        for h in handles {
            assert!(h.await.unwrap().is_ok());
        }
    }

    #[test]
    fn error_display() {
        let open: Error<String> = Error::CircuitOpen;
        assert_eq!(open.to_string(), "circuit breaker open");
        let inner: Error<String> = Error::Inner("db timeout".into());
        assert_eq!(inner.to_string(), "db timeout");
    }

    #[test]
    fn error_debug() {
        let open: Error<String> = Error::CircuitOpen;
        assert_eq!(format!("{open:?}"), "CircuitOpen");
    }

    #[tokio::test]
    async fn call_returns_inner_error_on_failure() {
        let cb = CircuitBreaker::new(cfg(5, 30, 1));
        let result = cb.call(|| async { Err::<(), _>("db error") }).await;
        match result {
            Err(Error::Inner(e)) => assert_eq!(e, "db error"),
            other => panic!("expected Inner error, got {other:?}"),
        }
    }
}
