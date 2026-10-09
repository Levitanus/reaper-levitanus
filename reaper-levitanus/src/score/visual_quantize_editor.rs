use std::collections::HashMap;

use anyhow::Result;
use rea_rs::{
    swell_gui::layout::{Insets, WidgetFills, WidgetSize},
    ActionHook, ComboBoxOptions, EventResponse, Reaper, ReaperResult, ReaperWindow, SwellId,
    WindowHandler, WindowSpec,
};

use crate::{LevitanusError, LevitanusResult};

const WINDOW_ID: &'static str = "RSVisualQuantizeEditor";

pub(super) fn toggle_visual_quantize_editor(hook: &mut ActionHook) -> Result<()> {
    let rpr = Reaper::get_mut();
    match hook.toggle_state().unwrap_or(false) {
        true => {
            rpr.register_window_handler(Box::new(VisualQuantizeEditor::new()?))?;
        }
        false => rpr.unregister_window_handler(&WINDOW_ID.into())?,
    }
    Ok(())
}

pub(super) struct VisualQuantizeEditor {
    window: ReaperWindow,
}
impl VisualQuantizeEditor {
    fn new() -> Result<Self> {
        let rpr = Reaper::get();
        let window = rpr.create_window(
            &WindowSpec::new("RS Visual Quantize Editor")
                .size(350, 200)
                .resizable(false),
        )?;
        let ui = window.build_ui()?.with_insets(Insets {
            left: 15,
            top: 15,
            right: 15,
            bottom: 15,
        });
        let row = ui.row(SwellId::new_control(), WidgetSize::new_fill_x(350, 30))?;
        let grid_units = row.combo_box(
            SwellId::new_control(),
            WidgetSize::new_fill_both(150, 30).set_fill_x(WidgetFills::FillPortion(1)),
            ComboBoxOptions::default(),
        )?;
        let mut grid_units_items = HashMap::new();
        for unit in ["1/1", "1/8", "1/16", "1/32", "1/64", "1/128"] {
            let index = grid_units.add_item(unit)?;
            grid_units_items.insert(unit, index);
        }
        row.on_widget_event(grid_units.id(), |event| -> Result<EventResponse> {
            Ok(EventResponse::Handled)
        });
        Ok(Self { window })
    }
}

impl WindowHandler for VisualQuantizeEditor {
    fn window_id(&self) -> rea_rs::WindowId {
        WINDOW_ID.into()
    }

    fn window(&self) -> &rea_rs::ReaperWindow {
        &self.window
    }
}
