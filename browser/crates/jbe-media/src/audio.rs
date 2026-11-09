//! Native audio engine for Jinn Browser Engine.
//!
//! Provides pure, zero-dependency RIFF/WAV PCM decoding, waveform synthesis,
//! multi-track software mixing with soft-clipping protection, and an `<audio>`
//! state machine.

extern crate alloc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::f32::consts::PI;

/// Playback state for audio sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Unloaded,
    Ready,
    Playing,
    Paused,
    Ended,
}

/// Information header for decoded audio.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioFormat {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
}

/// Decoded PCM audio clip. Samples are stored as normalized `f32` in `[-1.0, 1.0]`.
/// For multi-channel audio, samples are interleaved: `[L0, R0, L1, R1, ...]`.
#[derive(Debug, Clone)]
pub struct AudioClip {
    pub format: AudioFormat,
    pub samples: Vec<f32>,
    pub duration_secs: f32,
}

impl AudioClip {
    pub fn new(format: AudioFormat, samples: Vec<f32>) -> Self {
        let frame_count = if format.channels > 0 {
            samples.len() / (format.channels as usize)
        } else {
            0
        };
        let duration_secs = if format.sample_rate > 0 {
            frame_count as f32 / format.sample_rate as f32
        } else {
            0.0
        };
        Self {
            format,
            samples,
            duration_secs,
        }
    }

    /// Total number of multi-channel audio frames.
    pub fn frame_count(&self) -> usize {
        if self.format.channels == 0 {
            0
        } else {
            self.samples.len() / (self.format.channels as usize)
        }
    }

    /// Read sample at specific channel and frame index.
    pub fn sample_at(&self, frame: usize, channel: usize) -> f32 {
        if channel >= self.format.channels as usize {
            return 0.0;
        }
        let idx = frame * (self.format.channels as usize) + channel;
        if idx < self.samples.len() {
            self.samples[idx]
        } else {
            0.0
        }
    }
}

/// Errors occurring during audio decoding or playback.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioError {
    InvalidRiffHeader,
    UnsupportedFormat(String),
    CorruptedChunk,
    UnexpectedEof,
    BufferOverflow,
}

/// Pure Rust native RIFF/WAV PCM decoder.
pub struct WavDecoder;

impl WavDecoder {
    /// Decodes a raw byte slice containing a RIFF/WAVE PCM audio file.
    pub fn decode(bytes: &[u8]) -> Result<AudioClip, AudioError> {
        if bytes.len() < 44 {
            return Err(AudioError::UnexpectedEof);
        }

        // Validate RIFF header
        if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err(AudioError::InvalidRiffHeader);
        }

        let mut offset = 12;
        let mut audio_format: Option<u16> = None;
        let mut channels: Option<u16> = None;
        let mut sample_rate: Option<u32> = None;
        let mut bits_per_sample: Option<u16> = None;
        let mut pcm_data: Option<&[u8]> = None;

