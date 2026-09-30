use std::fmt::{Display, Formatter};
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::capture::PixelFormat;

pub const FPS_MODE_30: &str = "30";
pub const FPS_MODE_60: &str = "60";
pub const FPS_MODE_120: &str = "120";
#[allow(dead_code)]
pub const FPS_MODE_CUSTOM: &str = "custom";
pub const MIN_FPS: u32 = 30;
pub const MAX_FPS: u32 = 240;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub video_device: String,
    #[serde(default = "default_video_format")]
    pub video_format: VideoFormat,
    #[serde(default = "default_scaling_filter")]
    pub scaling_filter: ScaleFilter,
    #[serde(default = "default_color_space")]
    pub color_space: ColorSpace,
    #[serde(default = "default_color_range")]
    pub color_range: ColorRange,
    #[serde(default = "default_audio_index")]
    pub audio_input: i32,
    #[serde(default = "default_audio_index")]
    pub audio_output: i32,
    #[serde(default = "default_resolution")]
    pub resolution: String,
    #[serde(default = "default_fps_mode")]
    pub fps_mode: String,
    #[serde(default = "default_custom_fps")]
    pub custom_fps: u32,
    #[serde(default = "default_volume")]
    pub volume: f64,
    #[serde(default = "default_show_overlay")]
    pub show_overlay: bool,
    /// When set, the overlay also reports the active scaling filter, stacked
    /// over three lines. Off by default, keeping the overlay to one line.
    #[serde(default)]
    pub detailed_overlay: bool,
    #[serde(default = "default_sharpness")]
    pub sharpness: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub pixel_format: &'static str,
    pub decode_threads: usize,
}

/// Video / pixel format requested from the capture device.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VideoFormat {
    #[default]
    Auto,
    #[serde(rename = "mjpeg", alias = "mjpg")]
    Mjpeg,
    #[serde(rename = "nv12")]
    Nv12,
    #[serde(rename = "yuy2", alias = "yuyv", alias = "yuyv422")]
    Yuy2,
    #[serde(rename = "uyvy", alias = "uyvy422")]
    Uyvy,
    #[serde(rename = "yuv420p", alias = "yuv12", alias = "yv12", alias = "i420")]
    Yuv420p,
}

impl VideoFormat {
    /// Every variant, in menu order.
    pub const ALL: [Self; 6] = [
        Self::Auto,
        Self::Mjpeg,
        Self::Nv12,
        Self::Yuy2,
        Self::Uyvy,
        Self::Yuv420p,
    ];
}

impl Display for VideoFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("Auto"),
            Self::Mjpeg => f.write_str("MJPEG"),
            Self::Nv12 => f.write_str("NV12"),
            Self::Yuy2 => f.write_str("YUY2"),
            Self::Uyvy => f.write_str("UYVY"),
            Self::Yuv420p => f.write_str("YUV12 (YUV420P)"),
        }
    }
}

/// Upscaling filter applied to the video planes in the fragment shader.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScaleFilter {
    Bilinear,
    Bicubic,
    Lanczos,
    Fsr1,
}

impl ScaleFilter {
    /// The `filter_mode` value the shader branches on. Must stay in step with
    /// the `filter_mode` comparisons in `VIDEO_SHADER`.
    pub fn as_u32(self) -> u32 {
        match self {
            Self::Bilinear => 0,
            Self::Bicubic => 1,
            Self::Lanczos => 2,
            Self::Fsr1 => 3,
        }
    }

    /// Every variant, in menu order.
    pub const ALL: [Self; 4] = [Self::Bilinear, Self::Bicubic, Self::Lanczos, Self::Fsr1];
}

impl Display for ScaleFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bilinear => f.write_str("Bilinear"),
            Self::Bicubic => f.write_str("Bicubic"),
            Self::Lanczos => f.write_str("Lanczos"),
            Self::Fsr1 => f.write_str("FSR 1.0"),
        }
    }
}

/// Color space matrix applied in the fragment shader.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ColorSpace {
    Auto,
    #[serde(rename = "rec709")]
    Rec709,
    #[serde(rename = "bt601")]
    Bt601,
    #[serde(rename = "bt2020")]
    Bt2020,
}

impl ColorSpace {
    /// Resolves `Auto` into a concrete color space based on resolution.
    /// Standard HD/FHD/QHD/UHD (>= 720p) uses Rec.709; SD (< 720p) uses BT.601.
    pub fn resolve(self, _width: u32, height: u32) -> Self {
        match self {
            Self::Auto => {
                if height >= 720 {
                    Self::Rec709
                } else {
                    Self::Bt601
                }
            }
            other => other,
        }
    }

