#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]

mod editor;

use log::{LevelFilter, Log, RecordBuilder};
use nih_plug::params::persist::PersistentField;
use nih_plug::prelude::*;
use nih_plug_vizia::ViziaState;
use pxtone_sys::{
    pxtnDescriptor, pxtnError_get_string, pxtnPulse_Frequency, pxtnService, pxtnVOICETONE,
    pxtnWoice,
};
use simplelog::{Config, WriteLogger};
use std::ffi::CStr;
use std::fs::File;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, RwLock};

struct PtPlug {
    params: Arc<PtParams>,
    sample_rate: Option<f32>,

    master_gain: Smoother<f32>,

    logger: Option<Arc<WriteLogger<File>>>,

    file_select_recv: Receiver<FileSelectPayload>,
    file_select_send: SyncSender<FileSelectPayload>,

    pxtn_serv: pxtnService,
    pxtn_freq: pxtnPulse_Frequency,

    woice_state: WoiceState,
}

struct FileSelectPayload {
    file_data: Vec<u8>,
    file_name: String,
}

enum WoiceState {
    Unloaded,
    Loaded {
        pxtn_woice: pxtnWoice,
        pxtn_time_pan: [i32; pxtone_sys::pxtnMAX_CHANNEL as _],
        pxtn_vol_pan: [i32; pxtone_sys::pxtnMAX_CHANNEL as _],

        tones: Vec<Tone>,

        time_pan_index: usize,
    },
}

struct Tone {
    on: bool,
    note_id: u8,
    note_freq: f32,
    velocity: u8, // 0-127, default 104
    voice_tones: [pxtone_sys::pxtnVOICETONE; pxtone_sys::pxtnMAX_UNITCONTROLVOICE as _],
    time_pan_buf: [[i32; pxtone_sys::pxtnBUFSIZE_TIMEPAN as _]; pxtone_sys::pxtnMAX_CHANNEL as _],
}

// many types in pxtone-sys are not marked Send
unsafe impl Send for PtPlug {}

#[derive(Params)]
struct PtParams {
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

        unsafe {
            let mut serv = pxtnService::new();
            match serv.init() {
                0 => {
                    if let Some(logger) = &logger {
                        logger.log(
                            &RecordBuilder::new()
                                .args(format_args!("serv.init OK"))
                                .build(),
                        );
                    }
                },
                n => panic!(
                    "serv.init {}",
                    CStr::from_ptr(pxtnError_get_string(n)).to_str().unwrap()
                ),
            }

            let mut freq = pxtnPulse_Frequency::new();
            assert!(freq.Init(), "freq Init");

            let (send, recv) = std::sync::mpsc::sync_channel(1);

            Self {
                params: Arc::new(PtParams::default()),
                sample_rate: None,

                master_gain: Smoother::new(SmoothingStyle::Linear(5.0)),

                pxtn_serv: serv,
                pxtn_freq: freq,

                woice_state: WoiceState::Unloaded,

                logger: logger.map(|bl| Arc::new(*bl)),

                file_select_recv: recv,
                file_select_send: send,
            }
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

        if self.sample_rate.is_some() {
            self.init_woice();
        }
    }

    fn load_file_data(&mut self, file_data: &[u8]) {
        unsafe {
            let mut descriptor = pxtnDescriptor::new();

            println!("Loading {} bytes", file_data.len());
            descriptor.set_memory_r(file_data as *const _ as *mut _, file_data.len() as i32);

            let mut woice = pxtnWoice::new();
            woice.Voice_Allocate(pxtone_sys::pxtnMAX_UNITCONTROLVOICE as _);

            match woice.PTV_Read(&mut descriptor) {
                0 => {
                    if let Some(logger) = &self.logger {
                        logger.log(
                            &RecordBuilder::new()
                                .args(format_args!("woice.PTV_Read OK"))
                                .build(),
                        );
                    }
                },
                n => panic!(
                    "PTV_Read {}",
                    CStr::from_ptr(pxtnError_get_string(n)).to_str().unwrap()
                ),
            }

            self.woice_state = WoiceState::Loaded {
                pxtn_woice: woice,
                pxtn_time_pan: [0; pxtone_sys::pxtnMAX_CHANNEL as _],
                pxtn_vol_pan: [0; pxtone_sys::pxtnMAX_CHANNEL as _],

                time_pan_index: 0,

                tones: vec![],
            };
        }
    }

