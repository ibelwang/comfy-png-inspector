//! Classic A1111 / WebUI `parameters` text parsing.

use anyhow::Result;
use fancy_regex::Regex;
use serde_json::Value;

use super::{ImageMeta, LoraInfo, SamplerInfo};

const PARAM_PAIR: &str = r#"([A-Za-z][A-Za-z0-9 _/+\-]*?):\s*("(?:[^"\\]|\\.)*"|[^,]*?)(?=,\s*[A-Za-z][A-Za-z0-9 _/+\-]*?:\s*|$)"#;
const LORA_TAG: &str = r"(?i)<lora:([^:<>]+?)(?::([-+0-9.]+))?\s*>";
const SIZE: &str = r"(\d+)\s*[x×]\s*(\d+)";

/// Parse the classic A1111 `parameters` text block.
pub fn parse_a1111(text: &str, meta: &mut ImageMeta) -> Result<()> {
    let text = text.replace("\r\n", "\n");
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("参数文本为空");
    }

    let lines: Vec<&str> = text.split('\n').collect();
    let mut param_index = lines.len();
    for i in (0..lines.len()).rev() {
        if is_param_line(lines[i]) {
            param_index = i;
            break;
        }
    }

    let prompt_lines = &lines[..param_index];
    let param_text = lines[param_index..].join("\n");

    let negative_index = prompt_lines
        .iter()
        .position(|line| line.trim().to_lowercase().starts_with("negative prompt:"));

    match negative_index {
        None => {
            meta.positive = prompt_lines.join("\n").trim().to_string();
        }
        Some(index) => {
            meta.positive = prompt_lines[..index].join("\n").trim().to_string();
            let negative = prompt_lines[index..].join("\n").trim().to_string();
            meta.negative = strip_negative_prefix(&negative);
        }
    }

    let params = parse_pairs(&param_text);
    let mut info = SamplerInfo {
        class_type: "A1111".into(),
        ..Default::default()
    };

    info.steps = params.get("Steps").and_then(|v| as_int_str(v));
    info.cfg = params.get("CFG scale").and_then(|v| v.trim().parse::<f64>().ok());
    info.seed = params.get("Seed").map(|v| Value::String(v.clone()));
    info.denoise = params
        .get("Denoising strength")
        .and_then(|v| v.trim().parse::<f64>().ok());
    if let Some(sampler) = params.get("Sampler") {
        if !sampler.is_empty() {
            let (name, scheduler) = split_webui_sampler(sampler);
            info.sampler_name = Some(name);
            info.scheduler = scheduler;
        }
    }
    meta.samplers = vec![info];

    if let Some(model) = params.get("Model") {
        if !model.is_empty() {
            meta.models = vec![model.clone()];
        }
    }
    if let Some(clip_skip) = params.get("Clip skip").and_then(|v| as_int_str(v)) {
        meta.clip_skip = Some(clip_skip);
    }

    if let Some(size) = params.get("Size") {
        if let Ok(re) = Regex::new(SIZE) {
            if let Ok(Some(caps)) = re.captures(size) {
                let w = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
                let h = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
                if meta.width == 0 {
                    if let Some(w) = w {
                        meta.width = w;
                    }
                }
                if meta.height == 0 {
                    if let Some(h) = h {
                        meta.height = h;
                    }
                }
            }
        }
    }

    let mut loras = Vec::new();
    if let Ok(re) = Regex::new(LORA_TAG) {
        let haystack = format!("{}\n{}", meta.positive, meta.negative);
        for caps in re.captures_iter(&haystack).flatten() {
            let Some(name) = caps.get(1).map(|m| m.as_str().trim().to_string()) else {
                continue;
            };
            let strength = caps
                .get(2)
                .and_then(|m| m.as_str().parse::<f64>().ok())
                .unwrap_or(1.0);
            loras.push(LoraInfo {
                name,
                strength_model: Some(strength),
                strength_clip: Some(strength),
                node: String::new(),
            });
        }
    }
    meta.loras = loras;

    Ok(())
}

fn is_param_line(line: &str) -> bool {
    let rest = line.trim_start();
    let lower = rest.to_ascii_lowercase();
    let Some(after) = lower.strip_prefix("steps:") else {
        return false;
    };
    after.trim_start().starts_with(|c: char| c.is_ascii_digit())
}

fn strip_negative_prefix(text: &str) -> String {
    let lower = text.to_lowercase();
    if lower.starts_with("negative prompt:") {
        text["negative prompt:".len()..].trim().to_string()
    } else {
        text.to_string()
    }
}

fn parse_pairs(text: &str) -> std::collections::HashMap<String, String> {
    let mut params = std::collections::HashMap::new();
    let joined = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let Ok(re) = Regex::new(PARAM_PAIR) else {
        return params;
    };
    for caps in re.captures_iter(&joined).flatten() {
        let (Some(key), Some(value)) = (caps.get(1), caps.get(2)) else {
            continue;
        };
        let key = key.as_str().trim();
        let mut value = value.as_str().trim();
        if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            value = &value[1..value.len() - 1];
        }
        if !key.is_empty() && !value.is_empty() {
            params.insert(key.to_string(), value.to_string());
        }
    }
    params
}

fn split_webui_sampler(sampler: &str) -> (String, Option<String>) {
    for suffix in [
        " Karras",
        " Exponential",
        " SGM Uniform",
        " Uniform",
        " Polyexponential",
    ] {
        if let Some(rest) = sampler.strip_suffix(suffix) {
            return (rest.trim().to_string(), Some(suffix.trim().to_string()));
        }
    }
    (sampler.trim().to_string(), None)
}

fn as_int_str(value: &str) -> Option<i64> {
    let t = value.trim();
    let digits = t.strip_prefix('-').unwrap_or(t);
    if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        t.parse::<i64>().ok()
    } else {
        None
    }
}
