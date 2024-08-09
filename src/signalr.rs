use serde_json::{json, Value};
use thiserror::Error;

use crate::stream::RecvError;

/* This entire module can be rewritten in two lines when
https://github.com/serde-rs/serde/issues/745
is merged */

#[derive(Debug)]
pub enum Message {
    Empty,
    Invocation {
        target: String,
        arguments: Vec<Value>,
    },
    InvocationResult {
        id: String,
        result: serde_json::Value,
    },
    Ping,
    Other(serde_json::Value),
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Expecting object, received {0}")]
    ExpectingObject(Value),

    #[error("Missing `type` key")]
    MissingTypeKey,

    #[error("`type` is not a number")]
    TypeNotANumber,

    #[error("Unknown type {0}")]
    UnknownType(u64),

    #[error("Missing expected key {0}")]
    MissingKey(&'static str),

    #[error("Expecting string")]
    ExpectingString,

    #[error("Expecting array")]
    ExpectingArray,
}

impl Message {
    pub fn from_json(msg: Value) -> Result<Self, ParseError> {
        let Some(obj) = msg.as_object() else {
            return Err(ParseError::ExpectingObject(msg));
        };
        if obj.is_empty() {
            return Ok(Message::Empty);
        }
        let typ = obj
            .get("type")
            .ok_or(ParseError::MissingTypeKey)?
            .as_number()
            .and_then(|n| n.as_u64())
            .ok_or(ParseError::TypeNotANumber)?;

        match typ {
            1 => Ok(Message::Invocation {
                target: obj
                    .get("target")
                    .ok_or(ParseError::MissingKey("target"))?
                    .as_str()
                    .ok_or(ParseError::ExpectingString)?
                    .to_owned(),
                arguments: obj
                    .get("arguments")
                    .ok_or(ParseError::MissingKey("arguments"))?
                    .as_array()
                    .ok_or(ParseError::ExpectingArray)?
                    .to_owned(),
            }),
            3 => Ok(Message::InvocationResult {
                id: obj
                    .get("invocationId")
                    .ok_or(ParseError::MissingKey("invocationId"))?
                    .as_str()
                    .ok_or(ParseError::ExpectingString)?
                    .to_owned(),
                result: obj
                    .get("result")
                    .ok_or(ParseError::MissingKey("result"))?
                    .to_owned(),
            }),
            6 => Ok(Message::Ping),
            _ => Ok(Message::Other(msg)),
        }
    }
}

#[derive(Debug, Error)]
pub enum StreamError {
    #[error("Parse error: {0}")]
    ParseError(#[from] ParseError),

    #[error("Recv error: {0}")]
    StreamError(#[from] RecvError),
}

pub struct Stream {
    buffer: Vec<serde_json::Value>,
    ws: super::stream::Stream,
}

impl Stream {
    pub fn from_ws(ws: super::stream::Stream) -> Self {
        Self { ws, buffer: vec![] }
    }

    pub fn recv(&mut self) -> Result<Message, StreamError> {
        while self.buffer.is_empty() {
            self.buffer = self.ws.recv()?;
            self.buffer.reverse();
        }

        let json = self.buffer.pop().unwrap();
        Ok(Message::from_json(json)?)
    }

    pub fn invoke(
        &mut self,
        target: &str,
        args: serde_json::Value,
    ) -> Result<(), tungstenite::Error> {
        self.ws.send(json!( { "arguments": args,
                                  "invocationId": "0",
                                  "target": target,
                                  "type": 1} ))
    }
}
