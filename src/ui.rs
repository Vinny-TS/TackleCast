use crate::devices::AudioDevice;
use crate::settings::{
    ColorRange, ColorSpace, ScaleFilter, Settings, VideoFormat, FPS_MODE_120, FPS_MODE_30,
    FPS_MODE_CUSTOM, MAX_FPS, MIN_FPS,
};
use egui::{
    Align, Align2, Button, Checkbox, Color32, ComboBox, CornerRadius, FontId, Frame, Layout,
    Margin, RichText, Slider, Stroke,
};
use egui_wgpu::ScreenDescriptor;
use egui_winit::State;
use winit::event::WindowEvent;
use winit::window::Window;

const COLOR_TEXT_PRIMARY: Color32 = Color32::from_rgb(0xE0, 0xE0, 0xE0);
const COLOR_TEXT_SECONDARY: Color32 = Color32::from_rgb(0x88, 0x99, 0xAA);
const COLOR_TEXT_HINT: Color32 = Color32::from_rgb(0x44, 0x55, 0x66);
const COLOR_ACCENT: Color32 = Color32::from_rgb(0xE9, 0x45, 0x60);
const COLOR_PANEL_BG: Color32 = Color32::from_rgb(0x16, 0x21, 0x3E);
const COLOR_BORDER: Color32 = Color32::from_rgb(0x0F, 0x34, 0x60);
const COLOR_MENU_BORDER: Color32 = Color32::from_rgb(0x1A, 0x2A, 0x50);
const COLOR_DIM_OVERLAY: Color32 = Color32::from_black_alpha(120);
const COLOR_PILL_BG: Color32 = Color32::from_black_alpha(180);
const COLOR_EXIT_BG: Color32 = Color32::from_rgb(0x3A, 0x10, 0x20);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuTab {
    #[default]
    General,
    Picture,
}

pub struct UiState {
    egui_ctx: egui::Context,
    egui_winit: State,
    menu_open: bool,
    draft_settings: Settings,
    tab: MenuTab,
}

pub struct OverlayInfo {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f32>,
    pub filter: ScaleFilter,
    pub color_space: ColorSpace,
    pub color_range: ColorRange,
    pub active_format: Option<String>,
    pub show_overlay: bool,
    /// Stack resolution, filter, and FPS over three lines instead of one.
    pub detailed: bool,
    pub status_message: Option<String>,
    pub status_is_alert: bool,
}

pub struct UiFrame<'a> {
    pub overlay: &'a OverlayInfo,
    pub settings: &'a Settings,
    pub video_devices: &'a [String],
    pub audio_inputs: &'a [AudioDevice],
    pub audio_outputs: &'a [AudioDevice],
    pub is_fullscreen: bool,
}

pub struct PreparedUi {
    pub paint_jobs: Vec<egui::ClippedPrimitive>,
    pub textures_delta: egui::TexturesDelta,
    pub screen_descriptor: ScreenDescriptor,
    pub output: UiOutput,
}

#[derive(Default)]
pub struct UiOutput {
    pub apply_settings: Option<Settings>,
    pub live_picture: Option<(f32, f32, f32, f32)>,
    pub toggle_fullscreen: bool,
    pub exit_requested: bool,
    pub request_repaint: bool,
}

impl UiState {
    pub fn new(window: &Window, max_texture_side: usize) -> Self {
        let egui_ctx = egui::Context::default();
        configure_style(&egui_ctx);

        let egui_winit = State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(max_texture_side),
        );

