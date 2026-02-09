//! Sniff Module - Transport traffic inspection and debugging
//!
//! Provides two operating modes:
//! - **Tap**: Connect as a WS client and display all incoming frames
//! - **Proxy**: Stand up a WS relay (listen -> upstream) and log frames in both directions
//!
//! Output formats: `pretty` (human-readable), `json`, `hex`
//!
//! Optional schema-aware decoding uses the IR manifest to map discriminants to
//! message type names and display field structures.

use crate::ir::manifest::{RouterVariant, SchemaManifest};
use crate::runtime::codec::PROTOCOL_VERSION;
use futures_util::{SinkExt, StreamExt};
use std::fmt;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

// ─── Configuration ───────────────────────────────────────────────────────────

/// Sniff session configuration
#[derive(Debug, Clone)]
pub struct SniffConfig {
    /// Output format
    pub format: OutputFormat,
    /// Schema manifest for decode mode (None = raw bytes only)
    pub manifest: Option<SchemaManifest>,
    /// Whether to decode packets using the schema
    pub decode: bool,
    /// Redact field values (show structure only)
    pub redact: bool,
    /// Maximum bytes to display per packet (0 = unlimited)
    pub max_bytes: usize,
}

/// Output format for sniff display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable colored output
    Pretty,
    /// Machine-readable JSON
    Json,
    /// Raw hex dump
    Hex,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pretty" => Ok(Self::Pretty),
            "json" => Ok(Self::Json),
            "hex" => Ok(Self::Hex),
            _ => Err(format!(
                "unknown format '{}': expected 'pretty', 'json', or 'hex'",
                s
            )),
        }
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pretty => write!(f, "pretty"),
            Self::Json => write!(f, "json"),
            Self::Hex => write!(f, "hex"),
        }
    }
}

// ─── Frame representation ────────────────────────────────────────────────────

/// Direction of a captured frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Client -> Server (outbound / upstream)
    Send,
    /// Server -> Client (inbound / downstream)
    Recv,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send => write!(f, ">>>"),
            Self::Recv => write!(f, "<<<"),
        }
    }
}

/// A captured frame with metadata
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    /// Sequence number
    pub seq: u64,
    /// Direction
    pub direction: Direction,
    /// Time offset from sniff session start (milliseconds)
    pub time_ms: u64,
    /// Frame kind
    pub kind: FrameKind,
}

/// Classification of a captured frame
#[derive(Debug, Clone)]
pub enum FrameKind {
    /// Binary data frame
    Binary {
        data: Vec<u8>,
        decoded: Option<DecodedPacket>,
    },
    /// Text frame
    Text(String),
    /// Ping frame
    Ping(Vec<u8>),
    /// Pong frame
    Pong(Vec<u8>),
    /// Close frame
    Close(Option<String>),
}

/// Decoded packet info (from schema-aware decoding)
#[derive(Debug, Clone)]
pub struct DecodedPacket {
    /// Protocol version byte
    pub version: u8,
    /// Whether version matches expected
    pub version_ok: bool,
    /// Router discriminant (if present)
    pub discriminant: Option<u16>,
    /// Resolved message type name (if schema available)
    pub message_type: Option<String>,
    /// Field descriptions (if schema available)
    pub fields: Vec<DecodedField>,
    /// Payload bytes after header
    pub payload_size: usize,
}

/// A decoded field description (structural, not value-level)
#[derive(Debug, Clone)]
pub struct DecodedField {
    /// Field name
    pub name: String,
    /// Type reference string
    pub type_ref: String,
    /// Wire type description
    pub wire_type: String,
    /// Field index
    pub index: usize,
}

// ─── Frame formatting ────────────────────────────────────────────────────────

impl CapturedFrame {
    /// Format this frame according to the given config
    pub fn format(&self, config: &SniffConfig) -> String {
        match config.format {
            OutputFormat::Pretty => self.format_pretty(config),
            OutputFormat::Json => self.format_json(config),
            OutputFormat::Hex => self.format_hex(config),
        }
    }

