//! Transport Client - Async transport layer with WebSocket support
//!
//! Defines the `Transport` trait and provides a WebSocket implementation.

use crate::runtime::codec::PROTOCOL_VERSION;
use crate::runtime::state::{ConnectionState, RetryConfig, StateMachine};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio_tungstenite::tungstenite::Message;

// ─── Transport trait ─────────────────────────────────────────────────────────

/// Common interface for all transport implementations (WebSocket, WebTransport, etc.)
///
/// Every transport speaks the same binary protocol: a version byte prefix followed
/// by bitcode-encoded payload. The trait is intentionally *not* object-safe
/// (`async fn`) so that concrete types can be used directly without boxing.
pub trait Transport {
    /// Connect to the remote endpoint.
    fn connect(&mut self) -> impl std::future::Future<Output = Result<(), TransportError>> + Send;

    /// Disconnect from the remote endpoint. Idempotent.
    fn disconnect(&mut self) -> impl std::future::Future<Output = ()> + Send;

    /// Send a binary message (must start with the protocol version byte).
    fn send(
        &self,
        data: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<(), TransportError>> + Send;

    /// Receive a binary message. Blocks until a message arrives or the connection closes.
    fn receive(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, TransportError>> + Send;

    /// Try to receive without blocking.
    fn try_receive(&mut self) -> Result<Option<Vec<u8>>, TransportError>;

    /// Get the current connection state.
    fn state(&self) -> impl std::future::Future<Output = ConnectionState> + Send;

    /// Check if connected.
    fn is_connected(&self) -> impl std::future::Future<Output = bool> + Send;

    /// Get the server URL.
    fn url(&self) -> &str;
}

/// Transport client configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Server URL (ws:// or wss://)
    pub url: String,
    /// Retry configuration
    pub retry: RetryConfig,
    /// Connection timeout in milliseconds
    pub connect_timeout_ms: u64,
    /// Keep-alive interval in milliseconds (0 = disabled)
    pub keepalive_interval_ms: u64,
    /// Maximum message size in bytes
    pub max_message_size: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            retry: RetryConfig::default(),
            connect_timeout_ms: 10_000,
            keepalive_interval_ms: 30_000,
            max_message_size: 65535,
        }
    }
}