        Self {
            egui_ctx,
            egui_winit,
            menu_open: false,
            draft_settings: Settings::default(),
            tab: MenuTab::default(),
        }
    }

    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.egui_winit.on_window_event(window, event).consumed
    }

    pub fn is_menu_open(&self) -> bool {
        self.menu_open
    }

    pub fn open_menu(&mut self, settings: &Settings) {
        self.menu_open = true;
        self.draft_settings = settings.clone();
    }

    pub fn close_menu_and_apply(&mut self) -> Option<Settings> {
        if !self.menu_open {
            return None;
        }

        self.menu_open = false;
        Some(self.draft_settings.clone())
    }

    pub fn prepare(&mut self, window: &Window, frame: UiFrame<'_>) -> PreparedUi {
        if !self.menu_open {
            self.draft_settings = frame.settings.clone();
            if self.draft_settings.video_device.is_empty() && !frame.video_devices.is_empty() {
                self.draft_settings.video_device = frame.video_devices[0].clone();
            }
        }

        let mut raw_input = self.egui_winit.take_egui_input(window);
        if self.menu_open {
            raw_input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::MouseWheel { .. } | egui::Event::Zoom(_)
                )
            });
        }
        let mut ui_output = UiOutput::default();
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            draw_overlay(ctx, frame.overlay);
            if self.menu_open {
                draw_menu(
                    ctx,
                    &mut self.draft_settings,
                    &mut self.tab,
                    frame.video_devices,
                    frame.audio_inputs,
                    frame.audio_outputs,
                    frame.is_fullscreen,
                    &mut ui_output,
                );
            }
        });

        if self.menu_open {
            ui_output.live_picture = Some((
                self.draft_settings.brightness,
                self.draft_settings.contrast,
                self.draft_settings.saturation,
                self.draft_settings.gamma,
            ));
        }

        self.egui_winit
            .handle_platform_output(window, full_output.platform_output);

        if ui_output.apply_settings.is_some() {
            self.menu_open = false;
        }

        ui_output.request_repaint = self.menu_open || self.egui_ctx.has_requested_repaint();

        let pixels_per_point = egui_winit::pixels_per_point(&self.egui_ctx, window);
        let clipped_primitives = self.egui_ctx.tessellate(full_output.shapes, pixels_per_point);
        let size = window.inner_size();

        PreparedUi {
            paint_jobs: clipped_primitives,
            textures_delta: full_output.textures_delta,
            screen_descriptor: ScreenDescriptor {
                size_in_pixels: [size.width.max(1), size.height.max(1)],
                pixels_per_point,
            },
            output: ui_output,
        }
    }
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(COLOR_TEXT_PRIMARY);
    style.visuals.panel_fill = Color32::TRANSPARENT;
    style.visuals.window_fill = menu_background();
    style.visuals.window_stroke = Stroke::new(1.0_f32, COLOR_MENU_BORDER);
    style.visuals.window_corner_radius = CornerRadius::same(12);
    style.visuals.menu_corner_radius = CornerRadius::same(12);
    style.visuals.widgets.noninteractive.bg_fill = COLOR_PANEL_BG;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, COLOR_BORDER);
    style.visuals.widgets.noninteractive.fg_stroke.color = COLOR_TEXT_PRIMARY;
    style.visuals.widgets.inactive.bg_fill = COLOR_PANEL_BG;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, COLOR_BORDER);
    style.visuals.widgets.inactive.fg_stroke.color = COLOR_TEXT_PRIMARY;
    style.visuals.widgets.hovered.bg_fill = COLOR_PANEL_BG;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, COLOR_ACCENT);
    style.visuals.widgets.hovered.fg_stroke.color = COLOR_TEXT_PRIMARY;
    style.visuals.widgets.active.bg_fill = COLOR_PANEL_BG;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, COLOR_ACCENT);
    style.visuals.widgets.active.fg_stroke.color = COLOR_TEXT_PRIMARY;
    style.visuals.selection.bg_fill = COLOR_ACCENT;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, COLOR_ACCENT);
    style.visuals.slider_trailing_fill = true;
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    ctx.set_style(style);
}

