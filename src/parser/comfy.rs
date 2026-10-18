//! ComfyUI node-graph parsing.
//!
//! Works on the JSON stored in the PNG `prompt` chunk:
//! `{ "node id": { "class_type": "...", "inputs": {...} } }`, where a link is
//! represented as `["target node id", output index]`.

use std::collections::{BTreeMap, HashSet};

use serde_json::{Map, Value};

use super::{LoraInfo, ImageMeta, SamplerInfo};

/// Node map plus the display ordering of node ids.
#[derive(Debug, Clone, Default)]
struct Graph {
    map: BTreeMap<String, Value>,
    order: Vec<String>,
}

impl Graph {
    fn from_prompt(payload: &Value) -> Option<Graph> {
        let obj = payload.as_object()?;
        let mut map = BTreeMap::new();
        let mut order = Vec::new();
        for (id, node) in obj {
            if node.is_object() {
                map.insert(id.clone(), node.clone());
                order.push(id.clone());
            }
        }
        if map.is_empty() {
            return None;
        }
        order.sort_by_key(|id| sort_key(id));
        Some(Graph { map, order })
    }

    fn node(&self, id: &str) -> Option<&Value> {
        self.map.get(id)
    }

    fn node_at(&self, value: &Value) -> Option<&Value> {
        let id = linked_id(value)?;
        self.map.get(&id)
    }
}

fn sort_key(id: &str) -> (u8, String) {
    if let Ok(n) = id.parse::<i64>() {
        (0, format!("{n:020}"))
    } else {
        (1, id.to_string())
    }
}

