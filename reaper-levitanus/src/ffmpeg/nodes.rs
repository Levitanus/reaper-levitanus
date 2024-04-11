use std::{path::PathBuf, time::Duration};

use rea_rs::Position;
use serde::{Deserialize, Serialize};

use super::filters::Filter;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum NodeContent {
    Filter(Filter),
    Input {
        file: PathBuf,
        source_offset: Position,
        length: Duration,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Node {
    pub inputs: Vec<Pin>,
    pub outputs: Vec<Pin>,
    pub content: NodeContent,
}
impl Node {
    pub fn _get_name(&self) -> String {
        match &self.content {
            NodeContent::Filter(f) => f.name().into(),
            NodeContent::Input {
                file,
                source_offset: _,
                length: _,
            } => file
                .file_name()
                .expect("no base filename")
                .to_str()
                .expect("can not convert path to string")
                .to_string(),
        }
    }
    pub fn connect_sink(
        &mut self,
        other: &mut Node,
        sink_index: usize,
        source_index: usize,
    ) -> Result<(), String> {
        let sink = match self.inputs.get(sink_index) {
            Some(sink) => sink,
            None => return Err(format!("can not get sink with index: {sink_index}")),
        };
        let source = match other.outputs.get(source_index) {
            Some(source) => source,
            None => return Err(format!("can not get sink with index: {sink_index}")),
        };
        let new_sink = sink.clone().with_target(Some(source.get_name()));
        let new_source = source.clone().with_target(Some(sink.get_name()));
        self.inputs[sink_index] = new_sink;
        other.outputs[source_index] = new_source;
        Ok(())
    }
    pub fn connect_source(
        &mut self,
        other: &mut Node,
        source_index: usize,
        sink_index: usize,
    ) -> Result<(), String> {
        let sink = match other.inputs.get(sink_index) {
            Some(sink) => sink,
            None => return Err(format!("can not get sink with index: {sink_index}")),
        };
        let source = match self.outputs.get(source_index) {
            Some(source) => source,
            None => return Err(format!("can not get sink with index: {sink_index}")),
        };
        let new_sink = sink.clone().with_target(Some(source.get_name()));
        let new_source = source.clone().with_target(Some(sink.get_name()));
        other.inputs[sink_index] = new_sink;
        self.outputs[source_index] = new_source;
        Ok(())
    }
    pub fn _get_sink_target(&self, sink_index: usize) -> Result<Option<String>, String> {
        match self.inputs.get(sink_index) {
            Some(sink) => Ok(sink.get_target()),
            None => Err(format!("no sink with index: {sink_index}")),
        }
    }
    pub fn _get_sink_name(&self, sink_index: usize) -> Result<String, String> {
        match self.inputs.get(sink_index) {
            Some(sink) => Ok(sink.get_name()),
            None => Err(format!("no sink with index: {sink_index}")),
        }
    }
    pub fn _get_source_target(&self, source_index: usize) -> Result<Option<String>, String> {
        match self.outputs.get(source_index) {
            Some(source) => Ok(source.get_target()),
            None => Err(format!("no source with index: {source_index}")),
        }
    }
    pub fn _get_source_name(&self, source_index: usize) -> Result<String, String> {
        match self.outputs.get(source_index) {
            Some(source) => Ok(source.get_name()),
            None => Err(format!("no source with index: {source_index}")),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Pin {
    Video {
        name: String,
        target: Option<String>,
    },
    Audio {
        name: String,
        target: Option<String>,
    },
}
impl Pin {
    pub fn get_name(&self) -> String {
        match self {
            Pin::Video { name, target: _ } => name.clone(),
            Pin::Audio { name, target: _ } => name.clone(),
        }
    }
    pub fn get_target(&self) -> Option<String> {
        match self {
            Pin::Video { name: _, target } => target.clone(),
            Pin::Audio { name: _, target } => target.clone(),
        }
    }
    pub fn with_target(self, target: Option<String>) -> Self {
        match self {
            Pin::Video { name, target: _ } => Pin::Video { name, target },
            Pin::Audio { name, target: _ } => Pin::Audio { name, target },
        }
    }
    pub fn _connect(self, other: Pin) -> Result<(Self, Self), String> {
        match self {
            Pin::Video { name, target: _ } => match other {
                Pin::Video {
                    name: other_name,
                    target: _,
                } => Ok((
                    Pin::Video {
                        name: name.clone(),
                        target: Some(other_name.clone()),
                    },
                    Pin::Video {
                        name: other_name,
                        target: Some(name),
                    },
                )),
                Pin::Audio {
                    name: other_name,
                    target: _,
                } => Err(format!(
                    "can not connect Video Pin {name} to Audio Pin {other_name}"
                )),
            },
            Pin::Audio { name, target: _ } => match other {
                Pin::Audio {
                    name: other_name,
                    target: _,
                } => Ok((
                    Pin::Audio {
                        name: name.clone(),
                        target: Some(other_name.clone()),
                    },
                    Pin::Audio {
                        name: other_name,
                        target: Some(name),
                    },
                )),
                Pin::Video {
                    name: other_name,
                    target: _,
                } => Err(format!(
                    "can not connect Audio Pin {name} to Video Pin {other_name}"
                )),
            },
        }
    }
}
