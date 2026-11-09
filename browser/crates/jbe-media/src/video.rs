//! Native video playback and frame decoding engine for Jinn Browser Engine.
//!
//! Provides pure, zero-dependency sequential motion video frame decoding,
//! aspect-ratio viewport scaling, and `<video>` controller state machine.

extern crate alloc;
use alloc::vec::Vec;
use jbe_core::Rect;

/// Playback state for video playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoState {
    Unloaded,
    Ready,
    Playing,
    Paused,
    Ended,
}

/// Single decoded video frame.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub index: u32,
    pub timestamp_secs: f32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>, // 0xAARRGGBB format
}

/// Errors occurring during video decoding or playback.
#[derive(Debug, Clone, PartialEq)]
pub enum VideoError {
    InvalidHeader,
    UnsupportedCodec,
    CorruptedFrame,
    UnexpectedEof,
    DimensionMismatch,
}

/// Jinn Motion Video (JMVD) format decoder and encoder.
/// Magic: b"JMVD" (4 bytes)
/// Header:
///   magic: [u8; 4]
///   version: u16
///   width: u16
///   height: u16
///   fps: u16
///   frame_count: u32
pub struct MotionVideoContainer;

impl MotionVideoContainer {
    pub const MAGIC: &'static [u8; 4] = b"JMVD";
    pub const VERSION: u16 = 1;

    /// Encodes a sequence of video frames into the native JMVD binary stream.
    pub fn encode(width: u16, height: u16, fps: u16, frames: &[VideoFrame]) -> Vec<u8> {
        let frame_count = frames.len() as u32;
        let mut out = Vec::new();
        // Header
        out.extend_from_slice(Self::MAGIC);
        out.extend_from_slice(&Self::VERSION.to_le_bytes());
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&fps.to_le_bytes());
        out.extend_from_slice(&frame_count.to_le_bytes());

        // Frames
        for frame in frames {
            let ts_ms = (frame.timestamp_secs * 1000.0) as u32;
            out.extend_from_slice(&ts_ms.to_le_bytes());
            // Frame compression: 0 = Raw RGBA u32
            out.push(0u8);
            let payload_bytes = frame.pixels.len() * 4;
            out.extend_from_slice(&(payload_bytes as u32).to_le_bytes());

            for &px in &frame.pixels {
                out.extend_from_slice(&px.to_le_bytes());
            }
        }

        out
    }

    /// Decodes a native JMVD binary stream into a vector of video frames.
    pub fn decode(bytes: &[u8]) -> Result<(u16, u16, u16, Vec<VideoFrame>), VideoError> {
        if bytes.len() < 16 {
            return Err(VideoError::UnexpectedEof);
        }

        if &bytes[0..4] != Self::MAGIC {
            return Err(VideoError::InvalidHeader);
        }

        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != Self::VERSION {
            return Err(VideoError::UnsupportedCodec);
        }

        let width = u16::from_le_bytes([bytes[6], bytes[7]]);
        let height = u16::from_le_bytes([bytes[8], bytes[9]]);
        let fps = u16::from_le_bytes([bytes[10], bytes[11]]);
        let frame_count = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

        let mut frames = Vec::with_capacity(frame_count as usize);
        let mut offset = 16;
        let expected_pixels = (width as usize) * (height as usize);

        for idx in 0..frame_count {
            if offset + 9 > bytes.len() {
                return Err(VideoError::UnexpectedEof);
            }

            let ts_ms = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            let compression = bytes[offset + 4];
            let payload_len = u32::from_le_bytes([
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
                bytes[offset + 8],
            ]) as usize;
            offset += 9;

            if offset + payload_len > bytes.len() {
                return Err(VideoError::UnexpectedEof);
            }

            let frame_bytes = &bytes[offset..offset + payload_len];
            offset += payload_len;

            let mut pixels = Vec::with_capacity(expected_pixels);

            if compression == 0 {
                // Raw RGBA
                if payload_len != expected_pixels * 4 {
                    return Err(VideoError::DimensionMismatch);
                }
                for chunk in frame_bytes.chunks_exact(4) {
                    let px = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    pixels.push(px);
                }
            } else {
                return Err(VideoError::UnsupportedCodec);
            }

            frames.push(VideoFrame {
                index: idx,
                timestamp_secs: ts_ms as f32 / 1000.0,
                width: width as u32,
                height: height as u32,
                pixels,
            });
        }

        Ok((width, height, fps, frames))
    }
}

/// State machine and controller for `<video>` element.
#[derive(Debug, Clone)]
pub struct VideoPlayer {
    pub state: VideoState,
    pub width: u32,
    pub height: u32,
    pub fps: u16,
    pub frames: Vec<VideoFrame>,
    pub current_time_secs: f32,
    pub current_frame_index: usize,
    pub duration_secs: f32,
    pub volume: f32,
    pub is_looping: bool,
}

impl VideoPlayer {
    pub fn new() -> Self {
        Self {
            state: VideoState::Unloaded,
            width: 0,
            height: 0,
            fps: 30,
            frames: Vec::new(),
            current_time_secs: 0.0,
            current_frame_index: 0,
            duration_secs: 0.0,
            volume: 1.0,
            is_looping: false,
        }
    }

    pub fn load_video_bytes(&mut self, bytes: &[u8]) -> Result<(), VideoError> {
        let (w, h, fps, frames) = MotionVideoContainer::decode(bytes)?;
        let dur = if !frames.is_empty() {
            frames.last().unwrap().timestamp_secs + (1.0 / fps.max(1) as f32)
        } else {
            0.0
        };

        self.width = w as u32;
        self.height = h as u32;
        self.fps = fps;
        self.frames = frames;
        self.duration_secs = dur;
        self.current_time_secs = 0.0;
        self.current_frame_index = 0;
        self.state = VideoState::Ready;
        Ok(())
    }