fn draw_overlay(ctx: &egui::Context, overlay: &OverlayInfo) {
    let text = overlay_text(overlay);
    let color = if overlay.status_is_alert {
        COLOR_ACCENT
    } else {
        COLOR_TEXT_PRIMARY
    };

    if text.is_none() {
        return;
    }

    egui::Area::new("fps_overlay".into())
        .anchor(Align2::LEFT_TOP, [8.0, 8.0])
        .interactable(false)
        .movable(false)
        .show(ctx, |ui| {
            Frame::new()
                .fill(COLOR_PILL_BG)
                .corner_radius(CornerRadius::same(24))
                .inner_margin(Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    // Extend rather than wrap: the pill sizes itself to the
                    // text, so wrapping would fold it onto a second line inside
                    // a pill that's already the right width. Scoped to this
                    // label so the menu keeps its normal wrapping.
                    ui.add(
                        egui::Label::new(
                            RichText::new(text.unwrap_or_default())
                                .font(FontId::proportional(14.0))
                                .strong()
                                .color(color),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_menu(
    ctx: &egui::Context,
    draft: &mut Settings,
    tab: &mut MenuTab,
    video_devices: &[String],
    audio_inputs: &[AudioDevice],
    audio_outputs: &[AudioDevice],
    is_fullscreen: bool,
    output: &mut UiOutput,
) {
    let screen_rect = ctx.screen_rect();
    let window_width = screen_rect.width().max(1.0);
    let scale = (window_width / 1280.0).clamp(0.8, 1.4);
    let menu_width = 460.0 * scale;
    let text_scale = scale.clamp(0.9, 1.2);

    egui::Area::new("settings_backdrop".into())
        .anchor(Align2::LEFT_TOP, [0.0, 0.0])
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let rect = screen_rect;
            let response = ui.allocate_rect(rect, egui::Sense::click());
            ui.painter().rect_filled(rect, 0.0, COLOR_DIM_OVERLAY);
            if response.clicked() {
                output.apply_settings = Some(draft.clone());
            }
        });

    egui::Area::new("settings_menu".into())
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .order(egui::Order::Foreground)
        .movable(false)
        .show(ctx, |ui| {
            Frame::new()
                .fill(menu_background())
                .stroke(Stroke::new(1.0_f32, COLOR_MENU_BORDER))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(18))
                .show(ui, |ui| {
                    ui.set_width(menu_width);
                    ui.spacing_mut().item_spacing = egui::vec2(10.0 * text_scale, 10.0 * text_scale);
                    ui.spacing_mut().button_padding =
                        egui::vec2(12.0 * text_scale, 8.0 * text_scale);
                    ui.vertical(|ui| {
                        ui.with_layout(Layout::top_down(Align::Center), |ui| {
                            ui.label(
                                RichText::new("Settings")
                                    .font(FontId::proportional(24.0 * text_scale))
                                    .strong()
                                    .color(COLOR_TEXT_PRIMARY),
                            );
                        });
                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            ui.selectable_value(tab, MenuTab::General, RichText::new("General").size(14.0 * text_scale));
                            ui.selectable_value(tab, MenuTab::Picture, RichText::new("Picture").size(14.0 * text_scale));
                        });
                        ui.add_space(4.0);
                        separator(ui);

                        match tab {
                            MenuTab::General => {
                                section_header(ui, "VIDEO", text_scale);
                                if draft.video_device.is_empty() && !video_devices.is_empty() {
                                    draft.video_device = video_devices[0].clone();
                                }
                                let current_video_device = draft.video_device.clone();
                                labeled_combo_string(
                                    ui,
                                    "Video Device",
                                    &mut draft.video_device,
                                    video_device_options(video_devices, &current_video_device),
                                );
                                if draft.video_device != current_video_device {
                                    ctx.request_repaint();
                                }

                                let active_device = if draft.video_device.is_empty() {
                                    video_devices.first().map(|s| s.as_str()).unwrap_or("")
                                } else {
                                    draft.video_device.as_str()
                                };
                                let caps = crate::devices::get_device_capabilities(active_device);
                                crate::devices::sanitize_draft_settings(draft, &caps);

                                let available_resolutions = crate::devices::supported_resolutions(&caps);
                                let prev_resolution = draft.resolution.clone();
                                let prev_fps_mode = draft.fps_mode.clone();

                                ui.columns(2, |columns| {
                                    labeled_combo_static(
                                        &mut columns[0],
                                        "Resolution",
                                        &mut draft.resolution,
                                        &available_resolutions,
                                    );
                                    let available_fps = crate::devices::supported_fps_modes(&caps, &draft.resolution);
                                    fps_mode_combo(&mut columns[1], &mut draft.fps_mode, &available_fps);
                                });

                                if draft.resolution != prev_resolution || draft.fps_mode != prev_fps_mode {
                                    crate::devices::sanitize_draft_settings(draft, &caps);
                                    ctx.request_repaint();
                                }

                                let available_formats = crate::devices::supported_video_formats(
                                    &caps,
                                    &draft.resolution,
                                    draft.get_fps(),
                                );
                                let max_res_fps = crate::devices::max_fps_for_resolution(&caps, &draft.resolution);

                                ui.columns(2, |columns| {
                                    video_format_combo(
                                        &mut columns[0],
                                        "Video Format",
                                        &mut draft.video_format,
                                        &available_formats,
                                    );
                                    scaling_filter_combo(
                                        &mut columns[1],
                                        "Scaling Filter",
                                        &mut draft.scaling_filter,
                                    );
                                });

                                ui.columns(2, |columns| {
                                    color_space_combo(
                                        &mut columns[0],
                                        "Color Space",
                                        &mut draft.color_space,
                                    );
                                    color_range_combo(
                                        &mut columns[1],
                                        "Color Range",
                                        &mut draft.color_range,
                                    );
                                });

                                if draft.fps_mode == FPS_MODE_CUSTOM {
                                    labeled_custom_fps(ui, draft, max_res_fps);
                                    warning_text(
                                        ui,
                                        "Custom FPS is experimental and is not guaranteed to work with all devices.",
                                        text_scale,
                                    );
                                } else if draft.fps_mode == FPS_MODE_120 {
                                    warning_text(
                                        ui,
                                        "A fast CPU is required for 120 FPS. Performance may vary by hardware.",
                                        text_scale,
                                    );
                                }

                                separator(ui);
                                section_header(ui, "AUDIO", text_scale);
                                labeled_audio_combo(ui, "Audio Input", &mut draft.audio_input, audio_inputs);
                                labeled_audio_combo(ui, "Audio Output", &mut draft.audio_output, audio_outputs);
                                labeled_volume(ui, draft);

                                separator(ui);
                                section_header(ui, "DISPLAY", text_scale);
                                ui.horizontal(|ui| {
                                    let fullscreen_label = if is_fullscreen {
                                        "Exit Fullscreen"
                                    } else {
                                        "Enter Fullscreen"
                                    };
                                    if styled_button(ui, fullscreen_label).clicked() {
                                        output.toggle_fullscreen = true;
                                    }
                                    ui.add(Checkbox::new(
                                        &mut draft.show_overlay,
                                        RichText::new("Show FPS Overlay").color(COLOR_TEXT_PRIMARY),
                                    ));
                                });

                                if draft.show_overlay {
                                    ui.add(Checkbox::new(
                                        &mut draft.detailed_overlay,
                                        RichText::new("Include Scaling Filter In Overlay")
                                            .color(COLOR_TEXT_PRIMARY),
                                    ));
                                }
                            }
                            MenuTab::Picture => {
                                section_header(ui, "PICTURE ADJUSTMENTS", text_scale);
                                picture_slider(ui, "Brightness", &mut draft.brightness, -1.0..=1.0, 0.02);
                                picture_slider(ui, "Contrast", &mut draft.contrast, 0.0..=2.0, 0.02);
                                picture_slider(ui, "Saturation", &mut draft.saturation, 0.0..=2.0, 0.02);
                                picture_slider(ui, "Gamma", &mut draft.gamma, 0.2..=3.0, 0.05);

                                if draft.scaling_filter == ScaleFilter::Fsr1 {
                                    picture_slider(ui, "Sharpness (RCAS)", &mut draft.sharpness, 0.0..=2.0, 0.05);
                                }

                                ui.add_space(8.0);
                                if styled_button(ui, "Reset Picture to Defaults").clicked() {
                                    draft.brightness = 0.0;
                                    draft.contrast = 1.0;
                                    draft.saturation = 1.0;
                                    draft.gamma = 1.0;
                                    draft.sharpness = 1.0;
                                }
                            }
                        }

                        separator(ui);
                        if exit_button(ui).clicked() {
                            output.exit_requested = true;
                        }

                        ui.add_space(4.0);
                        ui.with_layout(Layout::top_down(Align::Center), |ui| {
                            ui.label(
                                RichText::new("Press Escape to close")
                                    .size(13.0 * text_scale)
                                    .color(COLOR_TEXT_HINT),
                            );
                        });
                    });
                });
        });
}

fn section_header(ui: &mut egui::Ui, text: &str, text_scale: f32) {
    ui.label(
        RichText::new(spaced_caps(text))
            .size(15.0 * text_scale)
            .strong()
            .color(COLOR_ACCENT),
    );
}

fn separator(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let width = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0_f32, COLOR_MENU_BORDER),
    );
    ui.add_space(4.0);
}

