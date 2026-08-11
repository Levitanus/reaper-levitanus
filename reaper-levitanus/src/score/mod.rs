use log::{debug, warn};
use rea_rs::{MidiMessage, MidiNoteEvent, Notation, NotationMessage, Reaper};
use serde::{Deserialize, Serialize};

mod dom;

#[derive(Debug, Serialize, Deserialize)]
struct NoteWithNotation {
    note: MidiNoteEvent,
    notation: Option<Vec<String>>,
}

pub fn test_notation() -> anyhow::Result<()> {
    let Some(item) = Reaper::get().current_project().get_selected_item(0)? else {
        warn!("no selected item");
        return Ok(());
    };
    let take = item.active_take()?;
    if !take.is_midi()? {
        warn!("take is not MIDI");
        return Ok(());
    }
    let iterator = take.iter_midi(None)?;

    let mut notes: Vec<NoteWithNotation> = iterator
        .clone()
        .filter_notes()
        .map(|event| NoteWithNotation {
            note: event,
            notation: None,
        })
        .collect();
    let mut track_notations = Vec::new();
    for event in iterator.filter_all_sys() {
        if let Some(msg) = NotationMessage::from_raw(event.message().get_raw()) {
            match msg.notation() {
                Notation::Note {
                    channel,
                    note: n_note,
                    tokens,
                } => {
                    for note in notes.iter_mut() {
                        if note.note.start_in_ppq == event.ppq_position()
                            && note.note.channel == channel
                            && note.note.note == n_note
                        {
                            note.notation = Some(tokens);
                            break;
                        }
                    }
                }
                Notation::Track(n) => track_notations.push((event.ppq_position(), n)),
                Notation::Unknown(n) => warn!("unknown notation: {:?}", n),
            }
        }
    }
    debug!("NOTES WITH NOTATIONS:\n{:#?}", notes);
    debug!("TRACK NOTATIONS:\n{:#?}", track_notations);

    Ok(())
}
