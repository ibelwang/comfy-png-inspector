//! Frameless dark UI: custom title bar, three columns, ComfyUI-inspired palette.

use std::time::Instant;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout, Margin, Rect, Response,
    RichText, Sense, Stroke, StrokeKind, Vec2,
};

use crate::app::{App, BG, BORDER, CARD, DANGER, MUTED, MUTED2, PRIMARY, PROMPT, TEXT};
use crate::parser::{fmt_num, ImageMeta};

const UV: Rect = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
const CARD_MARGIN: i8 = 13;

pub fn draw(app: &mut App, ctx: &egui::Context) {
    titlebar(app, ctx);
    statusbar(app, ctx);
    if app.meta.is_none() {
        empty_state(app, ctx);
    } else {
        left_panel(app, ctx);
        right_panel(app, ctx);
        center_panel(app, ctx);
    }
}

// --------------------------------------------------------------------------
// Window chrome
// --------------------------------------------------------------------------

enum WindowButton {
    Minimize,
    Close,
}

fn titlebar(_app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::top("titlebar")
        .exact_height(48.0)
        .frame(egui::Frame::default().fill(BG))
        .show(ctx, |ui| {
            let bar = ui.max_rect();
            ui.painter().hline(
                bar.x_range(),
                bar.bottom() - 0.5,
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(58, 50, 70, 150)),
            );

            let button = 28.0;
            let gap = 6.0;
            let pad = 12.0;
            let cy = bar.center().y;
            let close_rect = Rect::from_center_size(
                egui::pos2(bar.right() - pad - button / 2.0, cy),
                Vec2::splat(button),
            );
            let min_rect = close_rect.translate(egui::vec2(-(button + gap), 0.0));

            // Dragging anywhere on the bar except the window buttons.
            let drag_rect = Rect::from_min_max(bar.min, egui::pos2(min_rect.left() - 8.0, bar.max.y));
            let drag = ui.interact(
                drag_rect,
                ui.id().with("title-drag"),
                Sense::click_and_drag(),
            );
            if drag.hovered() {
                ctx.set_cursor_icon(CursorIcon::Grab);
            }
            if drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            let icon_rect = Rect::from_center_size(egui::pos2(bar.left() + 28.0, cy), Vec2::splat(24.0));
            draw_app_mark(ui.painter(), icon_rect);

            let title = ui.painter().layout_no_wrap(
                "ComfyPNG 查看器".to_owned(),
                FontId::proportional(14.0),
                TEXT,
            );
            let title_pos = egui::pos2(icon_rect.right() + 10.0, cy - title.size().y / 2.0);
            ui.painter().galley(title_pos, title.clone(), TEXT);

            if window_button(ui, min_rect, &WindowButton::Minimize).clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            if window_button(ui, close_rect, &WindowButton::Close).clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
}

fn window_button(ui: &mut egui::Ui, rect: Rect, kind: &WindowButton) -> Response {
    let id = ui.id().with((
        "window-button",
        match kind {
            WindowButton::Minimize => 0,
            WindowButton::Close => 1,
        },
    ));
    let response = ui.interact(rect, id, Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let accent = match kind {
        WindowButton::Minimize => PRIMARY,
        WindowButton::Close => DANGER,
    };
    let stroke_color = if hovered {
        accent
    } else {
        Color32::from_rgba_unmultiplied(167, 159, 181, 115)
    };
    let fill = if hovered {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 26)
    } else {
        Color32::TRANSPARENT
    };

    let painter = ui.painter();
    painter.circle_filled(rect.center(), rect.width() / 2.0, fill);
    painter.circle_stroke(
        rect.center(),
        rect.width() / 2.0,
        Stroke::new(1.4_f32, stroke_color),
    );

    let r = rect.width() * 0.18;
    let c = rect.center();
    match kind {
        WindowButton::Minimize => {
            painter.line_segment(
                [egui::pos2(c.x - r, c.y), egui::pos2(c.x + r, c.y)],
                Stroke::new(1.4_f32, stroke_color),
            );
        }
        WindowButton::Close => {
            painter.line_segment(
                [
                    egui::pos2(c.x - r, c.y - r),
                    egui::pos2(c.x + r, c.y + r),
                ],
                Stroke::new(1.4_f32, stroke_color),
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + r, c.y - r),
                    egui::pos2(c.x - r, c.y + r),
                ],
                Stroke::new(1.4_f32, stroke_color),
            );
        }
    }
    response
}

