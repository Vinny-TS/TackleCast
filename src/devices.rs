use std::collections::HashMap;
use std::sync::Mutex;

use crate::settings::{
    Settings, VideoFormat, FPS_MODE_120, FPS_MODE_30, FPS_MODE_60, FPS_MODE_CUSTOM,
};
use cpal::traits::{DeviceTrait, HostTrait};
use tracing::warn;
use windows::core::{Interface, GUID, HSTRING, VARIANT};
use windows::Win32::Media::DirectShow::{
    IAMStreamConfig, IBaseFilter, ICreateDevEnum, PINDIR_OUTPUT, PIN_INFO,
    VIDEO_STREAM_CONFIG_CAPS,
};
use windows::Win32::Media::MediaFoundation::{
    AM_MEDIA_TYPE, CLSID_SystemDeviceEnum, CLSID_VideoInputDeviceCategory,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Com::StructuredStorage::IPropertyBag;

#[allow(dead_code)]
const IGNORED_KEYWORDS: &[&str] = &["pro", "the", "and"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    pub index: i32,
    pub name: String,
}

pub fn enumerate_video_devices() -> Vec<String> {
    match enumerate_video_devices_dshow() {
        Ok(devices) => devices,
        Err(error) => {
            warn!("DirectShow video device enumeration failed: {error}");
            Vec::new()
        }
    }
}

fn enumerate_video_devices_dshow() -> Result<Vec<String>, String> {
    unsafe {
        // COM may already be initialized on this thread; ignore errors from re-init.
        // Use STA to be compatible with winit's OleInitialize.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dev_enum: ICreateDevEnum =
            CoCreateInstance(&CLSID_SystemDeviceEnum, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("CoCreateInstance for SystemDeviceEnum failed: {e}"))?;

        let mut enumerator = None;
        dev_enum
            .CreateClassEnumerator(
                &CLSID_VideoInputDeviceCategory as *const GUID,
                &mut enumerator,
                0,
            )
            .map_err(|e| format!("CreateClassEnumerator failed: {e}"))?;

        let Some(enumerator) = enumerator else {
            // No video capture devices on this system
            return Ok(Vec::new());
        };

        let mut devices = Vec::new();
        loop {
            let mut moniker = [None];
            let hr = enumerator.Next(&mut moniker, None);
            if hr.is_err() {
                break;
            }
            let Some(moniker) = moniker[0].take() else {
                break;
            };

            let bag: Result<IPropertyBag, _> =
                moniker.BindToStorage(None, None);
            let Ok(bag) = bag else {
                continue;
            };

            let mut var = VARIANT::default();
            let name_prop = HSTRING::from("FriendlyName");
            if bag.Read(&name_prop, &mut var, None).is_ok() {
                let name = format!("{}", var);
                let name = name.trim().to_string();
                if !name.is_empty() {
                    devices.push(name);
                }
            }
        }

        Ok(devices)
    }
}

pub fn enumerate_audio_inputs() -> Vec<AudioDevice> {
    enumerate_audio_devices(Direction::Input)
}

pub fn enumerate_audio_outputs() -> Vec<AudioDevice> {
    enumerate_audio_devices(Direction::Output)
}

#[allow(dead_code)]
pub fn find_audio_input_for_video(video_device_name: &str, inputs: &[AudioDevice]) -> Option<i32> {
    let keywords = keywords_for_device_name(video_device_name);
    if keywords.is_empty() {
        return None;
    }

    let mut best_index = None;
    let mut best_score = 0;
    for device in inputs {
        let haystack = device.name.to_ascii_lowercase();
        let score = keywords.iter().filter(|keyword| haystack.contains(keyword.as_str())).count();
        if score > best_score {
            best_score = score;
            best_index = Some(device.index);
        }
    }

    let threshold = if keywords.len() == 1 { 1 } else { 2 };
    (best_score >= threshold).then_some(best_index?).or(None)
}

fn enumerate_audio_devices(direction: Direction) -> Vec<AudioDevice> {
    let host = preferred_audio_host();
    let Ok(devices) = host.devices() else {
        return Vec::new();
    };

    devices
        .enumerate()
        .filter_map(|(index, device)| {
            let name = device.name().ok()?;
            let supported = match direction {
                Direction::Input => device
                    .supported_input_configs()
                    .ok()
                    .and_then(|mut configs| configs.next())
                    .is_some(),
                Direction::Output => device
                    .supported_output_configs()
                    .ok()
                    .and_then(|mut configs| configs.next())
                    .is_some(),
            };

            supported.then_some(AudioDevice {
                index: index as i32,
                name,
            })
        })
        .collect()
}

fn preferred_audio_host() -> cpal::Host {
    #[cfg(target_os = "windows")]
    {
        if let Ok(host) = cpal::host_from_id(cpal::HostId::Wasapi) {
            return host;
        }
    }

    cpal::default_host()
}

#[allow(dead_code)]
fn keywords_for_device_name(device_name: &str) -> Vec<String> {
    device_name
        .to_ascii_lowercase()
        .replace('-', " ")
        .split_whitespace()
        .filter(|word| word.len() >= 3 && !IGNORED_KEYWORDS.contains(word))
        .map(ToString::to_string)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFormatCapability {
    pub format: VideoFormat,
    pub width: u32,
    pub height: u32,
    pub min_fps: u32,
    pub max_fps: u32,
}

pub fn query_device_capabilities(device_name: &str) -> Vec<DeviceFormatCapability> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let Ok(dev_enum) = CoCreateInstance::<_, ICreateDevEnum>(&CLSID_SystemDeviceEnum, None, CLSCTX_INPROC_SERVER) else {
            return Vec::new();
        };

        let mut enumerator = None;
        if dev_enum.CreateClassEnumerator(&CLSID_VideoInputDeviceCategory as *const GUID, &mut enumerator, 0).is_err() {
            return Vec::new();
        }

        let Some(enumerator) = enumerator else {
            return Vec::new();
        };

        let mut moniker = [None];
        while enumerator.Next(&mut moniker, None).is_ok() {
            let Some(m) = moniker[0].take() else { break };
            let Ok(bag) = m.BindToStorage::<_, _, IPropertyBag>(None, None) else { continue };

            let mut var = VARIANT::default();
            let name_prop = HSTRING::from("FriendlyName");
            if bag.Read(&name_prop, &mut var, None).is_ok() {
                let name = format!("{}", var).trim().to_string();
                if name == device_name {
                    return query_filter_capabilities(&m);
                }
            }
        }

        Vec::new()
    }
}

