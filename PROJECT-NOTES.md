# ComfyPNG 查看器 —— Python 版进度 & Rust 重构说明

> 交接日期：2026-10-18
> 本工作区：`D:\Work\Rust-lab`（DSH 的独立工作区）
> Rust 新项目：`D:\Work\Rust-lab\comfy-png-inspector\`（本目录）
> Python 旧项目：`C:\Users\ibel_\Documents\deepseek-harness\default-workspace\comfy-png-inspector\`
>   （在另一个 DSH 工作区里，本工作区可能读不到。解析规则已完整写在第 4 节，
>    不依赖旧源码；要对照时可以把旧项目复制过来，或在旧工作区的会话里查看。）
> 测试图：本目录 `testdata/`（已复制好，见第 9 节）

---

## 1. 产品目标

打开或拖入一张 ComfyUI / WebUI 生成的图片，**一眼看到**：

- 左边：缩略图 + 文件名 / 尺寸 / 大小
- 中间：正向提示词、负向提示词（可一键复制，长文本可滚动）
- 右边：模型、LoRA 名称与强度、采样器 / 调度器 / Seed / 步数 / CFG / 降噪 / VAE / Clip skip

单文件 exe，双击即用。已交付 Python 版（v1.0，2026-10-18），用户验收通过。
**Rust 版 v0.1 也已实现**（2026-10-18，同一套解析规则 1:1 移植，见第 11 节）。

---

## 2. 当前版本已实现的功能

**输入方式**
- 拖拽图片进窗口（单文件；多文件取第一个）
- 「打开图片」按钮 / `Ctrl+O`
- `Ctrl+V` 粘贴资源管理器里复制的文件（CF_HDROP）
- 命令行参数：`ComfyPNG查看器.exe "路径.png"` 直接打开

**显示**
- 缩略图按窗口自适应缩放，带 1px 边框，双击用系统程序打开原图
- 已加载图片后**单击缩略图** → 弹出选择图片对话框（320ms 延迟区分双击）
- 提示词：只读文本框，可选中复制，带滚动条和「复制」按钮，右键菜单（复制全部/选中）
- 参数值（模型、Seed、采样器、LoRA…）**点击即复制**，底部状态栏 Toast 反馈
- 多采样器（高清修复两段）分组列出：`采样参数 1/2`、`2/2`
- 来源徽章：`ComfyUI`（蓝）/ `WebUI / A1111`（紫）/ `无生成参数`（橙）
- 无参数或解析失败时，右下角黄色「提示」卡片说明原因
- 设置：记住上次加载图片的文件夹，下次打开对话框默认进入

**已覆盖的解析场景**
- ComfyUI 标准 SDXL 工作流
- 高清修复（KSampler + KSamplerAdvanced 双采样）
- rgthree Power Lora Loader（dict 形式）
- Flux / SamplerCustomAdvanced 链（guider / sampler / sigmas / noise）
- CR Text / Lora Prompt Concatenation / CR Text Concatenate 拼接的提示词
- ConditioningZeroOut（Krea2 / Flux 空负向）
- SAM / YOLO / bbox 等辅助模型不混入「模型」
- WebUI / A1111 `parameters`（含 `<lora:name:strength>`）
- 中文提示词
- 无元数据图片

---

## 3. Python 版架构（供 Rust 参考）

```
comfy-png-inspector/
├── main.py                 # 入口：DPI、崩溃日志、logging、App.run()
├── comfymeta/
│   ├── parser.py           # PNG 文本块 + ComfyUI 节点图 + A1111 解析
│   ├── ui.py               # Tkinter 三栏界面 + 自定义消息循环
│   ├── win_dnd.py          # 拖拽（WNDPROC 子类化）、剪贴板、KnownFolder
│   └── config.py           # settings.json（last_dir）
├── launcher-fx/Launcher.cs # .NET Framework 自解压启动器
├── build_all.ps1           # PyInstaller onedir → payload.zip → csc
└── dev/                    # 样本生成 + 测试脚本
```

**分发方式（Python 版的特殊之处）**：PyInstaller `--onefile` 在本机无法创建临时目录，所以改成
`--onedir` + 内嵌 zip + .NET Framework 启动器。启动器按 **payload 的 SHA256** 生成缓存目录
`.comfypng/<hash>/`，启动前杀掉同名旧进程。**Rust 版不需要这一层**：直接 `cargo build --release`
出一个原生 exe 即可。

---

## 4. 解析规则（Rust 版必须 1:1 移植）

Python 参考实现：`C:\Users\ibel_\Documents\deepseek-harness\default-workspace\comfy-png-inspector\comfymeta\parser.py`
（若该工作区不可读，按本节规则直接实现即可，规则已完整）

### 4.1 PNG 文本块

- 校验 PNG 签名 `89 50 4E 47 0D 0A 1A 0A`
- 逐 chunk 读取；遇到 `tEXt` / `iTXt` 收集 `{keyword: text}`，其余 chunk（含 IDAT）**seek 跳过**
- `tEXt`：`keyword\0value`；value 先按 UTF-8 解码，失败再按 latin-1
- `iTXt`：`keyword\0` + 压缩标志(1) + 压缩方法(1) + 语言标签 `\0` + 翻译关键词 `\0` + 文本；
  压缩标志为 1 时文本是 **zlib** 压缩的（Rust 可用 `flate2`）
- ComfyUI 的 `prompt` 是 JSON 字符串（`ensure_ascii`），`workflow` 是 UI 节点图（只标记存在，不解析）
- WebUI 的 `parameters` 是纯文本

### 4.2 ComfyUI prompt 节点图

输入是 `{ "节点id": { "class_type": "...", "inputs": {...} } }`。
链接的表示是 `[ "目标节点id", 输出序号 ]`。

**节点名规范化**：转小写，去掉所有非字母数字字符。
例：`"Power Lora Loader (rgthree)"` → `powerloraloader rgthree`（去符号后 `powerloraloaderrgthree`）。

**采样器识别**（`_is_sampler_node`）：
- 精确匹配：`ksampler`、`ksampleradvanced`、`samplercustom`、`samplercustomadvanced`
- 含 `ksampler` 且不含 `select`、不含 `saver`（排除 `KSamplerSelect`、`KSampler to Image Saver`）
- **不要**把 `UltimateSDUpscale`、`DetailerForEach` 等当采样器（它们不含 ksampler）

**从采样器提取参数**（`_build_sampler`）：
- `seed` 或 `noise_seed`
- `steps`、`cfg`、`sampler_name`、`scheduler`、`denoise`、`start_at_step`、`end_at_step`
- `positive` / `negative`：沿链接追踪文本（见下）
- `model`：记录链接，用于模型链（见下）
- SamplerCustom 家族额外：
  - `guider` → CFGGuider：取 `positive` / `negative` / `cfg`
  - `sampler` → KSamplerSelect：取 `sampler_name`
  - `sigmas` → BasicScheduler：取 `scheduler` / `steps` / `denoise`
  - `noise` → RandomNoise：取 `noise_seed`

**文本追踪**（`_collect_texts` / `_collect_strings`）：
1. 从链接 `[id, 0]` 进入节点。
2. **若 class 含 `zeroout`（如 `ConditioningZeroOut`）→ 直接返回空列表**（Krea2/Flux 的空负向；否则会顺着它的输入把正向文本当成负向，这是已修 bug）。
3. 若节点是文本编码器（class 含 `textencode`，或含 `textencoder` 且不含 `loader`）：
   - 依次取 `text`、`prompt`、`text_g`、`text_l` 中**非空字符串**；
   - 都没有、且 `text` 是链接 → 递归 `_collect_strings`。
4. 其他节点：遍历所有 inputs：
   - 字符串值：若键不在 metadata 排除表 → 收集；
   - 链接值：递归 `_collect_texts`。

**`_collect_strings`（拼接节点专用）**：
- 入参可能是字符串（非空则返回）或链接；
- 链接节点：遍历 inputs，**字符串值直接收集**（排除 metadata 键），**链接递归**；
- 用于穿过 `CR Text` → `Lora Prompt Concatenation` → `CR Text Concatenate`（`text1` / `text2`）。
- **metadata 排除键**（这些是配置字段，不是提示词）：
  `model_name, ckpt_name, lora_name, vae_name, clip_name, unet_name, sampler_name,
   scheduler, filename, path, extension, labels, detection_hint, operation, mode,
   mode_type, seam_fix_mode, resize_mode, pad_color, crop_position, precision,
   font_name, font_color_hex, aspect_ratio, swap_dimensions, upscale_method, channel`
- 去重保序，多个文本用 `\n\n` 连接。

**正向 / 负向**：
- 主采样器 = 第一个 `positive` 或 `negative` 非空的采样器（否则取第一个采样器）
- `positive` / `negative` 各自追踪
- 若正向为空 → 兜底：收集**所有文本编码器**的文本，排除已知负向文本集合，作为正向，并加提示「请核对」
- 若正负都为空 → 按节点顺序推断（第一个正向，其余负向），加提示

**模型收集**（`_collect_models_chain`，优先）：
- 从每个采样器的 `model` 链接开始，沿节点的 `model` 输入向上走；
- 节点 inputs 含 `ckpt_name` / `unet_name` / `model_name` / `checkpoint_name` / `gguf_name` 之一 → 收集；
- 避免死循环（seen 集合、深度上限）。

**模型兜底**（`_collect_models`，链式找不到时）：
- 遍历所有节点，任何 inputs 有上述字段就收集；
- **排除** class 含 `lora`、`yolo`、`segment`、`detector`、`bbox`、`samloader`、`sam2` 的节点
  （SAM / YOLO / bbox 等辅助加载器不能算主模型）。

**LoRA**（`_collect_loras`）：
- class 含 `lora` 的节点：
  - 若 `inputs.lora_name` 是字符串 → 收集
    `name`、`strength_model`（缺省用 `strength`）、`strength_clip`（缺省用 `strength`）
  - 否则遍历 inputs 的 dict 值（rgthree 风格）：
    `on` 不为 False，且 `lora` 或 `name` 是字符串 → 收集，
    `strength` / `strength_clip`
- 强度显示：`0.8 / 0.6`（模型/CLIP 不同时）；相同时只显示一个

**VAE**：class 含 `vaeloader` → `vae_name`
**Clip skip**：class 含 `clipsetlastlayer` → `abs(stop_at_clip_layer)`

### 4.3 WebUI / A1111

- 从 `parameters` 文本（PNG chunk 或 JPEG EXIF UserComment）
- 从后往前找第一行匹配 `^\s*Steps:\s*\d`
- 该行之前是提示词区；`Negative prompt:` 之后是负向
- 参数行用 key-value 正则解析（值可带引号）
- 关键字段：`Steps`、`Sampler`（如 `DPM++ 2M Karras`，拆成采样器 + 调度器）、
  `CFG scale`、`Seed`、`Size`（`WxH`）、`Model`、`Clip skip`、`Denoising strength`
- 正向/负向里的 `<lora:name:strength>` 提取为 LoRA（强度默认 1.0）

### 4.4 图片尺寸 / 格式

Python 用 PIL 读头。Rust 用 `image` crate 的 `ImageReader` 或 `png` crate 读 IHDR 即可，**不要解码像素**（大图快）。

---

## 5. UI 行为（Rust 版建议照搬）

- 窗口 1200×790 逻辑像素起步，最小 1020×660，按 DPI 缩放；屏幕居中
- 三栏：左 320px、中弹性、右 380px（grid 列 minsize）
- 左栏：卡片「预览」+ 自适应缩略图 + 文件名/尺寸/大小/格式 + 提示「单击换图 · 双击用默认程序打开」
- 中栏：正向 3 份高度、负向 2 份高度；两个只读文本框 + 复制按钮
- 右栏：可滚动；卡片依次「模型」「LoRA」「采样参数」（可能多张）「其他」（VAE/Clip skip）「提示」（黄色，仅异常时）
- 顶部：logo + 标题 + 「打开图片」「粘贴」「清空」+ 来源徽章
- 底部：状态栏（就绪/文件名/复制 Toast）
- 空状态：居中卡片，提示拖拽 + 「打开图片文件」按钮 + 快捷键说明
- 快捷键：`Ctrl+O` 打开、`Ctrl+V` 粘贴文件、`Esc` 清空
- 复制：直接写系统剪贴板（Rust 用 `arboard`），复制后状态栏提示「已复制：前 28 字」
- 颜色：背景 `#f1f4f9`、卡片白、主色 `#2f6fed`、文字 `#1e2430`、次要 `#78829a`