    /// The `color_space` uniform value the shader branches on.
    /// Must stay in step with `VIDEO_SHADER`.
    pub fn as_u32(self) -> u32 {
        match self {
            Self::Rec709 | Self::Auto => 0,
            Self::Bt601 => 1,
            Self::Bt2020 => 2,
        }
    }

    /// Every variant, in menu order.
    pub const ALL: [Self; 4] = [Self::Auto, Self::Rec709, Self::Bt601, Self::Bt2020];
}

impl Display for ColorSpace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("Auto"),
            Self::Rec709 => f.write_str("Rec. 709"),
            Self::Bt601 => f.write_str("BT.601"),
            Self::Bt2020 => f.write_str("BT.2020"),
        }
    }
}

/// Color range (quantization range) applied in the fragment shader.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ColorRange {
    Auto,
    Limited,
    Full,
}

impl ColorRange {
    /// Resolves `Auto` into a concrete range based on pixel format.
    /// NV12 defaults to Limited range (16-235); MJPEG / YUVJ422P defaults to Full range (0-255).
    pub fn resolve(self, format: PixelFormat) -> Self {
        match self {
            Self::Auto => match format {
                PixelFormat::Nv12 => Self::Limited,
                PixelFormat::Yuvj422p => Self::Full,
            },
            other => other,
        }
    }

    /// The `color_range` uniform value the shader branches on.
    /// Must stay in step with `VIDEO_SHADER`.
    pub fn as_u32(self) -> u32 {
        match self {
            Self::Limited | Self::Auto => 0,
            Self::Full => 1,
        }
    }

    /// Every variant, in menu order.
    pub const ALL: [Self; 3] = [Self::Auto, Self::Limited, Self::Full];
}

impl Display for ColorRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => f.write_str("Auto"),
            Self::Limited => f.write_str("Limited (16-235)"),
            Self::Full => f.write_str("Full (0-255)"),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            video_device: String::new(),
            video_format: default_video_format(),
            scaling_filter: default_scaling_filter(),
            color_space: default_color_space(),
            color_range: default_color_range(),
            audio_input: default_audio_index(),
            audio_output: default_audio_index(),
            resolution: default_resolution(),
            fps_mode: default_fps_mode(),
            custom_fps: default_custom_fps(),
            volume: default_volume(),
            show_overlay: default_show_overlay(),
            detailed_overlay: false,
            sharpness: default_sharpness(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };

        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .expect("settings serialization should not fail");
        fs::write(path, json)
    }

    pub fn get_fps(&self) -> u32 {
        match self.fps_mode.as_str() {
            FPS_MODE_30 => 30,
            FPS_MODE_60 => 60,
            FPS_MODE_120 => 120,
            _ => self.custom_fps.clamp(MIN_FPS, MAX_FPS),
        }
    }

    /// Update resolution and fps_mode to reflect what the capture device
    /// actually negotiated (e.g. after fallback to a lower resolution/fps).
    pub fn apply_negotiated(&mut self, width: u32, height: u32, fps: u32) {
        let new_resolution = match (width, height) {
            (3840, 2160) => "4K",
            (2560, 1440) => "1440p",
            (1280, 720) => "720p",
            _ => "1080p",
        };
        let new_fps_mode = match fps {
            30 => FPS_MODE_30,
            120 => FPS_MODE_120,
            60 => FPS_MODE_60,
            other => {
                self.custom_fps = other;
                FPS_MODE_CUSTOM
            }
        };

        self.resolution = new_resolution.to_string();
        self.fps_mode = new_fps_mode.to_string();
    }
}

pub fn get_capture_config(resolution: &str, fps: u32, video_format: VideoFormat) -> CaptureConfig {
    let (width, height) = match resolution {
        "720p" => (1280, 720),
        "1440p" => (2560, 1440),
        "4K" => (3840, 2160),
        _ => (1920, 1080),
    };

    let (pixel_format, decode_threads) = match video_format {
        VideoFormat::Auto => {
            // For 4K @ >30fps, USB capture devices cannot transfer uncompressed NV12 due to USB 3.0 bandwidth limits.
            // For 1440p @ >60fps, MJPEG is also required.
            if (resolution == "4K" && fps > 30) || (resolution == "1440p" && fps > 60) || fps > 120 {
                ("mjpeg", 4)
            } else {
                ("nv12", 1)
            }
        }
        VideoFormat::Mjpeg => ("mjpeg", 4),
        VideoFormat::Nv12 => ("nv12", 1),
        VideoFormat::Yuy2 => ("yuyv422", 1),
        VideoFormat::Uyvy => ("uyvy422", 1),
        VideoFormat::Yuv420p => ("yuv420p", 1),
    };

    CaptureConfig {
        width,
        height,
        fps,
        pixel_format,
        decode_threads,
    }
}

