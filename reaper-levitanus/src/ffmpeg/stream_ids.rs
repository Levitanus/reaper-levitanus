use std::collections::HashMap;

#[derive(Debug)]
pub struct StreamId {
    names: HashMap<String, usize>,
}
impl StreamId {
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
        }
    }
    pub fn id(&mut self, id: impl AsRef<str>) -> String {
        let id = id.as_ref().to_string();
        match self.names.get_mut(&id) {
            None => {
                let new_id = format!("{}0", id);
                self.names.insert(id, 0);
                new_id
            }
            Some(index) => {
                *index += 1;
                format!("{}{}", id, index)
            }
        }
    }
    pub fn input_id(&mut self) -> String {
        self.id("v").split_off(1)
    }
    pub fn input_video_id(&mut self) -> String {
        self.input_id() + ":v"
    }
    pub fn input_audio_id(&mut self) -> String {
        self.input_id() + ":a"
    }
}

#[test]
fn test_named_id() {
    let mut id = StreamId::new();
    assert_eq!(id.id("video"), "video0".to_string());
    assert_eq!(id.id("video"), "video1".to_string());
    assert_eq!(id.id("background"), "background0".to_string());
    assert_eq!(id.id("background"), "background1".to_string());
    assert_eq!(id.id("video"), "video2".to_string());
}