fn warning_text(ui: &mut egui::Ui, text: &str, text_scale: f32) {
    ui.label(
        RichText::new(text)
            .size(13.0 * text_scale)
            .italics()
            .color(COLOR_ACCENT),
    );
}

fn labeled_combo_string(
    ui: &mut egui::Ui,
    label: &str,
    selected: &mut String,
    options: Vec<String>,
) {
    ui.label(RichText::new(label).color(COLOR_TEXT_SECONDARY));
    ComboBox::from_id_salt(label)
        .width(ui.available_width())
        .selected_text(display_or_default(selected))
        .show_ui(ui, |ui| {
            for option in options {
                ui.selectable_value(selected, option.clone(), option);
            }
        });
}

fn labeled_combo_static(
    ui: &mut egui::Ui,
    label: &str,
    selected: &mut String,
    options: &[&str],
) {
    ui.label(RichText::new(label).color(COLOR_TEXT_SECONDARY));
    ComboBox::from_id_salt(label)
        .width(ui.available_width())
        .selected_text(selected.clone())
        .show_ui(ui, |ui| {
            for option in options {
                ui.selectable_value(selected, (*option).to_string(), *option);
            }
        });
}

fn video_format_combo(
    ui: &mut egui::Ui,
    label: &str,
    selected: &mut VideoFormat,
    available_formats: &[VideoFormat],
) {
    ui.label(RichText::new(label).color(COLOR_TEXT_SECONDARY));
    ComboBox::from_id_salt(label)
        .width(ui.available_width())
        .selected_text(selected.to_string())
        .show_ui(ui, |ui| {
            for format in available_formats {
                ui.selectable_value(selected, *format, format.to_string());
            }
        });
}