fn statusbar(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("statusbar")
        .exact_height(32.0)
        .frame(egui::Frame::default().fill(BG))
        .show(ctx, |ui| {
            let bar = ui.max_rect();
            ui.painter().hline(
                bar.x_range(),
                bar.top() + 0.5,
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(58, 50, 70, 150)),
            );
            let (text, color) = match (&app.toast, &app.meta) {
                (Some((toast, _)), _) => (toast.clone(), PRIMARY),
                (None, Some(meta)) => (meta.filename(), MUTED2),
                (None, None) => ("就绪".to_string(), MUTED2),
            };
            ui.painter().galley(
                egui::pos2(bar.left() + 16.0, bar.center().y - 7.0),
                ui.painter()
                    .layout_no_wrap(text, FontId::proportional(11.5), color),
                color,
            );
            if let Some(meta) = &app.meta {
                let size = format_bytes(meta.size_bytes());
                let galley = ui.painter().layout_no_wrap(
                    size,
                    FontId::proportional(11.5),
                    MUTED2,
                );
                ui.painter().galley(
                    egui::pos2(
                        bar.right() - 16.0 - galley.size().x,
                        bar.center().y - galley.size().y / 2.0,
                    ),
                    galley,
                    MUTED2,
                );
            }
        });
}

// --------------------------------------------------------------------------
// Panels
// --------------------------------------------------------------------------

fn left_panel(app: &mut App, ctx: &egui::Context) {
    egui::SidePanel::left("left_panel")
        .exact_width(300.0)
        .resizable(false)
        .frame(egui::Frame::default().fill(BG).inner_margin(Margin::same(14)))
        .show(ctx, |ui| {
            let meta = app.meta.clone();
            let card_height = ui.available_height();
            card(ui, Some(card_height), |ui| {
                section_title(ui, "预览");
                ui.add_space(2.0);
                thumbnail(app, ui);
                ui.add_space(10.0);
                if let Some(meta) = &meta {
                    info_row(ui, "文件名", &meta.filename());
                    info_row(ui, "尺寸", &meta.dimensions_text());
                    info_row(ui, "大小", &format_bytes(meta.size_bytes()));
                    info_row(ui, "格式", &meta.format);
                    info_row(ui, "来源", meta.source.label());
                }
                let remaining = ui.available_height() - 22.0;
                if remaining > 0.0 {
                    ui.add_space(remaining);
                }
                ui.label(
                    RichText::new("单击换图 · 双击用默认程序打开")
                        .size(11.5)
                        .color(MUTED2),
                );
            });
        });
}

fn center_panel(app: &mut App, ctx: &egui::Context) {
    let Some(meta) = app.meta.clone() else {
        return;
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(BG).inner_margin(Margin::same(14)))
        .show(ctx, |ui| {
            let total = ui.available_height();
            let positive_height = (total * 0.66).clamp(200.0, 900.0);
            let negative_height = (total - positive_height - 12.0).max(120.0);

            prompt_card(
                ui,
                app,
                "正向提示词",
                &meta.positive,
                positive_height,
            );
            ui.add_space(12.0);
            prompt_card(
                ui,
                app,
                "负向提示词",
                &meta.negative,
                negative_height,
            );
        });
}

fn right_panel(app: &mut App, ctx: &egui::Context) {
    let Some(meta) = app.meta.clone() else {
        return;
    };
    egui::SidePanel::right("right_panel")
        .exact_width(360.0)
        .resizable(false)
        .frame(egui::Frame::default().fill(BG).inner_margin(Margin::same(14)))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    model_card(ui, app, &meta);
                    ui.add_space(12.0);
                    lora_card(ui, app, &meta);
                    ui.add_space(12.0);
                    sampler_cards(ui, app, &meta);
                    if !meta.vae.is_empty() || meta.clip_skip.is_some() {
                        ui.add_space(12.0);
                        other_card(ui, app, &meta);
                    }
                    if !meta.notes.is_empty() {
                        ui.add_space(12.0);
                        notes_card(ui, &meta);
                    }
                });
        });
}