    fn format_pretty(&self, config: &SniffConfig) -> String {
        let mut out = String::new();

        // Header line: [seq] direction time_ms kind
        out.push_str(&format!(
            "[{:>6}] {} {:>8}ms ",
            self.seq, self.direction, self.time_ms
        ));

        match &self.kind {
            FrameKind::Binary { data, decoded } => {
                let display_data = truncate_bytes(data, config.max_bytes);
                out.push_str(&format!("BIN ({} bytes)", data.len()));

                if let Some(dec) = decoded {
                    let ver_mark = if dec.version_ok { "ok" } else { "MISMATCH" };
                    out.push_str(&format!(" v={:#04x}[{}]", dec.version, ver_mark));

                    if let Some(disc) = dec.discriminant {
                        out.push_str(&format!(" disc={}", disc));
                    }
                    if let Some(ref msg_type) = dec.message_type {
                        out.push_str(&format!(" type={}", msg_type));
                    }
                    if !dec.fields.is_empty() && !config.redact {
                        out.push_str("\n         fields:");
                        for field in &dec.fields {
                            out.push_str(&format!(
                                "\n           [{}] {}: {} ({})",
                                field.index, field.name, field.type_ref, field.wire_type
                            ));
                        }
                    } else if !dec.fields.is_empty() && config.redact {
                        out.push_str(&format!(" ({} fields, redacted)", dec.fields.len()));
                    }
                    out.push_str(&format!(" payload={} bytes", dec.payload_size));
                } else {
                    // Raw hex preview
                    out.push(' ');
                    out.push_str(&hex_preview(&display_data, 32));
                }
            }
            FrameKind::Text(text) => {
                let display = if config.redact {
                    format!("TEXT ({} chars, redacted)", text.len())
                } else if config.max_bytes > 0 && text.len() > config.max_bytes {
                    format!("TEXT ({}): {}...", text.len(), &text[..config.max_bytes])
                } else {
                    format!("TEXT ({}): {}", text.len(), text)
                };
                out.push_str(&display);
            }
            FrameKind::Ping(payload) => {
                out.push_str(&format!("PING ({} bytes)", payload.len()));
            }
            FrameKind::Pong(payload) => {
                out.push_str(&format!("PONG ({} bytes)", payload.len()));
            }
            FrameKind::Close(reason) => {
                if let Some(reason) = reason {
                    out.push_str(&format!("CLOSE: {}", reason));
                } else {
                    out.push_str("CLOSE");
                }
            }
        }

        out
    }

    fn format_json(&self, config: &SniffConfig) -> String {
        let direction = match self.direction {
            Direction::Send => "send",
            Direction::Recv => "recv",
        };

        let (kind_str, size, hex_data, decoded_json) = match &self.kind {
            FrameKind::Binary { data, decoded } => {
                let display_data = truncate_bytes(data, config.max_bytes);
                let hex = if config.redact {
                    String::new()
                } else {
                    hex::encode(&display_data)
                };
                let dec = decoded.as_ref().map(|d| {
                    let fields_json: Vec<String> = if config.redact {
                        vec![]
                    } else {
                        d.fields
                            .iter()
                            .map(|f| {
                                format!(
                                    r#"{{"index":{},"name":"{}","type":"{}","wire":"{}"}}"#,
                                    f.index, f.name, f.type_ref, f.wire_type
                                )
                            })
                            .collect()
                    };
                    format!(
                        r#","decoded":{{"version":{},"version_ok":{},"discriminant":{},"message_type":{},"fields":[{}],"payload_size":{}}}"#,
                        d.version,
                        d.version_ok,
                        d.discriminant.map_or("null".to_string(), |v| v.to_string()),
                        d.message_type.as_ref().map_or("null".to_string(), |v| format!("\"{}\"", v)),
                        fields_json.join(","),
                        d.payload_size,
                    )
                });
                ("binary".to_string(), data.len(), hex, dec)
            }
            FrameKind::Text(text) => {
                let display = if config.redact {
                    String::new()
                } else if config.max_bytes > 0 && text.len() > config.max_bytes {
                    text[..config.max_bytes].to_string()
                } else {
                    text.clone()
                };
                ("text".to_string(), text.len(), display, None)
            }
            FrameKind::Ping(p) => ("ping".to_string(), p.len(), String::new(), None),
            FrameKind::Pong(p) => ("pong".to_string(), p.len(), String::new(), None),
            FrameKind::Close(reason) => (
                "close".to_string(),
                0,
                reason.clone().unwrap_or_default(),
                None,
            ),
        };

        format!(
            r#"{{"seq":{},"direction":"{}","time_ms":{},"kind":"{}","size":{},"data":"{}"{}}}"#,
            self.seq,
            direction,
            self.time_ms,
            kind_str,
            size,
            hex_data,
            decoded_json.unwrap_or_default(),
        )
    }