        while offset + 8 <= bytes.len() {
            let chunk_id = &bytes[offset..offset + 4];
            let chunk_size = u32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]) as usize;
            offset += 8;

            if offset + chunk_size > bytes.len() {
                // Truncated chunk or file
                return Err(AudioError::UnexpectedEof);
            }

            if chunk_id == b"fmt " {
                if chunk_size < 16 {
                    return Err(AudioError::CorruptedChunk);
                }
                let fmt_code = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
                let ch = u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]);
                let sr = u32::from_le_bytes([
                    bytes[offset + 4],
                    bytes[offset + 5],
                    bytes[offset + 6],
                    bytes[offset + 7],
                ]);
                let bps = u16::from_le_bytes([bytes[offset + 14], bytes[offset + 15]]);

                audio_format = Some(fmt_code);
                channels = Some(ch);
                sample_rate = Some(sr);
                bits_per_sample = Some(bps);
            } else if chunk_id == b"data" {
                pcm_data = Some(&bytes[offset..offset + chunk_size]);
            }

            offset += chunk_size;
            // Chunks in RIFF are word-aligned (2 bytes)
            if chunk_size % 2 != 0 {
                offset += 1;
            }
        }

        let fmt_code = audio_format.ok_or(AudioError::CorruptedChunk)?;
        let ch = channels.ok_or(AudioError::CorruptedChunk)?;
        let sr = sample_rate.ok_or(AudioError::CorruptedChunk)?;
        let bps = bits_per_sample.ok_or(AudioError::CorruptedChunk)?;
        let data = pcm_data.ok_or(AudioError::CorruptedChunk)?;

        if ch == 0 || sr == 0 {
            return Err(AudioError::CorruptedChunk);
        }

        // Format code 1 is PCM, 3 is IEEE float
        if fmt_code != 1 && fmt_code != 3 {
            return Err(AudioError::UnsupportedFormat(alloc::format!(
                "Unsupported audio format tag: {}",
                fmt_code
            )));
        }

        let mut samples = Vec::new();

        if fmt_code == 1 {
            // PCM Integer
            match bps {
                8 => {
                    // 8-bit unsigned PCM (0..255, center is 128)
                    samples.reserve(data.len());
                    for &b in data {
                        let norm = (b as f32 - 128.0) / 128.0;
                        samples.push(norm.clamp(-1.0, 1.0));
                    }
                }
                16 => {
                    // 16-bit signed PCM LE
                    let sample_count = data.len() / 2;
                    samples.reserve(sample_count);
                    for i in 0..sample_count {
                        let b0 = data[i * 2];
                        let b1 = data[i * 2 + 1];
                        let val = i16::from_le_bytes([b0, b1]);
                        let norm = val as f32 / 32768.0;
                        samples.push(norm.clamp(-1.0, 1.0));
                    }
                }
                24 => {
                    // 24-bit signed PCM LE
                    let sample_count = data.len() / 3;
                    samples.reserve(sample_count);
                    for i in 0..sample_count {
                        let b0 = data[i * 3];
                        let b1 = data[i * 3 + 1];
                        let b2 = data[i * 3 + 2];
                        // Sign extend 24-bit to 32-bit
                        let val = (b0 as i32) | ((b1 as i32) << 8) | ((b2 as i8 as i32) << 16);
                        let norm = val as f32 / 8388608.0;
                        samples.push(norm.clamp(-1.0, 1.0));
                    }
                }
                32 => {
                    // 32-bit signed PCM LE
                    let sample_count = data.len() / 4;
                    samples.reserve(sample_count);
                    for i in 0..sample_count {
                        let val = i32::from_le_bytes([
                            data[i * 4],
                            data[i * 4 + 1],
                            data[i * 4 + 2],
                            data[i * 4 + 3],
                        ]);
                        let norm = val as f32 / 2147483648.0;
                        samples.push(norm.clamp(-1.0, 1.0));
                    }
                }
                _ => {
                    return Err(AudioError::UnsupportedFormat(alloc::format!(
                        "Unsupported bit depth: {}",
                        bps
                    )));
                }
            }
        } else if fmt_code == 3 {
            // 32-bit IEEE Float
            if bps != 32 {
                return Err(AudioError::UnsupportedFormat(alloc::format!(
                    "Unsupported float bit depth: {}",
                    bps
                )));
            }
            let sample_count = data.len() / 4;
            samples.reserve(sample_count);
            for i in 0..sample_count {
                let val = f32::from_le_bytes([
                    data[i * 4],
                    data[i * 4 + 1],
                    data[i * 4 + 2],
                    data[i * 4 + 3],
                ]);
                samples.push(val.clamp(-1.0, 1.0));
            }
        }

        let format = AudioFormat {
            channels: ch,
            sample_rate: sr,
            bits_per_sample: bps,
        };

        Ok(AudioClip::new(format, samples))
    }

    /// Helper to encode an `AudioClip` back to standard 16-bit PCM RIFF/WAV format.
    pub fn encode_wav_16bit(clip: &AudioClip) -> Vec<u8> {
        let ch = clip.format.channels;
        let sr = clip.format.sample_rate;
        let num_samples = clip.samples.len();
        let bytes_per_sample = 2;
        let subchunk2_size = (num_samples * bytes_per_sample) as u32;
        let chunk_size = 36 + subchunk2_size;
        let byte_rate = sr * (ch as u32) * (bytes_per_sample as u32);
        let block_align = ch * (bytes_per_sample as u16);

        let mut out = Vec::with_capacity(44 + subchunk2_size as usize);
        // "RIFF"
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&chunk_size.to_le_bytes());
        out.extend_from_slice(b"WAVE");

        // "fmt " subchunk
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes()); // subchunk1 size (16 for PCM)
        out.extend_from_slice(&1u16.to_le_bytes());  // AudioFormat 1 = PCM
        out.extend_from_slice(&ch.to_le_bytes());
        out.extend_from_slice(&sr.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

        // "data" subchunk
        out.extend_from_slice(b"data");
        out.extend_from_slice(&subchunk2_size.to_le_bytes());

        for &sample in &clip.samples {
            let clamped = sample.clamp(-1.0, 1.0);
            let val = (clamped * 32767.0) as i16;
            out.extend_from_slice(&val.to_le_bytes());
        }

        out
    }
}