fn node_class(node: &Value) -> String {
    node.get("class_type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn node_inputs(node: &Value) -> Map<String, Value> {
    node.get("inputs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Follow a conditioning link and join every text it reaches.
fn trace_text(value: Option<&Value>, graph: &Graph) -> String {
    let mut texts: Vec<String> = Vec::new();
    if let Some(value) = value {
        for text in collect_texts(value, graph, &HashSet::new(), 0) {
            let text = text.trim().to_string();
            if !text.is_empty() && !texts.contains(&text) {
                texts.push(text);
            }
        }
    }
    texts.join("\n\n")
}

fn collect_texts(
    value: &Value,
    graph: &Graph,
    seen: &HashSet<String>,
    depth: usize,
) -> Vec<String> {
    let Some(id) = linked_id(value) else {
        return Vec::new();
    };
    if seen.contains(&id) || depth > 30 {
        return Vec::new();
    }
    let mut seen = seen.clone();
    seen.insert(id.clone());

    let Some(node) = graph.node(&id) else {
        return Vec::new();
    };
    let class_type = node_class(node);
    let inputs = node_inputs(node);

    // ConditioningZeroOut intentionally discards the text (Krea2 / Flux style
    // workflows that need no negative prompt).
    if norm(&class_type).contains("zeroout") {
        return Vec::new();
    }

    if is_text_encoder(&class_type) {
        let mut found = Vec::new();
        for key in ["text", "prompt", "text_g", "text_l"] {
            if let Some(Value::String(text)) = inputs.get(key) {
                if !text.trim().is_empty() {
                    found.push(text.clone());
                }
            }
        }
        if !found.is_empty() {
            return found;
        }
        // The text may come from a primitive/string node.
        for key in ["text", "prompt", "string", "value"] {
            if let Some(v) = inputs.get(key) {
                if linked_id(v).is_some() {
                    return collect_strings(v, graph, &seen, depth + 1);
                }
            }
        }
        return Vec::new();
    }

    let mut texts = Vec::new();
    for v in inputs.values() {
        if linked_id(v).is_some() {
            texts.extend(collect_texts(v, graph, &seen, depth + 1));
        }
    }
    texts
}

/// Collect the string payload behind a text input.
///
/// Handles helper nodes that stitch prompts together (CR Text Concatenate,
/// Lora Prompt Concatenation, ...): linked nodes are walked, string-valued
/// inputs are collected (except obvious metadata fields), and every other
/// link is followed recursively.
fn collect_strings(
    value: &Value,
    graph: &Graph,
    seen: &HashSet<String>,
    depth: usize,
) -> Vec<String> {
    if let Some(s) = value.as_str() {
        return if s.trim().is_empty() {
            Vec::new()
        } else {
            vec![s.to_string()]
        };
    }
    let Some(id) = linked_id(value) else {
        return Vec::new();
    };
    if seen.contains(&id) || depth > 25 {
        return Vec::new();
    }
    let Some(node) = graph.node(&id) else {
        return Vec::new();
    };
    let inputs = node_inputs(node);

    let mut seen = seen.clone();
    seen.insert(id);

    let mut texts = Vec::new();
    for (key, val) in &inputs {
        if STRING_METADATA_KEYS.contains(&key.as_str()) {
            continue;
        }
        if let Some(s) = val.as_str() {
            if !s.trim().is_empty() {
                texts.push(s.to_string());
            }
        } else if linked_id(val).is_some() {
            texts.extend(collect_strings(val, graph, &seen, depth + 1));
        }
    }
    texts
}

fn all_text_encoder_texts(graph: &Graph) -> Vec<String> {
    let mut texts = Vec::new();
    for node in graph.map.values() {
        if !is_text_encoder(&node_class(node)) {
            continue;
        }
        let inputs = node_inputs(node);
        for key in ["text", "prompt", "text_g", "text_l"] {
            if let Some(Value::String(text)) = inputs.get(key) {
                if !text.trim().is_empty() {
                    texts.push(text.trim().to_string());
                    break;
                }
            }
        }
    }
    texts
}

/// Walk each sampler's `model` chain up to the checkpoint/UNET loader.
fn collect_models_chain(graph: &Graph, samplers: &[SamplerInfo]) -> Vec<String> {
    const KEYS: [&str; 5] = [
        "ckpt_name",
        "unet_name",
        "model_name",
        "checkpoint_name",
        "gguf_name",
    ];
    let mut found = Vec::new();
    for sampler in samplers {
        let mut node_id = sampler.model_link.clone();
        let mut seen = HashSet::new();
        while !node_id.is_empty() && graph.map.contains_key(&node_id) && !seen.contains(&node_id) {
            seen.insert(node_id.clone());
            let node = &graph.map[&node_id];
            let inputs = node_inputs(node);
            for key in KEYS {
                if let Some(Value::String(val)) = inputs.get(key) {
                    if !val.trim().is_empty() {
                        found.push(val.trim().to_string());
                        break;
                    }
                }
            }
            node_id = inputs
                .get("model")
                .and_then(linked_id)
                .unwrap_or_default();
        }
    }
    dedupe(found)
}

fn collect_models(graph: &Graph) -> Vec<String> {
    const KEYS: [&str; 5] = [
        "ckpt_name",
        "unet_name",
        "model_name",
        "checkpoint_name",
        "gguf_name",
    ];
    let mut found = Vec::new();
    for node in graph.map.values() {
        let class_type = norm(&node_class(node));
        if class_type.contains("lora") {
            continue;
        }
        // Skip auxiliary loaders (SAM segmentation, YOLO/bbox detectors, ...).
        if ["yolo", "segment", "detector", "bbox", "samloader", "sam2"]
            .iter()
            .any(|marker| class_type.contains(marker))
        {
            continue;
        }
        let inputs = node_inputs(node);
        for key in KEYS {
            if let Some(Value::String(val)) = inputs.get(key) {
                if !val.trim().is_empty() {
                    found.push(val.trim().to_string());
                    break;
                }
            }
        }
    }
    dedupe(found)
}

fn collect_loras(graph: &Graph) -> Vec<LoraInfo> {
    let mut found = Vec::new();
    for id in &graph.order {
        let node = &graph.map[id];
        if !norm(&node_class(node)).contains("lora") {
            continue;
        }
        let inputs = node_inputs(node);

        if let Some(Value::String(name)) = inputs.get("lora_name") {
            if !name.trim().is_empty() {
                found.push(LoraInfo {
                    name: name.trim().to_string(),
                    strength_model: inputs
                        .get("strength_model")
                        .or_else(|| inputs.get("strength"))
                        .and_then(as_float),
                    strength_clip: inputs
                        .get("strength_clip")
                        .or_else(|| inputs.get("strength"))
                        .and_then(as_float),
                    node: id.clone(),
                });
                continue;
            }
        }

        // rgthree "Power Lora Loader" / stacker style: dict-valued inputs.
        for val in inputs.values() {
            let Some(obj) = val.as_object() else {
                continue;
            };
            let name = obj
                .get("lora")
                .or_else(|| obj.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            if obj.get("on") == Some(&Value::Bool(false)) {
                continue;
            }
            found.push(LoraInfo {
                name,
                strength_model: obj.get("strength").and_then(as_float),
                strength_clip: obj.get("strength_clip").and_then(as_float),
                node: id.clone(),
            });
        }
    }
    found
}

fn collect_vae(graph: &Graph) -> String {
    for node in graph.map.values() {
        if !norm(&node_class(node)).contains("vaeloader") {
            continue;
        }
        if let Some(Value::String(val)) = node_inputs(node).get("vae_name") {
            if !val.trim().is_empty() {
                return val.trim().to_string();
            }
        }
    }
    String::new()
}

fn collect_clip_skip(graph: &Graph) -> Option<i64> {
    for node in graph.map.values() {
        if !norm(&node_class(node)).contains("clipsetlastlayer") {
            continue;
        }
        if let Some(value) = node_inputs(node).get("stop_at_clip_layer").and_then(as_int) {
            return Some(value.abs());
        }
    }
    None
}

fn is_sampler_node(node: &Value) -> bool {
    let class_type = norm(&node_class(node));
    if matches!(
        class_type.as_str(),
        "ksampler" | "ksampleradvanced" | "samplercustom" | "samplercustomadvanced"
    ) {
        return true;
    }
    // "KSampler to Image Saver" and friends report sampler settings but are
    // not actual sampling stages.
    class_type.contains("ksampler") && !class_type.contains("select") && !class_type.contains("saver")
}

fn is_text_encoder(class_type: &str) -> bool {
    let normalised = norm(class_type);
    if normalised.contains("textencode") {
        return true;
    }
    normalised.contains("textencoder") && !normalised.contains("loader")
}

fn build_sampler(graph: &Graph, id: &str, node: &Value) -> SamplerInfo {
    let class_type = node_class(node);
    let inputs = node_inputs(node);
    let mut info = SamplerInfo {
        class_type: class_type.clone(),
        node_id: id.to_string(),
        positive: trace_text(inputs.get("positive"), graph),
        negative: trace_text(inputs.get("negative"), graph),
        ..Default::default()
    };
    info.model_link = inputs.get("model").and_then(linked_id).unwrap_or_default();
    info.seed = inputs
        .get("seed")
        .or_else(|| inputs.get("noise_seed"))
        .cloned();
    info.steps = inputs.get("steps").and_then(as_int);
    info.cfg = inputs.get("cfg").and_then(as_float);
    info.sampler_name = inputs.get("sampler_name").and_then(as_str);
    info.scheduler = inputs.get("scheduler").and_then(as_str);
    info.denoise = inputs.get("denoise").and_then(as_float);
    info.start_at_step = inputs.get("start_at_step").and_then(as_int);
    info.end_at_step = inputs.get("end_at_step").and_then(as_int);

    // SamplerCustom: parameters live in the linked helper nodes.
    if norm(&class_type).contains("samplercustom") {
        if let Some(guider) = inputs.get("guider").and_then(|v| graph.node_at(v)) {
            let guider_inputs = node_inputs(guider);
            if info.positive.is_empty() {
                info.positive = trace_text(guider_inputs.get("positive"), graph);
            }
            if info.negative.is_empty() {
                info.negative = trace_text(guider_inputs.get("negative"), graph);
            }
            if let Some(cfg) = guider_inputs.get("cfg").and_then(as_float) {
                info.cfg = Some(cfg);
            }
        }

        if let Some(select) = inputs.get("sampler").and_then(|v| graph.node_at(v)) {
            if let Some(name) = node_inputs(select).get("sampler_name").and_then(as_str) {
                info.sampler_name = Some(name);
            }
        }

        if let Some(sigmas) = inputs.get("sigmas").and_then(|v| graph.node_at(v)) {
            let sigma_inputs = node_inputs(sigmas);
            if let Some(scheduler) = sigma_inputs.get("scheduler").and_then(as_str) {
                info.scheduler = Some(scheduler);
            }
            if let Some(steps) = sigma_inputs.get("steps").and_then(as_int) {
                info.steps = Some(steps);
            }
            if let Some(denoise) = sigma_inputs.get("denoise").and_then(as_float) {
                info.denoise = Some(denoise);
            }
        }

        if let Some(noise) = inputs.get("noise").and_then(|v| graph.node_at(v)) {
            if let Some(seed) = node_inputs(noise).get("noise_seed") {
                info.seed = Some(seed.clone());
            }
        }

        // "SamplerCustom" itself links noise/sampler/sigmas straight in.
        if info.seed.is_none() {
            info.seed = inputs.get("noise_seed").cloned();
        }
    }

    // KSampler (Advanced) may carry the seed under noise_seed only.
    if info.seed.is_none() {
        info.seed = inputs.get("noise_seed").cloned();
    }

    info
}

/// Extract prompt text, models, LoRAs and sampler settings from a graph.
pub fn parse_comfy_prompt(payload: &Value, meta: &mut ImageMeta) -> anyhow::Result<()> {
    let Some(graph) = Graph::from_prompt(payload) else {
        anyhow::bail!("prompt 节点为空");
    };

    let mut samplers: Vec<SamplerInfo> = Vec::new();
    for id in &graph.order {
        let node = &graph.map[id];
        if is_sampler_node(node) {
            samplers.push(build_sampler(&graph, id, node));
        }
    }

    if samplers.is_empty() {
        meta.notes
            .push("没有识别到采样器节点（KSampler / SamplerCustom 等）。".into());
    } else {
        // The first sampler that actually carries text wins as the "main" one.
        let main = samplers
            .iter()
            .find(|s| !s.positive.is_empty() || !s.negative.is_empty())
            .unwrap_or(&samplers[0])
            .clone();
        meta.positive = main.positive.clone();
        meta.negative = main.negative.clone();

        if meta.negative.is_empty() {
            let main_inputs = graph
                .map
                .get(&main.node_id)
                .map(node_inputs)
                .unwrap_or_default();
            if let Some(negative_node) =
                main_inputs.get("negative").and_then(|v| graph.node_at(v))
            {
                if norm(&node_class(negative_node)).contains("zeroout") {
                    meta.notes.push(
                        "负向为空：工作流用 ConditioningZeroOut 清零负向\
                         （Krea2 / Flux 类模型通常不需要负向）。"
                            .into(),
                    );
                }
            }
        }

        if meta.positive.is_empty() {
            // The positive conditioning was not reachable from a sampler
            // (custom routing nodes, ZeroOut, ...). Fall back to every text
            // encoder that is not part of the known negative prompt.
            let mut negative_set: HashSet<String> = samplers
                .iter()
                .filter(|s| !s.negative.trim().is_empty())
                .map(|s| s.negative.trim().to_string())
                .collect();
            if !meta.negative.trim().is_empty() {
                negative_set.insert(meta.negative.trim().to_string());
            }
            let candidates = dedupe(
                all_text_encoder_texts(&graph)
                    .into_iter()
                    .filter(|t| !t.trim().is_empty() && !negative_set.contains(t.trim()))
                    .collect(),
            );
            if !candidates.is_empty() {
                meta.positive = candidates.join("\n\n");
                meta.notes.push(
                    "正向提示词未与采样器连线，已按节点文本推断，请核对。".into(),
                );
            }
        }

        if meta.positive.is_empty() && meta.negative.is_empty() {
            let texts = all_text_encoder_texts(&graph);
            if !texts.is_empty() {
                meta.positive = texts[0].clone();
                if texts.len() > 1 {
                    meta.negative = texts[1..].join("\n\n");
                }
                meta.notes
                    .push("未能从采样器追踪提示词，已按节点顺序推断正向/负向。".into());
            }
        }
    }

    let models = collect_models_chain(&graph, &samplers);
    meta.models = if models.is_empty() {
        collect_models(&graph)
    } else {
        models
    };
    meta.loras = collect_loras(&graph);
    meta.vae = collect_vae(&graph);
    meta.clip_skip = collect_clip_skip(&graph);
    meta.samplers = samplers;

    Ok(())
}

const STRING_METADATA_KEYS: [&str; 27] = [
    "model_name",
    "ckpt_name",
    "lora_name",
    "vae_name",
    "clip_name",
    "unet_name",
    "sampler_name",
    "scheduler",
    "filename",
    "path",
    "extension",
    "labels",
    "detection_hint",
    "operation",
    "mode",
    "mode_type",
    "seam_fix_mode",
    "resize_mode",
    "pad_color",
    "crop_position",
    "precision",
    "font_name",
    "font_color_hex",
    "aspect_ratio",
    "swap_dimensions",
    "upscale_method",
    "channel",
];

/// `[node id, output index]` link -> node id.
fn linked_id(value: &Value) -> Option<String> {
    let arr = value.as_array()?;
    if arr.len() != 2 {
        return None;
    }
    match &arr[0] {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) if n.is_i64() || n.is_u64() => Some(n.to_string()),
        _ => None,
    }
}

fn norm(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        if seen.insert(item.clone()) {
            out.push(item);
        }
    }
    out
}

fn as_str(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn as_int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i)
            } else if let Some(u) = n.as_u64() {
                i64::try_from(u).ok()
            } else {
                let f = n.as_f64()?;
                if f.fract() == 0.0 {
                    Some(f as i64)
                } else {
                    None
                }
            }
        }
        Value::String(s) => {
            let t = s.trim();
            let digits = t.strip_prefix('-').unwrap_or(t);
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                t.parse::<i64>().ok()
            } else {
                None
            }
        }
        _ => None,
    }
}

fn as_float(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}
