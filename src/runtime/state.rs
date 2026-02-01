//! State Machine - Connection and session state management

use std::time::{Duration, Instant};

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// Attempting to connect
    Connecting,
    /// Connected and ready
    Connected,
    /// Connection lost, attempting to reconnect
    Reconnecting,
    /// Error state
    Error,
}

impl ConnectionState {
    /// Check if we're in a connected state
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Check if we're attempting to connect
    pub fn is_connecting(&self) -> bool {
        matches!(self, Self::Connecting | Self::Reconnecting)
    }

    /// Check if we're in an error state
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay between retries (ms)
    pub initial_delay_ms: u64,
    /// Maximum delay between retries (ms)
    pub max_delay_ms: u64,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Add jitter to delays
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay_ms: 100,
            max_delay_ms: 30_000,
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Calculate delay for the given attempt
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let delay = self.initial_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32);
        let delay = delay.min(self.max_delay_ms as f64);

        let delay_with_jitter = if self.jitter {
            // Add up to 25% jitter
            let jitter = delay * 0.25 * rand_simple();
            delay + jitter
        } else {
            delay
        };

        Duration::from_millis(delay_with_jitter as u64)
    }

    /// Check if we should retry
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_retries
    }
}

/// Simple pseudo-random number generator for jitter
fn rand_simple() -> f64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let mut hasher = DefaultHasher::new();
    SystemTime::now().hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    (hasher.finish() as f64) / (u64::MAX as f64)
}

/// Connection state machine
pub struct StateMachine {
    state: ConnectionState,
    retry_attempt: u32,
    retry_config: RetryConfig,
    last_state_change: Instant,
    connected_at: Option<Instant>,
}

impl StateMachine {
    /// Create a new state machine
    pub fn new(retry_config: RetryConfig) -> Self {
        Self {
            state: ConnectionState::Disconnected,
            retry_attempt: 0,
            retry_config,
            last_state_change: Instant::now(),
            connected_at: None,
        }
    }

    /// Get current state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Get retry attempt count
    pub fn retry_attempt(&self) -> u32 {
        self.retry_attempt
    }

    /// Get time since last state change
    pub fn time_in_state(&self) -> Duration {
        self.last_state_change.elapsed()
    }

    /// Get connection duration (if connected)
    pub fn connection_duration(&self) -> Option<Duration> {
        self.connected_at.map(|t| t.elapsed())
    }

    /// Transition to connecting state
    pub fn start_connecting(&mut self) -> Result<(), StateError> {
        match self.state {
            ConnectionState::Disconnected | ConnectionState::Error => {
                self.transition(ConnectionState::Connecting);
                self.retry_attempt = 0;
                Ok(())
            }
            _ => Err(StateError::InvalidTransition {
                from: self.state,
                to: ConnectionState::Connecting,
            }),
        }
    }

    /// Mark connection as successful
    pub fn connected(&mut self) -> Result<(), StateError> {
        match self.state {
            ConnectionState::Connecting | ConnectionState::Reconnecting => {
                self.transition(ConnectionState::Connected);
                self.retry_attempt = 0;
                self.connected_at = Some(Instant::now());
                Ok(())
            }
            _ => Err(StateError::InvalidTransition {
                from: self.state,
                to: ConnectionState::Connected,
            }),
        }
    }

    /// Connection lost - attempt reconnection
    pub fn connection_lost(&mut self) -> Result<Duration, StateError> {
        match self.state {
            ConnectionState::Connected => {
                if self.retry_config.should_retry(self.retry_attempt) {
                    self.transition(ConnectionState::Reconnecting);
                    let delay = self.retry_config.calculate_delay(self.retry_attempt);
                    self.retry_attempt += 1;
                    Ok(delay)
                } else {
                    self.transition(ConnectionState::Error);
                    Err(StateError::MaxRetriesExceeded)
                }
            }
            _ => Err(StateError::InvalidTransition {
                from: self.state,
                to: ConnectionState::Reconnecting,
            }),
        }
    }