    fn format_hex(&self, config: &SniffConfig) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "[{:>6}] {} {:>8}ms ",
            self.seq, self.direction, self.time_ms
        ));

        match &self.kind {
            FrameKind::Binary { data, .. } => {
                let display_data = truncate_bytes(data, config.max_bytes);
                out.push_str(&format!("BIN ({} bytes)\n", data.len()));
                out.push_str(&hex_dump(&display_data));
            }
            FrameKind::Text(text) => {
                out.push_str(&format!("TEXT ({} bytes)\n", text.len()));
                let bytes = if config.max_bytes > 0 && text.len() > config.max_bytes {
                    &text.as_bytes()[..config.max_bytes]
                } else {
                    text.as_bytes()
                };
                out.push_str(&hex_dump(bytes));
            }
            FrameKind::Ping(p) => {
                out.push_str(&format!("PING ({} bytes)\n", p.len()));
                out.push_str(&hex_dump(p));
            }
            FrameKind::Pong(p) => {
                out.push_str(&format!("PONG ({} bytes)\n", p.len()));
                out.push_str(&hex_dump(p));
            }
            FrameKind::Close(reason) => {
                out.push_str(&format!("CLOSE: {}", reason.as_deref().unwrap_or("(none)")));
            }
        }

        out
    }
}

// ─── Packet decoding ─────────────────────────────────────────────────────────

/// Attempt structural decode of a binary frame using the schema manifest
pub fn decode_packet(data: &[u8], manifest: &SchemaManifest) -> Option<DecodedPacket> {
    if data.is_empty() {
        return None;
    }

    let version = data[0];
    let version_ok = version == PROTOCOL_VERSION;

    // After version byte, the motto protocol uses bitcode encoding.
    // We can't decode bitcode fields without generated Decode impls,
    // but we CAN provide structural info from the manifest.
    //
    // For the router-based protocol, the discriminant is the first field
    // encoded by bitcode. Without decoding bitcode, we show the raw payload.
    // If the packet has at least 3 bytes (version + 2-byte discriminant),
    // we attempt to read a LE u16 discriminant for heuristic display.

    let payload = &data[1..];
    let payload_size = payload.len();

    // Heuristic: try to read a LE u16 discriminant from first 2 payload bytes
    let (discriminant, message_type, fields) = if payload.len() >= 2 {
        let disc = u16::from_le_bytes([payload[0], payload[1]]);
        let router = manifest.router.as_ref();
        let variant: Option<&RouterVariant> =
            router.and_then(|r| r.variants.iter().find(|v| v.discriminant == disc));

        if let Some(variant) = variant {
            // Resolve fields from the matching message definition
            let msg_fields: Vec<DecodedField> = manifest
                .messages
                .iter()
                .find(|m| m.name == variant.message_type)
                .map(|m| {
                    m.fields
                        .iter()
                        .map(|f| DecodedField {
                            name: f.name.clone(),
                            type_ref: f.type_ref.clone(),
                            wire_type: format!("{:?}", f.wire_type),
                            index: f.index,
                        })
                        .collect()
                })
                .unwrap_or_default();

            (Some(disc), Some(variant.message_type.clone()), msg_fields)
        } else {
            // Discriminant didn't match any router variant — might not be
            // a router-framed packet, or might be bitcode-encoded differently
            (Some(disc), None, vec![])
        }
    } else {
        (None, None, vec![])
    };

    Some(DecodedPacket {
        version,
        version_ok,
        discriminant,
        message_type,
        fields,
        payload_size,
    })
}

