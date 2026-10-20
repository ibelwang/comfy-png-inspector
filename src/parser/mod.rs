//! Image-generation metadata parser.
//!
//! Supports ComfyUI PNG files (`prompt` / `workflow` tEXt chunks) and
//! A1111 / WebUI images (`parameters` text chunk or JPEG `UserComment`).
//! Pixel data is never decoded: only PNG chunks and the image header are read,
//! so parsing stays fast even for huge images.

use std::path::{Path, PathBuf};

use serde_json::Value;

pub mod a1111;
pub mod comfy;
pub mod exif;
pub mod png;

/// Where a picture's parameters came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Unknown,
    ComfyUI,
    A1111,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::ComfyUI => "ComfyUI",
            Source::A1111 => "WebUI / A1111",
            Source::Unknown => "无生成参数",
        }
    }

    /// Stable machine name (mirrors the Python implementation).
    pub fn name(self) -> &'static str {
        match self {
            Source::ComfyUI => "comfyui",
            Source::A1111 => "a1111",
            Source::Unknown => "unknown",
        }
    }
}

/// One LoRA entry with its strength(s).
#[derive(Debug, Clone, Default)]
pub struct LoraInfo {
    pub name: String,
    pub strength_model: Option<f64>,
    pub strength_clip: Option<f64>,
    pub node: String,
}

impl LoraInfo {
    pub fn strength_text(&self) -> String {
        match (fmt_num(self.strength_model), fmt_num(self.strength_clip)) {
            (None, None) => "?".to_string(),
            (None, Some(clip)) => clip,
            (Some(model), None) => model,
            (Some(model), Some(clip)) if model == clip => model,
            (Some(model), Some(clip)) => format!("{model} / {clip}"),
        }
    }
}

/// One sampler node (KSampler, KSamplerAdvanced, SamplerCustom, ...).
#[derive(Debug, Clone, Default)]
pub struct SamplerInfo {
    pub class_type: String,
    pub node_id: String,
    pub sampler_name: Option<String>,
    pub scheduler: Option<String>,
    pub seed: Option<Value>,
    pub steps: Option<i64>,
    pub cfg: Option<f64>,
    pub denoise: Option<f64>,
    pub start_at_step: Option<i64>,
    pub end_at_step: Option<i64>,
    pub positive: String,
    pub negative: String,
    pub model_link: String,
}

impl SamplerInfo {
    pub fn seed_text(&self) -> Option<String> {
        self.seed.as_ref().map(seed_to_string)
    }
}

/// Everything the UI needs to know about one image.
#[derive(Debug, Clone, Default)]
pub struct ImageMeta {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub source: Source,
    pub positive: String,
    pub negative: String,
    pub models: Vec<String>,
    pub loras: Vec<LoraInfo>,
    pub samplers: Vec<SamplerInfo>,
    pub vae: String,
    pub clip_skip: Option<i64>,
    pub has_workflow: bool,
    pub raw_parameters: String,
    pub notes: Vec<String>,
}

impl ImageMeta {
    pub fn filename(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn size_bytes(&self) -> u64 {
        std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)
    }

    pub fn has_metadata(&self) -> bool {
        self.source != Source::Unknown
    }

    pub fn dimensions_text(&self) -> String {
        if self.width > 0 && self.height > 0 {
            format!("{} × {}", self.width, self.height)
        } else {
            "未知尺寸".to_string()
        }
    }
}

/// Read generation metadata from `path` (PNG or JPEG).
pub fn load_meta(path: &Path) -> anyhow::Result<ImageMeta> {
    if !path.is_file() {
        anyhow::bail!("文件不存在：{}", path.display());
    }

    let mut meta = ImageMeta {
        path: path.to_path_buf(),
        ..Default::default()
    };
    let exif_comment = fill_image_info(&mut meta, path);

    let mut texts = std::collections::BTreeMap::new();
    if png::looks_like_png(path) {
        match png::read_text_chunks(path) {
            Ok(t) => texts = t,
            Err(err) => meta.notes.push(format!("读取 PNG 文本块失败：{err}")),
        }
    }

    let prompt_raw = texts.get("prompt").cloned().unwrap_or_default();
    let workflow_raw = texts.get("workflow").cloned().unwrap_or_default();
    let parameters_raw = {
        let from_png = texts.get("parameters").cloned().unwrap_or_default();
        if from_png.is_empty() {
            exif_comment.unwrap_or_default()
        } else {
            from_png
        }
    };
    meta.has_workflow = !workflow_raw.is_empty();

    let mut comfy_parsed = false;
    if !prompt_raw.is_empty() {
        match serde_json::from_str::<Value>(&prompt_raw) {
            Ok(payload) => match comfy::parse_comfy_prompt(&payload, &mut meta) {
                Ok(()) => {
                    meta.source = Source::ComfyUI;
                    comfy_parsed = true;
                }
                Err(err) => meta.notes.push(format!("ComfyUI prompt 解析失败：{err}")),
            },
            Err(err) => meta
                .notes
                .push(format!("ComfyUI prompt 解析失败：{err}")),
        }
    }

    if comfy_parsed && !parameters_raw.is_empty() {
        // The node graph wins; the A1111 `parameters` block only fills the gaps
        // left by custom nodes (wildcard text, literal seeds, sampler names).
        let mut fallback = ImageMeta::default();
        if a1111::parse_a1111(&parameters_raw, &mut fallback).is_ok()
            && merge_missing(&mut meta, &fallback)
        {
            meta.raw_parameters = parameters_raw.clone();
            meta.notes.push(
                "ComfyUI 节点图缺失的字段已用同文件的 A1111 `parameters` 文本块补齐。".into(),
            );
        }
    } else if meta.source == Source::Unknown && !parameters_raw.is_empty() {
        match a1111::parse_a1111(&parameters_raw, &mut meta) {
            Ok(()) => {
                meta.source = Source::A1111;
                meta.raw_parameters = parameters_raw;
            }
            Err(err) => meta.notes.push(format!("A1111 参数解析失败：{err}")),
        }
    }

    if meta.source == Source::Unknown {
        if !workflow_raw.is_empty() {
            meta.notes
                .push("这张图只包含 workflow，没有 prompt 节点数据，无法提取参数。".into());
        } else if texts.is_empty() {
            meta.notes
                .push("没有找到生成参数（可能不是 ComfyUI / WebUI 生成的图片）。".into());
        } else {
            meta.notes
                .push("PNG 里没有 ComfyUI 或 WebUI 的生成参数。".into());
        }
    }

    Ok(meta)
}

