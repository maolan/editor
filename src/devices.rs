use std::fmt;

use crate::preferences::EditorPreferences;

#[derive(Debug, Clone)]
pub struct AudioDeviceOption {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) supported_bits: Vec<usize>,
    pub(crate) supported_sample_rates: Vec<i32>,
    #[cfg(all(feature = "standalone", target_os = "freebsd"))]
    pub(crate) max_channels: usize,
    #[cfg(all(feature = "standalone", target_os = "freebsd"))]
    pub(crate) max_buffer_bytes: usize,
    pub(crate) supports_input: bool,
    pub(crate) supports_output: bool,
}

impl PartialEq for AudioDeviceOption {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for AudioDeviceOption {}

impl std::hash::Hash for AudioDeviceOption {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl AudioDeviceOption {
    pub(crate) fn with_supported_caps(
        id: impl Into<String>,
        label: impl Into<String>,
        mut supported_bits: Vec<usize>,
        mut supported_sample_rates: Vec<i32>,
    ) -> Self {
        supported_bits.sort_by(|a, b| b.cmp(a));
        supported_bits.dedup();
        supported_sample_rates.retain(|rate| *rate > 0);
        supported_sample_rates.sort_unstable();
        supported_sample_rates.dedup();
        Self {
            id: id.into(),
            label: label.into(),
            supported_bits,
            supported_sample_rates,
            #[cfg(all(feature = "standalone", target_os = "freebsd"))]
            max_channels: 0,
            #[cfg(all(feature = "standalone", target_os = "freebsd"))]
            max_buffer_bytes: 0,
            supports_input: true,
            supports_output: true,
        }
    }

