use vizia::prelude::*;

use crate::ffmpeg::{options::Muxer, RenderSettings};

use super::{FrontMessage, FrontState, Widgets};

#[derive(Debug, Lens, Clone, Data)]
pub struct RenderSettingsWidget {
    muxers: Vec<String>,
    muxer: usize,
}
impl RenderSettingsWidget {
    pub fn new(render_settings: &RenderSettings, muxers: &Vec<Muxer>) -> Self {
        let muxers: Vec<String> = muxers.iter().map(|mux| mux.name.clone()).collect();
        let muxer = muxers
            .iter()
            .enumerate()
            .find(|(_idx, m)| *m == &render_settings.muxer)
            .unwrap_or((0, &String::default()))
            .0;
        Self { muxers, muxer }
    }
}

pub fn render_settings(cx: &mut Context) {
    Binding::new(
        cx,
        FrontState::widgets.then(Widgets::render_settings),
        |cx, lens| {
            // let settings = lens.get(cx);
            VStack::new(cx, |cx| {
                Label::new(cx, "render settings").font_weight(500);
                HStack::new(cx, |cx| {
                    Label::new(cx, "muxer:");
                    ComboBox::new(cx, lens.map(|l| l.muxers.clone()), lens.map(|l| l.muxer));
                });
            });
        },
    );
}