unsafe fn query_filter_capabilities(moniker: &windows::Win32::System::Com::IMoniker) -> Vec<DeviceFormatCapability> {
    let Ok(filter) = moniker.BindToObject::<_, _, IBaseFilter>(None, None) else {
        return Vec::new();
    };

    let Ok(enum_pins) = filter.EnumPins() else {
        return Vec::new();
    };

    let mut capabilities = Vec::new();
    let mut pin_arr = [None];

    while enum_pins.Next(&mut pin_arr, None).is_ok() {
        let Some(pin) = pin_arr[0].take() else { break };
        let mut pin_info = PIN_INFO::default();
        if pin.QueryPinInfo(&mut pin_info).is_err() {
            continue;
        }
        if pin_info.dir != PINDIR_OUTPUT {
            continue;
        }

        let Ok(stream_config) = pin.cast::<IAMStreamConfig>() else {
            continue;
        };

        let mut count = 0i32;
        let mut size = 0i32;
        if stream_config.GetNumberOfCapabilities(&mut count, &mut size).is_err() {
            continue;
        }

        if size as usize != std::mem::size_of::<VIDEO_STREAM_CONFIG_CAPS>() {
            continue;
        }

        let mut caps = VIDEO_STREAM_CONFIG_CAPS::default();
        for i in 0..count {
            let mut p_media_type: *mut AM_MEDIA_TYPE = std::ptr::null_mut();
            if stream_config.GetStreamCaps(i, &mut p_media_type, &mut caps as *mut _ as *mut u8).is_ok() {
                if !p_media_type.is_null() {
                    let mt = &*p_media_type;
                    let fourcc = mt.subtype.data1;
                    let format_opt = match fourcc {
                        0x3231564E => Some(VideoFormat::Nv12),      // "NV12"
                        0x47504A4D => Some(VideoFormat::Mjpeg),     // "MJPG"
                        0x32595559 => Some(VideoFormat::Yuy2),      // "YUY2"
                        0x59565955 => Some(VideoFormat::Uyvy),      // "UYVY"
                        0x32315659 | 0x30323449 => Some(VideoFormat::Yuv420p), // "YV12", "I420"
                        _ => None,
                    };

                    let width = caps.MaxOutputSize.cx as u32;
                    let height = caps.MaxOutputSize.cy as u32;
                    let max_fps = if caps.MinFrameInterval > 0 {
                        (10_000_000.0 / caps.MinFrameInterval as f64).round() as u32
                    } else {
                        0
                    };
                    let min_fps = if caps.MaxFrameInterval > 0 {
                        (10_000_000.0 / caps.MaxFrameInterval as f64).round() as u32
                    } else {
                        0
                    };

                    if let Some(format) = format_opt {
                        if width > 0 && height > 0 {
                            capabilities.push(DeviceFormatCapability {
                                format,
                                width,
                                height,
                                min_fps,
                                max_fps,
                            });
                        }
                    }

                    if !mt.pbFormat.is_null() {
                        CoTaskMemFree(Some(mt.pbFormat as *const _));
                    }
                    CoTaskMemFree(Some(p_media_type as *const _));
                }
            }
        }
    }

    // Deduplicate and consolidate min_fps/max_fps for identical (format, width, height)
    let mut consolidated: Vec<DeviceFormatCapability> = Vec::new();
    for cap in capabilities {
        if let Some(existing) = consolidated.iter_mut().find(|c| {
            c.format == cap.format && c.width == cap.width && c.height == cap.height
        }) {
            existing.min_fps = existing.min_fps.min(cap.min_fps);
            existing.max_fps = existing.max_fps.max(cap.max_fps);
        } else {
            consolidated.push(cap);
        }
    }

    consolidated
}