    #[cfg(all(feature = "standalone", target_os = "freebsd"))]
    pub(crate) fn with_oss_caps(
        id: impl Into<String>,
        label: impl Into<String>,
        supported_bits: Vec<usize>,
        supported_sample_rates: Vec<i32>,
        max_channels: usize,
        max_buffer_bytes: usize,
    ) -> Self {
        let mut out = Self::with_supported_caps(id, label, supported_bits, supported_sample_rates);
        out.max_channels = max_channels;
        out.max_buffer_bytes = max_buffer_bytes;
        out
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn with_supported_direction_caps(
        id: impl Into<String>,
        label: impl Into<String>,
        mut supported_bits: Vec<usize>,
        mut supported_sample_rates: Vec<i32>,
        supports_input: bool,
        supports_output: bool,
    ) -> Self {
        supported_bits.sort_by(|a, b| b.cmp(a));
        supported_bits.dedup();
        supported_sample_rates.retain(|rate| *rate > 0);
        supported_sample_rates.sort_unstable();
        supported_sample_rates.dedup();
        Self {
            id: id.into(),
            label: label.into(),
            supported_bits,
            supported_sample_rates,
            #[cfg(target_os = "freebsd")]
            max_channels: 0,
            #[cfg(target_os = "freebsd")]
            max_buffer_bytes: 0,
            supports_input,
            supports_output,
        }
    }
}

#[cfg(all(feature = "standalone", target_os = "freebsd"))]
impl From<maolan_engine::audio_devices::AudioDeviceDescriptor> for AudioDeviceOption {
    fn from(device: maolan_engine::audio_devices::AudioDeviceDescriptor) -> Self {
        let mut out = Self::with_oss_caps(
            device.id,
            device.label,
            device.supported_bits,
            device.supported_sample_rates,
            device.max_channels,
            device.max_buffer_bytes,
        );
        out.supports_input = device.supports_input;
        out.supports_output = device.supports_output;
        out
    }
}

impl fmt::Display for AudioDeviceOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.supported_bits.is_empty() {
            return f.write_str(&self.label);
        }
        let formats = self
            .supported_bits
            .iter()
            .map(|bits| format!("{bits}"))
            .collect::<Vec<_>>()
            .join("/");
        write!(f, "{} [{}-bit]", self.label, formats)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartupSetup {
    pub(crate) audio_engine: AudioEngineOption,
    pub(crate) output_devices: Vec<AudioDeviceOption>,
    pub(crate) input_devices: Vec<AudioDeviceOption>,
    pub(crate) output_device: Option<AudioDeviceOption>,
    pub(crate) input_device: Option<AudioDeviceOption>,
    pub(crate) sample_rate_hz: i32,
    pub(crate) bits: usize,
    pub(crate) exclusive: bool,
    pub(crate) period_frames: usize,
    pub(crate) nperiods: usize,
    pub(crate) sync_mode: bool,
}

impl StartupSetup {
    pub(crate) fn with_preferences(
        preferences: &EditorPreferences,
        output_devices: Vec<AudioDeviceOption>,
        input_devices: Vec<AudioDeviceOption>,
    ) -> Self {
        let audio_engine = AudioEngineOption::default();
        let output_device = preferences
            .default_output_device_id
            .as_deref()
            .and_then(|id| {
                output_devices
                    .iter()
                    .find(|device| device.id == id)
                    .cloned()
            })
            .or_else(|| output_devices.first().cloned());
        let input_device = preferences
            .default_input_device_id
            .as_deref()
            .and_then(|id| input_devices.iter().find(|device| device.id == id).cloned())
            .or_else(|| input_devices.first().cloned());
        let mut setup = Self {
            audio_engine,
            output_devices,
            input_devices,
            output_device,
            input_device,
            sample_rate_hz: 48_000,
            bits: 32,
            exclusive: true,
            period_frames: 1024,
            nperiods: maolan_widgets::audio_setup::DEFAULT_N_PERIODS,
            sync_mode: false,
        };
        setup.sample_rate_hz = pick_sample_rate(&setup);
        setup.bits = pick_bits(&setup);
        setup.period_frames = pick_period_frames(&setup);
        setup
    }
}

impl Default for StartupSetup {
    fn default() -> Self {
        let preferences = EditorPreferences::load();
        let audio_engine = AudioEngineOption::default();
        let output_devices = discover_output_audio_devices(audio_engine);
        let input_devices = discover_input_audio_devices(audio_engine);
        Self::with_preferences(&preferences, output_devices, input_devices)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioEngineOption {
    #[cfg(target_os = "linux")]
    #[default]
    Alsa,
    #[cfg(unix)]
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")),
        default
    )]
    Jack,
    #[cfg(target_os = "freebsd")]
    #[default]
    Oss,
    #[cfg(target_os = "openbsd")]
    #[default]
    Sndio,
    #[cfg(target_os = "windows")]
    #[default]
    Wasapi,
}

impl AudioEngineOption {
    pub(crate) const ALL: &'static [Self] = &[
        #[cfg(target_os = "linux")]
        Self::Alsa,
        #[cfg(target_os = "freebsd")]
        Self::Oss,
        #[cfg(target_os = "openbsd")]
        Self::Sndio,
        #[cfg(target_os = "windows")]
        Self::Wasapi,
        #[cfg(unix)]
        Self::Jack,
    ];

