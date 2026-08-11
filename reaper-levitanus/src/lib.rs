use rea_rs::ReaRsError;
use thiserror::Error;

pub mod envelope_snap;
// pub mod ffmpeg;
pub mod autofreeze;
pub mod ffmpeg_new;
pub mod normalization;
pub mod otio_export;
pub mod utils;
pub mod score;

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
    #[error("EnumError: can not set value '{0}' to enum")]
    Enum(String),
    #[error("KeyError: {0} has no key {1}")]
    KeyError(String, String),
    #[error("ReaperError")]
    Reaper(#[from] ReaRsError),
    #[error("RenderError, original message: {0}")]
    Render(String),
    #[error("TrackValidationError, Can not find track with GUID {0}")]
    TrackValidationError(String),
    #[error("Invalid object")]
    InvalidObject,
}

type LevitanusResult<T> = Result<T, LevitanusError>;