    pub fn play(&mut self) {
        if !self.frames.is_empty() {
            if self.state == VideoState::Ended {
                self.current_time_secs = 0.0;
                self.current_frame_index = 0;
            }
            self.state = VideoState::Playing;
        }
    }

    pub fn pause(&mut self) {
        if self.state == VideoState::Playing {
            self.state = VideoState::Paused;
        }
    }

    pub fn seek(&mut self, time_secs: f32) {
        let clamped = time_secs.clamp(0.0, self.duration_secs);
        self.current_time_secs = clamped;
        self.update_frame_index();
        if self.state == VideoState::Ended && clamped < self.duration_secs {
            self.state = VideoState::Paused;
        }
    }

    pub fn advance_clock(&mut self, delta_secs: f32) {
        if self.state != VideoState::Playing {
            return;
        }

        self.current_time_secs += delta_secs;
        if self.current_time_secs >= self.duration_secs {
            if self.is_looping {
                self.current_time_secs %= self.duration_secs.max(0.001);
                self.update_frame_index();
            } else {
                self.current_time_secs = self.duration_secs;
                self.current_frame_index = self.frames.len().saturating_sub(1);
                self.state = VideoState::Ended;
            }
        } else {
            self.update_frame_index();
        }
    }

    fn update_frame_index(&mut self) {
        if self.frames.is_empty() {
            self.current_frame_index = 0;
            return;
        }

        // Binary search or linear scan for current frame
        let mut best_idx = 0;
        for (i, frame) in self.frames.iter().enumerate() {
            if frame.timestamp_secs <= self.current_time_secs {
                best_idx = i;
            } else {
                break;
            }
        }
        self.current_frame_index = best_idx;
    }

    pub fn current_frame(&self) -> Option<&VideoFrame> {
        self.frames.get(self.current_frame_index)
    }

    /// Compute letterboxed / aspect-ratio preserved destination rectangle inside target viewport.
    pub fn fit_contain(&self, target: Rect) -> Rect {
        if self.width == 0 || self.height == 0 || target.width <= 0.0 || target.height <= 0.0 {
            return target;
        }

        let aspect_video = self.width as f32 / self.height as f32;
        let aspect_target = target.width / target.height;

        if aspect_video > aspect_target {
            // Letterbox (black bars top/bottom)
            let w = target.width;
            let h = w / aspect_video;
            let y = target.y + (target.height - h) / 2.0;
            Rect::new(target.x, y, w, h)
        } else {
            // Pillarbox (black bars left/right)
            let h = target.height;
            let w = h * aspect_video;
            let x = target.x + (target.width - w) / 2.0;
            Rect::new(x, target.y, w, h)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_video_encode_and_decode() {
        let width = 64;
        let height = 48;
        let fps = 10;
        let num_frames = 5;

        let mut frames = Vec::new();
        for i in 0..num_frames {
            let px_color = 0xFF000000 | ((i * 40) as u32);
            let pixels = vec![px_color; (width * height) as usize];
            frames.push(VideoFrame {
                index: i as u32,
                timestamp_secs: i as f32 * 0.1,
                width: width as u32,
                height: height as u32,
                pixels,
            });
        }

        let encoded = MotionVideoContainer::encode(width, height, fps, &frames);
        assert!(encoded.len() > 16);

        let (w, h, decoded_fps, dec_frames) =
            MotionVideoContainer::decode(&encoded).expect("decode valid JMVD");
        assert_eq!(w, width);
        assert_eq!(h, height);
        assert_eq!(decoded_fps, fps);
        assert_eq!(dec_frames.len(), num_frames as usize);

        for (orig, dec) in frames.iter().zip(dec_frames.iter()) {
            assert_eq!(orig.index, dec.index);
            assert!((orig.timestamp_secs - dec.timestamp_secs).abs() < 0.001);
            assert_eq!(orig.pixels, dec.pixels);
        }
    }

    #[test]
    fn test_video_player_clock_and_aspect_fit() {
        let mut player = VideoPlayer::new();
        let width = 320;
        let height = 240; // 4:3
        let fps = 10;

        let frames = vec![
            VideoFrame {
                index: 0,
                timestamp_secs: 0.0,
                width,
                height,
                pixels: vec![0xFFFFFFFF; (width * height) as usize],
            },
            VideoFrame {
                index: 1,
                timestamp_secs: 0.1,
                width,
                height,
                pixels: vec![0xFF00FF00; (width * height) as usize],
            },
            VideoFrame {
                index: 2,
                timestamp_secs: 0.2,
                width,
                height,
                pixels: vec![0xFFFF0000; (width * height) as usize],
            },
        ];

        let bytes = MotionVideoContainer::encode(width as u16, height as u16, fps, &frames);
        player.load_video_bytes(&bytes).expect("load video");
        assert_eq!(player.state, VideoState::Ready);

        player.play();
        assert_eq!(player.state, VideoState::Playing);
        assert_eq!(player.current_frame_index, 0);

        player.advance_clock(0.12);
        assert_eq!(player.current_frame_index, 1);

        // Aspect fit in 800x600 (4:3 into 4:3) -> exact
        let r1 = player.fit_contain(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert!((r1.width - 800.0).abs() < 0.01);
        assert!((r1.height - 600.0).abs() < 0.01);

        // Aspect fit in 1000x500 (widescreen target) -> pillarbox
        let r2 = player.fit_contain(Rect::new(0.0, 0.0, 1000.0, 500.0));
        assert!((r2.height - 500.0).abs() < 0.01);
        let expected_w = 500.0 * (4.0 / 3.0);
        assert!((r2.width - expected_w).abs() < 0.01);
    }
}