    pub(crate) fn is_jack(self) -> bool {
        #[cfg(unix)]
        {
            self == Self::Jack
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

impl fmt::Display for AudioEngineOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(target_os = "linux")]
            Self::Alsa => write!(f, "ALSA"),
            #[cfg(unix)]
            Self::Jack => write!(f, "JACK"),
            #[cfg(target_os = "freebsd")]
            Self::Oss => write!(f, "OSS"),
            #[cfg(target_os = "openbsd")]
            Self::Sndio => write!(f, "sndio"),
            #[cfg(target_os = "windows")]
            Self::Wasapi => write!(f, "WASAPI"),
        }
    }
}

pub(crate) fn pick_sample_rate(setup: &StartupSetup) -> i32 {
    let options = sample_rate_options(setup);
    if options.contains(&setup.sample_rate_hz) {
        setup.sample_rate_hz
    } else {
        options
            .iter()
            .min_by_key(|candidate| ((*candidate).saturating_sub(setup.sample_rate_hz)).abs())
            .copied()
            .unwrap_or(48_000)
    }
}

pub(crate) fn pick_bits(setup: &StartupSetup) -> usize {
    let options = bit_options(setup);
    if options.contains(&setup.bits) {
        setup.bits
    } else {
        options.first().copied().unwrap_or(32)
    }
}

pub(crate) fn pick_period_frames(setup: &StartupSetup) -> usize {
    let options = period_frame_options(setup);
    if options.contains(&setup.period_frames) {
        setup.period_frames
    } else {
        options
            .iter()
            .copied()
            .find(|value| *value >= setup.period_frames)
            .or_else(|| options.last().copied())
            .unwrap_or(setup.period_frames)
    }
}

pub(crate) fn playhead_label(app: &crate::state::EditApp) -> String {
    let sample_rate = app
        .audio
        .as_ref()
        .map(|audio| audio.sample_rate)
        .unwrap_or(48_000)
        .max(1);
    let seconds = app.playhead_samples as f64 / sample_rate as f64;
    let minutes = (seconds / 60.0).floor() as u64;
    let secs = (seconds % 60.0).floor() as u64;
    let millis = ((seconds.fract()) * 1000.0).floor() as u64;
    format!("{minutes:02}:{secs:02}.{millis:03}")
}

pub(crate) fn discover_output_audio_devices(engine: AudioEngineOption) -> Vec<AudioDeviceOption> {
    if engine.is_jack() {
        return vec![simple_audio_device("jack")];
    }
    let mut devices = platform_audio_devices()
        .into_iter()
        .filter(|device| device.supports_output)
        .collect::<Vec<_>>();
    if devices.is_empty() {
        devices.push(simple_audio_device(default_audio_device(engine)));
    }
    devices.sort_by_key(|device| device.label.to_lowercase());
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}

pub(crate) fn discover_input_audio_devices(engine: AudioEngineOption) -> Vec<AudioDeviceOption> {
    if engine.is_jack() {
        return Vec::new();
    }
    let mut devices = platform_audio_devices()
        .into_iter()
        .filter(|device| device.supports_input)
        .collect::<Vec<_>>();
    devices.sort_by_key(|device| device.label.to_lowercase());
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}

#[cfg(feature = "standalone")]
fn platform_audio_devices() -> Vec<AudioDeviceOption> {
    #[cfg(target_os = "freebsd")]
    {
        maolan_engine::audio_devices::discover_freebsd_audio_devices()
            .into_iter()
            .map(AudioDeviceOption::from)
            .collect()
    }
    #[cfg(target_os = "linux")]
    {
        let mut output_devices = platform_linux::discover_alsa_output_devices();
        let mut input_devices = platform_linux::discover_alsa_input_devices();
        output_devices.append(&mut input_devices);
        output_devices.sort_by_key(|device| device.label.to_lowercase());
        output_devices.dedup_by(|a, b| a.id == b.id);
        output_devices
    }
    #[cfg(target_os = "openbsd")]
    {
        vec![simple_audio_device("default")]
    }
    #[cfg(target_os = "windows")]
    {
        vec![simple_audio_device("default")]
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "windows"
    )))]
    {
        vec![simple_audio_device("default")]
    }
}

#[cfg(not(feature = "standalone"))]
fn platform_audio_devices() -> Vec<AudioDeviceOption> {
    vec![simple_audio_device("default")]
}

#[cfg(all(feature = "standalone", target_os = "linux"))]
mod platform_linux {
    use alsa::{
        Direction,
        pcm::{Access, Format, HwParams, PCM},
    };

    const SAMPLE_RATE_CANDIDATES: [u32; 12] = [
        8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
        384_000,
    ];