// --------------------------------------------------------------------------
// Cards
// --------------------------------------------------------------------------

fn model_card(ui: &mut egui::Ui, app: &mut App, meta: &ImageMeta) {
    card(ui, None, |ui| {
        section_title(ui, "模型");
        ui.add_space(6.0);
        if meta.models.is_empty() {
            muted(ui, "未找到");
        }
        for model in &meta.models {
            let width = ui.available_width();
            copy_value(ui, app, model, width);
        }
    });
}

fn lora_card(ui: &mut egui::Ui, app: &mut App, meta: &ImageMeta) {
    card(ui, None, |ui| {
        section_title(ui, "LoRA");
        ui.add_space(6.0);
        if meta.loras.is_empty() {
            muted(ui, "未找到");
        }
        for lora in &meta.loras {
            ui.horizontal(|ui| {
                let row_width = ui.available_width();
                copy_value(ui, app, &lora.name, (row_width - 64.0).max(80.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let strength = lora.strength_text();
                    let response = ui.add(
                        egui::Label::new(RichText::new(&strength).size(12.5).color(MUTED))
                            .sense(Sense::click()),
                    );
                    if response.on_hover_text("点击复制").clicked() {
                        app.copy(&strength);
                    }
                });
            });
        }
    });
}

fn sampler_cards(ui: &mut egui::Ui, app: &mut App, meta: &ImageMeta) {
    let count = meta.samplers.len();
    if count == 0 {
        card(ui, None, |ui| {
            section_title(ui, "采样参数");
            ui.add_space(6.0);
            muted(ui, "未找到");
        });
        return;
    }
    for (index, sampler) in meta.samplers.iter().enumerate() {
        let title = if count > 1 {
            format!("采样参数 {}/{}", index + 1, count)
        } else {
            "采样参数".to_string()
        };
        card(ui, None, |ui| {
            section_title(ui, &title);
            if !sampler.class_type.is_empty() {
                ui.label(RichText::new(&sampler.class_type).size(11.5).color(MUTED2));
            }
            if let Some(seed) = sampler.seed_text() {
                kv_row(ui, app, "Seed", seed);
            }
            if let Some(steps) = sampler.steps {
                kv_row(ui, app, "步数", steps.to_string());
            }
            if let Some(cfg) = fmt_num(sampler.cfg) {
                kv_row(ui, app, "CFG", cfg);
            }
            if let Some(name) = &sampler.sampler_name {
                kv_row(ui, app, "采样器", name.clone());
            }
            if let Some(scheduler) = &sampler.scheduler {
                kv_row(ui, app, "调度器", scheduler.clone());
            }
            if let Some(denoise) = fmt_num(sampler.denoise) {
                kv_row(ui, app, "降噪", denoise);
            }
            if let Some(start) = sampler.start_at_step {
                kv_row(ui, app, "起始步", start.to_string());
            }
            if let Some(end) = sampler.end_at_step {
                kv_row(ui, app, "结束步", end.to_string());
            }
        });
        if index + 1 < count {
            ui.add_space(12.0);
        }
    }
}

fn other_card(ui: &mut egui::Ui, app: &mut App, meta: &ImageMeta) {
    card(ui, None, |ui| {
        section_title(ui, "其他");
        ui.add_space(6.0);
        if !meta.vae.is_empty() {
            kv_row(ui, app, "VAE", meta.vae.clone());
        }
        if let Some(clip_skip) = meta.clip_skip {
            kv_row(ui, app, "Clip skip", clip_skip.to_string());
        }
    });
}

