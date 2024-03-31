use std::error::Error;

use rea_rs::{Reaper, UndoFlags, Volume};

pub fn normalize_all_takes_on_selected_items(
    common_gain: Option<bool>,
) -> Result<(), Box<dyn Error>> {
    let common_gain = common_gain.unwrap_or(true);
    let rpr = Reaper::get_mut();
    let mut pr = rpr.current_project();
    pr.begin_undo_block();
    let mut max_gain: f64 = f64::INFINITY;
    for item_idx in 0..pr.n_selected_items() {
        let mut item = match pr.get_selected_item_mut(item_idx) {
            Some(item) => item,
            None => return Err("can not get selected item".into()),
        };
        let length = item.length();
        for take_idx in 0..item.n_takes() {
            let mut take = match item.get_take_mut(take_idx) {
                Some(take) => take,
                None => return Err(format!("can not get take with index {take_idx}").into()),
            };
            let start = take.start_offset();
            let end = start + length;
            let norm_amount = take.source().unwrap().calculate_normalization(
                rea_rs::SourceNoramlizeUnit::TruePeak,
                Volume::from(1.0),
                start,
                end,
            );
            max_gain = max_gain.min(norm_amount.get());
            if !common_gain {
                take.set_volume(norm_amount);
            }
        }
    }
    if common_gain {
        for item_idx in 0..pr.n_selected_items() {
            let mut item = match pr.get_selected_item_mut(item_idx) {
                Some(item) => item,
                None => return Err("can not get selected item".into()),
            };
            for take_idx in 0..item.n_takes() {
                let mut take = match item.get_take_mut(take_idx) {
                    Some(take) => take,
                    None => return Err(format!("can not get take with index {take_idx}").into()),
                };
                take.set_volume(max_gain.into());
            }
        }
    }
    pr.end_undo_block("Normalize all takes in selected items", UndoFlags::all());
    rpr.update_arrange();
    Ok(())
}
