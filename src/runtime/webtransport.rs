//! WebTransport Client - HTTP/3 over QUIC transport implementation
//!
//! Provides `WebTransportClient` that implements the `Transport` trait using
//! the `wtransport` crate. Messages are exchanged as QUIC datagrams, which
//! map naturally to motto's protocol (version byte + bitcode payload).
//!
//! Gated behind the `webtransport` feature flag.

use crate::runtime::codec::PROTOCOL_VERSION;
use crate::runtime::state::{ConnectionState, StateMachine};
use crate::runtime::transport::{Transport, TransportConfig, TransportError};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

/// WebTransport client backed by QUIC datagrams.
///
/// Connects to a server via WebTransport (https://) and exchanges
/// binary datagrams prefixed with the motto protocol version byte.
///
/// Uses the same channel-bridging pattern as `WebSocketClient`:
/// a background task reads/writes datagrams and bridges them to
/// mpsc channels visible to the caller.
pub struct WebTransportClient {
    config: TransportConfig,
    state: Arc<RwLock<StateMachine>>,
    outgoing_tx: Option<mpsc::Sender<Vec<u8>>>,
    incoming_rx: Option<mpsc::Receiver<Vec<u8>>>,
    /// Handle to the background connection task so we can abort on disconnect
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl WebTransportClient {
    /// Create a new WebTransport client
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

    /// Connect to the server via WebTransport (HTTP/3 over QUIC)
    pub async fn connect(&mut self) -> Result<(), TransportError> {
        // Validate URL before attempting connection
        let url: url::Url = self
            .config
            .url
            .parse()
            .map_err(|e| TransportError::ConnectionFailed(format!("invalid URL: {}", e)))?;

        let scheme = url.scheme();
        if scheme != "https" {
            return Err(TransportError::ConnectionFailed(format!(
                "unsupported URL scheme '{}': WebTransport requires 'https'",
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

        // Build client config with dangerous self-signed cert support for dev
        let client_config = wtransport::ClientConfig::default();

        // Create endpoint and connect with timeout
        let endpoint = wtransport::Endpoint::client(client_config)
            .map_err(|e| TransportError::ConnectionFailed(format!("endpoint error: {}", e)))?;

        let timeout = tokio::time::Duration::from_millis(self.config.connect_timeout_ms);

        let connection = tokio::time::timeout(timeout, endpoint.connect(&self.config.url))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::ConnectionFailed(format!("connect error: {}", e)))?;

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

        // Spawn background task that drives the datagram exchange
        let state = Arc::clone(&self.state);

        let handle = tokio::spawn(async move {
            Self::datagram_loop(connection, outgoing_rx, incoming_tx, state).await;
        });
        self.task_handle = Some(handle);

        Ok(())
    }

    /// Background loop: bridges mpsc channels <-> QUIC datagrams
    async fn datagram_loop(
        connection: wtransport::Connection,
        mut outgoing_rx: mpsc::Receiver<Vec<u8>>,
        incoming_tx: mpsc::Sender<Vec<u8>>,
        state: Arc<RwLock<StateMachine>>,
    ) {
        loop {
            tokio::select! {
                // Outgoing: user sends data through channel -> forward as datagram
                Some(data) = outgoing_rx.recv() => {
                    if connection.send_datagram(data).is_err() {
                        break;
                    }
                }

                // Incoming: datagram arrives -> forward to user channel
                result = connection.receive_datagram() => {
                    match result {
                        Ok(datagram) => {
                            let payload = datagram.payload().to_vec();
                            if incoming_tx.send(payload).await.is_err() {
                                break; // receiver dropped
                            }
                        }
                        Err(_) => {
                            break; // connection error
                        }
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

impl Transport for WebTransportClient {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::state::RetryConfig;
    use pretty_assertions::assert_eq;

    fn test_config(url: &str) -> TransportConfig {
        TransportConfig {
            url: url.to_string(),
            retry: RetryConfig::default(),
            connect_timeout_ms: 5_000,
            keepalive_interval_ms: 0,
            max_message_size: 65535,
        }
    }

    #[tokio::test]
    async fn test_client_initial_state() {
        let client = WebTransportClient::new(test_config("https://localhost:4433"));
        assert_eq!(client.state().await, ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn test_connect_wrong_scheme() {
        let mut client = WebTransportClient::new(test_config("ws://example.com"));
        let result = client.connect().await;
        assert!(matches!(result, Err(TransportError::ConnectionFailed(_))));
        if let Err(TransportError::ConnectionFailed(msg)) = result {
            assert!(msg.contains("https"), "error should mention https: {}", msg);
        }
    }

    #[tokio::test]
    async fn test_connect_invalid_url() {
        let mut client = WebTransportClient::new(test_config("not-a-url"));
        let result = client.connect().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_requires_version_byte() {
        let client = WebTransportClient::new(test_config("https://localhost:4433"));

        // Empty data should fail with InvalidPacket
        let result = client.send(vec![]).await;
        assert!(matches!(result, Err(TransportError::InvalidPacket)));

        // Wrong version byte
        let result = client.send(vec![0xFF, 0x01]).await;
        assert!(matches!(result, Err(TransportError::InvalidPacket)));
    }

    #[tokio::test]
    async fn test_send_not_connected() {
        let client = WebTransportClient::new(test_config("https://localhost:4433"));
        let result = client.send(vec![PROTOCOL_VERSION, 0x01]).await;
        assert!(matches!(result, Err(TransportError::NotConnected)));
    }

    #[tokio::test]
    async fn test_receive_not_connected() {
        let mut client = WebTransportClient::new(test_config("https://localhost:4433"));
        let result = client.receive().await;
        assert!(matches!(result, Err(TransportError::NotConnected)));
    }

    #[tokio::test]
    async fn test_try_receive_not_connected() {
        let mut client = WebTransportClient::new(test_config("https://localhost:4433"));
        let result = client.try_receive();
        assert!(matches!(result, Err(TransportError::NotConnected)));
    }

    #[tokio::test]
    async fn test_disconnect_is_idempotent() {
        let mut client = WebTransportClient::new(test_config("https://localhost:4433"));
        client.disconnect().await;
        assert_eq!(client.state().await, ConnectionState::Disconnected);
        client.disconnect().await;
        assert_eq!(client.state().await, ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn test_url_accessor() {
        let client = WebTransportClient::new(test_config("https://localhost:4433"));
        assert_eq!(client.url(), "https://localhost:4433");
    }

    #[tokio::test]
    async fn test_packet_too_large() {
        let mut config = test_config("https://localhost:4433");
        config.max_message_size = 10;
        let client = WebTransportClient::new(config);

        // Create a packet that's too large (version byte + 10 payload bytes = 11 > 10)
        let mut data = vec![PROTOCOL_VERSION];
        data.extend_from_slice(&[0u8; 10]);
        // Not connected, but size check happens first... actually version check happens first,
        // then size check, then the NotConnected check. Let's verify the size check path:
        // Looking at send(): version check -> size check -> tx check
        let result = client.send(data).await;
        assert!(matches!(result, Err(TransportError::PacketTooLarge { .. })));
    }

    // Integration tests that require a real QUIC server are ignored
    #[tokio::test]
    #[ignore]
    async fn test_connect_to_real_server() {
        let mut client = WebTransportClient::new(test_config("https://localhost:4433"));
        let result = client.connect().await;
        assert!(result.is_ok());
        assert!(client.is_connected().await);
        client.disconnect().await;
        assert_eq!(client.state().await, ConnectionState::Disconnected);
    }
}