fn notes_card(ui: &mut egui::Ui, meta: &ImageMeta) {
    const FONT: f32 = 12.5;
    let width = ui.available_width();
    card(ui, None, |ui| {
        section_title(ui, "提示");
        ui.add_space(6.0);
        for note in &meta.notes {
            let text = wrap_text(&format!("· {note}"), width - 34.0, FONT);
            ui.add(
                egui::Label::new(RichText::new(text).size(FONT).color(MUTED))
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        }
    });
}

fn prompt_card(ui: &mut egui::Ui, app: &mut App, title: &str, text: &str, height: f32) {
    card(ui, Some(height), |ui| {
        let inner = ui.max_rect();
        ui.horizontal(|ui| {
            section_title(ui, title);
            ui.label(
                RichText::new(format!("{} 字", text.chars().count()))
                    .size(11.5)
                    .color(MUTED2),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (copy_rect, _) =
                    ui.allocate_exact_size(egui::vec2(48.0, 24.0), Sense::hover());
                if ghost_button(ui, copy_rect, "复制", 11.5).clicked() {
                    app.copy(text);
                }
                ui.add_space(6.0);

                let (plus_rect, _) =
                    ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::hover());
                if ghost_button(ui, plus_rect, "+", 13.0).clicked() {
                    app.bump_prompt_size(1.0);
                }

                let (size_rect, _) =
                    ui.allocate_exact_size(egui::vec2(28.0, 22.0), Sense::hover());
                let size_text = format!("{}", app.prompt_font_size.round() as i32);
                let galley = ui.painter().layout_no_wrap(
                    size_text,
                    FontId::proportional(11.5),
                    MUTED,
                );
                ui.painter()
                    .galley(size_rect.center() - galley.size() / 2.0, galley, MUTED);

                let (minus_rect, _) =
                    ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::hover());
                if ghost_button(ui, minus_rect, "−", 13.0).clicked() {
                    app.bump_prompt_size(-1.0);
                }
            });
        });
        ui.add_space(6.0);
        ui.painter().hline(
            inner.x_range(),
            ui.cursor().top() + 1.0,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(58, 50, 70, 120)),
        );
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height((height - 70.0).max(60.0))
            .show(ui, |ui| {
                if text.is_empty() {
                    ui.label(
                        RichText::new("（空）")
                            .size(app.prompt_font_size)
                            .color(MUTED2),
                    );
                } else {
                    ui.add(
                        egui::Label::new(
                            RichText::new(text).size(app.prompt_font_size).color(PROMPT),
                        )
                        .selectable(true),
                    );
                }
            });
    });
}

// --------------------------------------------------------------------------
// Empty state
// --------------------------------------------------------------------------

fn empty_state(app: &mut App, ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(BG))
        .show(ctx, |ui| {
            let rect = ui.max_rect().shrink(20.0);
            let area = ui.interact(rect, ui.id().with("drop-zone"), Sense::click());
            dashed_round_rect(
                ui.painter(),
                rect,
                22.0,
                Stroke::new(2.0_f32, Color32::from_rgba_unmultiplied(242, 255, 89, 110)),
                9.0,
                6.0,
            );

            let cx = rect.center().x;
            let start = rect.center().y - 110.0;

            let icon_rect = Rect::from_center_size(egui::pos2(cx, start + 40.0), Vec2::splat(80.0));
            ui.painter().rect_filled(
                icon_rect,
                CornerRadius::same(23),
                Color32::from_rgba_unmultiplied(242, 255, 89, 20),
            );
            ui.painter().rect_stroke(
                icon_rect,
                CornerRadius::same(23),
                Stroke::new(1.6_f32, Color32::from_rgba_unmultiplied(242, 255, 89, 150)),
                StrokeKind::Inside,
            );
            draw_image_glyph(ui.painter(), icon_rect.shrink(24.0), PRIMARY, 1.8);

            let title_y = start + 80.0 + 32.0;
            let title = ui.painter().layout_no_wrap(
                "把图片拖到这里".to_owned(),
                FontId::proportional(27.0),
                TEXT,
            );
            ui.painter().galley(
                egui::pos2(cx - title.size().x / 2.0, title_y),
                title.clone(),
                TEXT,
            );

            let sub = ui.painter().layout_no_wrap(
                "支持 ComfyUI / WebUI 生成的 PNG、JPEG —— 也可以直接点击这块区域".to_owned(),
                FontId::proportional(13.5),
                MUTED,
            );
            ui.painter().galley(
                egui::pos2(cx - sub.size().x / 2.0, title_y + title.size().y + 10.0),
                sub.clone(),
                MUTED,
            );

            let keys = ui.painter().layout_no_wrap(
                "Ctrl+O 打开   ·   Ctrl+V 粘贴文件   ·   Esc 清空".to_owned(),
                FontId::proportional(11.5),
                MUTED2,
            );
            ui.painter().galley(
                egui::pos2(
                    cx - keys.size().x / 2.0,
                    title_y + title.size().y + sub.size().y + 34.0,
                ),
                keys,
                MUTED2,
            );

            if area.clicked() {
                app.open_dialog(ctx);
            }

            if let Some(error) = &app.error {
                let text = ui.painter().layout_no_wrap(
                    error.clone(),
                    FontId::proportional(12.0),
                    DANGER,
                );
                ui.painter().galley(
                    egui::pos2(
                        cx - text.size().x / 2.0,
                        title_y + title.size().y + sub.size().y + 58.0,
                    ),
                    text,
                    DANGER,
                );
            }
        });
}

