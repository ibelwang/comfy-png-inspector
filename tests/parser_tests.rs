//! Acceptance tests on the sample images (mirrors PROJECT-NOTES.md §9).

use std::path::PathBuf;

use comfy_png_inspector::parser::{self, comfy, ImageMeta, Source};

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name)
}

fn load(name: &str) -> ImageMeta {
    parser::load_meta(&testdata(name)).expect("metadata should parse")
}

fn approx(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("value missing");
    assert!(
        (actual - expected).abs() < 1e-9,
        "{actual} != {expected}"
    );
}

#[test]
fn comfy_standard() {
    let meta = load("comfy_standard.png");
    assert_eq!(meta.source, Source::ComfyUI);
    assert!(
        meta.positive
            .starts_with("masterpiece, best quality, 1girl, silver hair"),
        "positive: {}",
        meta.positive
    );
    assert_eq!(
        meta.negative,
        "lowres, bad anatomy, blurry, watermark, text"
    );
    assert_eq!(meta.models, vec!["sd_xl_base_1.0.safetensors"]);

    assert_eq!(meta.loras.len(), 1);
    assert_eq!(meta.loras[0].name, "add_detail.safetensors");
    approx(meta.loras[0].strength_model, 0.8);
    approx(meta.loras[0].strength_clip, 0.6);
    assert_eq!(meta.loras[0].strength_text(), "0.8 / 0.6");

    let sampler = &meta.samplers[0];
    assert_eq!(sampler.sampler_name.as_deref(), Some("dpmpp_2m"));
    assert_eq!(sampler.scheduler.as_deref(), Some("karras"));
    assert_eq!(sampler.seed_text().unwrap(), "1234567890123");
    assert_eq!(sampler.steps, Some(25));
    approx(sampler.cfg, 7.0);
    assert!(meta.width > 0 && meta.height > 0);
    assert!(meta.notes.is_empty());
}

#[test]
fn comfy_hires_has_two_samplers() {
    let meta = load("comfy_hires.png");
    assert_eq!(meta.source, Source::ComfyUI);
    assert_eq!(meta.samplers.len(), 2);
    assert_eq!(meta.models, vec!["illustriousXL_v01.safetensors"]);

    let names: Vec<&str> = meta.loras.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["neon_glow.safetensors", "anime_style.safetensors"]
    );
    approx(meta.loras[0].strength_model, 0.65);
    approx(meta.loras[1].strength_model, 0.45);

    assert_eq!(meta.samplers[0].seed_text().unwrap(), "987654321");
    assert_eq!(meta.samplers[1].seed_text().unwrap(), "1122334455");
    approx(meta.samplers[1].denoise, 0.4);
}

#[test]
fn comfy_custom_flux() {
    let meta = load("comfy_custom.png");
    let sampler = &meta.samplers[0];
    assert_eq!(sampler.class_type, "SamplerCustomAdvanced");
    assert_eq!(sampler.sampler_name.as_deref(), Some("euler"));
    assert_eq!(sampler.scheduler.as_deref(), Some("simple"));
    assert_eq!(sampler.steps, Some(28));
    approx(sampler.cfg, 3.5);
    assert_eq!(sampler.seed_text().unwrap(), "424242");
    assert_eq!(meta.models, vec!["flux1-dev.safetensors"]);
    assert_eq!(meta.vae, "ae.safetensors");
}

#[test]
fn comfy_chinese_prompt() {
    let meta = load("comfy_chinese.png");
    assert_eq!(
        meta.positive,
        "杰作，最佳品质，银发蓝眼的少女，白色连衣裙，向日葵花田，柔和光线"
    );
}

#[test]
fn webui_parameters() {
    let meta = load("a1111.png");
    assert_eq!(meta.source, Source::A1111);
    assert!(meta.positive.starts_with("masterpiece, 1girl, red kimono"));
    assert_eq!(meta.negative, "lowres, bad hands, blurry, watermark");

    let sampler = &meta.samplers[0];
    assert_eq!(sampler.sampler_name.as_deref(), Some("DPM++ 2M"));
    assert_eq!(sampler.scheduler.as_deref(), Some("Karras"));
    assert_eq!(sampler.seed_text().unwrap(), "271828");
    assert_eq!(sampler.steps, Some(28));
    approx(sampler.cfg, 6.5);

    assert_eq!(meta.clip_skip, Some(2));
    assert_eq!(meta.models, vec!["sd_xl_base_1.0"]);

    let names: Vec<&str> = meta.loras.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, vec!["sakura_style", "detail_tweak"]);
    approx(meta.loras[0].strength_model, 0.7);
    approx(meta.loras[1].strength_model, 0.4);
}

#[test]
fn plain_png_has_no_parameters() {
    let meta = load("plain.png");
    assert_eq!(meta.source, Source::Unknown);
    assert!(!meta.has_metadata());
    assert_eq!(meta.positive, "");
    assert!(!meta.notes.is_empty());
}