fn scaling_filter_combo(ui: &mut egui::Ui, label: &str, selected: &mut ScaleFilter) {
    ui.label(RichText::new(label).color(COLOR_TEXT_SECONDARY));
    ComboBox::from_id_salt(label)
        .width(ui.available_width())
        .selected_text(selected.to_string())
        .show_ui(ui, |ui| {
            for filter in ScaleFilter::ALL {
                ui.selectable_value(selected, filter, filter.to_string());
            }
        });
}

fn color_space_combo(ui: &mut egui::Ui, label: &str, selected: &mut ColorSpace) {
    ui.label(RichText::new(label).color(COLOR_TEXT_SECONDARY));
    ComboBox::from_id_salt(label)
        .width(ui.available_width())
        .selected_text(selected.to_string())
        .show_ui(ui, |ui| {
            for space in ColorSpace::ALL {
                ui.selectable_value(selected, space, space.to_string());
            }
        });
}

fn color_range_combo(ui: &mut egui::Ui, label: &str, selected: &mut ColorRange) {
    ui.label(RichText::new(label).color(COLOR_TEXT_SECONDARY));
    ComboBox::from_id_salt(label)
        .width(ui.available_width())
        .selected_text(selected.to_string())
        .show_ui(ui, |ui| {
            for range in ColorRange::ALL {
                ui.selectable_value(selected, range, range.to_string());
            }
        });
}

