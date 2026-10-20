# 修复 comfypng：ComfyUI 自定义节点的 prompt 读不出来

> 来源：ComfyVault 2026-10-20 的元数据解析修复记录。可以直接照着改 comfypng 的 `src/parser/`。

## 现象

文件：

```text
D:\ComfyUI\Outputs\Backup\2024-12-31\2024-12-31-205233_ntrMIXIllustriousXL_v40_694162098710117.png
```

PNG 里明明有 prompt，但工具读出来是空的。

## 这张图实际存了什么

PNG 里有三个 `tEXt` 块（未压缩）：

| key | 内容 |
|---|---|
| `parameters` | A1111 风格文本：正向、`Negative prompt:`、Steps 25、Sampler、CFG 6.0、Seed、Model |
| `prompt` | ComfyUI API 节点图 JSON |
| `workflow` | ComfyUI UI 节点图 JSON |

关键点：

- 正向/负向文本在 `prompt` 图里的 **`ImpactWildcardEncode`** 节点，字段是 `populated_text`（实际文本）和 `wildcard_text`（原始模板）。
- 采样器是 **`SamplerCustomAdvanced`**，通过 `CFGGuider` 连接 positive/negative。
- 图里还有一个 **`KSamplerSelect`**，它只负责选采样器名字，不是真正的采样节点。
- 没有 `CLIPTextEncode` 节点。

旧解析器的问题有三个：

1. 找采样器时先撞上 `KSamplerSelect`，它没有 positive/negative，于是文本为空。
2. 只处理 `CLIPTextEncode` 的 `text` 字段，不认识 `ImpactWildcardEncode`。
3. 看到有 `prompt` 块就直接返回，即使解析结果为空，也不会回退去读同文件的 `parameters`。

## 需要的修复

### 1. 找真正的采样器，排除 `KSamplerSelect`

```rust
fn sampler_score(node: &Value) -> i32 {
    let class = node.get("class_type").and_then(Value::as_str).unwrap_or("");
    if class.contains("SamplerCustomAdvanced") {
        4
    } else if class.contains("KSampler") && !class.contains("Select") {
        3
    } else if class.contains("SamplerCustom") {
        2
    } else {
        0
    }
}

fn find_sampler(nodes: &Map<String, Value>) -> Option<(&String, &Value)> {
    nodes
        .iter()
        .filter(|(_, node)| sampler_score(node) > 0)
        .max_by_key(|(_, node)| sampler_score(node))
}
```

### 2. 文本节点支持 `ImpactWildcardEncode`

优先 `populated_text`，没有再退回 `wildcard_text` / `text`：

```rust
fn node_text(node: &Value) -> Option<String> {
    let class = node.get("class_type").and_then(Value::as_str).unwrap_or("");
    let inputs = node.get("inputs").and_then(Value::as_object)?;
    let keys: &[&str] = if class.contains("ImpactWildcardEncode") {
        &["populated_text", "wildcard_text", "text"]
    } else if class.contains("CLIPTextEncode") {
        &["text"]
    } else {
        return None;
    };
    for key in keys {
        if let Some(text) = inputs.get(*key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}
```

递归沿链接找文本：

```rust
fn collect_node_text(
    nodes: &Map<String, Value>,
    id: &str,
    depth: usize,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
) {
    if depth > 64 || !seen.insert(id.to_string()) {
        return;
    }
    let Some(node) = nodes.get(id) else { return };
    if let Some(text) = node_text(node) {
        out.push(text);
        return;
    }
    if let Some(inputs) = node.get("inputs").and_then(Value::as_object) {
        for value in inputs.values() {
            if let Some(next) = link_id(value) {
                collect_node_text(nodes, next, depth + 1, seen, out);
            }
        }
    }
}
```

`link_id` 就是取数组第一个字符串：

```rust
fn link_id(value: &Value) -> Option<&str> {
    value.as_array()?.first()?.as_str()
}
```

### 3. 从 `SamplerCustomAdvanced → CFGGuider` 取 positive/negative