---

## 6. 踩过的坑（Rust 版大多不会遇到，但要知道）

| 坑 | Python 版原因 | Rust 版情况 |
| --- | --- | --- |
| 拖拽导致窗口消失 | Tk `mainloop()` 释放 GIL，ctypes WNDPROC 回调崩溃 | **不会**，eframe 原生 `DroppedFile` |
| 点击/滚动条/关闭卡几秒 | 手写消息泵只 DispatchMessage，Tk 窗口事件队列不分发 | **不会**，egui 事件循环独立 |
| 启动空白 | 初始布局 idle 未执行 | 一般不会，egui 每帧重绘 |
| Canvas/Text 撑爆布局 | Tk 默认 Canvas 378px、Text 80×24 | **不会**，egui 无这种默认尺寸 |
| 复制后粘贴是旧内容 | Tk 剪贴板需要事件循环响应 OLE | **不会**，arboard 直接写 |
| 改了代码用户仍跑旧版 | 启动器按人工版本号缓存 | **不会**，单 exe 无缓存层 |
| SmartScreen 弹窗 | 未签名 + 自解压 + 内嵌 30MB | 仍可能弹（未签名），但无自解压层，概率低一些 |
| 中文字体 | Tk 用系统字体 | **要注意**：egui 默认不含中文字形，需加载系统字体（见第 7 节） |
| 配置/日志路径 | 环境变量可能缺失 | 用 `dirs` / `known-folders`，不依赖环境变量 |