fn fps_mode_combo(
    ui: &mut egui::Ui,
    fps_mode: &mut String,
    available_fps: &[(&'static str, &'static str)],
) {
    ui.label(RichText::new("Frame Rate").color(COLOR_TEXT_SECONDARY));
    let selected_label = available_fps
        .iter()
        .find(|(mode, _)| *mode == fps_mode.as_str())
        .map(|(_, label)| *label)
        .unwrap_or(match fps_mode.as_str() {
            FPS_MODE_30 => "30 FPS",
            FPS_MODE_120 => "120 FPS",
            FPS_MODE_CUSTOM => "Custom",
            _ => "60 FPS",
        });

    ComboBox::from_id_salt("fps_mode")
        .width(ui.available_width())
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            for &(mode, label) in available_fps {
                ui.selectable_value(fps_mode, mode.to_string(), label);
            }
        });
}

fn labeled_custom_fps(ui: &mut egui::Ui, draft: &mut Settings, max_fps: u32) {
    ui.label(RichText::new("Custom FPS").color(COLOR_TEXT_SECONDARY));
    let effective_max = max_fps.min(MAX_FPS);
    ui.horizontal(|ui| {
        if ui.small_button("-").clicked() {
            draft.custom_fps = draft.custom_fps.saturating_sub(1).max(MIN_FPS);
        }
        ui.label(
            RichText::new(format!("{} FPS", draft.custom_fps))
                .strong()
                .color(COLOR_TEXT_PRIMARY),
        );
        if ui.small_button("+").clicked() {
            draft.custom_fps = draft.custom_fps.saturating_add(1).min(effective_max);
        }
    });
}

fn labeled_audio_combo(
    ui: &mut egui::Ui,
    label: &str,
    selected: &mut i32,
    devices: &[AudioDevice],
) {
    ui.label(RichText::new(label).color(COLOR_TEXT_SECONDARY));
    ComboBox::from_id_salt(label)
        .width(ui.available_width())
        .selected_text(audio_device_name(*selected, devices))
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, -1, "Default");
            for device in devices {
                ui.selectable_value(selected, device.index, device.name.clone());
            }
        });
}

fn labeled_volume(ui: &mut egui::Ui, draft: &mut Settings) {
    ui.label(RichText::new("Volume").color(COLOR_TEXT_SECONDARY));
    ui.horizontal(|ui| {
        let mut volume_percent = (draft.volume.clamp(0.0, 1.0) * 100.0).round() as u32;
        let slider = Slider::new(&mut volume_percent, 0..=100).show_value(false);
        let changed = ui
            .scope(|ui| {
                let visuals = &mut ui.visuals_mut().widgets;
                visuals.inactive.fg_stroke = Stroke::new(2.0_f32, COLOR_ACCENT);
                visuals.hovered.fg_stroke = Stroke::new(2.0_f32, COLOR_ACCENT);
                visuals.active.fg_stroke = Stroke::new(2.0_f32, COLOR_ACCENT);
                ui.add(slider).changed()
            })
            .inner;
        if changed {
            draft.volume = (volume_percent as f64 / 100.0).clamp(0.0, 1.0);
        }
        ui.label(
            RichText::new(format!("{volume_percent}%"))
                .color(COLOR_TEXT_PRIMARY)
                .strong(),
        );
    });
}