// --------------------------------------------------------------------------
// Widgets
// --------------------------------------------------------------------------

fn thumbnail(app: &mut App, ui: &mut egui::Ui) {
    let width = ui.available_width();
    let reserve = 128.0;
    let max_height = (ui.available_height() - reserve).max(140.0);

    let Some(texture) = &app.texture else {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 140.0), Sense::hover());
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(8),
            Color32::from_rgb(0x1a, 0x17, 0x20),
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            if app.error.is_some() { "没有预览" } else { "载入中" },
            FontId::proportional(12.0),
            MUTED2,
        );
        return;
    };
    let texture_id = texture.id();
    let aspect = app.texture_size.x / app.texture_size.y.max(1.0);

    let mut w = width;
    let mut h = w / aspect.max(0.01);
    if h > max_height {
        h = max_height;
        w = h * aspect;
    }

    let (slot, _) = ui.allocate_exact_size(Vec2::new(width, h.max(1.0)), Sense::hover());
    let rect = Rect::from_center_size(slot.center(), Vec2::new(w, h));
    ui.painter().image(texture_id, rect, UV, Color32::WHITE);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, BORDER),
        StrokeKind::Inside,
    );

    let response = ui
        .interact(rect, ui.id().with("thumbnail"), Sense::click())
        .on_hover_text("单击换图 · 双击用默认程序打开");
    if response.double_clicked() {
        app.pending_click = None;
        app.open_externally();
    } else if response.clicked() {
        app.pending_click = Some(Instant::now());
    }
}

fn card<R>(ui: &mut egui::Ui, height: Option<f32>, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let margin = f32::from(CARD_MARGIN);
    let width = ui.available_width();
    let inner_width = (width - 2.0 * margin).max(0.0);
    egui::Frame::default()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(CARD_MARGIN))
        .show(ui, |ui| {
            ui.set_min_width(inner_width);
            ui.set_max_width(inner_width);
            if let Some(height) = height {
                ui.set_min_height((height - 2.0 * margin).max(0.0));
            }
            add(ui)
        })
        .inner
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(13.5).strong().color(TEXT));
}

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.5).color(MUTED2));
}

fn info_row(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).size(12.5).color(MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(
                egui::Label::new(RichText::new(value).size(12.5).color(TEXT)).truncate(),
            );
        });
    });
}

fn kv_row(ui: &mut egui::Ui, app: &mut App, key: &str, value: String) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).size(12.5).color(MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let width = ui.available_width();
            copy_value(ui, app, &value, width);
        });
    });
}

fn copy_value(ui: &mut egui::Ui, app: &mut App, text: &str, max_width: f32) {
    let wrapped = wrap_text(text, max_width, 12.5);
    let response = ui.add(
        egui::Label::new(RichText::new(wrapped).size(12.5).color(PRIMARY))
            .wrap_mode(egui::TextWrapMode::Extend)
            .sense(Sense::click()),
    );
    if response.on_hover_text("点击复制").clicked() {
        app.copy(text);
    }
}

fn ghost_button(ui: &mut egui::Ui, rect: Rect, label: &str, font_size: f32) -> Response {
    let id = ui.id().with(("ghost", label, rect.min.x.to_bits(), rect.min.y.to_bits()));
    let response = ui.interact(rect, id, Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let fill = if hovered {
        Color32::from_rgba_unmultiplied(242, 255, 89, 26)
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(7), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(7),
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(242, 255, 89, 70)),
        StrokeKind::Inside,
    );
    let galley = painter.layout_no_wrap(label.to_owned(), FontId::proportional(font_size), PRIMARY);
    painter.galley(
        rect.center() - galley.size() / 2.0,
        galley,
        PRIMARY,
    );
    response
}