---

## 7. Rust 重构建议

**GUI 框架**：`eframe` + `egui`（简单、单文件、跨平台、原生拖拽）。

**推荐 crates**

| 用途 | crate |
| --- | --- |
| GUI | `eframe` / `egui` |
| JSON | `serde`、`serde_json` |
| PNG tEXt/iTXt | `png`（或手写 chunk 解析） |
| zlib（iTXt） | `flate2` |
| 缩略图 | `image` |
| 剪贴板 | `arboard` |
| 文件对话框 | `rfd` |
| 配置目录 | `dirs`（`dirs::data_local_dir()`） |
| 错误处理 | `anyhow` / `thiserror` |
| 编码 | `encoding_rs`（latin-1 解码） |

**模块划分建议**

```
src/
├── main.rs        # eframe 启动、窗口图标
├── app.rs         # App 状态：当前图片、meta、缩略图纹理、toast、设置
├── ui.rs          # 三栏布局绘制
├── parser/
│   ├── mod.rs     # ImageMeta / LoraInfo / SamplerInfo
│   ├── png.rs     # tEXt/iTXt
│   ├── comfy.rs   # 节点图解析
│   └── a1111.rs
└── settings.rs    # last_dir 持久化
```

**中文字体**（关键）：egui 默认字体不含中文。启动时加载系统字体：