fn picture_slider(
    ui: &mut egui::Ui,
    label: &str,
    val: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(COLOR_TEXT_SECONDARY));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(Slider::new(val, range).step_by(step));
        });
    });
}

fn styled_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        Button::new(RichText::new(text).color(COLOR_TEXT_PRIMARY))
            .fill(COLOR_PANEL_BG)
            .stroke(Stroke::new(1.0_f32, COLOR_BORDER)),
    )
}

fn exit_button(ui: &mut egui::Ui) -> egui::Response {
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.add(
            Button::new(RichText::new("Exit TackleCast").color(COLOR_TEXT_PRIMARY))
                .fill(COLOR_EXIT_BG)
                .stroke(Stroke::new(1.0_f32, COLOR_ACCENT))
                .min_size(egui::vec2(ui.available_width(), 0.0)),
        )
    })
    .inner
}

fn menu_background() -> Color32 {
    Color32::from_rgba_unmultiplied(12, 12, 28, 240)
}

fn video_device_options(video_devices: &[String], current: &str) -> Vec<String> {
    let mut options = video_devices.to_vec();
    if !current.is_empty() && !options.iter().any(|device| device == current) {
        options.push(current.to_string());
    }
    if options.is_empty() {
        options.push(String::new());
    }
    options
}

fn display_or_default(value: &str) -> String {
    if value.is_empty() {
        "No Devices Found".to_string()
    } else {
        value.to_string()
    }
}

fn audio_device_name(index: i32, devices: &[AudioDevice]) -> String {
    if index < 0 {
        return "Default".to_string();
    }

    devices
        .iter()
        .find(|device| device.index == index)
        .map(|device| device.name.clone())
        .unwrap_or_else(|| format!("Saved Device #{index}"))
}

fn overlay_text(overlay: &OverlayInfo) -> Option<String> {
    if let Some(message) = overlay.status_message.as_ref() {
        return Some(message.clone());
    }

    if !overlay.show_overlay {
        return None;
    }

    match (overlay.width, overlay.height, overlay.fps) {
        (Some(width), Some(height), Some(fps)) => Some({
            let fmt_suffix = overlay
                .active_format
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| format!(" | {s}"))
                .unwrap_or_default();
            if overlay.detailed {
                format!(
                    "{width}x{height}{fmt_suffix}\n{} | {} {}\n{fps:.1} FPS",
                    overlay.filter, overlay.color_space, overlay.color_range
                )
            } else {
                format!("{width}x{height}{fmt_suffix} | {fps:.1} FPS")
            }
        }),
        _ => Some("Waiting For Video...".to_string()),
    }
}

