use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const SAMPLE_RATE: u32 = 48_000;
const VOLUME: f32 = 0.32;
const SOUNDS_DIR_NAME: &str = "sounds";

/// Audio cues replace desktop notifications entirely. Each cue is a short
/// synthesized frequency sweep cached as a WAV file under the app data dir.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cue {
    /// A service crossed below the low-usage threshold.
    Warn,
    /// A service recovered back above the low-usage threshold.
    Recover,
}

impl Cue {
    fn file_name(self) -> &'static str {
        match self {
            Self::Warn => "warn.wav",
            Self::Recover => "recover.wav",
        }
    }

    /// (start_hz, end_hz, seconds) segments, played back to back.
    fn segments(self) -> &'static [(f32, f32, f32)] {
        match self {
            Self::Warn => &[(660.0, 440.0, 0.09), (440.0, 330.0, 0.12)],
            Self::Recover => &[(440.0, 587.33, 0.08), (587.33, 880.0, 0.10)],
        }
    }
}

pub fn play(cue: Cue, app_data_dir: &Path) {
    let dir = app_data_dir.join(SOUNDS_DIR_NAME);

    std::thread::spawn(move || {
        let Ok(path) = cue_path(cue, &dir) else {
            return;
        };

        for player in ["pw-play", "paplay", "aplay"] {
            if command_exists(player) {
                let _ = Command::new(player)
                    .arg(&path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                return;
            }
        }
    });
}

fn cue_path(cue: Cue, dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|error| format!("Could not create sounds dir: {error}"))?;
    let path = dir.join(cue.file_name());

    if !path.is_file() {
        write_wav(&path, cue.segments())?;
    }

    Ok(path)
}

fn write_wav(path: &Path, segments: &[(f32, f32, f32)]) -> Result<(), String> {
    let samples = synthesize(segments);
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    let data_len = (samples.len() * 2) as u32;

    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());

    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    fs::write(path, bytes).map_err(|error| format!("Could not write cue file: {error}"))
}

fn synthesize(segments: &[(f32, f32, f32)]) -> Vec<i16> {
    let mut samples = Vec::new();

    for &(from_hz, to_hz, secs) in segments {
        let count = (SAMPLE_RATE as f32 * secs) as usize;
        let mut phase = 0f32;

        for index in 0..count {
            let t = index as f32 / count as f32;
            let hz = from_hz + (to_hz - from_hz) * t;
            phase += std::f32::consts::TAU * hz / SAMPLE_RATE as f32;
            // Short attack/release envelope to avoid clicks.
            let envelope = (t * 24.0).min(1.0) * ((1.0 - t) * 6.0).min(1.0);
            let value = phase.sin() * envelope * VOLUME;
            samples.push((value * i16::MAX as f32) as i16);
        }
    }

    samples
}

fn command_exists(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_files_are_written_once_and_are_valid_riff_wavs() {
        let dir = std::env::temp_dir().join(format!(
            "pickgauge-sounds-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));

        for cue in [Cue::Warn, Cue::Recover] {
            let path = cue_path(cue, &dir).expect("cue file is created");
            let bytes = fs::read(&path).expect("cue file reads");

            assert!(bytes.len() > 44, "cue file has audio data");
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WAVE");

            let modified_before = fs::metadata(&path).expect("metadata").modified().ok();
            let path_again = cue_path(cue, &dir).expect("cue file is reused");
            assert_eq!(path, path_again);
            let modified_after = fs::metadata(&path).expect("metadata").modified().ok();
            assert_eq!(modified_before, modified_after, "cue file is not rewritten");
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn synthesized_cues_are_short_and_non_silent() {
        for cue in [Cue::Warn, Cue::Recover] {
            let samples = synthesize(cue.segments());
            let seconds = samples.len() as f32 / SAMPLE_RATE as f32;

            assert!(seconds > 0.05 && seconds < 0.5, "cue stays brief");
            assert!(samples.iter().any(|sample| sample.abs() > 1000));
        }
    }
}
