use std::{
    fs::File,
    sync::{mpsc::SyncSender, Arc},
    thread,
};

use log::{Log, RecordBuilder};
use nih_plug::prelude::Editor;
use nih_plug_vizia::{
    assets, create_vizia_editor,
    vizia::{
        image,
        prelude::*,
        resource::ImageRetentionPolicy,
        views::{Button, Label, VStack},
    },
    ViziaState, ViziaTheming,
};
use rfd::FileDialog;
use simplelog::WriteLogger;

use crate::{FileSelectPayload, PtParams};

#[derive(Lens)]
struct Data {
    params: Arc<PtParams>,
}

impl Model for Data {}

pub(crate) fn default_state() -> Arc<ViziaState> {
    ViziaState::new(|| (300, 250))
}

#[allow(clippy::too_many_lines)]
pub(crate) fn create(
    params: Arc<PtParams>,
    editor_state: Arc<ViziaState>,
    logger: Option<Arc<WriteLogger<File>>>,
    sender: SyncSender<FileSelectPayload>,
) -> Option<Box<dyn Editor>> {
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        assets::register_noto_sans_regular(cx);
        cx.add_stylesheet(
            r#"
            #root {
                background-color: #4E4B61;
                color: #D2CA9C;
            }

            .button {
                background-color: transparent;
                border-width: 0px;
                border-radius: 0px;
            }

            .selectButton {
                width: 18px;
                height: 18px;
                background-image: url("select_btn_18x18.png");
            }

            .button:hover {
                background-color: transparent;
                border-width: 0px;
                border-radius: 0px;
            }

            .selectButton label {
                color: #FFFF80;
            }

            .selectButton:active label {
                color: #FFFF80;
            }

            .textField {
                width: 200px;
                height: 18px;

                background-color: transparent;
                border-width: 0px;
                border-radius: 0px;

                background-image: url("text_field_200x18.png");
                color: #00F080;
            }

            .textField label {
                left: 4px;
                font-size: 14;
            }
        "#,
        )
        .unwrap();

        cx.set_image_loader(|cx, path| {
            let mut load = |buf| {
                cx.load_image(
                    path.to_owned(),
                    image::load_from_memory_with_format(buf, image::ImageFormat::Png).unwrap(),
                    ImageRetentionPolicy::Forever,
                );
            };

            match path {
                "button_200x20.png" => load(include_bytes!("../res/button_200x20.png")),
                "select_btn_18x18.png" => load(include_bytes!("../res/select_btn_18x18.png")),
                "text_field_200x18.png" => load(include_bytes!("../res/text_field_200x18.png")),
                _ => panic!(),
            }
        });

        Data { params: params.clone() }.build(cx);

        // MenuBar::new(cx, |cx| {
        //     Submenu::new(
        //         cx,
        //         |cx| Label::new(cx, "menu"),
        //         |cx| {
        //             MenuButton::new(cx, |_| {}, |cx| Label::new(cx, "button"));
        //     });
        // });

        // ResizeHandle::new(cx);

        // need to clone since this is a `Fn`, not `FnOnce`
        let logger = logger.clone();
        let sender = sender.clone();

        let select = move || {
            // need to clone since this is a `Fn`, not `FnOnce`
            let logger = logger.clone();
            let sender = sender.clone();

            thread::spawn(move || {
                let path = FileDialog::new()
                    .add_filter("ptVoice", &["ptvoice"])
                    .set_directory("/")
                    .pick_file();

                if let Some(logger) = &logger {
                    logger.log(
                        &RecordBuilder::new()
                            .args(format_args!("select {path:?}"))
                            .build(),
                    );
                }

                if let Some(path) = &path {
                    match std::fs::read(path) {
                        Ok(data) => {
                            if let Err(e) = sender.send(FileSelectPayload {
                                file_data: data,
                                file_name: path.file_name().unwrap().to_string_lossy().to_string(),
                            }) {
                                if let Some(logger) = &logger {
                                    logger.log(
                                        &RecordBuilder::new()
                                            .args(format_args!("File send failed: {e:?}"))
                                            .build(),
                                    );
                                }
                            };
                        },
                        Err(e) => {
                            if let Some(logger) = &logger {
                                logger.log(
                                    &RecordBuilder::new()
                                        .args(format_args!("File read failed: {e:?}"))
                                        .build(),
                                );
                            }
                        },
                    }
                }
            });
        };

        VStack::new(cx, move |cx| {
            Label::new(cx, "pxtone Plug")
                .top(Units::Pixels(4.0))
                .height(Units::Pixels(24.0));

            // debug
            // Label::new(cx, Data::params.map(|p| p.num_tones.load(std::sync::atomic::Ordering::SeqCst)));

            HStack::new(cx, |cx| {
                // TODO: this clone is avoidable

                ZStack::new(cx, |cx| {
                    Label::new(
                        cx,
                        Data::params.map(|p| {
                            p.woice_name
                                .read()
                                .unwrap()
                                .as_ref()
                                .cloned()
                                .unwrap_or("None".into())
                        }),
                    );
                })
                .class("textField");

                Button::new(cx, move |_| select(), Element::new)
                    .left(Units::Pixels(4.0))
                    .class("button")
                    .class("selectButton");
            })
            .width(Units::Pixels(220.0));
        })
        .child_left(Stretch(1.0))
        .child_right(Stretch(1.0))
        .id("root");
    })
}
