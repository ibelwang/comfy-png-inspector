//! Dump the Rust parser's output as JSON lines (for comparison with Python).

use std::path::Path;

use comfy_png_inspector::parser::{self, ImageMeta};
use serde_json::{json, Value};

fn main() {
    let mut failed = false;
    for arg in std::env::args().skip(1) {
        match parser::load_meta(Path::new(&arg)) {
            Ok(meta) => println!("{}", serde_json::to_string(&to_json(&meta)).unwrap()),
            Err(err) => {
                eprintln!("{arg}: {err}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}

fn to_json(meta: &ImageMeta) -> Value {
    let loras: Vec<Value> = meta
        .loras
        .iter()
        .map(|l| {
            json!({
                "name": l.name,
                "strength_model": l.strength_model,
                "strength_clip": l.strength_clip,
                "node": l.node,
            })
        })
        .collect();
    let samplers: Vec<Value> = meta
        .samplers
        .iter()
        .map(|s| {
            json!({
                "class_type": s.class_type,
                "node_id": s.node_id,
                "sampler_name": s.sampler_name,
                "scheduler": s.scheduler,
                "seed": s.seed,
                "steps": s.steps,
                "cfg": s.cfg,
                "denoise": s.denoise,
                "start_at_step": s.start_at_step,
                "end_at_step": s.end_at_step,
                "positive": s.positive,
                "negative": s.negative,
                "model_link": s.model_link,
            })
        })
        .collect();
    json!({
        "path": meta.path.to_string_lossy(),
        "source": meta.source.name(),
        "positive": meta.positive,
        "negative": meta.negative,
        "models": meta.models,
        "loras": loras,
        "samplers": samplers,
        "vae": meta.vae,
        "clip_skip": meta.clip_skip,
        "has_workflow": meta.has_workflow,
        "notes": meta.notes,
        "raw_parameters": meta.raw_parameters,
    })
}