/// Pure native waveform synthesizer for generating tones, chimes, and alert sounds.
pub struct WaveformSynthesizer;

impl WaveformSynthesizer {
    /// Generate a pure sine wave tone.
    pub fn sine(frequency_hz: f32, duration_secs: f32, sample_rate: u32, volume: f32) -> AudioClip {
        let num_frames = (duration_secs * sample_rate as f32) as usize;
        let mut samples = Vec::with_capacity(num_frames);
        let angular_freq = 2.0 * PI * frequency_hz;

        for i in 0..num_frames {
            let t = i as f32 / sample_rate as f32;
            let sample = (angular_freq * t).sin() * volume;
            samples.push(sample.clamp(-1.0, 1.0));
        }

        AudioClip::new(
            AudioFormat {
                channels: 1,
                sample_rate,
                bits_per_sample: 16,
            },
            samples,
        )
    }

    /// Generate a square wave tone with given duty cycle.
    pub fn square(
        frequency_hz: f32,
        duration_secs: f32,
        sample_rate: u32,
        volume: f32,
        duty_cycle: f32,
    ) -> AudioClip {
        let num_frames = (duration_secs * sample_rate as f32) as usize;
        let mut samples = Vec::with_capacity(num_frames);
        let period_frames = sample_rate as f32 / frequency_hz.max(1.0);

        for i in 0..num_frames {
            let phase = (i as f32 % period_frames) / period_frames;
            let sample = if phase < duty_cycle { volume } else { -volume };
            samples.push(sample.clamp(-1.0, 1.0));
        }

        AudioClip::new(
            AudioFormat {
                channels: 1,
                sample_rate,
                bits_per_sample: 16,
            },
            samples,
        )
    }

    /// Generate a pleasant two-tone web notification chime.
    pub fn notification_chime(sample_rate: u32) -> AudioClip {
        let tone1 = Self::sine(523.25, 0.12, sample_rate, 0.4); // C5
        let tone2 = Self::sine(659.25, 0.20, sample_rate, 0.5); // E5

        let mut samples = Vec::with_capacity(tone1.samples.len() + tone2.samples.len());
        // Apply smooth decay envelope to tone1
        for (i, &s) in tone1.samples.iter().enumerate() {
            let env = 1.0 - (i as f32 / tone1.samples.len() as f32);
            samples.push(s * env);
        }
        // Apply smooth decay envelope to tone2
        for (i, &s) in tone2.samples.iter().enumerate() {
            let env = 1.0 - (i as f32 / tone2.samples.len() as f32);
            samples.push(s * env);
        }

        AudioClip::new(
            AudioFormat {
                channels: 1,
                sample_rate,
                bits_per_sample: 16,
            },
            samples,
        )
    }
}

/// Single active track in the multi-track audio mixer.
#[derive(Debug, Clone)]
pub struct MixerTrack {
    pub clip: AudioClip,
    pub volume: f32,
    pub pan: f32, // -1.0 (full left) to 1.0 (full right)
    pub current_frame: usize,
    pub looping: bool,
    pub is_active: bool,
}

impl MixerTrack {
    pub fn new(clip: AudioClip) -> Self {
        Self {
            clip,
            volume: 1.0,
            pan: 0.0,
            current_frame: 0,
            looping: false,
            is_active: true,
        }
    }
}