/// WebSocket transport client
///
/// Connects to a server via WebSocket (ws:// or wss://) and exchanges
/// binary frames prefixed with the motto protocol version byte.
pub struct WebSocketClient {
    config: TransportConfig,
    state: Arc<RwLock<StateMachine>>,
    outgoing_tx: Option<mpsc::Sender<Vec<u8>>>,
    incoming_rx: Option<mpsc::Receiver<Vec<u8>>>,
    /// Handle to the background connection task so we can abort on disconnect
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl WebSocketClient {
    /// Create a new WebSocket client
    pub fn new(config: TransportConfig) -> Self {
        Self {
            config: config.clone(),
            state: Arc::new(RwLock::new(StateMachine::new(config.retry))),
            outgoing_tx: None,
            incoming_rx: None,
            task_handle: None,
        }
    }

    /// Get the current connection state
    pub async fn state(&self) -> ConnectionState {
        self.state.read().await.state()
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        self.state.read().await.state().is_connected()
    }

    /// Connect to the server via WebSocket
    pub async fn connect(&mut self) -> Result<(), TransportError> {
        // Validate URL before attempting connection
        let url: url::Url = self
            .config
            .url
            .parse()
            .map_err(|e| TransportError::ConnectionFailed(format!("invalid URL: {}", e)))?;

        let scheme = url.scheme();
        if scheme != "ws" && scheme != "wss" {
            return Err(TransportError::ConnectionFailed(format!(
                "unsupported URL scheme '{}': expected 'ws' or 'wss'",
                scheme
            )));
        }

        // Transition state machine
        {
            let mut state = self.state.write().await;
            state
                .start_connecting()
                .map_err(|_| TransportError::InvalidState)?;
        }

        // Attempt WebSocket handshake with timeout
        let connect_future = tokio_tungstenite::connect_async(&self.config.url);
        let timeout = tokio::time::Duration::from_millis(self.config.connect_timeout_ms);

        let (ws_stream, _response) = tokio::time::timeout(timeout, connect_future)
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        let (ws_sink, ws_source) = ws_stream.split();

        // Create channels for message passing
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<Vec<u8>>(256);
        let (incoming_tx, incoming_rx) = mpsc::channel::<Vec<u8>>(256);

        self.outgoing_tx = Some(outgoing_tx);
        self.incoming_rx = Some(incoming_rx);

        // Mark connected
        {
            let mut state = self.state.write().await;
            state
                .connected()
                .map_err(|_| TransportError::InvalidState)?;
        }

        // Spawn background task that drives the WebSocket connection
        let state = Arc::clone(&self.state);
        let keepalive_interval_ms = self.config.keepalive_interval_ms;

        let handle = tokio::spawn(async move {
            Self::connection_loop(
                ws_sink,
                ws_source,
                outgoing_rx,
                incoming_tx,
                state,
                keepalive_interval_ms,
            )
            .await;
        });
        self.task_handle = Some(handle);

        Ok(())
    }

    /// Background loop: bridges mpsc channels <-> WebSocket frames
    async fn connection_loop(
        mut ws_sink: futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
        mut ws_source: futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
        mut outgoing_rx: mpsc::Receiver<Vec<u8>>,
        incoming_tx: mpsc::Sender<Vec<u8>>,
        state: Arc<RwLock<StateMachine>>,
        keepalive_interval_ms: u64,
    ) {
        let keepalive = if keepalive_interval_ms > 0 {
            Some(tokio::time::interval(tokio::time::Duration::from_millis(
                keepalive_interval_ms,
            )))
        } else {
            None
        };

        // We pin an optional interval. If keepalive is disabled, we never tick.
        let mut keepalive_interval = keepalive;

        loop {
            tokio::select! {
                // Outgoing: user sends data through channel -> forward as Binary frame
                Some(data) = outgoing_rx.recv() => {
                    if ws_sink.send(Message::Binary(data.into())).await.is_err() {
                        break;
                    }
                }

                // Incoming: WebSocket frame arrives -> forward to user channel
                frame = ws_source.next() => {
                    match frame {
                        Some(Ok(Message::Binary(data))) => {
                            if incoming_tx.send(data.to_vec()).await.is_err() {
                                break; // receiver dropped
                            }
                        }
                        Some(Ok(Message::Ping(payload))) => {
                            // Auto-respond with Pong
                            if ws_sink.send(Message::Pong(payload)).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            break; // server closed or stream ended
                        }
                        Some(Err(_)) => {
                            break; // connection error
                        }
                        // Ignore Text, Pong, Frame
                        _ => {}
                    }
                }

                // Keep-alive ping
                _ = async {
                    if let Some(ref mut interval) = keepalive_interval {
                        interval.tick().await
                    } else {
                        // Never resolves
                        std::future::pending::<tokio::time::Instant>().await
                    }
                } => {
                    if ws_sink.send(Message::Ping(vec![].into())).await.is_err() {
                        break;
                    }
                }
            }
        }

        // Mark disconnected
        let mut s = state.write().await;
        s.disconnect();
    }

    /// Disconnect from the server
    pub async fn disconnect(&mut self) {
        {
            let mut state = self.state.write().await;
            state.disconnect();
        }
        self.outgoing_tx = None;
        self.incoming_rx = None;
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
    }

    /// Send a binary message (must start with protocol version byte)
    pub async fn send(&self, data: Vec<u8>) -> Result<(), TransportError> {
        // Validate version byte
        if data.is_empty() || data[0] != PROTOCOL_VERSION {
            return Err(TransportError::InvalidPacket);
        }

        if data.len() > self.config.max_message_size {
            return Err(TransportError::PacketTooLarge {
                size: data.len(),
                max: self.config.max_message_size,
            });
        }

        let tx = self
            .outgoing_tx
            .as_ref()
            .ok_or(TransportError::NotConnected)?;
        tx.send(data)
            .await
            .map_err(|_| TransportError::SendFailed)?;
        Ok(())
    }

    /// Receive a binary message (blocking)
    pub async fn receive(&mut self) -> Result<Vec<u8>, TransportError> {
        let rx = self
            .incoming_rx
            .as_mut()
            .ok_or(TransportError::NotConnected)?;
        rx.recv().await.ok_or(TransportError::ConnectionClosed)
    }

    /// Try to receive without blocking
    pub fn try_receive(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        let rx = self
            .incoming_rx
            .as_mut()
            .ok_or(TransportError::NotConnected)?;
        match rx.try_recv() {
            Ok(data) => Ok(Some(data)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Err(TransportError::ConnectionClosed),
        }
    }

    /// Get the server URL
    pub fn url(&self) -> &str {
        &self.config.url
    }
}

impl Transport for WebSocketClient {
    async fn connect(&mut self) -> Result<(), TransportError> {
        self.connect().await
    }

    async fn disconnect(&mut self) {
        self.disconnect().await
    }

    async fn send(&self, data: Vec<u8>) -> Result<(), TransportError> {
        self.send(data).await
    }

    async fn receive(&mut self) -> Result<Vec<u8>, TransportError> {
        self.receive().await
    }

    fn try_receive(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        self.try_receive()
    }

    async fn state(&self) -> ConnectionState {
        self.state().await
    }

    async fn is_connected(&self) -> bool {
        self.is_connected().await
    }

    fn url(&self) -> &str {
        self.url()
    }
}

/// Transport errors
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Not connected")]
    NotConnected,

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Invalid state for operation")]
    InvalidState,

    #[error("Failed to send")]
    SendFailed,

    #[error("Invalid packet (missing or wrong version byte)")]
    InvalidPacket,

    #[error("Packet too large: {size} bytes (max: {max})")]
    PacketTooLarge { size: usize, max: usize },

    #[error("Connection timeout")]
    Timeout,

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Builder for protocol-framed messages (version byte + payload)
pub struct DatagramBuilder {
    data: Vec<u8>,
}

impl DatagramBuilder {
    /// Create a new datagram with version header
    pub fn new() -> Self {
        Self {
            data: vec![PROTOCOL_VERSION],
        }
    }

    /// Add raw bytes
    pub fn bytes(mut self, bytes: &[u8]) -> Self {
        self.data.extend_from_slice(bytes);
        self
    }

    /// Add a u8
    pub fn u8(mut self, val: u8) -> Self {
        self.data.push(val);
        self
    }

    /// Add a u16 (little-endian)
    pub fn u16(mut self, val: u16) -> Self {
        self.data.extend_from_slice(&val.to_le_bytes());
        self
    }

    /// Add a u32 (little-endian)
    pub fn u32(mut self, val: u32) -> Self {
        self.data.extend_from_slice(&val.to_le_bytes());
        self
    }

    /// Build the datagram
    pub fn build(self) -> Vec<u8> {
        self.data
    }
}

impl Default for DatagramBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn test_client_state() {
        let config = TransportConfig {
            url: "ws://example.com".to_string(),
            ..Default::default()
        };

        let client = WebSocketClient::new(config);
        assert_eq!(client.state().await, ConnectionState::Disconnected);
    }

    #[test]
    fn test_datagram_builder() {
        let datagram = DatagramBuilder::new()
            .u8(1)
            .u16(42)
            .u32(12345)
            .bytes(b"hello")
            .build();

        assert_eq!(datagram[0], PROTOCOL_VERSION);
        assert_eq!(datagram[1], 1);
        assert_eq!(u16::from_le_bytes([datagram[2], datagram[3]]), 42);
    }

    #[tokio::test]
    async fn test_send_requires_version_byte() {
        let config = TransportConfig {
            url: "ws://example.com".to_string(),
            ..Default::default()
        };
        let client = WebSocketClient::new(config);

        // Not connected, should fail
        let result = client.send(vec![PROTOCOL_VERSION, 0x01]).await;
        assert!(result.is_err());

        // Empty data should fail with InvalidPacket
        let client2 = WebSocketClient::new(TransportConfig {
            url: "ws://example.com".to_string(),
            ..Default::default()
        });
        // We can't test send properly without connecting, but we validate the
        // NotConnected error path
        let result = client2.send(vec![]).await;
        assert!(matches!(result, Err(TransportError::InvalidPacket)));
    }

    #[tokio::test]
    async fn test_connect_invalid_url() {
        let config = TransportConfig {
            url: "not-a-url".to_string(),
            ..Default::default()
        };
        let mut client = WebSocketClient::new(config);
        let result = client.connect().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_connect_wrong_scheme() {
        let config = TransportConfig {
            url: "http://example.com".to_string(),
            ..Default::default()
        };
        let mut client = WebSocketClient::new(config);
        let result = client.connect().await;
        assert!(matches!(result, Err(TransportError::ConnectionFailed(_))));
    }

    #[tokio::test]
    async fn test_receive_not_connected() {
        let config = TransportConfig {
            url: "ws://example.com".to_string(),
            ..Default::default()
        };
        let mut client = WebSocketClient::new(config);
        let result = client.receive().await;
        assert!(matches!(result, Err(TransportError::NotConnected)));
    }

    #[tokio::test]
    async fn test_disconnect_is_idempotent() {
        let config = TransportConfig {
            url: "ws://example.com".to_string(),
            ..Default::default()
        };
        let mut client = WebSocketClient::new(config);
        client.disconnect().await;
        assert_eq!(client.state().await, ConnectionState::Disconnected);
        client.disconnect().await;
        assert_eq!(client.state().await, ConnectionState::Disconnected);
    }
}
