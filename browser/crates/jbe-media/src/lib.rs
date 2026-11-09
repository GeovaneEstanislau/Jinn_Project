//! `jbe-media`: Pure native audio and video engine for Jinn Browser Engine.
//!
//! Provides zero-dependency native multimedia decoders and controllers:
//! - RIFF/WAV PCM audio decoder (8, 16, 24, 32-bit; mono/stereo).
//! - Waveform synthesizer (sine, square, chimes, beeps).
//! - Multi-track software audio mixer with volume, pan, and soft limiter.
//! - AudioPlayer state machine for `<audio>` tags.
//! - Motion Video container (JMVD) decoder and encoder.
//! - VideoPlayer controller with aspect-ratio viewport fitting for `<video>` tags.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod audio;
pub mod video;

pub use audio::{
    AudioClip, AudioError, AudioFormat, AudioMixer, AudioPlayer, MixerTrack, PlaybackState,
    WavDecoder, WaveformSynthesizer,
};
pub use video::{MotionVideoContainer, VideoError, VideoFrame, VideoPlayer, VideoState};