    #[allow(clippy::too_many_lines)] // TODO: split into smaller fns
    fn sample(&mut self) -> [f32; 2] {
        // ported from some original pxtone code not included in pxtone-sys

        let Some(sample_rate) = self.sample_rate else {
            return [0.0; 2]
        };

        let dst_ch: usize = 2;
        let loop_ = true;
        let min_ct = sample_rate as i32 * 100 / 1000;
        let smooth = sample_rate as i32 / 100;

        unsafe {
            if let WoiceState::Loaded {
                pxtn_woice,
                pxtn_time_pan,
                pxtn_vol_pan,
                tones,
                time_pan_index,
            } = &mut self.woice_state
            {
                // update envelope
                for tone in &mut *tones {
                    #[allow(clippy::used_underscore_binding)]
                    for v in 0..pxtn_woice._voice_num as usize {
                        unsafe fn update_env(
                            vi: &mut pxtone_sys::pxtnVOICEINSTANCE,
                            vt: &mut pxtnVOICETONE,
                            on: bool,
                        ) {
                            if vt.life_count <= 0 || vi.env_size == 0 {
                                return;
                            }

                            if on {
                                if vt.env_pos < vi.env_size {
                                    vt.env_volume = i32::from(*vi.p_env.offset(vt.env_pos as _));
                                    vt.env_pos += 1;
                                }
                            } else {
                                if vt.env_pos < vi.env_release {
                                    vt.env_volume = vt.env_start
                                        + (-vt.env_start * vt.env_pos / vi.env_release);
                                } else {
                                    vt.life_count = 0;
                                    vt.env_volume = 0;
                                }
                                vt.env_pos += 1;
                            }
                        }

                        let vi = &mut {
                            #[allow(clippy::used_underscore_binding)]
                            std::slice::from_raw_parts_mut(
                                pxtn_woice._voinsts,
                                pxtone_sys::pxtnMAX_UNITCONTROLVOICE as _,
                            )
                        }[v];
                        let vt = &mut tone.voice_tones[v];

                        update_env(vi, vt, tone.on);
                    }
                }

                // sample into time pan buffer
                #[allow(clippy::needless_range_loop)]
                for ch in 0..dst_ch {
                    for tone in &mut *tones {
                        let mut pan_buf: i32 = 0;

                        #[allow(clippy::used_underscore_binding)]
                        for v in 0..pxtn_woice._voice_num as usize {
                            let vi = &mut {
                                #[allow(clippy::used_underscore_binding)]
                                std::slice::from_raw_parts_mut(
                                    pxtn_woice._voinsts,
                                    pxtone_sys::pxtnMAX_UNITCONTROLVOICE as _,
                                )
                            }[v];
                            let vt = &mut tone.voice_tones[v];

                            let mut work: i32 = 0;

                            #[allow(clippy::cast_ptr_alignment)]
                            if vt.life_count > 0 {
                                let pos = vt.smp_pos as i32 * 4 + ch as i32 * 2;
                                work += *vi.p_smp_w.offset(pos as _).cast::<i16>() as i32;

                                if dst_ch == 1 {
                                    work += *vi.p_smp_w.offset((pos + 2) as _).cast::<i16>() as i32;
                                    work /= 2;
                                }

                                work = (work * tone.velocity as i32) / 128;
                                work = (work * pxtn_vol_pan[ch]) / 64;

                                if vi.env_release > 0 {
                                    work = (work * vt.env_volume) / 128;
                                } else if loop_ && vt.smp_count > min_ct - smooth {
                                    work = (min_ct - smooth) / smooth;
                                }

                                vt.smp_pos += vi.smp_body_w as f64 * tone.note_freq as f64
                                    / sample_rate as f64
                                    * vt.offset_freq as f64
                                    / 4.0;

                                if loop_ {
                                    if vt.smp_pos >= vi.smp_body_w as _ {
                                        vt.smp_pos -= vi.smp_body_w as f64;
                                    }

                                    if vt.smp_pos >= vi.smp_body_w as _ {
                                        vt.smp_pos = 0.0;
                                    }

                                    if vi.smp_tail_w == 0 && vi.env_release == 0 && !tone.on {
                                        vt.smp_count += 1;
                                    }
                                } else {
                                    if vt.smp_pos >= vi.smp_body_w as _ {
                                        vt.life_count = 0;
                                    }

                                    if !tone.on {
                                        vt.smp_count += 1;
                                    }
                                }

                                if vt.smp_count >= min_ct {
                                    vt.life_count = 0;
                                }
                            }

                            pan_buf += work;
                        }

                        tone.time_pan_buf[ch][*time_pan_index] = pan_buf;
                    }
                }

                // update time pan & calc output
                let mut out = [0.0; 2];
                #[allow(clippy::needless_range_loop)] // terrible suggestion
                for ch in 0..dst_ch {
                    let mut work: i32 = 0;

                    for tone in &mut *tones {
                        let index = (*time_pan_index as i32 - pxtn_time_pan[ch])
                            & (pxtone_sys::pxtnBUFSIZE_TIMEPAN - 1) as i32;
                        work += tone.time_pan_buf[ch][index as usize];
                    }

                    out[ch] += (work as f64 / 0x7fff as f64).clamp(-1.0, 1.0);
                }

                *time_pan_index =
                    (*time_pan_index + 1) & (pxtone_sys::pxtnBUFSIZE_TIMEPAN as usize - 1);

                #[allow(clippy::used_underscore_binding)]
                tones.retain(|t| {
                    (0..pxtn_woice._voice_num).any(|v| t.voice_tones[v as usize].life_count > 0)
                });

                out.map(|f| f as _)
            } else {
                [0.0; 2]
            }
        }
    }

