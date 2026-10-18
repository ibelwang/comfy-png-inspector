//! Application state, theme and the frameless-window plumbing.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::parser::{self, ImageMeta};
use crate::settings::Settings;

// ComfyUI-inspired dark palette.
pub const BG: egui::Color32 = egui::Color32::from_rgb(0x17, 0x14, 0x1b);
pub const CARD: egui::Color32 = egui::Color32::from_rgb(0x23, 0x1f, 0x2b);
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(0x3a, 0x32, 0x46);
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(0xf1, 0xee, 0xf6);
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(0xa7, 0x9f, 0xb5);
pub const MUTED2: egui::Color32 = egui::Color32::from_rgb(0x94, 0x8c, 0xa3);
pub const PRIMARY: egui::Color32 = egui::Color32::from_rgb(0xf2, 0xff, 0x59);
pub const PROMPT: egui::Color32 = egui::Color32::from_rgb(0xde, 0xd9, 0xe6);
pub const DANGER: egui::Color32 = egui::Color32::from_rgb(0xff, 0x6b, 0x6b);

const TOAST_DURATION: Duration = Duration::from_millis(2600);
const CLICK_DELAY: Duration = Duration::from_millis(320);

pub const DEFAULT_PROMPT_FONT: f32 = 14.0;
pub const MIN_PROMPT_FONT: f32 = 10.0;
pub const MAX_PROMPT_FONT: f32 = 24.0;

