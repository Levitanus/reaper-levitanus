use thiserror::Error;

// pub mod ffmpeg;
pub mod envelope_snap;
pub mod normalization;

#[derive(Debug, Error)]
pub enum LevitanusError {
    #[error("unexpected behavior: {0}")]
    Unexpected(String),
}