pub fn settings_path() -> PathBuf {
    let mut base = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    if cfg!(debug_assertions) {
        if let Ok(current_dir) = std::env::current_dir() {
            base = current_dir;
        }
    }

    base.join("tacklecast_settings.json")
}

fn default_audio_index() -> i32 {
    -1
}

fn default_video_format() -> VideoFormat {
    VideoFormat::Auto
}

fn default_scaling_filter() -> ScaleFilter {
    ScaleFilter::Bilinear
}

fn default_color_space() -> ColorSpace {
    ColorSpace::Auto
}

fn default_color_range() -> ColorRange {
    ColorRange::Auto
}

fn default_resolution() -> String {
    "1080p".to_string()
}

fn default_fps_mode() -> String {
    FPS_MODE_60.to_string()
}

fn default_custom_fps() -> u32 {
    120
}

fn default_volume() -> f64 {
    1.0
}

fn default_show_overlay() -> bool {
    true
}

fn default_sharpness() -> f32 {
    0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_round_trip() {
        let settings = Settings::default();
        let json = serde_json::to_string_pretty(&settings).unwrap();
        let decoded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, settings);
    }

    #[test]
    fn python_settings_shape_deserializes() {
        let json = r#"{
  "video_device": "ShadowCast 3",
  "scaling_filter": "bicubic",
  "audio_input": 15,
  "audio_output": 12,
  "resolution": "1440p",
  "fps_mode": "120",
  "custom_fps": 120,
  "volume": 1.0,
  "show_overlay": true
}"#;

        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.video_device, "ShadowCast 3");
        assert_eq!(settings.scaling_filter, ScaleFilter::Bicubic);
        assert_eq!(settings.video_format, VideoFormat::Auto);
        assert_eq!(settings.color_space, ColorSpace::Auto);
        assert_eq!(settings.color_range, ColorRange::Auto);
        assert_eq!(settings.audio_input, 15);
        assert_eq!(settings.audio_output, 12);
        assert_eq!(settings.resolution, "1440p");
        assert_eq!(settings.fps_mode, "120");
        assert_eq!(settings.get_fps(), 120);
        assert!(settings.show_overlay);
    }

    #[test]
    fn video_format_serialization_and_aliases() {
        let mut settings = Settings::default();
        settings.video_format = VideoFormat::Yuy2;
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains(r#""video_format":"yuy2""#));

        // Test aliases
        let mjpg: Settings = serde_json::from_str(r#"{"video_format":"mjpg"}"#).unwrap();
        assert_eq!(mjpg.video_format, VideoFormat::Mjpeg);

        let yuyv: Settings = serde_json::from_str(r#"{"video_format":"yuyv"}"#).unwrap();
        assert_eq!(yuyv.video_format, VideoFormat::Yuy2);

        let yuyv422: Settings = serde_json::from_str(r#"{"video_format":"yuyv422"}"#).unwrap();
        assert_eq!(yuyv422.video_format, VideoFormat::Yuy2);

        let yuv12: Settings = serde_json::from_str(r#"{"video_format":"yuv12"}"#).unwrap();
        assert_eq!(yuv12.video_format, VideoFormat::Yuv420p);

        let yv12: Settings = serde_json::from_str(r#"{"video_format":"yv12"}"#).unwrap();
        assert_eq!(yv12.video_format, VideoFormat::Yuv420p);

        let i420: Settings = serde_json::from_str(r#"{"video_format":"i420"}"#).unwrap();
        assert_eq!(i420.video_format, VideoFormat::Yuv420p);
    }

    #[test]
    fn color_space_and_range_serialize() {
        let mut settings = Settings::default();
        settings.color_space = ColorSpace::Rec709;
        settings.color_range = ColorRange::Full;
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains(r#""color_space":"rec709""#));
        assert!(json.contains(r#""color_range":"full""#));

        let decoded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.color_space, ColorSpace::Rec709);
        assert_eq!(decoded.color_range, ColorRange::Full);
    }

    #[test]
    fn color_space_resolve_auto() {
        assert_eq!(ColorSpace::Auto.resolve(1920, 1080), ColorSpace::Rec709);
        assert_eq!(ColorSpace::Auto.resolve(1280, 720), ColorSpace::Rec709);
        assert_eq!(ColorSpace::Auto.resolve(2560, 1440), ColorSpace::Rec709);
        assert_eq!(ColorSpace::Auto.resolve(3840, 2160), ColorSpace::Rec709);
        assert_eq!(ColorSpace::Auto.resolve(640, 480), ColorSpace::Bt601);

        assert_eq!(ColorSpace::Bt601.resolve(1920, 1080), ColorSpace::Bt601);
        assert_eq!(ColorSpace::Bt2020.resolve(1920, 1080), ColorSpace::Bt2020);
        assert_eq!(ColorSpace::Rec709.resolve(640, 480), ColorSpace::Rec709);
    }

    #[test]
    fn color_range_resolve_auto() {
        assert_eq!(ColorRange::Auto.resolve(PixelFormat::Nv12), ColorRange::Limited);
        assert_eq!(ColorRange::Auto.resolve(PixelFormat::Yuvj422p), ColorRange::Full);

        assert_eq!(ColorRange::Limited.resolve(PixelFormat::Yuvj422p), ColorRange::Limited);
        assert_eq!(ColorRange::Full.resolve(PixelFormat::Nv12), ColorRange::Full);
    }

    #[test]
    fn capture_config_matches_python_logic() {
        let nv12 = get_capture_config("1080p", 60, VideoFormat::Auto);
        assert_eq!(nv12.pixel_format, "nv12");
        assert_eq!(nv12.decode_threads, 1);

        let mjpeg = get_capture_config("1440p", 120, VideoFormat::Auto);
        assert_eq!(mjpeg.width, 2560);
        assert_eq!(mjpeg.height, 1440);
        assert_eq!(mjpeg.pixel_format, "mjpeg");
        assert_eq!(mjpeg.decode_threads, 4);
    }

    #[test]
    fn get_capture_config_explicit_formats() {
        let mjpeg = get_capture_config("1080p", 60, VideoFormat::Mjpeg);
        assert_eq!(mjpeg.pixel_format, "mjpeg");
        assert_eq!(mjpeg.decode_threads, 4);

        let nv12 = get_capture_config("1440p", 120, VideoFormat::Nv12);
        assert_eq!(nv12.pixel_format, "nv12");
        assert_eq!(nv12.decode_threads, 1);

        let yuy2 = get_capture_config("1080p", 60, VideoFormat::Yuy2);
        assert_eq!(yuy2.pixel_format, "yuyv422");
        assert_eq!(yuy2.decode_threads, 1);

        let uyvy = get_capture_config("1080p", 60, VideoFormat::Uyvy);
        assert_eq!(uyvy.pixel_format, "uyvy422");

        let yuv420p = get_capture_config("1080p", 60, VideoFormat::Yuv420p);
        assert_eq!(yuv420p.pixel_format, "yuv420p");
    }

    #[test]
    fn test_smart_auto_resolution() {
        // 4K @ 60 FPS with Auto must pick MJPEG
        let cfg_4k60 = get_capture_config("4K", 60, VideoFormat::Auto);
        assert_eq!(cfg_4k60.pixel_format, "mjpeg");
        assert_eq!(cfg_4k60.decode_threads, 4);

        // 4K @ 30 FPS with Auto can use NV12
        let cfg_4k30 = get_capture_config("4K", 30, VideoFormat::Auto);
        assert_eq!(cfg_4k30.pixel_format, "nv12");

        // 1080p @ 60 FPS with Auto uses NV12
        let cfg_1080p60 = get_capture_config("1080p", 60, VideoFormat::Auto);
        assert_eq!(cfg_1080p60.pixel_format, "nv12");

        // 1080p @ 120 FPS with Auto uses NV12
        let cfg_1080p120 = get_capture_config("1080p", 120, VideoFormat::Auto);
        assert_eq!(cfg_1080p120.pixel_format, "nv12");

        // 1440p @ 120 FPS with Auto must use MJPEG
        let cfg_1440p120 = get_capture_config("1440p", 120, VideoFormat::Auto);
        assert_eq!(cfg_1440p120.pixel_format, "mjpeg");
    }
}
