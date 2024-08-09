use super::api::{ApiError, Context};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::TcpStream;
use thiserror::Error;
use tungstenite::{stream::MaybeTlsStream, Message, WebSocket};

const STREAM_API_NEGOTIATION_URL: &str =
    "https://streams.easee.com/hubs/products/negotiate?negotiateVersion=1";
const WSS_URL: &str = "wss://streams.easee.com/hubs/products";

#[derive(Clone, Debug, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "camelCase")]
struct NegotiateResponse {
    negotiate_version: u16,
    connection_id: String,
    connection_token: String,
}

#[derive(Debug, Error)]
pub enum NegotiateError {
    #[error("API error: {0}")]
    ApiError(#[from] ApiError),

    #[error("WS error: {0}")]
    TungsteniteError(#[from] tungstenite::Error),
}

#[derive(Debug, Error)]
pub enum RecvError {
    #[error("Bad message type")]
    BadMessageType,

    #[error("Invalid json: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("WS error: {0}")]
    TungsteniteError(#[from] tungstenite::Error),
}

pub struct Stream {
    sock: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl Stream {
    pub fn open(ctx: &mut Context) -> Result<Stream, NegotiateError> {
        let r: NegotiateResponse = ctx.post_raw(STREAM_API_NEGOTIATION_URL, &())?;

        let token = ctx.auth_token();
        let wss_url = format!(
            "{}?id={}&access_token={}",
            WSS_URL, r.connection_token, token
        );

        let resp = tungstenite::client::connect(&wss_url);

        if let Err(tungstenite::Error::Http(he)) = &resp {
            eprintln!(
                "Response: {}",
                std::str::from_utf8(&he.body().as_ref().unwrap()).unwrap()
            );
        }

        let mut stream = Stream { sock: resp?.0 };
        stream.send(json!({ "protocol": "json", "version": 1 }))?;

        Ok(stream)
    }

    pub fn send<T: Serialize>(&mut self, msg: T) -> Result<(), tungstenite::Error> {
        let mut msg = serde_json::to_string(&msg).unwrap();
        msg.push('\x1E');
        self.sock.send(Message::Text(msg))
    }

    pub fn recv(&mut self) -> Result<Vec<serde_json::Value>, RecvError> {
        let msg = self.sock.read()?;
        let Message::Text(txt) = msg else {
            return Err(RecvError::BadMessageType);
        };

        let msgs = txt
            .split_terminator('\x1E')
            .filter_map(|s| serde_json::from_str(s).ok())
            .collect();

        Ok(msgs)
    }
}