fn spaced_caps(text: &str) -> String {
    text.chars()
        .map(|c| c.to_ascii_uppercase().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_text_includes_active_format() {
        let overlay = OverlayInfo {
            width: Some(3840),
            height: Some(2160),
            fps: Some(60.0),
            filter: ScaleFilter::Bilinear,
            color_space: ColorSpace::Auto,
            color_range: ColorRange::Auto,
            active_format: Some("MJPEG (SW)".to_string()),
            show_overlay: true,
            detailed: false,
            status_message: None,
            status_is_alert: false,
        };

        let text = overlay_text(&overlay);
        assert_eq!(text, Some("3840x2160 | MJPEG (SW) | 60.0 FPS".to_string()));

        let mut detailed_overlay = overlay;
        detailed_overlay.detailed = true;
        let detailed_text = overlay_text(&detailed_overlay);
        assert_eq!(
            detailed_text,
            Some("3840x2160 | MJPEG (SW)\nBilinear | Auto Auto\n60.0 FPS".to_string())
        );
    }

    #[test]
    fn test_menu_resolution_switch() {
        let ctx = egui::Context::default();
        let mut draft = Settings::default();
        draft.video_device = "VEDO-VDV66003".to_string();
        draft.resolution = "1080p".to_string();
        draft.fps_mode = "60".to_string();
        draft.video_format = VideoFormat::Auto;
        let video_devices = vec!["VEDO-VDV66003".to_string()];
        let mut output = UiOutput::default();

        let caps = crate::devices::get_device_capabilities(&draft.video_device);
        if caps.is_empty() {
            // If running in an environment without the physical device, skip device-specific test
            return;
        }

        let mut tab = MenuTab::General;

        // Frame 1: 1080p60 -> all formats supported by hardware should be available
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw_menu(&ctx, &mut draft, &mut tab, &video_devices, &[], &[], false, &mut output);
        });
        let fmts_frame1 = crate::devices::supported_video_formats(&caps, &draft.resolution, draft.get_fps());
        assert!(fmts_frame1.contains(&VideoFormat::Auto));
        assert!(fmts_frame1.contains(&VideoFormat::Mjpeg));
        assert!(fmts_frame1.contains(&VideoFormat::Nv12));
        assert!(fmts_frame1.contains(&VideoFormat::Yuy2));
        assert!(fmts_frame1.contains(&VideoFormat::Yuv420p));

        // Frame 2: Switch to 4K60 -> only Auto and MJPEG available
        draft.resolution = "4K".to_string();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw_menu(&ctx, &mut draft, &mut tab, &video_devices, &[], &[], false, &mut output);
        });
        let fmts_frame2 = crate::devices::supported_video_formats(&caps, &draft.resolution, draft.get_fps());
        assert_eq!(fmts_frame2, vec![VideoFormat::Auto, VideoFormat::Mjpeg]);

        // Frame 3: Switch back to 1080p60 -> all formats MUST reappear!
        draft.resolution = "1080p".to_string();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw_menu(&ctx, &mut draft, &mut tab, &video_devices, &[], &[], false, &mut output);
        });
        let fmts_frame3 = crate::devices::supported_video_formats(&caps, &draft.resolution, draft.get_fps());
        assert_eq!(fmts_frame3, fmts_frame1);
        assert!(fmts_frame3.contains(&VideoFormat::Auto));
        assert!(fmts_frame3.contains(&VideoFormat::Mjpeg));
        assert!(fmts_frame3.contains(&VideoFormat::Nv12));
        assert!(fmts_frame3.contains(&VideoFormat::Yuy2));
        assert!(fmts_frame3.contains(&VideoFormat::Yuv420p));

        // Frame 4: Test format fallback when switching from 1080p NV12 to 4K and back
        draft.video_format = VideoFormat::Nv12;
        draft.resolution = "4K".to_string();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw_menu(&ctx, &mut draft, &mut tab, &video_devices, &[], &[], false, &mut output);
        });
        // Since NV12 is only supported up to 30fps at 4K on VEDO-VDV66003, at 60fps it must fall back to Auto
        assert_eq!(draft.video_format, VideoFormat::Auto);

        // Frame 5: Switch back to 1080p and select NV12 again
        draft.resolution = "1080p".to_string();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw_menu(&ctx, &mut draft, &mut tab, &video_devices, &[], &[], false, &mut output);
        });
        let fmts_frame5 = crate::devices::supported_video_formats(&caps, &draft.resolution, draft.get_fps());
        assert!(fmts_frame5.contains(&VideoFormat::Nv12));
    }

    #[test]
    fn test_menu_picture_tab_renders() {
        let ctx = egui::Context::default();
        let mut draft = Settings::default();
        let mut tab = MenuTab::Picture;
        let mut output = UiOutput::default();

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw_menu(&ctx, &mut draft, &mut tab, &[], &[], &[], false, &mut output);
        });

        assert_eq!(tab, MenuTab::Picture);
    }
}
