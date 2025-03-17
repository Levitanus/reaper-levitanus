use vizia::prelude::*;

use crate::ffmpeg::RenderSettings;

use super::{FrontMessage, FrontState, State, StateMessage};

pub fn render_settings(cx: &mut Context) {
    Binding::new(
        cx,
        FrontState::gui_state.then(State::render_settings),
        |cx, lens| {
            let settings = lens.get(cx);
            VStack::new(cx, |cx| {
                Label::new(cx, "render settings").font_weight(500);
                HStack::new(cx, |cx| {
                    Label::new(cx, "Video muxer:");
                    Dropdown::new(
                        cx,
                        |cx| {
                            Button::new(cx, |cx| {
                                Label::new(
                                    cx,
                                    FrontState::gui_state
                                        .then(State::render_settings)
                                        .map(|settings| settings.muxer.name.clone()),
                                )
                            })
                            .on_press(|ex| ex.emit(PopupEvent::Switch));
                        },
                        |cx| {
                            List::new(cx, FrontState::muxers, |cx, selected, map_ref| {
                                Label::new(cx, map_ref.get(cx).name).hoverable(false);
                            })
                            .selectable(Selectable::Single)
                            .selected(FrontState::gui_state.then(State::render_settings).map(|settings|settings.));
                            // ScrollView::new(cx, |cx| {
                            //     VStack::new(cx, |cx| {
                            //         for muxer in FrontState::muxers
                            //             .get(cx)
                            //             .iter()
                            //             .filter(|mux| mux.video_codec.is_some())
                            //         {
                            //             let name = muxer.name.clone();
                            //             let json = serde_json::to_string(muxer)
                            //                 .expect("can not serialize muxer");
                            //             Button::new(cx, move |cx| Label::new(cx, name)).on_press(
                            //                 move |ex| {
                            //                     ex.emit(FrontMessage::Mutate(
                            //                         StateMessage::VideoMuxer(json.clone()),
                            //                     ))
                            //                 },
                            //             );
                            //         }
                            //     });
                            // });
                        },
                    );
                });
            });
        },
    );
}