    fn read_alsa_card_labels() -> std::collections::HashMap<u32, String> {
        let mut labels = std::collections::HashMap::new();
        let Ok(contents) = std::fs::read_to_string("/proc/asound/cards") else {
            return labels;
        };
        for line in contents.lines() {
            let line = line.trim_start();
            let Some((num_str, rest)) = line.split_once(' ') else {
                continue;
            };
            let Ok(card) = num_str.parse::<u32>() else {
                continue;
            };
            let Some((_, desc)) = rest.split_once("]:") else {
                continue;
            };
            let desc = desc.trim();
            if !desc.is_empty() {
                labels.insert(card, desc.to_string());
            }
        }
        labels
    }

    fn probe_alsa_supported_bits(device: &str, direction: Direction) -> Vec<usize> {
        let Ok(pcm) = PCM::new(device, direction, false) else {
            return Vec::new();
        };
        let Ok(hwp) = HwParams::any(&pcm) else {
            return Vec::new();
        };
        if hwp.set_access(Access::RWInterleaved).is_err() {
            return Vec::new();
        }

        fn supports(hwp: &HwParams<'_>, fmt: Format) -> bool {
            hwp.test_format(fmt).is_ok()
        }

        let candidates: Vec<(usize, Vec<Format>)> = vec![
            (32, vec![native_s32(), foreign_s32()]),
            (24, vec![native_s24(), foreign_s24()]),
            (16, vec![native_s16(), foreign_s16()]),
            (8, vec![Format::S8]),
        ];

        let mut supported = Vec::new();
        for (bits, formats) in candidates {
            if formats.iter().any(|f| supports(&hwp, *f)) {
                supported.push(bits);
            }
        }
        supported
    }

    fn probe_alsa_supported_sample_rates(device: &str, direction: Direction) -> Vec<i32> {
        let Ok(pcm) = PCM::new(device, direction, false) else {
            return Vec::new();
        };
        let Ok(hwp) = HwParams::any(&pcm) else {
            return Vec::new();
        };
        if hwp.set_access(Access::RWInterleaved).is_err() {
            return Vec::new();
        }

        let mut supported = Vec::new();
        for rate in SAMPLE_RATE_CANDIDATES {
            if hwp.test_rate(rate).is_ok() {
                supported.push(rate as i32);
            }
        }
        supported
    }

    #[cfg(target_endian = "little")]
    fn native_s16() -> Format {
        Format::S16LE
    }
    #[cfg(target_endian = "big")]
    fn native_s16() -> Format {
        Format::S16BE
    }
    #[cfg(target_endian = "little")]
    fn foreign_s16() -> Format {
        Format::S16BE
    }
    #[cfg(target_endian = "big")]
    fn foreign_s16() -> Format {
        Format::S16LE
    }

    #[cfg(target_endian = "little")]
    fn native_s24() -> Format {
        Format::S24LE
    }
    #[cfg(target_endian = "big")]
    fn native_s24() -> Format {
        Format::S24BE
    }
    #[cfg(target_endian = "little")]
    fn foreign_s24() -> Format {
        Format::S24BE
    }
    #[cfg(target_endian = "big")]
    fn foreign_s24() -> Format {
        Format::S24LE
    }

    #[cfg(target_endian = "little")]
    fn native_s32() -> Format {
        Format::S32LE
    }
    #[cfg(target_endian = "big")]
    fn native_s32() -> Format {
        Format::S32BE
    }
    #[cfg(target_endian = "little")]
    fn foreign_s32() -> Format {
        Format::S32BE
    }
    #[cfg(target_endian = "big")]
    fn foreign_s32() -> Format {
        Format::S32LE
    }

