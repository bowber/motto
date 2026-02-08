//! WebTransport Client - Async transport layer

use crate::runtime::codec::PROTOCOL_VERSION;
use crate::runtime::state::{ConnectionState, RetryConfig, StateMachine};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// WebTransport client configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Server URL
    pub url: String,
    /// Retry configuration
    pub retry: RetryConfig,
    /// Connection timeout in milliseconds
    pub connect_timeout_ms: u64,
    /// Keep-alive interval in milliseconds (0 = disabled)
    pub keepalive_interval_ms: u64,
    /// Maximum datagram size
    pub max_datagram_size: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            retry: RetryConfig::default(),
            connect_timeout_ms: 10_000,
            keepalive_interval_ms: 30_000,
            max_datagram_size: 65535,
        }
    }
}

/// WebTransport client
/// 
/// Note: This is a placeholder implementation. Actual WebTransport
/// requires a proper QUIC/HTTP3 implementation like `wtransport` or `h3`.
pub struct WebTransportClient {
    config: TransportConfig,
    state: Arc<RwLock<StateMachine>>,
    outgoing_tx: Option<mpsc::Sender<Vec<u8>>>,
    incoming_rx: Option<mpsc::Receiver<Vec<u8>>>,
}

impl WebTransportClient {
    /// Create a new WebTransport client
    pub fn new(config: TransportConfig) -> Self {
        Self {
            config: config.clone(),
            state: Arc::new(RwLock::new(StateMachine::new(config.retry))),
            outgoing_tx: None,
            incoming_rx: None,
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

    /// Connect to the server
    pub async fn connect(&mut self) -> Result<(), TransportError> {
        {
            let mut state = self.state.write().await;
            state.start_connecting().map_err(|_| TransportError::InvalidState)?;
        }

        // TODO: Implement actual WebTransport connection
        // This would use a library like `wtransport` or `h3`
        //
        // Example with hypothetical wtransport:
        // let endpoint = wtransport::Endpoint::client()?;
        // let connection = endpoint.connect(&self.config.url).await?;

        // Create channels for message passing
        let (outgoing_tx, _outgoing_rx) = mpsc::channel::<Vec<u8>>(100);
        let (incoming_tx, incoming_rx) = mpsc::channel::<Vec<u8>>(100);

        self.outgoing_tx = Some(outgoing_tx);
        self.incoming_rx = Some(incoming_rx);

        // Simulate successful connection
        {
            let mut state = self.state.write().await;
            state.connected().map_err(|_| TransportError::InvalidState)?;
        }

        // Start background tasks for sending/receiving
        let state = Arc::clone(&self.state);
        let _incoming_tx = incoming_tx;
        
        tokio::spawn(async move {
            // TODO: Receive loop
            // while let Some(datagram) = connection.receive_datagram().await {
            //     if incoming_tx.send(datagram).await.is_err() {
            //         break;
            //     }
            // }
            // Mark disconnected
            let mut s = state.write().await;
            s.disconnect();
        });

        Ok(())
    }

    /// Disconnect from the server
    pub async fn disconnect(&mut self) {
        let mut state = self.state.write().await;
        state.disconnect();
        self.outgoing_tx = None;
        self.incoming_rx = None;
    }

    /// Send a datagram
    pub async fn send(&self, data: Vec<u8>) -> Result<(), TransportError> {
        // Validate version byte
        if data.is_empty() || data[0] != PROTOCOL_VERSION {
            return Err(TransportError::InvalidPacket);
        }

        if data.len() > self.config.max_datagram_size {
            return Err(TransportError::PacketTooLarge {
                size: data.len(),
                max: self.config.max_datagram_size,
            });
        }

        let tx = self.outgoing_tx.as_ref().ok_or(TransportError::NotConnected)?;
        tx.send(data).await.map_err(|_| TransportError::SendFailed)?;
        Ok(())
    }

    /// Receive a datagram
    pub async fn receive(&mut self) -> Result<Vec<u8>, TransportError> {
        let rx = self.incoming_rx.as_mut().ok_or(TransportError::NotConnected)?;
        rx.recv().await.ok_or(TransportError::ConnectionClosed)
    }

    /// Try to receive without blocking
    pub fn try_receive(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        let rx = self.incoming_rx.as_mut().ok_or(TransportError::NotConnected)?;
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

/// Builder for WebTransport datagrams
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
            url: "https://example.com".to_string(),
            ..Default::default()
        };

        let client = WebTransportClient::new(config);
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
}