```rust
let mut fonts = egui::FontDefinitions::default();
if let Ok(data) = std::fs::read(r"C:\Windows\Fonts\msyh.ttc") {
    fonts.font_data.insert("msyh".into(), egui::FontData::from_owned(data).into());
    fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap()
        .insert(0, "msyh".into());
}
ctx.set_fonts(fonts);
```

（`msyh.ttc` 是集合字体，egui 可能只取第一个 face；若渲染异常，改用 `msyhbd.ttc` 或 `simhei.ttf`。）

**拖拽**：

```rust
let dropped: Vec<egui::DroppedFile> = ctx.input(|i| i.raw.dropped_files.clone());
if let Some(file) = dropped.first() {
    if let Some(path) = &file.path { load_image(path); }
}
```

**文件对话框**：`rfd::FileDialog::new().set_directory(&last_dir).pick_file()`

**设置**：`dirs::data_local_dir()` + `ComfyPNG/settings.json`，字段 `{ "last_dir": "..." }`。

**打包**：
```toml
[profile.release]
lto = true
opt-level = "s"
strip = true
```
`cargo build --release` → `target/release/<名字>.exe`（约 5–15 MB，视字体/图标）。

**图标**：eframe 的 `ViewportBuilder::with_icon(egui::IconData { ... })`，把现有 `assets/icon.ico` 转成 RGBA 数组。

---

## 8. 时间线

- 2026-10-18：Python 版 v1.0 完成并验收（拖拽、解析、复制、设置、启动器全部通过）
- 2026-10-18：Rust 重构项目区建立，交接文档完成

---

## 9. 测试样本（`testdata/`）

| 文件 | 来源 | 期望结果 |
| --- | --- | --- |
| `comfy_standard.png` | 构造 | 正向 `masterpiece, best quality, 1girl, silver hair...`；负向 `lowres, bad anatomy, blurry, watermark, text`；模型 `sd_xl_base_1.0.safetensors`；LoRA `add_detail` 0.8/0.6；`dpmpp_2m`/`karras`/seed 1234567890123/25 步/CFG 7 |
| `comfy_hires.png` | 构造 | 2 个采样器；LoRA `neon_glow` 0.65、`anime_style` 0.45；模型 `illustriousXL_v01`；seed 987654321 / 1122334455 |
| `comfy_custom.png` | 构造 | `SamplerCustomAdvanced`；`euler`/`simple`/28 步/CFG 3.5/seed 424242；模型 `flux1-dev`；VAE `ae` |
| `comfy_chinese.png` | 构造 | 正向 `杰作，最佳品质，银发蓝眼的少女，白色连衣裙，向日葵花田，柔和光线` |
| `a1111.png` | 构造 | WebUI；`DPM++ 2M`/`Karras`/seed 271828/28 步/CFG 6.5；LoRA `sakura_style` 0.7、`detail_tweak` 0.4；Clip skip 2 |
| `plain.png` | 构造 | 无参数（`source=unknown`，显示提示） |
| `2026-01-08-...png` | 用户真实图 | 正向 274 字（CR Text 拼接）；负向 111 字；模型 `IL\hassakuXLIllustrious_v13StyleB.safetensors`；LoRA `IL\GBF_Illustrious.safetensors` 0.5；1 个主采样器 `euler_ancestral`/`normal`/seed 1627677155/20 步/CFG 8/降噪 0.5 |
| `avatar_cyber_00004_.png` | 用户真实图（Krea2） | 正向 957 字；负向为空（ConditioningZeroOut）；模型 `Krea2\krea2TurboOfficialComfy_krea2TurboBf16.safetensors`；`er_sde`/`simple`/seed 1806789522/8 步/CFG 1/降噪 1；VAE `qwen_image_vae` |
| `long_names.png` | 构造（`dev/make_long_names.py`） | 超长模型 / LoRA 文件名 + 18 位 seed；右侧面板不应被撑破、数值不应被挤出窗口 |

