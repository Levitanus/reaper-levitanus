use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod envelope_snap;
pub mod ffmpeg;
pub mod gui;
pub mod normalization;
pub mod reaper_interraction;
pub mod sample_editor;

pub static EXT_SECTION: &str = "Levitanus";

#[derive(Debug, Error, Serialize, Deserialize, Clone)]
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
    #[error("ReaperError, original message: {0}")]
    Reaper(String),
    #[error("ReaperError, original message: {0}")]
    Render(String),
}

#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum SampleEditorError {
    #[error("EmptyRegion: no audio sources in region found.")]
    EmptyRegion,
}