// ─── Tap mode ────────────────────────────────────────────────────────────────

/// Run tap mode: connect to a WS or WebTransport endpoint and display all incoming frames
pub async fn run_tap(
    url: &str,
    config: &SniffConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Dispatch to WebTransport tap if URL scheme is https://
    #[cfg(feature = "webtransport")]
    if url.starts_with("https://") {
        return run_tap_webtransport(url, config).await;
    }

    #[cfg(not(feature = "webtransport"))]
    if url.starts_with("https://") {
        return Err("WebTransport URLs (https://) require the 'webtransport' feature flag".into());
    }

    eprintln!("motto sniff: connecting to {} ...", url);

    let (ws_stream, _response) = tokio_tungstenite::connect_async(url).await?;
    let (_ws_sink, mut ws_source) = ws_stream.split();

    eprintln!("motto sniff: connected, listening for frames (Ctrl+C to stop)");
    eprintln!();

    let start = Instant::now();
    let mut seq: u64 = 0;

    loop {
        tokio::select! {
            frame = ws_source.next() => {
                match frame {
                    Some(Ok(msg)) => {
                        seq += 1;
                        let time_ms = start.elapsed().as_millis() as u64;
                        let kind = ws_message_to_kind(&msg, config);
                        let captured = CapturedFrame {
                            seq,
                            direction: Direction::Recv,
                            time_ms,
                            kind,
                        };
                        println!("{}", captured.format(config));
                    }
                    Some(Err(e)) => {
                        eprintln!("motto sniff: connection error: {}", e);
                        break;
                    }
                    None => {
                        eprintln!("motto sniff: connection closed");
                        break;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nmotto sniff: interrupted");
                break;
            }
        }
    }

    eprintln!(
        "motto sniff: session ended ({} frames captured in {}ms)",
        seq,
        start.elapsed().as_millis()
    );

    Ok(())
}

// ─── WebTransport tap mode ────────────────────────────────────────────────────

/// Run tap mode over WebTransport (QUIC datagrams)
#[cfg(feature = "webtransport")]
async fn run_tap_webtransport(
    url: &str,
    config: &SniffConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    eprintln!("motto sniff: connecting via WebTransport to {} ...", url);

    let client_config = wtransport::ClientConfig::default();
    let endpoint = wtransport::Endpoint::client(client_config)?;
    let connection = endpoint.connect(url).await?;

    eprintln!("motto sniff: WebTransport connected, listening for datagrams (Ctrl+C to stop)");
    eprintln!();

    let start = Instant::now();
    let mut seq: u64 = 0;

    loop {
        tokio::select! {
            result = connection.receive_datagram() => {
                match result {
                    Ok(datagram) => {
                        seq += 1;
                        let time_ms = start.elapsed().as_millis() as u64;
                        let data = datagram.payload().to_vec();
                        let decoded = if config.decode {
                            config.manifest.as_ref().and_then(|m| decode_packet(&data, m))
                        } else {
                            None
                        };
                        let captured = CapturedFrame {
                            seq,
                            direction: Direction::Recv,
                            time_ms,
                            kind: FrameKind::Binary { data, decoded },
                        };
                        println!("{}", captured.format(config));
                    }
                    Err(e) => {
                        eprintln!("motto sniff: WebTransport error: {}", e);
                        break;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nmotto sniff: interrupted");
                break;
            }
        }
    }

    eprintln!(
        "motto sniff: session ended ({} datagrams captured in {}ms)",
        seq,
        start.elapsed().as_millis()
    );

    Ok(())
}

// ─── Proxy mode ──────────────────────────────────────────────────────────────

/// Run proxy mode: listen for client connections, relay to upstream, log all frames
pub async fn run_proxy(
    listen_addr: &str,
    upstream_url: &str,
    config: &SniffConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(listen_addr).await?;

    eprintln!(
        "motto sniff proxy: listening on {} -> {}",
        listen_addr, upstream_url
    );
    eprintln!("motto sniff proxy: waiting for client connections (Ctrl+C to stop)");
    eprintln!();

    let start = Instant::now();
    let mut connection_count: u64 = 0;

    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, peer_addr)) => {
                        connection_count += 1;
                        let conn_id = connection_count;
                        eprintln!("motto sniff proxy: [conn {}] accepted from {}", conn_id, peer_addr);

                        let upstream_url = upstream_url.to_string();
                        let config = config.clone();
                        let session_start = start;

                        tokio::spawn(async move {
                            if let Err(e) = handle_proxy_connection(
                                stream,
                                &upstream_url,
                                &config,
                                conn_id,
                                session_start,
                            ).await {
                                eprintln!("motto sniff proxy: [conn {}] error: {}", conn_id, e);
                            }
                            eprintln!("motto sniff proxy: [conn {}] closed", conn_id);
                        });
                    }
                    Err(e) => {
                        eprintln!("motto sniff proxy: accept error: {}", e);
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nmotto sniff proxy: interrupted");
                break;
            }
        }
    }

    eprintln!(
        "motto sniff proxy: session ended ({} connections in {}ms)",
        connection_count,
        start.elapsed().as_millis()
    );

    Ok(())
}