/// Fill fields the ComfyUI graph could not provide from the A1111 `parameters`
/// block (custom nodes hide text, seeds, sampler names behind helper nodes).
/// Graph values always win. Returns true when anything was copied.
fn merge_missing(meta: &mut ImageMeta, fallback: &ImageMeta) -> bool {
    let mut changed = false;

    if meta.positive.trim().is_empty() && !fallback.positive.trim().is_empty() {
        meta.positive = fallback.positive.clone();
        changed = true;
    }
    if meta.negative.trim().is_empty() && !fallback.negative.trim().is_empty() {
        meta.negative = fallback.negative.clone();
        changed = true;
    }
    if meta.models.is_empty() && !fallback.models.is_empty() {
        meta.models = fallback.models.clone();
        changed = true;
    }
    if meta.loras.is_empty() && !fallback.loras.is_empty() {
        meta.loras = fallback.loras.clone();
        changed = true;
    }
    if meta.vae.is_empty() && !fallback.vae.is_empty() {
        meta.vae = fallback.vae.clone();
        changed = true;
    }
    if meta.clip_skip.is_none() {
        if let Some(value) = fallback.clip_skip {
            meta.clip_skip = Some(value);
            changed = true;
        }
    }
    if meta.width == 0 && fallback.width > 0 {
        meta.width = fallback.width;
        changed = true;
    }
    if meta.height == 0 && fallback.height > 0 {
        meta.height = fallback.height;
        changed = true;
    }

    if meta.samplers.is_empty() && !fallback.samplers.is_empty() {
        meta.samplers = fallback.samplers.clone();
        changed = true;
    } else if let (Some(main), Some(fb)) = (meta.samplers.first_mut(), fallback.samplers.first())
    {
        if main.sampler_name.is_none() {
            if let Some(value) = &fb.sampler_name {
                main.sampler_name = Some(value.clone());
                changed = true;
            }
        }
        if main.scheduler.is_none() {
            if let Some(value) = &fb.scheduler {
                main.scheduler = Some(value.clone());
                changed = true;
            }
        }
        if main.steps.is_none() {
            if let Some(value) = fb.steps {
                main.steps = Some(value);
                changed = true;
            }
        }
        if main.cfg.is_none() {
            if let Some(value) = fb.cfg {
                main.cfg = Some(value);
                changed = true;
            }
        }
        if main.denoise.is_none() {
            if let Some(value) = fb.denoise {
                main.denoise = Some(value);
                changed = true;
            }
        }
        if main.seed.is_none() {
            if let Some(value) = &fb.seed {
                main.seed = Some(value.clone());
                changed = true;
            }
        }
    }

    changed
}

/// Fill size/format from the image header and return the JPEG UserComment.
fn fill_image_info(meta: &mut ImageMeta, path: &Path) -> Option<String> {
    let mut comment = None;
    match image::ImageReader::open(path)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
    {
        Some(reader) => {
            let format = reader.format();
            if let Some(fmt) = format {
                meta.format = format_name(fmt);
            }
            if let Ok((w, h)) = reader.into_dimensions() {
                meta.width = w;
                meta.height = h;
            }
            if meta.format == "JPEG" {
                comment = exif::user_comment_from_file(path);
            }
        }
        None => {
            meta.format = path
                .extension()
                .map(|e| e.to_string_lossy().to_uppercase())
                .unwrap_or_else(|| "?".to_string());
        }
    }
    if meta.format.is_empty() {
        meta.format = "?".to_string();
    }
    comment
}

fn format_name(format: image::ImageFormat) -> String {
    match format {
        image::ImageFormat::Png => "PNG".into(),
        image::ImageFormat::Jpeg => "JPEG".into(),
        image::ImageFormat::WebP => "WEBP".into(),
        image::ImageFormat::Gif => "GIF".into(),
        image::ImageFormat::Bmp => "BMP".into(),
        other => other.extensions_str().first().unwrap_or(&"?").to_uppercase(),
    }
}

fn seed_to_string(seed: &Value) -> String {
    match seed {
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                n.to_string()
            }
        }
        other => other.to_string(),
    }
}

/// Python-style `f"{value:g}"` for the values we display.
pub fn fmt_num(value: Option<f64>) -> Option<String> {
    let v = value?;
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return Some(format!("{}", v as i64));
    }
    Some(format!("{v}"))
}