原来的采样器节点没有 positive/negative，要再跟一层 `guider`：

```rust
if let Some((_id, node)) = find_sampler(nodes) {
    if let Some(inputs) = node.get("inputs").and_then(Value::as_object) {
        meta.positive = collect_link_text(nodes, inputs.get("positive"));
        meta.negative = collect_link_text(nodes, inputs.get("negative"));
        meta.steps = as_i64(inputs.get("steps"));
        meta.cfg = as_f64(inputs.get("cfg"));
        meta.seed = as_i64(inputs.get("seed"));
        meta.sampler = as_str(inputs.get("sampler_name"));
        meta.scheduler = as_str(inputs.get("scheduler"));
    }

    if meta.positive.is_empty() || meta.negative.is_empty() {
        if let Some(guider) = node
            .get("inputs")
            .and_then(Value::as_object)
            .and_then(|inputs| inputs.get("guider"))
            .and_then(link_id)
            .and_then(|id| nodes.get(id))
        {
            if let Some(inputs) = guider.get("inputs").and_then(Value::as_object) {
                if meta.positive.is_empty() {
                    meta.positive = collect_link_text(nodes, inputs.get("positive"));
                }
                if meta.negative.is_empty() {
                    meta.negative = collect_link_text(nodes, inputs.get("negative"));
                }
                if meta.cfg.is_none() {
                    meta.cfg = as_f64(inputs.get("cfg"));
                }
            }
        }
    }
}
```

### 4. 兜底：`Image Saver` 节点的 positive/negative

有些图采样器链路不完整，但 `Image Saver` 节点直接挂了 positive/negative，也读一下：

```rust
if meta.positive.is_empty() || meta.negative.is_empty() {
    if let Some(saver) = nodes.values().find(|node| {
        node.get("class_type")
            .and_then(Value::as_str)
            .map(|class| class.contains("Image Saver"))
            .unwrap_or(false)
    }) {
        if let Some(inputs) = saver.get("inputs").and_then(Value::as_object) {
            if meta.positive.is_empty() {
                meta.positive = collect_link_text(nodes, inputs.get("positive"));
            }
            if meta.negative.is_empty() {
                meta.negative = collect_link_text(nodes, inputs.get("negative"));
            }
            if meta.sampler.is_none() {
                meta.sampler = as_str(inputs.get("sampler_name"));
            }
        }
    }
}
```

### 5. 顶层解析：`prompt` 解析失败/为空时合并 `parameters`

这是最关键的一步。不要看到 `prompt` 有内容就直接返回空结果。

```rust
pub fn parse(bytes: &[u8], ext: &str) -> ParsedMeta {
    let chunks = png_text_chunks(bytes);
    let graph = chunks.get("prompt").map(|text| parse_comfy_prompt(text));
    let params = chunks.get("parameters").map(|text| parse_a1111(text));

    let mut meta = match (graph, params) {
        (Some(mut graph), Some(fallback)) => {
            merge_missing(&mut graph, &fallback);
            graph.source = "comfyui".into();
            graph
        }
        (Some(mut graph), None) => {
            graph.source = "comfyui".into();
            graph
        }
        (None, Some(mut params)) => {
            params.source = "a1111".into();
            params
        }
        (None, None) => return ParsedMeta::unknown(),
    };

    if meta.positive.is_empty()
        && meta.negative.is_empty()
        && meta.checkpoint.is_none()
        && meta.loras.is_empty()
    {
        return ParsedMeta::unknown();
    }
    meta.recipe_key = Some(recipe_key(&meta));
    meta
}

fn merge_missing(meta: &mut ParsedMeta, fallback: &ParsedMeta) {
    if meta.positive.trim().is_empty() {
        meta.positive = fallback.positive.clone();
    }
    if meta.negative.trim().is_empty() {
        meta.negative = fallback.negative.clone();
    }
    if meta.checkpoint.is_none() {
        meta.checkpoint = fallback.checkpoint.clone();
    }
    if meta.sampler.is_none() {
        meta.sampler = fallback.sampler.clone();
    }
    if meta.scheduler.is_none() {
        meta.scheduler = fallback.scheduler.clone();
    }
    if meta.steps.is_none() {
        meta.steps = fallback.steps;
    }
    if meta.cfg.is_none() {
        meta.cfg = fallback.cfg;
    }
    if meta.seed.is_none() {
        meta.seed = fallback.seed;
    }
    if meta.loras.is_empty() {
        meta.loras = fallback.loras.clone();
    }
}
```