/// Handle a single proxied connection
async fn handle_proxy_connection(
    stream: tokio::net::TcpStream,
    upstream_url: &str,
    config: &SniffConfig,
    conn_id: u64,
    session_start: Instant,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Accept WebSocket handshake from the client
    let client_ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut client_sink, mut client_source) = client_ws.split();

    // Connect to upstream
    let (upstream_ws, _response) = tokio_tungstenite::connect_async(upstream_url).await?;
    let (mut upstream_sink, mut upstream_source) = upstream_ws.split();

    eprintln!(
        "motto sniff proxy: [conn {}] upstream connected to {}",
        conn_id, upstream_url
    );

    let mut seq: u64 = 0;

    loop {
        tokio::select! {
            // Client -> Upstream (Send direction)
            frame = client_source.next() => {
                match frame {
                    Some(Ok(msg)) => {
                        seq += 1;
                        let time_ms = session_start.elapsed().as_millis() as u64;
                        let kind = ws_message_to_kind(&msg, config);
                        let captured = CapturedFrame {
                            seq,
                            direction: Direction::Send,
                            time_ms,
                            kind,
                        };
                        println!("{}", captured.format(config));

                        // Forward to upstream
                        if upstream_sink.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(_)) | None => break,
                }
            }

            // Upstream -> Client (Recv direction)
            frame = upstream_source.next() => {
                match frame {
                    Some(Ok(msg)) => {
                        seq += 1;
                        let time_ms = session_start.elapsed().as_millis() as u64;
                        let kind = ws_message_to_kind(&msg, config);
                        let captured = CapturedFrame {
                            seq,
                            direction: Direction::Recv,
                            time_ms,
                            kind,
                        };
                        println!("{}", captured.format(config));

                        // Forward to client
                        if client_sink.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(_)) | None => break,
                }
            }
        }
    }

    eprintln!(
        "motto sniff proxy: [conn {}] {} frames relayed",
        conn_id, seq
    );

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Convert a tungstenite Message to our FrameKind, optionally decoding
fn ws_message_to_kind(msg: &Message, config: &SniffConfig) -> FrameKind {
    match msg {
        Message::Binary(data) => {
            let data_vec = data.to_vec();
            let decoded = if config.decode {
                config
                    .manifest
                    .as_ref()
                    .and_then(|m| decode_packet(&data_vec, m))
            } else {
                None
            };
            FrameKind::Binary {
                data: data_vec,
                decoded,
            }
        }
        Message::Text(text) => FrameKind::Text(text.to_string()),
        Message::Ping(payload) => FrameKind::Ping(payload.to_vec()),
        Message::Pong(payload) => FrameKind::Pong(payload.to_vec()),
        Message::Close(frame) => FrameKind::Close(frame.as_ref().map(|f| f.reason.to_string())),
        // Frame variant (raw) — treat as binary
        _ => FrameKind::Binary {
            data: vec![],
            decoded: None,
        },
    }
}