static CAPABILITIES_CACHE: Mutex<Option<HashMap<String, Vec<DeviceFormatCapability>>>> =
    Mutex::new(None);

pub fn get_device_capabilities(device_name: &str) -> Vec<DeviceFormatCapability> {
    if device_name.is_empty() {
        return Vec::new();
    }
    let mut guard = CAPABILITIES_CACHE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(caps) = map.get(device_name) {
        return caps.clone();
    }
    let caps = query_device_capabilities(device_name);
    map.insert(device_name.to_string(), caps.clone());
    caps
}

#[allow(dead_code)]
pub fn clear_capabilities_cache() {
    let mut guard = CAPABILITIES_CACHE.lock().unwrap();
    *guard = None;
}

pub fn resolution_dimensions(resolution: &str) -> (u32, u32) {
    match resolution {
        "720p" => (1280, 720),
        "1440p" => (2560, 1440),
        "4K" => (3840, 2160),
        _ => (1920, 1080),
    }
}

pub fn supported_resolutions(caps: &[DeviceFormatCapability]) -> Vec<&'static str> {
    if caps.is_empty() {
        return vec!["720p", "1080p", "1440p", "4K"];
    }
    let candidates = ["720p", "1080p", "1440p", "4K"];
    let mut res = Vec::new();
    for &candidate in &candidates {
        let (w, h) = resolution_dimensions(candidate);
        if caps.iter().any(|c| c.width == w && c.height == h) {
            res.push(candidate);
        }
    }
    if res.is_empty() {
        vec!["720p", "1080p", "1440p", "4K"]
    } else {
        res
    }
}

pub fn max_fps_for_resolution(caps: &[DeviceFormatCapability], resolution: &str) -> u32 {
    if caps.is_empty() {
        return 240;
    }
    let (w, h) = resolution_dimensions(resolution);
    caps.iter()
        .filter(|c| c.width == w && c.height == h)
        .map(|c| c.max_fps)
        .max()
        .unwrap_or(60)
}

pub fn supported_fps_modes(
    caps: &[DeviceFormatCapability],
    resolution: &str,
) -> Vec<(&'static str, &'static str)> {
    if caps.is_empty() {
        return vec![
            (FPS_MODE_30, "30 FPS"),
            (FPS_MODE_60, "60 FPS"),
            (FPS_MODE_120, "120 FPS"),
            (FPS_MODE_CUSTOM, "Custom"),
        ];
    }
    let max_fps = max_fps_for_resolution(caps, resolution);
    let mut modes = Vec::new();
    if max_fps >= 30 {
        modes.push((FPS_MODE_30, "30 FPS"));
    }
    if max_fps >= 60 {
        modes.push((FPS_MODE_60, "60 FPS"));
    }
    if max_fps >= 120 {
        modes.push((FPS_MODE_120, "120 FPS"));
    }
    modes.push((FPS_MODE_CUSTOM, "Custom"));
    modes
}