要点：**图内解析到的字段优先，`parameters` 只补缺失字段**；不要用 `parameters` 覆盖图里的 checkpoint/LoRA。

## 这张图修复后的预期结果

- `source = "comfyui"`
- 正向包含 `masterpiece, best quality` 和 `red_eyes`
- 负向包含 `deformed` / `lowres`
- `checkpoint = "ntrMIXIllustriousXL_v40.safetensors"`
- `steps = 25`
- `cfg = 6.0`
- `seed = 694162098710117`
- `sampler = "dpmpp_2m_dy_cfg_pp_ays+"`

注意：`parameters` 里写的是 `Model: ntrMIXIllustriousXL_v40`，图里是 `.safetensors`；按“图优先”会得到带扩展名的那个，这是预期。

## 验收用例

### 合成图 JSON 测试

```rust
#[test]
fn parses_custom_sampler_with_wildcard_nodes() {
    let graph = r#"{
      "82":  {"class_type":"SamplerCustomAdvanced","inputs":{"guider":["103",0]}},
      "103": {"class_type":"CFGGuider","inputs":{"positive":["167",2],"negative":["168",2]}},
      "167": {"class_type":"ImpactWildcardEncode","inputs":{"populated_text":"1girl, silver hair","wildcard_text":"ignored"}},
      "168": {"class_type":"ImpactWildcardEncode","inputs":{"populated_text":"lowres, bad anatomy"}},
      "166": {"class_type":"Checkpoint Loader with Name (Image Saver)","inputs":{"ckpt_name":"ntrMIX.safetensors"}},
      "217": {"class_type":"KSamplerSelect","inputs":{"sampler_name":["169",0]}}
    }"#;
    let meta = parse_comfy_prompt(graph);
    assert_eq!(meta.positive, "1girl, silver hair");
    assert_eq!(meta.negative, "lowres, bad anatomy");
    assert_eq!(meta.checkpoint.as_deref(), Some("ntrMIX.safetensors"));
}
```

### 真实文件测试（如果文件在）

```rust
#[test]
fn parses_ntrmix_sample() {
    let path = r"D:\ComfyUI\Outputs\Backup\2024-12-31\2024-12-31-205233_ntrMIXIllustriousXL_v40_694162098710117.png";
    if !std::path::Path::new(path).exists() {
        return;
    }
    let bytes = std::fs::read(path).unwrap();
    let meta = parse(&bytes, "png");
    assert_eq!(meta.source, "comfyui");
    assert!(meta.positive.contains("red_eyes"));
    assert!(meta.negative.contains("lowres"));
    assert_eq!(meta.checkpoint.as_deref(), Some("ntrMIXIllustriousXL_v40.safetensors"));
    assert_eq!(meta.steps, Some(25));
    assert_eq!(meta.cfg, Some(6.0));
    assert_eq!(meta.seed, Some(694162098710117));
    assert_eq!(meta.sampler.as_deref(), Some("dpmpp_2m_dy_cfg_pp_ays+"));
}
```

## 额外提醒

- 这张图是 **`tEXt` 未压缩块**，不需要 zlib；如果 comfypng 只读 `tEXt`，先把逻辑修好就行。
- 真正的提示词不要读 `workflow`，它是 UI 图；优先 `prompt`，再用 `parameters` 补。
- 如果 comfypng 也有“导入后缓存”或 sidecar，修完后需要加一个「重新解析/重新读取」入口，否则旧记录不会自动更新。
- 字段名和结构体名按 comfypng 自己的改，逻辑照上面即可。