/// Multi-track software audio mixer with volume, panning, and soft limiter.
pub struct AudioMixer {
    pub sample_rate: u32,
    pub master_volume: f32,
    pub tracks: Vec<MixerTrack>,
}

impl AudioMixer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            master_volume: 1.0,
            tracks: Vec::new(),
        }
    }

    pub fn add_track(&mut self, track: MixerTrack) -> usize {
        let id = self.tracks.len();
        self.tracks.push(track);
        id
    }

    /// Renders `num_frames` of interleaved stereo 16-bit PCM bytes with soft clipping limiter.
    pub fn render_stereo_pcm16(&mut self, num_frames: usize) -> Vec<u8> {
        let mut left_acc = vec![0.0f32; num_frames];
        let mut right_acc = vec![0.0f32; num_frames];

        for track in &mut self.tracks {
            if !track.is_active {
                continue;
            }

            let total_frames = track.clip.frame_count();
            if total_frames == 0 {
                continue;
            }

            // Panning law: constant power approximation
            let pan = track.pan.clamp(-1.0, 1.0);
            let left_gain = (1.0 - pan).min(1.0) * track.volume;
            let right_gain = (1.0 + pan).min(1.0) * track.volume;

            for f in 0..num_frames {
                if track.current_frame >= total_frames {
                    if track.looping {
                        track.current_frame = 0;
                    } else {
                        track.is_active = false;
                        break;
                    }
                }

                let (s_left, s_right) = if track.clip.format.channels == 1 {
                    let s = track.clip.sample_at(track.current_frame, 0);
                    (s, s)
                } else {
                    (
                        track.clip.sample_at(track.current_frame, 0),
                        track.clip.sample_at(track.current_frame, 1),
                    )
                };

                left_acc[f] += s_left * left_gain;
                right_acc[f] += s_right * right_gain;

                track.current_frame += 1;
            }
        }

        // Apply master volume, soft limiter (tanh approximation), and convert to 16-bit PCM
        let mut out = Vec::with_capacity(num_frames * 4); // 2 channels * 2 bytes
        for f in 0..num_frames {
            let l = Self::soft_limit(left_acc[f] * self.master_volume);
            let r = Self::soft_limit(right_acc[f] * self.master_volume);

            let l_i16 = (l * 32767.0) as i16;
            let r_i16 = (r * 32767.0) as i16;

            out.extend_from_slice(&l_i16.to_le_bytes());
            out.extend_from_slice(&r_i16.to_le_bytes());
        }

        out
    }

    /// Smooth non-linear soft-clipping function to prevent harsh audio distortion.
    fn soft_limit(x: f32) -> f32 {
        if x > 1.0 {
            1.0 - (-x + 1.0).exp() * 0.5
        } else if x < -1.0 {
            -1.0 + (x + 1.0).exp() * 0.5
        } else {
            // Cubic soft saturation
            x - (x * x * x) / 6.0
        }
        .clamp(-1.0, 1.0)
    }
}

