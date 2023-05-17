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
        prelude::*,
        views::{Button, Label, VStack},
    },
    widgets::ResizeHandle,
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
    ViziaState::new(|| (200, 150))
}

pub(crate) fn create(
    params: Arc<PtParams>,
    editor_state: Arc<ViziaState>,
    logger: Option<Arc<WriteLogger<File>>>,
    sender: SyncSender<FileSelectPayload>,
) -> Option<Box<dyn Editor>> {
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        assets::register_noto_sans_regular(cx);

        Data { params: params.clone() }.build(cx);

        ResizeHandle::new(cx);

        // need to clone since this is a `Fn`, not `FnOnce`
        let logger = logger.clone();
        let sender = sender.clone();

        VStack::new(cx, move |cx| {
            Label::new(cx, "pxtone Plug");

            // TODO: this clone is avoidable
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

            // debug
            // Label::new(cx, Data::params.map(|p| p.num_tones.load(std::sync::atomic::Ordering::SeqCst)));

            Button::new(
                cx,
                move |_| {
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
                                        file_name: path
                                            .file_name()
                                            .unwrap()
                                            .to_string_lossy()
                                            .to_string(),
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
                },
                |cx| Label::new(cx, "Select Woice"),
            );
        })
        .child_left(Stretch(1.0))
        .child_right(Stretch(1.0));
    })
}