/// Truncate bytes to max_bytes (0 = no truncation)
fn truncate_bytes(data: &[u8], max_bytes: usize) -> Vec<u8> {
    if max_bytes > 0 && data.len() > max_bytes {
        data[..max_bytes].to_vec()
    } else {
        data.to_vec()
    }
}

/// Short hex preview (up to `max` bytes, then "...")
fn hex_preview(data: &[u8], max: usize) -> String {
    if data.is_empty() {
        return "(empty)".to_string();
    }
    let show = if data.len() > max { &data[..max] } else { data };
    let hex_str: String = show
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ");
    if data.len() > max {
        format!("{}...", hex_str)
    } else {
        hex_str
    }
}

/// Classic hex dump (16 bytes per line, with ASCII sidebar)
fn hex_dump(data: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in data.chunks(16).enumerate() {
        // Offset
        out.push_str(&format!("  {:08x}  ", i * 16));

        // Hex bytes
        for (j, byte) in chunk.iter().enumerate() {
            out.push_str(&format!("{:02x} ", byte));
            if j == 7 {
                out.push(' ');
            }
        }

        // Padding for incomplete last line
        let remaining = 16 - chunk.len();
        for j in 0..remaining {
            out.push_str("   ");
            if chunk.len() + j == 7 {
                out.push(' ');
            }
        }

        // ASCII
        out.push_str(" |");
        for byte in chunk {
            if byte.is_ascii_graphic() || *byte == b' ' {
                out.push(*byte as char);
            } else {
                out.push('.');
            }
        }
        out.push('|');
        out.push('\n');
    }
    out
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SniffConfig {
        SniffConfig {
            format: OutputFormat::Pretty,
            manifest: None,
            decode: false,
            redact: false,
            max_bytes: 0,
        }
    }

    #[test]
    fn test_output_format_parse() {
        assert_eq!(
            "pretty".parse::<OutputFormat>().unwrap(),
            OutputFormat::Pretty
        );
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!("hex".parse::<OutputFormat>().unwrap(), OutputFormat::Hex);
        assert_eq!(
            "Pretty".parse::<OutputFormat>().unwrap(),
            OutputFormat::Pretty
        );
        assert!("unknown".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn test_output_format_display() {
        assert_eq!(OutputFormat::Pretty.to_string(), "pretty");
        assert_eq!(OutputFormat::Json.to_string(), "json");
        assert_eq!(OutputFormat::Hex.to_string(), "hex");
    }

    #[test]
    fn test_direction_display() {
        assert_eq!(Direction::Send.to_string(), ">>>");
        assert_eq!(Direction::Recv.to_string(), "<<<");
    }

    #[test]
    fn test_format_pretty_binary() {
        let config = test_config();
        let frame = CapturedFrame {
            seq: 1,
            direction: Direction::Recv,
            time_ms: 42,
            kind: FrameKind::Binary {
                data: vec![0x01, 0x02, 0x03, 0xff],
                decoded: None,
            },
        };
        let output = frame.format(&config);
        assert!(output.contains("<<<"));
        assert!(output.contains("42ms"));
        assert!(output.contains("BIN (4 bytes)"));
        assert!(output.contains("01 02 03 ff"));
    }

    #[test]
    fn test_format_pretty_text() {
        let config = test_config();
        let frame = CapturedFrame {
            seq: 2,
            direction: Direction::Send,
            time_ms: 100,
            kind: FrameKind::Text("hello world".to_string()),
        };
        let output = frame.format(&config);
        assert!(output.contains(">>>"));
        assert!(output.contains("TEXT (11)"));
        assert!(output.contains("hello world"));
    }

    #[test]
    fn test_format_pretty_close() {
        let config = test_config();
        let frame = CapturedFrame {
            seq: 3,
            direction: Direction::Recv,
            time_ms: 200,
            kind: FrameKind::Close(Some("goodbye".to_string())),
        };
        let output = frame.format(&config);
        assert!(output.contains("CLOSE: goodbye"));
    }

    #[test]
    fn test_format_json_binary() {
        let config = SniffConfig {
            format: OutputFormat::Json,
            ..test_config()
        };
        let frame = CapturedFrame {
            seq: 1,
            direction: Direction::Send,
            time_ms: 10,
            kind: FrameKind::Binary {
                data: vec![0xab, 0xcd],
                decoded: None,
            },
        };
        let output = frame.format(&config);
        assert!(output.contains(r#""seq":1"#));
        assert!(output.contains(r#""direction":"send""#));
        assert!(output.contains(r#""kind":"binary""#));
        assert!(output.contains(r#""data":"abcd""#));
    }

    #[test]
    fn test_format_hex_binary() {
        let config = SniffConfig {
            format: OutputFormat::Hex,
            ..test_config()
        };
        let frame = CapturedFrame {
            seq: 1,
            direction: Direction::Recv,
            time_ms: 5,
            kind: FrameKind::Binary {
                data: vec![0x01, 0x02, 0x03],
                decoded: None,
            },
        };
        let output = frame.format(&config);
        assert!(output.contains("BIN (3 bytes)"));
        assert!(output.contains("00000000"));
        assert!(output.contains("01 02 03"));
    }

    #[test]
    fn test_max_bytes_truncation() {
        let config = SniffConfig {
            max_bytes: 2,
            ..test_config()
        };
        let frame = CapturedFrame {
            seq: 1,
            direction: Direction::Recv,
            time_ms: 0,
            kind: FrameKind::Binary {
                data: vec![0x01, 0x02, 0x03, 0x04, 0x05],
                decoded: None,
            },
        };
        let output = frame.format(&config);
        // Should show 5 bytes total but hex preview only shows truncated data
        assert!(output.contains("BIN (5 bytes)"));
        assert!(output.contains("01 02"));
    }

    #[test]
    fn test_redact_text() {
        let config = SniffConfig {
            redact: true,
            ..test_config()
        };
        let frame = CapturedFrame {
            seq: 1,
            direction: Direction::Send,
            time_ms: 0,
            kind: FrameKind::Text("secret data".to_string()),
        };
        let output = frame.format(&config);
        assert!(output.contains("redacted"));
        assert!(!output.contains("secret data"));
    }

    #[test]
    fn test_hex_dump_formatting() {
        let data = b"Hello, World! This is a test of the hex dump formatter.";
        let output = hex_dump(data);
        // Should have offset markers
        assert!(output.contains("00000000"));
        // Should have ASCII sidebar
        assert!(output.contains("|Hello, World! Th|"));
    }

    #[test]
    fn test_hex_preview() {
        assert_eq!(hex_preview(&[], 32), "(empty)");
        assert_eq!(hex_preview(&[0x01, 0x02], 32), "01 02");
        assert_eq!(hex_preview(&[0x01, 0x02, 0x03], 2), "01 02...");
    }

    #[test]
    fn test_decode_packet_empty() {
        let manifest = SchemaManifest {
            meta: crate::ir::manifest::ManifestMeta {
                name: "Test".to_string(),
                fingerprint: String::new(),
                version_byte: 1,
                generated_at: String::new(),
                motto_version: String::new(),
            },
            types: vec![],
            messages: vec![],
            enums: vec![],
            type_aliases: vec![],
            router: None,
        };
        assert!(decode_packet(&[], &manifest).is_none());
    }

    #[test]
    fn test_decode_packet_version_only() {
        let manifest = SchemaManifest {
            meta: crate::ir::manifest::ManifestMeta {
                name: "Test".to_string(),
                fingerprint: String::new(),
                version_byte: 1,
                generated_at: String::new(),
                motto_version: String::new(),
            },
            types: vec![],
            messages: vec![],
            enums: vec![],
            type_aliases: vec![],
            router: None,
        };

        let packet = decode_packet(&[PROTOCOL_VERSION], &manifest).unwrap();
        assert!(packet.version_ok);
        assert_eq!(packet.payload_size, 0);
        assert!(packet.discriminant.is_none());
    }

    #[test]
    fn test_decode_packet_version_mismatch() {
        let manifest = SchemaManifest {
            meta: crate::ir::manifest::ManifestMeta {
                name: "Test".to_string(),
                fingerprint: String::new(),
                version_byte: 1,
                generated_at: String::new(),
                motto_version: String::new(),
            },
            types: vec![],
            messages: vec![],
            enums: vec![],
            type_aliases: vec![],
            router: None,
        };

        let packet = decode_packet(&[0xFF, 0x00, 0x01], &manifest).unwrap();
        assert!(!packet.version_ok);
    }

    #[test]
    fn test_decode_packet_with_router() {
        use crate::ir::manifest::*;

        let manifest = SchemaManifest {
            meta: ManifestMeta {
                name: "Test".to_string(),
                fingerprint: String::new(),
                version_byte: 1,
                generated_at: String::new(),
                motto_version: String::new(),
            },
            types: vec![],
            messages: vec![MessageDef {
                name: "PlayerMove".to_string(),
                fields: vec![
                    FieldManifest {
                        name: "x".to_string(),
                        index: 0,
                        type_ref: "f32".to_string(),
                        wire_type: WireType::Fixed32,
                        offset: Some(0),
                        size: Some(4),
                        optional: false,
                        default: None,
                        docs: None,
                    },
                    FieldManifest {
                        name: "y".to_string(),
                        index: 1,
                        type_ref: "f32".to_string(),
                        wire_type: WireType::Fixed32,
                        offset: Some(4),
                        size: Some(4),
                        optional: false,
                        default: None,
                        docs: None,
                    },
                ],
                fixed_size: Some(8),
                min_size: 8,
                alignment: 4,
                docs: None,
                generics: vec![],
            }],
            enums: vec![],
            type_aliases: vec![],
            router: Some(RouterManifest {
                name: "TestRouter".to_string(),
                variants: vec![RouterVariant {
                    name: "PlayerMove".to_string(),
                    message_type: "PlayerMove".to_string(),
                    discriminant: 0,
                    docs: None,
                }],
                docs: None,
            }),
        };

        // version byte + discriminant 0x0000 + some payload
        let data = vec![PROTOCOL_VERSION, 0x00, 0x00, 0xAA, 0xBB];
        let packet = decode_packet(&data, &manifest).unwrap();
        assert!(packet.version_ok);
        assert_eq!(packet.discriminant, Some(0));
        assert_eq!(packet.message_type.as_deref(), Some("PlayerMove"));
        assert_eq!(packet.fields.len(), 2);
        assert_eq!(packet.fields[0].name, "x");
        assert_eq!(packet.fields[1].name, "y");
    }
}