/// State machine and controller for an `<audio>` tag in the browser.
#[derive(Debug, Clone)]
pub struct AudioPlayer {
    pub state: PlaybackState,
    pub clip: Option<AudioClip>,
    pub current_time_secs: f32,
    pub volume: f32,
    pub muted: bool,
    pub is_looping: bool,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            state: PlaybackState::Unloaded,
            clip: None,
            current_time_secs: 0.0,
            volume: 1.0,
            muted: false,
            is_looping: false,
        }
    }

    pub fn load_wav(&mut self, wav_bytes: &[u8]) -> Result<(), AudioError> {
        let clip = WavDecoder::decode(wav_bytes)?;
        self.clip = Some(clip);
        self.state = PlaybackState::Ready;
        self.current_time_secs = 0.0;
        Ok(())
    }

    pub fn play(&mut self) {
        if self.clip.is_some() {
            if self.state == PlaybackState::Ended {
                self.current_time_secs = 0.0;
            }
            self.state = PlaybackState::Playing;
        }
    }

    pub fn pause(&mut self) {
        if self.state == PlaybackState::Playing {
            self.state = PlaybackState::Paused;
        }
    }

    pub fn seek(&mut self, time_secs: f32) {
        if let Some(clip) = &self.clip {
            let clamped = time_secs.clamp(0.0, clip.duration_secs);
            self.current_time_secs = clamped;
            if self.state == PlaybackState::Ended && clamped < clip.duration_secs {
                self.state = PlaybackState::Paused;
            }
        }
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 1.0);
    }

    pub fn duration(&self) -> f32 {
        self.clip.as_ref().map(|c| c.duration_secs).unwrap_or(0.0)
    }

    /// Advance playback clock by delta time.
    pub fn advance_clock(&mut self, delta_secs: f32) {
        if self.state != PlaybackState::Playing {
            return;
        }

        if let Some(clip) = &self.clip {
            self.current_time_secs += delta_secs;
            if self.current_time_secs >= clip.duration_secs {
                if self.is_looping {
                    self.current_time_secs %= clip.duration_secs.max(0.001);
                } else {
                    self.current_time_secs = clip.duration_secs;
                    self.state = PlaybackState::Ended;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waveform_synthesis_and_wav_roundtrip() {
        let sample_rate = 44100;
        let tone = WaveformSynthesizer::sine(440.0, 0.1, sample_rate, 0.8);
        assert_eq!(tone.format.channels, 1);
        assert_eq!(tone.format.sample_rate, 44100);
        assert!((tone.duration_secs - 0.1).abs() < 0.001);

        // Encode to WAV
        let wav_bytes = WavDecoder::encode_wav_16bit(&tone);
        assert!(wav_bytes.len() > 44);
        assert_eq!(&wav_bytes[0..4], b"RIFF");
        assert_eq!(&wav_bytes[8..12], b"WAVE");

        // Decode back
        let decoded = WavDecoder::decode(&wav_bytes).expect("decoding synthesized wav must succeed");
        assert_eq!(decoded.format.channels, 1);
        assert_eq!(decoded.format.sample_rate, 44100);
        assert_eq!(decoded.format.bits_per_sample, 16);
        assert_eq!(decoded.frame_count(), tone.frame_count());

        // Check sample proximity (within 16-bit quantization epsilon)
        for (orig, dec) in tone.samples.iter().zip(decoded.samples.iter()) {
            assert!((orig - dec).abs() < 0.01);
        }
    }

    #[test]
    fn test_multi_track_audio_mixer() {
        let sample_rate = 44100;
        let tone1 = WaveformSynthesizer::sine(220.0, 0.05, sample_rate, 0.5);
        let tone2 = WaveformSynthesizer::square(440.0, 0.05, sample_rate, 0.5, 0.5);

        let mut mixer = AudioMixer::new(sample_rate);
        let mut track1 = MixerTrack::new(tone1);
        track1.pan = -0.8; // mostly left
        let mut track2 = MixerTrack::new(tone2);
        track2.pan = 0.8;  // mostly right

        mixer.add_track(track1);
        mixer.add_track(track2);

        let pcm16 = mixer.render_stereo_pcm16(100);
        assert_eq!(pcm16.len(), 100 * 4); // 100 frames * 2 channels * 2 bytes
    }

    #[test]
    fn test_audio_player_state_machine() {
        let mut player = AudioPlayer::new();
        assert_eq!(player.state, PlaybackState::Unloaded);

        let tone = WaveformSynthesizer::notification_chime(44100);
        let wav_bytes = WavDecoder::encode_wav_16bit(&tone);

        player.load_wav(&wav_bytes).expect("load wav");
        assert_eq!(player.state, PlaybackState::Ready);
        assert!(player.duration() > 0.0);

        player.play();
        assert_eq!(player.state, PlaybackState::Playing);

        // Advance 0.1s
        player.advance_clock(0.1);
        assert_eq!(player.state, PlaybackState::Playing);
        assert!((player.current_time_secs - 0.1).abs() < 0.001);

        player.pause();
        assert_eq!(player.state, PlaybackState::Paused);

        // Advance while paused should not move clock
        player.advance_clock(0.1);
        assert!((player.current_time_secs - 0.1).abs() < 0.001);

        // Seek
        player.seek(0.0);
        assert_eq!(player.current_time_secs, 0.0);

        // Play to end
        player.play();
        player.advance_clock(10.0);
        assert_eq!(player.state, PlaybackState::Ended);
    }
}