---

## 10. 验收清单（Rust 版）

- [ ] 拖拽 PNG 进窗口 → 三栏正常显示（实现完成，待实机拖拽验证）
- [ ] 「打开图片」+ `Ctrl+O` + `Ctrl+V` 粘贴文件（实现完成，待实机验证）
- [ ] 复制正向/负向提示词，粘贴到记事本内容正确（arboard，实现完成，待实机验证）
- [ ] 点击模型/Seed/采样器值可复制（实现完成，待实机验证）
- [x] 长提示词可滚动、可选中（截图：957 字提示词正常滚动）
- [ ] 单击缩略图弹选择框；双击打开原图（实现完成，待实机验证）
- [ ] 记住上次文件夹（实现完成，待实机验证）
- [x] 多采样器分组显示（`comfy_hires` 两张采样参数卡）
- [x] `comfy_hires`（双采样）、`comfy_custom`（Flux）、`2026-01-08`（拼接）、`avatar_cyber`（ZeroOut）四张图解析正确
- [x] 无元数据图片显示提示（`plain.png` 黄色提示卡）
- [x] 单 exe 可运行，启动 &lt; 1 秒（5.7 MB，双击后窗口即刻出现）
- [x] 中文字体正常显示（`simhei.ttf`，中文提示词与界面均为中文）

解析正确性另有一层硬验证：8 张图与 Python 旧解析器逐字段比对全部一致（第 11 节）。

---

## 11. Rust 版 v0.1（2026-10-18）

**状态**：功能全部实现；解析层与 Python 版逐字段一致；界面截图验收 7 个状态通过；待用户实机验收交互项。

### 结构

```
src/
├── main.rs          # eframe 启动、窗口图标（assets/icon.ico）
├── app.rs           # 状态：图片、纹理、设置、Toast、延迟单击
├── ui.rs            # 三栏布局、卡片、可复制值、提示卡
├── platform.rs      # CF_HDROP 文件剪贴板（Ctrl+V）
├── settings.rs      # %LOCALAPPDATA%\ComfyPNG\settings.json
├── parser/{mod,png,comfy,a1111,exif}.rs
└── bin/dump.rs      # 解析结果 JSON 导出（与 Python 比对用）
tests/parser_tests.rs      # 8 个验收用例
dev/dump_python.py         # 旧 Python 解析器导出（ground truth）
dev/compare.py             # 逐字段比对
dev/capture.ps1            # 启动 → 截窗 → 关闭（界面验收）
```

### 验证结果

- `cargo test`：9/9 通过（对应第 9 节样本表）。
- `dev/compare.py dev/python-out.jsonl dev/rust-out.jsonl`：8/8 `[OK]`，除路径分隔符外**全部字段一致**
  （正向/负向全文、采样器全部字段、模型、LoRA 强度、VAE、clip skip、提示语）。
- 界面截图：`dev/shot-*.png`（standard / a1111 / chinese / custom / avatar / plain / empty / longnames）。
- 发布版 exe 5.7 MB，启动后窗口立刻出现（无首帧空白）。

### 2026-10-18 修复：长文件名把右侧面板撑破

**现象**（用户实测 `avatar_yubei_00004_.png`）：右侧面板里 Seed / 采样器 / 调度器的值被挤到窗口外，LoRA 文件名横向溢出，拖宽窗口也没用。
**根因**：卡片内宽由内容决定，一个不可断行的长文件名（如
`Krea2\yubei-krea2_krea2_3321279_epoch_10.safetensors`）把卡片撑宽 → 整个右栏内容跟着变宽 → 右对齐的数值被推出窗口；
而右侧面板是固定 380px，所以拖窗口无效。
**修复**：`ui.rs::card()` 同时 `set_min_width` + `set_max_width` 锁死卡片内宽；所有可复制的长文本（模型 / LoRA / VAE / 参数值）
改用 `wrap_text()` 按宽度手动断行（`TextWrapMode::Extend`），LoRA 行给强度值预留 64px。
**验证**：`dev/make_long_names.py` 生成 `testdata/long_names.png`（超长模型/LoRA 名 + 18 位 seed），
截图 `dev/shot-longnames.png` 显示全部内容在面板内；`comfy_standard.png` 复测无回归；新增 `long_names_fixture` 测试。

