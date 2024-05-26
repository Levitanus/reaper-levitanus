use std::error::Error;

use int_enum::IntEnum;
use log::{debug, info};
use rea_rs::Reaper;
use regex::Regex;

pub fn register_envelope_actions(rpr: &mut Reaper) -> Result<(), Box<dyn Error>> {
    let snap_re = Regex::new(r"(?<first>DEFSHAPE \d )(?<range>-?\d+) (?<snap>-?\d)")?;
    let snap_def = snap_re.clone();
    let snap_semi = snap_re.clone();
    let snap_50 = snap_re.clone();
    let snap_25 = snap_re.clone();
    let snap_10 = snap_re.clone();
    let snap_5 = snap_re.clone();
    let snap_1 = snap_re.clone();
    let range = snap_re.clone();
    rpr.register_action(
        "LEVITANUS_PITCHEVN_SNAP_OFF",
        "Take Pitch envelope snap OFF (Levitanus)",
        move |_| envelope_snap_range(EnvelopeChange::Snap(EnvelopeSnap::Off), snap_re.clone()),
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_PITCHEVN_SNAP_DEFAULT",
        "Take Pitch envelope snap DEFAULT (Levitanus)",
        move |_| {
            envelope_snap_range(
                EnvelopeChange::Snap(EnvelopeSnap::Default),
                snap_def.clone(),
            )
        },
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_PITCHEVN_SNAP_SEMINOTE",
        "Take Pitch envelope snap SEMINOTE (Levitanus)",
        move |_| {
            envelope_snap_range(
                EnvelopeChange::Snap(EnvelopeSnap::Semitone),
                snap_semi.clone(),
            )
        },
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_PITCHEVN_SNAP_50_CENTS",
        "Take Pitch envelope snap 50 CENTS (Levitanus)",
        move |_| envelope_snap_range(EnvelopeChange::Snap(EnvelopeSnap::Cents50), snap_50.clone()),
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_PITCHEVN_SNAP_25_CENTS",
        "Take Pitch envelope snap 25 CENTS (Levitanus)",
        move |_| envelope_snap_range(EnvelopeChange::Snap(EnvelopeSnap::Cents25), snap_25.clone()),
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_PITCHEVN_SNAP_10_CENTS",
        "Take Pitch envelope snap 10 CENTS (Levitanus)",
        move |_| envelope_snap_range(EnvelopeChange::Snap(EnvelopeSnap::Cents10), snap_10.clone()),
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_PITCHEVN_SNAP_5_CENTS",
        "Take Pitch envelope snap 5 CENTS (Levitanus)",
        move |_| envelope_snap_range(EnvelopeChange::Snap(EnvelopeSnap::Cents5), snap_5.clone()),
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_PITCHEVN_SNAP_1_CENT",
        "Take Pitch envelope snap 1 CENT1 (Levitanus)",
        move |_| envelope_snap_range(EnvelopeChange::Snap(EnvelopeSnap::Cent), snap_1.clone()),
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_PITCHEVN_RANGE",
        "Set Take Pitch envelope range (Levitanus)",
        move |_| {
            let rpr = Reaper::get();
            let resp =
                rpr.get_user_inputs("Enter pitch envelope range", vec!["positive integer"], None)?;
            let rng: u8 = resp
                .get("positive integer")
                .expect("no key positive integer")
                .parse()?;
            envelope_snap_range(EnvelopeChange::Range(rng), range.clone())
        },
        None,
    )?;
    Ok(())
}

fn envelope_snap_range(change: EnvelopeChange, re: Regex) -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get_mut();
    let mut pr = rpr.current_project();
    for idx in 0..pr.n_selected_items() {
        let mut item = pr
            .get_selected_item_mut(idx)
            .ok_or("Out of bounds of selected items")?;
        let mut take = item.active_take_mut();
        for env_idx in 0..take.n_envelopes() {
            let mut env = take
                .get_envelope_mut(env_idx)
                .ok_or("Out of bound for envelope idx")?;
            debug!("{}", env.name());
            if !env.name().contains("Pitch") {
                continue;
            }
            let chunk = env.state_chunk();
            let mut new_chunk = Vec::new();
            debug!("{}", chunk);
            for line in chunk.split("\n") {
                if let Some(cap) = re.captures(line) {
                    match change {
                        EnvelopeChange::Snap(snap) => {
                            new_chunk.push(format!(
                                "{}{} {}",
                                cap.name("first").ok_or("no name first")?.as_str(),
                                cap.name("range").ok_or("no name first")?.as_str(),
                                snap as i32
                            ));
                            info!("set pitch snap to {:?}", snap);
                        }
                        EnvelopeChange::Range(range) => {
                            new_chunk.push(format!(
                                "{}{} {}",
                                cap.name("first").ok_or("no name first")?.as_str(),
                                range as i32,
                                cap.name("snap").ok_or("no name first")?.as_str(),
                            ));
                            info!("set pitch range to {}", range);
                        }
                    }
                } else {
                    new_chunk.push(line.to_string())
                }
            }
            let new_chunk = new_chunk.join("\n");
            env.set_state_chunk(&new_chunk, true)?;
            info!("new chunk is: {}", new_chunk);
        }
    }

    Ok(())
}

#[repr(i32)]
#[derive(Debug, IntEnum, Clone, Copy)]
enum EnvelopeSnap {
    Default = -1,
    Off = 0,
    Semitone = 1,
    Cents50 = 2,
    Cents25 = 3,
    Cents10 = 4,
    Cents5 = 5,
    Cent = 6,
}

#[derive(Debug, Clone, Copy)]
enum EnvelopeChange {
    Snap(EnvelopeSnap),
    Range(u8),
}
