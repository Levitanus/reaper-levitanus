use thiserror::Error;

pub mod envelope_snap;
pub mod ffmpeg;
pub mod normalization;

#[derive(Debug, Error)]
pub enum LevitanusError {
    #[error("unexpected behavior: {0}")]
    Unexpected(String),
    #[error("Front-end didn't got init state. Got message: {0}")]
    FrontInitialization(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Posion error: {0}")]
    Poison(String),
}
