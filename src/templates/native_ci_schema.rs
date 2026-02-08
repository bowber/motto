use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ping {
    pub seq: u32,
    pub ok: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pong {
    pub seq: u32,
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Status {
    Ready = 1,
    Busy = 2,
}