    /// Reconnection attempt failed
    pub fn reconnection_failed(&mut self) -> Result<Duration, StateError> {
        match self.state {
            ConnectionState::Reconnecting => {
                if self.retry_config.should_retry(self.retry_attempt) {
                    let delay = self.retry_config.calculate_delay(self.retry_attempt);
                    self.retry_attempt += 1;
                    Ok(delay)
                } else {
                    self.transition(ConnectionState::Error);
                    Err(StateError::MaxRetriesExceeded)
                }
            }
            _ => Err(StateError::InvalidTransition {
                from: self.state,
                to: ConnectionState::Reconnecting,
            }),
        }
    }

    /// Disconnect
    pub fn disconnect(&mut self) {
        self.transition(ConnectionState::Disconnected);
        self.retry_attempt = 0;
        self.connected_at = None;
    }

    /// Mark as error
    pub fn error(&mut self) {
        self.transition(ConnectionState::Error);
        self.connected_at = None;
    }

    /// Reset state machine
    pub fn reset(&mut self) {
        self.state = ConnectionState::Disconnected;
        self.retry_attempt = 0;
        self.last_state_change = Instant::now();
        self.connected_at = None;
    }

    fn transition(&mut self, new_state: ConnectionState) {
        self.state = new_state;
        self.last_state_change = Instant::now();
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new(RetryConfig::default())
    }
}

/// State machine errors
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: ConnectionState,
        to: ConnectionState,
    },

    #[error("Maximum retries exceeded")]
    MaxRetriesExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transitions() {
        let mut sm = StateMachine::default();

        assert_eq!(sm.state(), ConnectionState::Disconnected);

        sm.start_connecting().unwrap();
        assert_eq!(sm.state(), ConnectionState::Connecting);

        sm.connected().unwrap();
        assert_eq!(sm.state(), ConnectionState::Connected);
        assert!(sm.connection_duration().is_some());

        sm.disconnect();
        assert_eq!(sm.state(), ConnectionState::Disconnected);
    }

    #[test]
    fn test_retry_logic() {
        let config = RetryConfig {
            max_retries: 3,
            initial_delay_ms: 100,
            max_delay_ms: 1000,
            backoff_multiplier: 2.0,
            jitter: false,
        };

        let mut sm = StateMachine::new(config);

        sm.start_connecting().unwrap();
        sm.connected().unwrap();

        // First reconnection attempt
        let delay = sm.connection_lost().unwrap();
        assert_eq!(sm.state(), ConnectionState::Reconnecting);
        assert_eq!(delay, Duration::from_millis(100));

        // Second attempt
        let delay = sm.reconnection_failed().unwrap();
        assert_eq!(delay, Duration::from_millis(200));

        // Third attempt
        let delay = sm.reconnection_failed().unwrap();
        assert_eq!(delay, Duration::from_millis(400));

        // Max retries exceeded
        assert!(sm.reconnection_failed().is_err());
        assert_eq!(sm.state(), ConnectionState::Error);
    }

    #[test]
    fn test_retry_delay_calculation() {
        let config = RetryConfig {
            max_retries: 10,
            initial_delay_ms: 100,
            max_delay_ms: 1000,
            backoff_multiplier: 2.0,
            jitter: false,
        };

        assert_eq!(config.calculate_delay(0), Duration::from_millis(100));
        assert_eq!(config.calculate_delay(1), Duration::from_millis(200));
        assert_eq!(config.calculate_delay(2), Duration::from_millis(400));
        assert_eq!(config.calculate_delay(3), Duration::from_millis(800));
        // Should cap at max
        assert_eq!(config.calculate_delay(4), Duration::from_millis(1000));
        assert_eq!(config.calculate_delay(5), Duration::from_millis(1000));
    }
}
