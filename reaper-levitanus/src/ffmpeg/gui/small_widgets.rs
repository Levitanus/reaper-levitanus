use log::debug;
use vizia::prelude::*;

use super::FrontState;
use crate::ffmpeg::{gui::FrontMessage, parser::ParsingProgress};

pub fn modal_yes_no<Y, N>(
    cx: &mut Context,
    title: impl AsRef<str>,
    description: impl AsRef<str> + 'static,
    on_yes: Y,
    on_no: N,
) where
    Y: 'static + Fn(&mut EventContext) + Send + Sync + Copy,
    N: 'static + Fn(&mut EventContext) + Send + Sync + Copy,
{
    let description = description.as_ref().to_string();
    Window::popup(cx, true, move |cx| {
        VStack::new(cx, |cx| {
            Label::new(cx, description.clone())
                .width(Pixels(400.0))
                .text_align(TextAlign::Center);
            HStack::new(cx, |cx| {
                let width = Percentage(40.0);
                Button::new(cx, |cx| Label::new(cx, "No"))
                    .on_press(on_no)
                    .width(width);
                Button::new(cx, |cx| Label::new(cx, "Yes"))
                    .on_press(on_yes)
                    .width(width);
            })
            .alignment(Alignment::Center);
        });
    })
    .title(title.as_ref())
    .always_on_top(true)
    .on_close(|_| std::process::exit(0))
    .inner_size((400, 150))
    .resizable(false);
}

pub fn widget_parser(cx: &mut Context) {
    Binding::new(cx, FrontState::parsing_progress, |cx, lens| {
        match lens.get(cx) {
            ParsingProgress::Unparsed => {
                modal_yes_no(
                    cx,
                    "Parse FFMPEG",
                    "FFMPEG muxers, codecs and filters are not yet parsed.\n\
                            Do you wish to parse them now?\n\
                            It will take up to 30 seconds.",
                    |ex| {
                        debug!("pressed yes");
                        ex.emit(FrontMessage::Parse);
                    },
                    |_| std::process::exit(0),
                );
            }
            other => {
                HStack::new(cx, |cx| match other {
                    ParsingProgress::Result(r) => {
                        if let Err(e) = r {
                            Label::new(cx, format!("failed to parse FFMPEG: {}", e))
                                .color(Color::red());
                        }
                        Button::new(cx, |cx| Label::new(cx, "Reparse FFMPEG"))
                            .on_press(|ex| ex.emit(FrontMessage::Parse));
                    }

                    ParsingProgress::Progress(_) => {
                        ProgressBar::horizontal(cx, FrontState::parsing_progress_f32)
                            .width(Percentage(90.0));
                    }
                    ParsingProgress::Unparsed => panic!("this match arm has to be filled earlier"),
                })
                .background_color(Color::rgba(0, 0, 0, 100))
                .height(Pixels(35.0))
                .alignment(Alignment::Center)
                .gap(Stretch(1.0))
                .padding_left(Pixels(20.0))
                .padding_right(Pixels(20.0));
            }
        }
    })
}