    fn discover_alsa_devices(
        direction_marker: &str,
        direction: Direction,
    ) -> Vec<super::AudioDeviceOption> {
        let mut devices = Vec::new();
        let card_labels = read_alsa_card_labels();
        if let Ok(contents) = std::fs::read_to_string("/proc/asound/pcm") {
            for line in contents.lines() {
                let Some((card_dev, rest)) = line.split_once(':') else {
                    continue;
                };
                if !rest.contains(direction_marker) {
                    continue;
                }
                let mut parts = card_dev.trim().split('-');
                let (Some(card), Some(dev)) = (parts.next(), parts.next()) else {
                    continue;
                };
                let Ok(card) = card.parse::<u32>() else {
                    continue;
                };
                let Ok(dev) = dev.parse::<u32>() else {
                    continue;
                };
                let device_name = rest.split(':').next().unwrap_or("").trim();
                let card_label = card_labels
                    .get(&card)
                    .cloned()
                    .unwrap_or_else(|| format!("Card {card}"));
                let base_label = if device_name.is_empty() {
                    card_label
                } else {
                    format!("{card_label} - {device_name}")
                };
                let id = format!("hw:{card},{dev}");
                let label = format!("{base_label} (hw:{card},{dev})");
                let supported_bits = probe_alsa_supported_bits(&id, direction);
                let supported_sample_rates = {
                    let rates = probe_alsa_supported_sample_rates(&id, direction);
                    if rates.is_empty() {
                        super::fallback_sample_rates()
                    } else {
                        rates
                    }
                };
                let (supports_input, supports_output) = match direction {
                    Direction::Playback => (false, true),
                    Direction::Capture => (true, false),
                };
                devices.push(super::AudioDeviceOption::with_supported_direction_caps(
                    id,
                    label,
                    supported_bits,
                    supported_sample_rates,
                    supports_input,
                    supports_output,
                ));
            }
        }
        devices.sort_by_key(|a| a.label.to_lowercase());
        devices.dedup_by(|a, b| a.id == b.id);
        devices
    }

    pub(crate) fn discover_alsa_output_devices() -> Vec<super::AudioDeviceOption> {
        discover_alsa_devices("playback", Direction::Playback)
    }

    pub(crate) fn discover_alsa_input_devices() -> Vec<super::AudioDeviceOption> {
        discover_alsa_devices("capture", Direction::Capture)
    }
}

fn simple_audio_device(id: impl Into<String>) -> AudioDeviceOption {
    let id = id.into();
    AudioDeviceOption::with_supported_caps(
        id.clone(),
        id,
        vec![32, 24, 16, 8],
        fallback_sample_rates(),
    )
}

fn fallback_sample_rates() -> Vec<i32> {
    vec![
        8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
        384_000,
    ]
}

fn fallback_bits() -> Vec<usize> {
    vec![32, 24, 16, 8]
}

pub(crate) fn sample_rate_options(setup: &StartupSetup) -> Vec<i32> {
    if setup.audio_engine.is_jack() {
        return fallback_sample_rates();
    }
    setup
        .output_device
        .as_ref()
        .map(|device| device.supported_sample_rates.clone())
        .filter(|rates| !rates.is_empty())
        .unwrap_or_else(fallback_sample_rates)
}

pub(crate) fn bit_options(setup: &StartupSetup) -> Vec<usize> {
    if setup.audio_engine.is_jack() {
        return fallback_bits();
    }
    setup
        .output_device
        .as_ref()
        .map(|device| {
            if device.supported_bits.is_empty() {
                fallback_bits()
            } else {
                device.supported_bits.clone()
            }
        })
        .unwrap_or_else(fallback_bits)
}

pub(crate) fn default_audio_device(engine: AudioEngineOption) -> &'static str {
    if engine.is_jack() {
        return "jack";
    }
    #[cfg(target_os = "linux")]
    {
        "default"
    }
    #[cfg(target_os = "freebsd")]
    {
        "/dev/dsp"
    }
    #[cfg(target_os = "openbsd")]
    {
        "default"
    }
    #[cfg(target_os = "windows")]
    {
        "default"
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "windows"
    )))]
    {
        "default"
    }
}

#[cfg(all(feature = "standalone", target_os = "freebsd"))]
pub(crate) fn period_frame_options(setup: &StartupSetup) -> Vec<usize> {
    if !setup.audio_engine.is_jack()
        && let Some(device) = setup.output_device.as_ref()
        && let Some(options) =
            oss_period_frame_options(device, crate::engine::selected_bits(setup) as usize)
    {
        return options;
    }
    default_period_frame_options()
}