### 本机构建注意（重要，踩过的坑）

1. **CARGO_HOME 指向工作区内**：`D:\Work\Rust-lab\.cargo-home`。DSH 沙箱不允许写 `%USERPROFILE%\.cargo`，
   否则 `cargo` 报 `failed to write Cargo.lock / 拒绝访问`。
2. **cargo 必须在 DSH「完整权限」下运行**：
   - 受限沙箱阻断 cargo ↔ rustc 的管道通信 → `could not execute process rustc.exe ... 拒绝访问`；同样阻断测试程序的管道。
   - Windows schannel 在沙箱内 `SEC_E_NO_CREDENTIALS`（curl.exe 同样失败），首次 `cargo fetch` 需要完整权限。
3. **Windows 完整性级别**：项目目录曾残留 Medium 完整性标签（工作区根目录是 Low），沙箱低权限进程写不进去
   （表现：工作区根目录可写、`comfy-png-inspector\` 内一律 Permission denied）。修复：
   ```powershell
   icacls <dir> /grant "ibel_:(OI)(CI)F" /T
   icacls <dir> /setintegritylevel "(OI)(CI)L" /T
   ```
4. egui 0.33 小坑：`Label` 在 `Frame` 内自动换行时最后一行会向左溢出（提示卡文字跑到卡片边框外），
   改成按宽度手动断行 + `TextWrapMode::Extend` 解决（见 `ui.rs::wrap_text`）。
5. **exe 的完整性级别会拖累运行**：从工作区（Low）构建出的 exe 带 Low 标签，Windows 会让进程也以低完整性运行，
   于是写 `%LOCALAPPDATA%\ComfyPNG\settings.json` 报「拒绝访问」，窗口记忆静默失效。
   修复（构建后对新 exe 目录做一次即可，新文件会继承）：
   ```powershell
   icacls target\release /setintegritylevel "(OI)(CI)M" /T
   ```
   验证方式：启动后看 `%LOCALAPPDATA%\ComfyPNG\settings.json` 是否出现 `window_size` / `window_pos`。

### 构建命令

```powershell
$env:CARGO_HOME = 'D:\Work\Rust-lab\.cargo-home'
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
cd D:\Work\Rust-lab\comfy-png-inspector
cargo test
cargo build --release
```

### 后续可选

- 单实例、最近文件列表
- 大图缩略图缓存（目前每次打开解码一次，1024² 图 < 50 ms，暂无明显体感）
- 窗口圆角（DWM `DWMWA_WINDOW_CORNER_PREFERENCE`），目前是无边框直角

---

## 12. v0.2 界面重做（ComfyUI 深色风，2026-10-18）

设计稿：`dev/design/mockup-startup.png`、`dev/design/mockup-usage.png`（HTML 源在 `dev/design/*.html`，
用无头 Edge 渲染，`--force-device-scale-factor=2`）。用户拍板：图标方案 C、来源信息用普通灰字、提示卡不高亮。

### 配色（取自 comfy.org/download）

| 用途 | 值 | 对比度 |
| --- | --- | --- |
| 窗口背景 | `#17141B` | — |
| 卡片 | `#231F2B` | 正文 14.0:1 |
| 边框 | `#3A3246` | — |
| 主色（按钮/可点值） | `#F2FF59` | 14.8:1 |
| 正文 | `#F1EEF6` | — |
| 次要文字 | `#A79FB5` | 6.4:1 |
| 小字提示 | `#948CA3` | 5.0:1（AA） |
| 提示词正文 | `#DED9E6` | 11.7:1 |

### 行为变化

- **无边框窗口**：`with_decorations(false)`；自绘标题栏（图标 + 标题 + 「打开图片」ghost 按钮 + − / ×）。
  标题栏可拖动，双击不最大化；窗口四边/四角 6px 热区发 `BeginResize`，另外窗口保留 `WS_THICKFRAME`，原生边缘也能拉。
- **记住窗口大小和位置**：`window_size` / `window_pos` 写进 settings.json；尺寸变化在松开鼠标后立即保存，退出时再存一次。
  实测预置 `1100×700 @150,120` 启动后 `GetWindowRect` 精确恢复。
- **空状态整块可点**：虚线框内任意位置点击都会弹文件框（按钮也在框内）。
- **提示卡**改为普通卡片：与其它卡片同字号、灰字、无黄底高亮。
- **来源**：顶部不再有 ComfyUI 徽章，改为左栏一行「来源 ComfyUI」，与其它信息同样的灰色。
- 三栏宽度 300 / 弹性 / 360。

### 图标

- `assets/icon.svg`（方案 C）→ `dev/design/icon-1024.png`（无头 Edge 透明底渲染）→ `dev/make_icon.py`
  生成 `assets/icon.ico`（16/24/32/48/64/128/256）、`icon-256.png`、`icon-512.png`。
- 运行时用 `icon-256.png` 设窗口/任务栏图标；`build.rs` 用 `winres` 把 `icon.ico` 编进 exe（资源编译器取自 Windows Kits，已实测生效）。

### 测试钩子

- `COMFYPNG_DATA_DIR` 环境变量可覆盖设置目录（sandbox 内测试设置逻辑用）。

### 已知取舍

- ~~标题栏额外放了「打开图片」按钮~~（v0.2.1 已按用户要求删除）。
- ~~窗口直角~~（v0.2.1 已做 DWM 圆角）。

---

## 13. v0.2.1 细节修正（2026-10-18）

用户实机反馈后的六项改动：

1. **删掉两个「打开图片」入口**：标题栏的 ghost 按钮和空状态的大按钮都去掉了。
   现在打开图片只有三个入口：空状态整块虚线区点击、`Ctrl+O`、已加载时单击缩略图。
2. **DWM 圆角**：`DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND)`。
   eframe 的 `CreationContext` 不暴露 HWND，所以用 `FindWindowW(None, "ComfyPNG 查看器")` 拿句柄，
   首次 `update` 里调用，失败会逐帧重试（`App::corners_applied`）。
3. **标题栏图标发毛**：原因是把 256px 位图缩到 24px（无 mipmap 时只采样几个像素，圆角变直线）。
   改成**矢量绘制** `ui.rs::draw_app_mark`（圆角方块 + 图形符号），24px 下也干净。
   缩略图仍用位图，但纹理改成带 mipmap 的 `TextureOptions`，缩小后更平滑。
4. **字号整体上调**（原来偏小约 1pt）：

   | 位置 | v0.2 | v0.2.1 |
   | --- | --- | --- |
   | 卡片标题 | 12.5 | **13.5** |
   | 信息行 / 参数键值 | 11.5 | **12.5** |
   | 提示词正文 | 12.5 | **14**（可调） |
   | 小字提示 | 10.5–11 | **11.5** |
   | 窗口标题 | 13.5 | **14** |
   | 状态栏 | 11 | **11.5** |

5. **提示词字号可调**：正向/负向卡片右上角「复制」左边加了 `−  14  +` 步进器，
   范围 10–24，两个框共享同一字号，改动立即写入 `settings.json` 的 `prompt_font_size`。
   默认值/范围常量在 `app.rs`：`DEFAULT_PROMPT_FONT` / `MIN_PROMPT_FONT` / `MAX_PROMPT_FONT`。
6. 实测：预置 `prompt_font_size: 18` 启动后文本按 18 渲染；窗口几何记忆仍正常；exe 为 Medium 完整性（可写设置）。

---

## 14. 收尾：留档与清理（2026-10-18）

**成品**：`dist\ComfyPNG.exe`（5.5 MB，已试启动正常）。源码、测试、设计稿、文档全部保留。

**已删除**（共释放 2.74 GB）：
- `target/`（2.1 GB）与 `.cargo-home/`（699 MB）——构建缓存，下次构建会重新下载依赖。
- `dev/` 里的旧主题截图、日志、比对输出（`python-out.jsonl` / `rust-out.jsonl` 等）。
- `acl-recovery/`——当时的权限修复备份；修复方法已记在第 11/12 节，记录本身不再需要。

**保留**：
- `dev/*.py` 与 `dev/capture.ps1`：测试样本生成、与旧解析器比对、界面截图工具。
- `dev/design/`：设计稿 HTML 源 + mockup + 最终截图（`shot-empty.png` / `shot-usage.png` / `shot-longnames.png`）。

**以后要重新构建**（缓存已清空，第一次会比较慢）：
```powershell
$env:CARGO_HOME = 'D:\Work\Rust-lab\.cargo-home'
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
cd D:\Work\Rust-lab\comfy-png-inspector
cargo test
cargo build --release
icacls target\release /setintegritylevel "(OI)(CI)M" /T   # 否则 exe 低权限、写不了设置
copy target\release\comfy-png-inspector.exe dist\ComfyPNG.exe
```
依赖下载与编译都需要 DSH 的「完整权限」。

---

## 15. 开源发布（2026-10-18）

- 仓库：<https://github.com/ibelwang/comfy-png-inspector>（public）
- 许可证：**MIT**（`LICENSE`，版权署名 "ComfyPNG contributors"；要写个人名就改这一行）
- 首个 Release：`v0.2.1`（2026-10-18），附件 `ComfyPNG.exe`
- 第二个 Release：`v0.2.2`（2026-10-20），包含第 16 节的自定义节点解析修复；`Cargo.toml` 版本同步为 0.2.2
- 文档：`README.md`（英）、`README.zh-CN.md`（中，顶部有语言切换），截图放 `docs/`
- CI：暂未启用。`.github/workflows/ci.yml`（windows-latest 跑 `cargo test --release`）写好过，
  但 `gh` 的 OAuth token 没有 `workflow` scope，推送被 GitHub 拒绝；文件已移除。
  以后要开 CI：`gh auth refresh -s workflow` 授权后再加文件，或直接在 GitHub 网页上新建。
- `.gitignore`：`target/`、`dist/`、`reference-python/`、`*.log`（exe 走 Release，不进仓库）
- 依赖许可：eframe/egui（MIT/Apache-2.0）、image（MIT/Apache-2.0）、serde（MIT/Apache-2.0）、
  arboard（MIT/Apache-2.0）、rfd（MIT）、windows-sys（MIT/Apache-2.0）、flate2（MIT/Apache-2.0）、
  fancy-regex（MIT）、dirs（MIT/Apache-2.0）、anyhow（MIT/Apache-2.0）、winres（MIT）——全部为宽松许可，
  与 MIT 兼容；无需附带源码。`dev/design/` 里有参考 comfy.org 的截图，已在发布前删除，仅保留配色记录。
- ⚠️ `testdata/` 里有两张用户真实生成的图（`avatar_cyber_00004_.png`、`2026-01-08-…png`）已随仓库公开；
  若不想公开，换成构造样本并同步改 `tests/parser_tests.rs` 的期望值即可。
- `PROJECT-NOTES.md` 本身也公开（含本机构建踩坑记录），不想要可在发布前移除。

发版流程：
```powershell
cargo build --release
icacls target\release /setintegritylevel "(OI)(CI)M" /T
copy target\release\comfy-png-inspector.exe dist\ComfyPNG.exe
gh release create v0.2.2 dist/ComfyPNG.exe --title "v0.2.2" --notes "…"
```

---

## 16. 自定义节点提示词修复（2026-10-20）

依据 `docs/comfyui-custom-node-prompt-fix.md`（ComfyVault 的修复记录）移植，样本：
`D:\ComfyUI\Outputs\Backup\2024-12-31\2024-12-31-205233_ntrMIXIllustriousXL_v40_694162098710117.png`。

### 根因（四个叠加）

1. **采样器选错**：旧逻辑取「第一个有文本的采样器」，图上还有 `KSamplerSelect`；虽然它被排除，
   但真正的问题是 `SamplerCustomAdvanced` 的文本藏在 `CFGGuider` 后面，且自定义采样器链更深。
   现在按分数选主采样器：`SamplerCustomAdvanced(4) > KSampler(3) > SamplerCustom(2)`，有文本 +10，
   同分保留节点 id 靠前的那个。
2. **不认识 `ImpactWildcardEncode`**：文本在 `populated_text`（实际结果），`wildcard_text` 只是模板。
   现在优先取 `populated_text`，再退回 `wildcard_text` / `text`。
3. **图里有 `parameters` 也不回退**：现在 ComfyUI 解析完成后，把同文件的 A1111 `parameters`
   解析成临时结果，只补空字段（提示词、模型、LoRA、VAE、clip skip、采样器的 name/scheduler/steps/cfg/denoise/seed）；
   图里的值永远优先。补了字段会加一条提示，并把原文存进 `raw_parameters`。
4. **模型兜底把放大模型当成主模型**：`UpscaleModelLoader` 也有 `model_name`（4x_NMKD…），
   现在与 SAM/YOLO/bbox 一起排除。

另外：
- `seed` / `steps` / `cfg` 现在是「标量或沿链接取字面量」（`follow_scalar`，最多两跳），
  能读 `Seed Generator (Image Saver)`、`Cfg Literal`、`Int Literal` 这类辅助节点。
- 新增 `Image Saver` 兜底：采样器链路缺 positive/negative 时，从挂了 `positive`/`negative` 的
  `Image Saver` 节点取。

### 验证

- 新增 `parses_custom_sampler_with_wildcard_nodes`（合成图，`SamplerCustomAdvanced + CFGGuider + ImpactWildcardEncode + KSamplerSelect`）。
- 新增 `parses_ntrmix_sample`（真实样本，文件不在则跳过）：`source=comfyui`、正向含 `red_eyes`、负向含 `lowres`、
  模型 `ntrMIXIllustriousXL_v40.safetensors`、`steps=25`、`cfg=6.0`、`seed=694162098710117`、
  采样器 `dpmpp_2m_dy_cfg_pp_ays+`。
- `cargo test --release`：**11/11 通过**。