/// The app mark (rounded lime square + picture glyph), drawn as vector shapes
/// so it stays crisp when it is only 24 px in the title bar.
fn draw_app_mark(painter: &egui::Painter, rect: Rect) {
    let radius = CornerRadius::same((rect.width() * 0.226).round().clamp(2.0, 255.0) as u8);
    painter.rect_filled(rect, radius, PRIMARY);
    let inner = rect.shrink(rect.width() * 0.235);
    draw_image_glyph(
        painter,
        inner,
        Color32::from_rgb(0x17, 0x14, 0x1b),
        (rect.width() * 0.047).max(0.9),
    );
}

fn draw_image_glyph(painter: &egui::Painter, rect: Rect, color: Color32, stroke_width: f32) {
    let stroke = Stroke::new(stroke_width, color);
    painter.rect_stroke(rect, CornerRadius::same(5), stroke, StrokeKind::Inside);
    let w = rect.width();
    let h = rect.height();
    painter.circle_filled(
        egui::pos2(rect.left() + w * 0.27, rect.top() + h * 0.28),
        w * 0.085,
        color,
    );
    painter.add(egui::Shape::line(
        vec![
            egui::pos2(rect.left() + w * 0.10, rect.bottom() - h * 0.14),
            egui::pos2(rect.left() + w * 0.38, rect.top() + h * 0.50),
            egui::pos2(rect.left() + w * 0.55, rect.top() + h * 0.68),
            egui::pos2(rect.left() + w * 0.69, rect.top() + h * 0.55),
            egui::pos2(rect.right() - w * 0.08, rect.bottom() - h * 0.14),
        ],
        stroke,
    ));
}

fn dashed_round_rect(
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    stroke: Stroke,
    dash: f32,
    gap: f32,
) {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let mut points = Vec::new();
    let step = 6.0;

    let mut x = rect.left() + r;
    while x < rect.right() - r {
        points.push(egui::pos2(x, rect.top()));
        x += step;
    }
    arc_points(&mut points, egui::pos2(rect.right() - r, rect.top() + r), r, -90.0, 0.0);

    let mut y = rect.top() + r;
    while y < rect.bottom() - r {
        points.push(egui::pos2(rect.right(), y));
        y += step;
    }
    arc_points(&mut points, egui::pos2(rect.right() - r, rect.bottom() - r), r, 0.0, 90.0);

    let mut x = rect.right() - r;
    while x > rect.left() + r {
        points.push(egui::pos2(x, rect.bottom()));
        x -= step;
    }
    arc_points(&mut points, egui::pos2(rect.left() + r, rect.bottom() - r), r, 90.0, 180.0);

    let mut y = rect.bottom() - r;
    while y > rect.top() + r {
        points.push(egui::pos2(rect.left(), y));
        y -= step;
    }
    arc_points(&mut points, egui::pos2(rect.left() + r, rect.top() + r), r, 180.0, 270.0);

    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    for shape in egui::Shape::dashed_line(&points, stroke, dash, gap) {
        painter.add(shape);
    }
}

fn arc_points(points: &mut Vec<egui::Pos2>, center: egui::Pos2, radius: f32, from: f32, to: f32) {
    let steps = 8;
    for i in 0..=steps {
        let angle = from + (to - from) * (i as f32 / steps as f32);
        let radians = angle.to_radians();
        points.push(center + egui::vec2(radians.cos(), radians.sin()) * radius);
    }
}

fn wrap_text(text: &str, max_width: f32, font_size: f32) -> String {
    let max_width = max_width.max(40.0);
    let mut out = String::new();
    let mut line_width = 0.0f32;
    for ch in text.chars() {
        if ch == '\n' {
            out.push(ch);
            line_width = 0.0;
            continue;
        }
        let width = if ch.is_ascii() {
            font_size * 0.55
        } else {
            font_size
        };
        if line_width + width > max_width && line_width > 0.0 {
            out.push('\n');
            line_width = 0.0;
        }
        out.push(ch);
        line_width += width;
    }
    out
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let value = bytes as f64;
    if value < KB {
        format!("{bytes} B")
    } else if value < KB * KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{:.2} MB", value / (KB * KB))
    }
}