#[test]
fn real_image_with_concatenated_text() {
    let meta = load("2026-01-08-101925_hassakuXLIllustrious_v13StyleB_1627677155.png");
    assert_eq!(meta.source, Source::ComfyUI);
    assert_eq!(meta.positive.chars().count(), 274);
    assert_eq!(meta.negative.chars().count(), 111);
    assert_eq!(
        meta.models,
        vec!["IL\\hassakuXLIllustrious_v13StyleB.safetensors"]
    );
    assert_eq!(meta.loras.len(), 1);
    assert_eq!(meta.loras[0].name, "IL\\GBF_Illustrious.safetensors");
    approx(meta.loras[0].strength_model, 0.5);

    let sampler = &meta.samplers[0];
    assert_eq!(sampler.sampler_name.as_deref(), Some("euler_ancestral"));
    assert_eq!(sampler.scheduler.as_deref(), Some("normal"));
    assert_eq!(sampler.seed_text().unwrap(), "1627677155");
    assert_eq!(sampler.steps, Some(20));
    approx(sampler.cfg, 8.0);
    approx(sampler.denoise, 0.5);
}

#[test]
fn real_image_with_zero_out_negative() {
    let meta = load("avatar_cyber_00004_.png");
    assert_eq!(meta.source, Source::ComfyUI);
    assert_eq!(meta.positive.chars().count(), 957);
    assert!(meta.negative.is_empty());
    assert_eq!(
        meta.models,
        vec!["Krea2\\krea2TurboOfficialComfy_krea2TurboBf16.safetensors"]
    );
    assert_eq!(meta.vae, "qwen_image_vae.safetensors");
    assert!(meta.loras.is_empty());

    let sampler = &meta.samplers[0];
    assert_eq!(sampler.sampler_name.as_deref(), Some("er_sde"));
    assert_eq!(sampler.scheduler.as_deref(), Some("simple"));
    assert_eq!(sampler.seed_text().unwrap(), "1806789522");
    assert_eq!(sampler.steps, Some(8));
    approx(sampler.cfg, 1.0);
    approx(sampler.denoise, 1.0);
    assert!(meta
        .notes
        .iter()
        .any(|note| note.contains("ConditioningZeroOut")));
}

#[test]
fn long_names_fixture() {
    // Layout regression: very long model / LoRA names must still parse and be
    // rendered inside the side panel (see dev/make_long_names.py).
    let meta = load("long_names.png");
    assert_eq!(meta.source, Source::ComfyUI);
    assert_eq!(meta.models.len(), 1);
    assert!(meta.models[0].ends_with("for_testing.safetensors"));
    assert_eq!(meta.loras.len(), 1);
    assert!(meta.loras[0].name.contains("super_long_lora_name_test"));
    approx(meta.loras[0].strength_model, 0.8);
    approx(meta.loras[0].strength_clip, 0.6);
    assert_eq!(meta.samplers[0].seed_text().unwrap(), "202610182026101");
}

/// ComfyUI workflows with custom nodes: `SamplerCustomAdvanced` + `CFGGuider`
/// feeding `ImpactWildcardEncode` text nodes, and a `KSamplerSelect` that must
/// not be mistaken for the real sampler.
#[test]
fn parses_custom_sampler_with_wildcard_nodes() {
    let graph: serde_json::Value = serde_json::from_str(
        r#"{
          "82":  {"class_type":"SamplerCustomAdvanced","inputs":{"guider":["103",0]}},
          "103": {"class_type":"CFGGuider","inputs":{"positive":["167",2],"negative":["168",2]}},
          "167": {"class_type":"ImpactWildcardEncode","inputs":{"populated_text":"1girl, silver hair","wildcard_text":"ignored"}},
          "168": {"class_type":"ImpactWildcardEncode","inputs":{"populated_text":"lowres, bad anatomy"}},
          "166": {"class_type":"Checkpoint Loader with Name (Image Saver)","inputs":{"ckpt_name":"ntrMIX.safetensors"}},
          "217": {"class_type":"KSamplerSelect","inputs":{"sampler_name":["169",0]}}
        }"#,
    )
    .unwrap();

    let mut meta = ImageMeta::default();
    comfy::parse_comfy_prompt(&graph, &mut meta).unwrap();

    assert_eq!(meta.positive, "1girl, silver hair");
    assert_eq!(meta.negative, "lowres, bad anatomy");
    assert_eq!(meta.models, vec!["ntrMIX.safetensors"]);
}

/// Real world sample: the graph stores text in `ImpactWildcardEncode` and the
/// sampler never exposes steps/CFG/seed/sampler name directly, so those must be
/// merged from the same file's A1111 `parameters` block.
#[test]
fn parses_ntrmix_sample() {
    let path = std::path::Path::new(
        r"D:\ComfyUI\Outputs\Backup\2024-12-31\2024-12-31-205233_ntrMIXIllustriousXL_v40_694162098710117.png",
    );
    if !path.exists() {
        eprintln!("ntrMIX sample not present on this machine, skipped");
        return;
    }

    let meta = parser::load_meta(path).expect("sample should parse");
    assert_eq!(meta.source, Source::ComfyUI);
    assert!(meta.positive.contains("red_eyes"));
    assert!(meta.negative.contains("lowres"));
    assert_eq!(meta.models, vec!["ntrMIXIllustriousXL_v40.safetensors"]);

    let sampler = &meta.samplers[0];
    assert_eq!(
        sampler.sampler_name.as_deref(),
        Some("dpmpp_2m_dy_cfg_pp_ays+")
    );
    assert_eq!(sampler.steps, Some(25));
    approx(sampler.cfg, 6.0);
    assert_eq!(sampler.seed_text().unwrap(), "694162098710117");
}