    fn init_woice(&mut self) {
        let Some(sample_rate) = self.sample_rate else {
            return
        };

        unsafe {
            if !self
                .pxtn_serv
                .set_destination_quality(2, sample_rate as i32)
            {
                panic!("serv.set_destination_quality() failed");
            }

            if let WoiceState::Loaded { pxtn_woice, .. } = &mut self.woice_state {
                match pxtn_woice.Tone_Ready(std::ptr::null(), sample_rate as i32) {
                    0 => {
                        if let Some(logger) = &self.logger {
                            logger.log(
                                &RecordBuilder::new()
                                    .args(format_args!("woice.Tone_Ready OK"))
                                    .build(),
                            );
                        }
                    },
                    n => panic!(
                        "Tone_Ready {}",
                        CStr::from_ptr(pxtnError_get_string(n)).to_str().unwrap()
                    ),
                }

                if let Some(logger) = &self.logger {
                    logger.log(
                        #[allow(clippy::used_underscore_binding)]
                        &RecordBuilder::new()
                            .args(format_args!(
                                "sps {} smp_body_w = {}",
                                sample_rate,
                                (*pxtn_woice._voinsts).smp_body_w
                            ))
                            .build(),
                    );
                }
            }
        }
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
        self.sample_rate = Some(buffer_config.sample_rate);

        if let WoiceState::Unloaded = self.woice_state {
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
        self.master_gain.reset(0.0);

        if let WoiceState::Loaded { tones, .. } = &mut self.woice_state {
            tones.clear();
        }
    }

    #[allow(clippy::too_many_lines)] // TODO: split into smaller fns
    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let dst_ch: usize = 2;

        let Some(sample_rate) = self.sample_rate else {
            return ProcessStatus::KeepAlive;
        };

        if let Ok(file_data) = self.file_select_recv.try_recv() {
            self.load_file(file_data);
        }

        let mut next_event = context.next_event();
        for (sample_id, channel_samples) in buffer.iter_samples().enumerate() {
            let gain = self.params.gain.smoothed.next();

            if let WoiceState::Loaded { pxtn_woice, pxtn_time_pan, pxtn_vol_pan, tones, .. } =
                &mut self.woice_state
            {
                self.params.num_tones.set(tones.len());

                // Act on the next MIDI event
                while let Some(event) = next_event {
                    if event.timing() > sample_id as u32 {
                        break;
                    }

                    match event {
                        NoteEvent::NoteOn { note, velocity, .. } => {
                            self.master_gain.set_target(sample_rate, velocity);

                            let mut tone = Tone {
                                on: true,
                                note_id: note,
                                note_freq: util::midi_note_to_freq(note),
                                velocity: (velocity * 127.0) as _,
                                voice_tones: [pxtnVOICETONE {
                                    smp_pos: 0.0,
                                    offset_freq: 1.0,
                                    env_volume: 128,
                                    life_count: 1,
                                    on_count: 0,
                                    smp_count: 0,
                                    env_start: 0,
                                    env_pos: 0,
                                    env_release_clock: 0,
                                    smooth_volume: 0,
                                };
                                    pxtone_sys::pxtnMAX_UNITCONTROLVOICE as _],
                                time_pan_buf: [[0; pxtone_sys::pxtnBUFSIZE_TIMEPAN as _];
                                    pxtone_sys::pxtnMAX_CHANNEL as _],
                            };

                            unsafe {
                                pxtn_vol_pan[0] = 64;
                                pxtn_vol_pan[1] = 64;
                                if dst_ch == 2 {
                                    let vol_pan = 64; // TODO

                                    if vol_pan >= 64 {
                                        pxtn_vol_pan[0] = 128 - vol_pan;
                                    } else {
                                        pxtn_vol_pan[1] = vol_pan;
                                    }
                                }

                                pxtn_time_pan[0] = 0;
                                pxtn_time_pan[1] = 0;
                                if dst_ch == 2 {
                                    let time_pan = 64; // TODO

                                    if time_pan >= 64 {
                                        pxtn_time_pan[0] = time_pan - 64;
                                        if pxtn_time_pan[0] >= 64 {
                                            pxtn_time_pan[0] = 63;
                                        }
                                        pxtn_time_pan[0] =
                                            pxtn_time_pan[0] * 44100 / sample_rate as i32;
                                    } else {
                                        pxtn_time_pan[1] = 64 - time_pan;
                                        if pxtn_time_pan[1] >= 64 {
                                            pxtn_time_pan[1] = 63;
                                        }
                                        pxtn_time_pan[1] =
                                            pxtn_time_pan[1] * 44100 / sample_rate as i32;
                                    }
                                }

                                #[allow(clippy::used_underscore_binding)]
                                for v in 0..pxtn_woice._voice_num as usize {
                                    let vi = &mut {
                                        std::slice::from_raw_parts_mut(
                                            pxtn_woice._voinsts,
                                            pxtone_sys::pxtnMAX_UNITCONTROLVOICE as _,
                                        )
                                    }[v];
                                    let vt = &mut tone.voice_tones[v];
                                    let vu = &*pxtn_woice.get_voice(v as _);

                                    vt.life_count = 1;
                                    vt.smp_pos = 0.0;
                                    vt.smp_count = 0;
                                    vt.env_pos = 0;

                                    if vu.voice_flags & pxtone_sys::PTV_VOICEFLAG_BEATFIT == 0 {
                                        vt.offset_freq = self.pxtn_freq.Get(
                                            pxtone_sys::EVENTDEFAULT_BASICKEY as i32 - vu.basic_key,
                                        ) * vu.tuning;
                                    }

                                    if vi.env_size > 0 {
                                        vt.env_volume = 0;
                                        vt.env_start = 0;
                                    } else {
                                        vt.env_volume = 128;
                                        vt.env_start = 128;
                                    }
                                }
                            }

                            tones.push(tone);
                        },
                        NoteEvent::NoteOff { note, .. } => {
                            self.master_gain.set_target(sample_rate, 0.0);

                            if let Some(tone) = tones.iter_mut().find(|t| t.on && t.note_id == note)
                            {
                                tone.on = false;
                                #[allow(clippy::used_underscore_binding)]
                                for v in 0..pxtn_woice._voice_num as usize {
                                    let vt = &mut tone.voice_tones[v];
                                    vt.env_start = vt.env_volume;
                                    vt.env_pos = 0;
                                }
                            }
                        },
                        NoteEvent::PolyPressure { note: _, pressure, .. } => {
                            self.master_gain.set_target(sample_rate, pressure);
                        },
                        NoteEvent::Choke { note, .. } => {
                            tones.retain(|t| t.note_id != note);
                        },
                        _ => (),
                    }

                    next_event = context.next_event();
                }
            };

            let sample = self.sample();

            for (ch, smp_out) in channel_samples.into_iter().enumerate() {
                *smp_out = sample[ch] * util::db_to_gain_fast(gain);
            }
        }

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