#[cfg(any(not(feature = "standalone"), not(target_os = "freebsd")))]
pub(crate) fn period_frame_options(_setup: &StartupSetup) -> Vec<usize> {
    default_period_frame_options()
}

fn default_period_frame_options() -> Vec<usize> {
    vec![
        16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536,
    ]
}

#[cfg(all(feature = "standalone", target_os = "freebsd"))]
fn oss_period_frame_options(device: &AudioDeviceOption, bits: usize) -> Option<Vec<usize>> {
    if device.max_channels == 0 || device.max_buffer_bytes == 0 {
        return None;
    }
    let channels = device.max_channels.max(1);
    let bytes_per_sample = match bits {
        8 => 1,
        16 => 2,
        24 => 3,
        32 => 4,
        _ => return None,
    };
    let frame_bytes = channels.checked_mul(bytes_per_sample)?.max(1);
    let min_bytes = frame_bytes.next_power_of_two();
    let max_fragment_bytes = 1_usize << 16;
    let max_bytes = device
        .max_buffer_bytes
        .min(max_fragment_bytes)
        .max(min_bytes);
    if min_bytes > max_bytes {
        return None;
    }
    let mut out = Vec::new();
    let mut bytes = min_bytes;
    while bytes <= max_bytes {
        out.push(bytes.div_ceil(frame_bytes).max(1));
        match bytes.checked_mul(2) {
            Some(next) => bytes = next,
            None => break,
        }
    }
    out.sort_unstable();
    out.dedup();
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_device_option_equality_compares_id_only() {
        let a = AudioDeviceOption::with_supported_caps("dev", "A", vec![16], vec![44_100]);
        let b = AudioDeviceOption::with_supported_caps("dev", "B", vec![32], vec![48_000]);
        assert_eq!(a, b);
    }

    #[test]
    fn startup_setup_with_preferences_selects_saved_devices() {
        let preferences = EditorPreferences {
            default_output_device_id: Some(String::from("out2")),
            default_input_device_id: Some(String::from("in2")),
        };
        let output_devices = vec![
            AudioDeviceOption::with_supported_caps("out1", "Out One", vec![32], vec![48_000]),
            AudioDeviceOption::with_supported_caps("out2", "Out Two", vec![32], vec![48_000]),
        ];
        let input_devices = vec![
            AudioDeviceOption::with_supported_caps("in1", "In One", vec![32], vec![48_000]),
            AudioDeviceOption::with_supported_caps("in2", "In Two", vec![32], vec![48_000]),
        ];

        let setup = StartupSetup::with_preferences(&preferences, output_devices, input_devices);

        assert_eq!(
            setup.output_device.as_ref().map(|d| d.id.as_str()),
            Some("out2")
        );
        assert_eq!(
            setup.input_device.as_ref().map(|d| d.id.as_str()),
            Some("in2")
        );
    }

    #[test]
    fn startup_setup_with_preferences_falls_back_to_first_device() {
        let preferences = EditorPreferences {
            default_output_device_id: Some(String::from("missing")),
            default_input_device_id: Some(String::from("missing")),
        };
        let output_devices = vec![AudioDeviceOption::with_supported_caps(
            "out1",
            "Out One",
            vec![32],
            vec![48_000],
        )];
        let input_devices = vec![AudioDeviceOption::with_supported_caps(
            "in1",
            "In One",
            vec![32],
            vec![48_000],
        )];

        let setup = StartupSetup::with_preferences(&preferences, output_devices, input_devices);

        assert_eq!(
            setup.output_device.as_ref().map(|d| d.id.as_str()),
            Some("out1")
        );
        assert_eq!(
            setup.input_device.as_ref().map(|d| d.id.as_str()),
            Some("in1")
        );
    }
}
