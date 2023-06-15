#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]

mod editor;
pub mod synth;

use log::{LevelFilter, Log, RecordBuilder};
use nih_plug::params::persist::PersistentField;
use nih_plug::prelude::*;
use nih_plug_vizia::ViziaState;
use simplelog::{Config, WriteLogger};
use std::fs::File;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, RwLock};
use synth::PxtoneSynth;

pub struct PtPlug {
    params: Arc<PtParams>,
    initted: bool,

    logger: Option<Arc<WriteLogger<File>>>,

    file_select_recv: Receiver<FileSelectPayload>,
    file_select_send: SyncSender<FileSelectPayload>,

    synth: PxtoneSynth,
}

struct FileSelectPayload {
    file_data: Vec<u8>,
    file_name: String,
}

// many types in pxtone-sys are not marked Send
unsafe impl Send for PtPlug {}

#[derive(Params)]
pub struct PtParams {
    #[persist = "editor-state"]
    editor_state: Arc<ViziaState>,

    #[persist = "file-data"]
    file_data: Arc<RwLock<Option<Vec<u8>>>>,

    #[persist = "woice-name"]
    woice_name: Arc<RwLock<Option<String>>>,

    #[id = "gain"]
    pub gain: FloatParam,

    pub num_tones: AtomicUsize,
}

impl Default for PtPlug {
    fn default() -> Self {
        let mut log_file = None;
        log_file = log_file.or_else(|| File::create("pxtone-plug.log").ok());

        let logger = log_file.map(|f| WriteLogger::new(LevelFilter::Info, Config::default(), f));

        if let Some(logger) = &logger {
            logger.log(
                &RecordBuilder::new()
                    .args(format_args!("LOG STARTED"))
                    .build(),
            );
        }

        let synth = PxtoneSynth::new().unwrap();

        if let Some(logger) = &logger {
            logger.log(
                &RecordBuilder::new()
                    .args(format_args!("serv.init OK"))
                    .build(),
            );
        }

        let (send, recv) = std::sync::mpsc::sync_channel(1);

        Self {
            params: Arc::new(PtParams::default()),
            initted: false,

            synth,

            logger: logger.map(|bl| Arc::new(*bl)),

            file_select_recv: recv,
            file_select_send: send,
        }
    }
}

impl Default for PtParams {
    fn default() -> Self {
        Self {
            editor_state: editor::default_state(),
            gain: FloatParam::new("Gain", -10.0, FloatRange::Linear { min: -30.0, max: 0.0 })
                .with_smoother(SmoothingStyle::Linear(3.0))
                .with_step_size(0.01)
                .with_unit(" dB"),
            file_data: Arc::default(),
            woice_name: Arc::default(),
            num_tones: AtomicUsize::new(0),
        }
    }
}

impl PtPlug {
    fn load_file(&mut self, file: FileSelectPayload) {
        self.load_file_data(&file.file_data);

        self.params.file_data.set(Some(file.file_data));
        self.params.woice_name.set(Some(file.file_name));

        if self.initted {
            self.init_woice();
        }
    }

    fn load_file_data(&mut self, file_data: &[u8]) {
        self.synth.load_woice(file_data).unwrap();

        if let Some(logger) = &self.logger {
            logger.log(
                &RecordBuilder::new()
                    .args(format_args!("woice.PTV_Read OK"))
                    .build(),
            );
        }
    }

    fn init_woice(&mut self) {
        self.synth.tone_ready().unwrap();
    }
}

impl Plugin for PtPlug {
    const NAME: &'static str = "pxtone Plug";
    const VENDOR: &'static str = "PieKing1215";
    const URL: &'static str = "https://github.com/PieKing1215/pxtone-plug";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(
            self.params.clone(),
            self.params.editor_state.clone(),
            self.logger.clone(),
            self.file_select_send.clone(),
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.initted = true;
        self.synth
            .set_audio_format(2, buffer_config.sample_rate)
            .unwrap();

        if !self.synth.loaded() {
            let fd_rw = self.params.file_data.read().unwrap();
            let fd = fd_rw.as_ref().cloned();
            drop(fd_rw);

            if let Some(fd) = fd {
                self.load_file_data(&fd);
            }
        }

        self.init_woice();

        true
    }

    fn reset(&mut self) {
        self.synth.stop_all();
    }

    #[allow(clippy::too_many_lines)] // TODO: split into smaller fns
    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        if !self.initted {
            return ProcessStatus::KeepAlive;
        }

        if let Ok(file_data) = self.file_select_recv.try_recv() {
            self.load_file(file_data);
        }

        self.synth.process(buffer, aux, context, &self.params);

        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for PtPlug {
    const CLAP_ID: &'static str = "me.pieking1215.pxtoneplug";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Plays pxtone woices");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> =
        Some("https://github.com/PieKing1215/pxtone-plug/issues");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

impl Vst3Plugin for PtPlug {
    const VST3_CLASS_ID: [u8; 16] = *b"pxtonePlug______";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(PtPlug);
nih_export_vst3!(PtPlug);