pub struct App {
    pub meta: Option<ImageMeta>,
    pub texture: Option<egui::TextureHandle>,
    pub texture_size: egui::Vec2,
    pub settings: Settings,
    pub toast: Option<(String, Instant)>,
    pub error: Option<String>,
    pub pending_click: Option<Instant>,
    pub prompt_font_size: f32,
    corners_applied: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial: Option<PathBuf>,
        settings: Settings,
    ) -> Self {
        install_fonts(&cc.egui_ctx);
        install_style(&cc.egui_ctx);

        let prompt_font_size = settings.prompt_font_size.unwrap_or(DEFAULT_PROMPT_FONT).clamp(MIN_PROMPT_FONT, MAX_PROMPT_FONT);
        let mut app = Self {
            meta: None,
            texture: None,
            texture_size: egui::Vec2::ZERO,
            settings,
            toast: None,
            error: None,
            pending_click: None,
            prompt_font_size,
            corners_applied: false,
        };
        if let Some(path) = initial {
            app.load(&path, &cc.egui_ctx);
        }
        app
    }

    /// Change the prompt font size by one step and persist it.
    pub fn bump_prompt_size(&mut self, delta: f32) {
        self.prompt_font_size =
            (self.prompt_font_size + delta).clamp(MIN_PROMPT_FONT, MAX_PROMPT_FONT);
        self.settings.prompt_font_size = Some(self.prompt_font_size);
        self.settings.save();
    }

    pub fn load(&mut self, path: &Path, ctx: &egui::Context) {
        match parser::load_meta(path) {
            Ok(meta) => {
                match load_texture(ctx, path) {
                    Some((texture, size)) => {
                        self.texture = Some(texture);
                        self.texture_size = size;
                    }
                    None => {
                        self.texture = None;
                        self.texture_size = egui::Vec2::ZERO;
                        self.error = Some("无法解码图片像素，只能显示元数据。".into());
                    }
                }
                if self.texture.is_some() {
                    self.error = None;
                }
                if let Some(dir) = path.parent() {
                    self.settings.last_dir = Some(dir.to_string_lossy().into_owned());
                }
                self.meta = Some(meta);
                self.pending_click = None;
            }
            Err(err) => {
                self.meta = None;
                self.texture = None;
                self.texture_size = egui::Vec2::ZERO;
                self.error = Some(format!("打开失败：{err}"));
            }
        }
    }

    pub fn clear(&mut self) {
        self.meta = None;
        self.texture = None;
        self.texture_size = egui::Vec2::ZERO;
        self.error = None;
        self.pending_click = None;
    }

    pub fn open_dialog(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("图片", &["png", "jpg", "jpeg", "webp"])
            .add_filter("所有文件", &["*"]);
        if let Some(dir) = &self.settings.last_dir {
            if Path::new(dir).is_dir() {
                dialog = dialog.set_directory(dir);
            }
        }
        if let Some(path) = dialog.pick_file() {
            self.load(&path, ctx);
        }
    }

    pub fn copy(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text.to_string())) {
            Ok(()) => {
                let preview: String = text.chars().take(28).collect();
                let suffix = if text.chars().count() > 28 { "…" } else { "" };
                self.set_toast(format!("已复制：{preview}{suffix}"));
            }
            Err(err) => self.set_toast(format!("复制失败：{err}")),
        }
    }

    pub fn set_toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    pub fn open_externally(&self) {
        if let Some(meta) = &self.meta {
            let _ = std::process::Command::new("explorer.exe")
                .arg(&meta.path)
                .spawn();
        }
    }

    /// Drag & drop, shortcuts, delayed single-click and window-edge resizing.
    pub fn handle_input(&mut self, ctx: &egui::Context) {
        self.handle_edges(ctx);

        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.iter().find_map(|f| f.path.clone()) {
            self.load(&path, ctx);
        }

        let (open, paste, clear) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::O) && i.modifiers.command,
                i.key_pressed(egui::Key::V) && i.modifiers.command,
                i.key_pressed(egui::Key::Escape),
            )
        });
        if open {
            self.open_dialog(ctx);
        }
        if paste {
            if let Some(path) = crate::platform::clipboard_files().into_iter().next() {
                self.load(&path, ctx);
            }
        }
        if clear {
            self.clear();
        }

        if let Some(at) = self.pending_click {
            if at.elapsed() >= CLICK_DELAY {
                self.pending_click = None;
                self.open_dialog(ctx);
            } else {
                ctx.request_repaint_after(CLICK_DELAY - at.elapsed());
            }
        }

        if let Some((_, at)) = &self.toast {
            if at.elapsed() >= TOAST_DURATION {
                self.toast = None;
            } else {
                ctx.request_repaint_after(TOAST_DURATION - at.elapsed());
            }
        }
    }

    fn handle_edges(&self, ctx: &egui::Context) {
        let rect = ctx.content_rect();
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        let edge = 6.0;
        let left = pos.x <= rect.left() + edge;
        let right = pos.x >= rect.right() - edge;
        let top = pos.y <= rect.top() + edge;
        let bottom = pos.y >= rect.bottom() - edge;

        use egui::viewport::ResizeDirection as Dir;
        let direction = match (left, right, top, bottom) {
            (true, _, true, _) => Some((Dir::NorthWest, egui::CursorIcon::ResizeNwSe)),
            (true, _, _, true) => Some((Dir::SouthWest, egui::CursorIcon::ResizeNeSw)),
            (_, true, true, _) => Some((Dir::NorthEast, egui::CursorIcon::ResizeNeSw)),
            (_, true, _, true) => Some((Dir::SouthEast, egui::CursorIcon::ResizeNwSe)),
            (true, _, _, _) => Some((Dir::West, egui::CursorIcon::ResizeHorizontal)),
            (_, true, _, _) => Some((Dir::East, egui::CursorIcon::ResizeHorizontal)),
            (_, _, true, _) => Some((Dir::North, egui::CursorIcon::ResizeVertical)),
            (_, _, _, true) => Some((Dir::South, egui::CursorIcon::ResizeVertical)),
            _ => None,
        };
        if let Some((direction, cursor)) = direction {
            ctx.set_cursor_icon(cursor);
            if ctx.input(|i| i.pointer.any_pressed()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        }
    }

    /// Remember the window size / position.
    ///
    /// Saved as soon as a resize/move settles, so a hard kill (or a crash) does
    /// not lose the geometry; `on_exit` writes it again as a backup.
    fn remember_window(&mut self, ctx: &egui::Context) {
        let mut changed = false;

        let inner = ctx.input(|i| i.viewport().inner_rect);
        if let Some(rect) = inner {
            if rect.width() > 300.0 && rect.height() > 200.0 {
                let size = [rect.width(), rect.height()];
                if self.settings.window_size != Some(size) {
                    self.settings.window_size = Some(size);
                    changed = true;
                }
            }
        }
        let outer = ctx.input(|i| i.viewport().outer_rect);
        if let Some(rect) = outer {
            let pos = [rect.min.x, rect.min.y];
            if self.settings.window_pos != Some(pos) {
                self.settings.window_pos = Some(pos);
                changed = true;
            }
        }

        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if changed && !pointer_down {
            self.settings.save();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.corners_applied {
            self.corners_applied = apply_round_corners();
        }
        self.handle_input(ctx);
        crate::ui::draw(self, ctx);
        self.remember_window(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.save();
        #[cfg(debug_assertions)]
        eprintln!("settings saved on exit");
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // msyh.ttc is a font collection; simhei.ttf is a plain TrueType file, which
    // egui parses reliably, so prefer it when present.
    let candidates = [
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyhbd.ttc",
        r"C:\Windows\Fonts\simsun.ttc",
    ];
    for path in candidates {
        let Ok(data) = std::fs::read(path) else {
            continue;
        };
        fonts
            .font_data
            .insert("cjk".to_owned(), egui::FontData::from_owned(data).into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "cjk".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("cjk".to_owned());
        break;
    }
    ctx.set_fonts(fonts);
}

fn install_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = CARD;
    visuals.extreme_bg_color = egui::Color32::from_rgb(0x1a, 0x17, 0x20);
    visuals.faint_bg_color = egui::Color32::from_rgb(0x1f, 0x1b, 0x26);
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = PRIMARY;
    visuals.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(242, 255, 89, 70);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, PRIMARY);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, BORDER);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, BORDER);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, PRIMARY);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(13.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::proportional(12.5),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::proportional(11.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::proportional(19.0),
    );
    ctx.set_style(style);
}

fn load_texture(ctx: &egui::Context, path: &Path) -> Option<(egui::TextureHandle, egui::Vec2)> {
    let img = image::open(path).ok()?;
    let thumb = img.thumbnail(1600, 1600);
    let rgba = thumb.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    if size[0] == 0 || size[1] == 0 {
        return None;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let handle = ctx.load_texture("thumbnail", color, smooth_texture());
    Some((handle, egui::vec2(size[0] as f32, size[1] as f32)))
}

/// Linear filtering plus mipmaps keeps downscaled thumbnails smooth.
fn smooth_texture() -> egui::TextureOptions {
    egui::TextureOptions {
        magnification: egui::TextureFilter::Linear,
        minification: egui::TextureFilter::Linear,
        wrap_mode: egui::TextureWrapMode::ClampToEdge,
        mipmap_mode: Some(egui::TextureFilter::Linear),
    }
}

/// Ask DWM (Windows 11) for rounded window corners on the frameless window.
/// Returns true when the call succeeded; otherwise it is retried next frame.
#[cfg(windows)]
fn apply_round_corners() -> bool {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    let title: Vec<u16> = "ComfyPNG 查看器"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        let preference: i32 = DWMWCP_ROUND;
        let result = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &preference as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
        result >= 0
    }
}

#[cfg(not(windows))]
fn apply_round_corners() -> bool {
    true
}