pub fn supported_video_formats(
    caps: &[DeviceFormatCapability],
    resolution: &str,
    target_fps: u32,
) -> Vec<VideoFormat> {
    if caps.is_empty() {
        return VideoFormat::ALL.to_vec();
    }
    let (w, h) = resolution_dimensions(resolution);
    let fps_thresh = target_fps.saturating_sub(1);
    let mut formats = vec![VideoFormat::Auto];
    for &fmt in &[
        VideoFormat::Mjpeg,
        VideoFormat::Nv12,
        VideoFormat::Yuy2,
        VideoFormat::Uyvy,
        VideoFormat::Yuv420p,
    ] {
        if caps
            .iter()
            .any(|c| c.format == fmt && c.width == w && c.height == h && c.max_fps >= fps_thresh)
        {
            formats.push(fmt);
        }
    }
    formats
}

pub fn sanitize_draft_settings(draft: &mut Settings, caps: &[DeviceFormatCapability]) {
    if caps.is_empty() {
        return;
    }
    let res_options = supported_resolutions(caps);
    if !res_options.contains(&draft.resolution.as_str()) {
        if let Some(&first) = res_options.last() {
            draft.resolution = first.to_string();
        }
    }
    let fps_modes = supported_fps_modes(caps, &draft.resolution);
    if !fps_modes.iter().any(|(mode, _)| *mode == draft.fps_mode.as_str()) {
        if let Some(&(best_mode, _)) = fps_modes.iter().filter(|(m, _)| *m != FPS_MODE_CUSTOM).last() {
            draft.fps_mode = best_mode.to_string();
        }
    }
    let max_fps = max_fps_for_resolution(caps, &draft.resolution);
    if draft.custom_fps > max_fps {
        draft.custom_fps = max_fps;
    }
    let valid_formats = supported_video_formats(caps, &draft.resolution, draft.get_fps());
    if !valid_formats.contains(&draft.video_format) {
        draft.video_format = VideoFormat::Auto;
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Input,
    Output,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_detect_matches_keyword_overlap() {
        let inputs = vec![
            AudioDevice {
                index: 2,
                name: "Microphone (USB Audio Device)".to_string(),
            },
            AudioDevice {
                index: 7,
                name: "ShadowCast Capture Audio".to_string(),
            },
        ];

        assert_eq!(
            find_audio_input_for_video("ShadowCast Capture", &inputs),
            Some(7)
        );
    }

    #[test]
    fn test_device_capabilities_query() {
        let devices = enumerate_video_devices();
        println!("Discovered devices: {devices:?}");
        if let Some(first) = devices.first() {
            let caps = query_device_capabilities(first);
            println!("Capabilities for '{first}':");
            for cap in &caps {
                println!(
                    "  {:?} {}x{} (min_fps={}, max_fps={})",
                    cap.format, cap.width, cap.height, cap.min_fps, cap.max_fps
                );
            }
        }
    }

    #[test]
    fn test_supported_resolutions() {
        let empty_caps = Vec::new();
        assert_eq!(
            supported_resolutions(&empty_caps),
            vec!["720p", "1080p", "1440p", "4K"]
        );

        let card_1080p = vec![
            DeviceFormatCapability {
                format: VideoFormat::Mjpeg,
                width: 1280,
                height: 720,
                min_fps: 30,
                max_fps: 60,
            },
            DeviceFormatCapability {
                format: VideoFormat::Mjpeg,
                width: 1920,
                height: 1080,
                min_fps: 30,
                max_fps: 60,
            },
        ];
        assert_eq!(supported_resolutions(&card_1080p), vec!["720p", "1080p"]);

        let card_4k = vec![DeviceFormatCapability {
            format: VideoFormat::Mjpeg,
            width: 3840,
            height: 2160,
            min_fps: 30,
            max_fps: 60,
        }];
        assert_eq!(supported_resolutions(&card_4k), vec!["4K"]);
    }

    #[test]
    fn test_supported_fps_modes() {
        let card_caps = vec![
            DeviceFormatCapability {
                format: VideoFormat::Mjpeg,
                width: 3840,
                height: 2160,
                min_fps: 25,
                max_fps: 60,
            },
            DeviceFormatCapability {
                format: VideoFormat::Mjpeg,
                width: 1920,
                height: 1080,
                min_fps: 25,
                max_fps: 240,
            },
        ];

        let fps_4k = supported_fps_modes(&card_caps, "4K");
        let fps_4k_keys: Vec<&str> = fps_4k.iter().map(|(k, _)| *k).collect();
        assert_eq!(fps_4k_keys, vec![FPS_MODE_30, FPS_MODE_60, FPS_MODE_CUSTOM]);

        let fps_1080p = supported_fps_modes(&card_caps, "1080p");
        let fps_1080p_keys: Vec<&str> = fps_1080p.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            fps_1080p_keys,
            vec![FPS_MODE_30, FPS_MODE_60, FPS_MODE_120, FPS_MODE_CUSTOM]
        );
    }

    #[test]
    fn test_supported_video_formats() {
        let card_caps = vec![
            // 4K caps: MJPEG up to 60fps, NV12 up to 30fps, YUV12 up to 30fps
            DeviceFormatCapability {
                format: VideoFormat::Mjpeg,
                width: 3840,
                height: 2160,
                min_fps: 25,
                max_fps: 60,
            },
            DeviceFormatCapability {
                format: VideoFormat::Nv12,
                width: 3840,
                height: 2160,
                min_fps: 25,
                max_fps: 30,
            },
            DeviceFormatCapability {
                format: VideoFormat::Yuv420p,
                width: 3840,
                height: 2160,
                min_fps: 25,
                max_fps: 30,
            },
            // 1080p caps: MJPEG up to 240, NV12 up to 120, YUY2 up to 60
            DeviceFormatCapability {
                format: VideoFormat::Mjpeg,
                width: 1920,
                height: 1080,
                min_fps: 25,
                max_fps: 240,
            },
            DeviceFormatCapability {
                format: VideoFormat::Nv12,
                width: 1920,
                height: 1080,
                min_fps: 5,
                max_fps: 120,
            },
            DeviceFormatCapability {
                format: VideoFormat::Yuy2,
                width: 1920,
                height: 1080,
                min_fps: 5,
                max_fps: 60,
            },
        ];

        // 4K @ 60 FPS -> only Auto and MJPEG
        let fmts_4k60 = supported_video_formats(&card_caps, "4K", 60);
        assert_eq!(fmts_4k60, vec![VideoFormat::Auto, VideoFormat::Mjpeg]);

        // 4K @ 30 FPS -> Auto, MJPEG, NV12, YUV12
        let fmts_4k30 = supported_video_formats(&card_caps, "4K", 30);
        assert_eq!(
            fmts_4k30,
            vec![
                VideoFormat::Auto,
                VideoFormat::Mjpeg,
                VideoFormat::Nv12,
                VideoFormat::Yuv420p,
            ]
        );

        // 1080p @ 120 FPS -> Auto, MJPEG, NV12 (YUY2 excluded!)
        let fmts_1080p120 = supported_video_formats(&card_caps, "1080p", 120);
        assert_eq!(
            fmts_1080p120,
            vec![VideoFormat::Auto, VideoFormat::Mjpeg, VideoFormat::Nv12]
        );

        // 1080p @ 60 FPS -> Auto, MJPEG, NV12, YUY2
        let fmts_1080p60 = supported_video_formats(&card_caps, "1080p", 60);
        assert_eq!(
            fmts_1080p60,
            vec![
                VideoFormat::Auto,
                VideoFormat::Mjpeg,
                VideoFormat::Nv12,
                VideoFormat::Yuy2,
            ]
        );
    }

    #[test]
    fn test_sanitize_draft_settings() {
        let card_caps = vec![
            DeviceFormatCapability {
                format: VideoFormat::Mjpeg,
                width: 3840,
                height: 2160,
                min_fps: 25,
                max_fps: 60,
            },
            DeviceFormatCapability {
                format: VideoFormat::Nv12,
                width: 1920,
                height: 1080,
                min_fps: 5,
                max_fps: 120,
            },
        ];

        let mut draft = Settings {
            resolution: "4K".to_string(),
            fps_mode: FPS_MODE_120.to_string(),
            video_format: VideoFormat::Nv12,
            ..Settings::default()
        };

        sanitize_draft_settings(&mut draft, &card_caps);

        // 120 FPS is not supported in 4K (max is 60), so fps_mode should clamp to 60 FPS
        assert_eq!(draft.fps_mode, FPS_MODE_60);
        // NV12 is not supported in 4K on this device, so format should fall back to Auto
        assert_eq!(draft.video_format, VideoFormat::Auto);
    }
}
