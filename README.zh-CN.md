# ComfyPNG 查看器

[![License: MIT](https://img.shields.io/badge/license-MIT-yellow.svg)](LICENSE)
[![Platform: Windows](https://img.shields.io/badge/platform-Windows%2010%2F11%20x64-0078d4.svg)](#系统要求)

[English](README.md) | **简体中文**

一个轻量、快速的 Windows 小工具：打开 ComfyUI 或 Automatic1111 / WebUI 生成的图片，
一眼看到正向 / 负向提示词、模型、LoRA、采样器参数。不用浏览器、不用 Python、不用起服务。

![ComfyPNG 查看器](docs/screenshot-usage.png)

## 功能

- **怎么打开都行**：拖拽图片进窗口、`Ctrl+O`、`Ctrl+V`（粘贴资源管理器里复制的文件），
  或直接 `ComfyPNG.exe "C:\图片路径.png"`。
- **ComfyUI 节点图**：KSampler / KSamplerAdvanced / SamplerCustom(Advanced)、Flux guider 链、
  rgthree Power Lora Loader、CR 文本拼接、ConditioningZeroOut、多采样器（高清修复）工作流。
- **Automatic1111 / WebUI**：PNG 文本块里的 `parameters`，或 JPEG 的 EXIF `UserComment`，
  包括 `<lora:名称:权重>` 标签。
- **点一下就复制**：模型名、Seed、采样器、LoRA 等参数值都可以点击复制。
- **深色界面**：无边框窗口 + 紧凑标题栏，记住窗口大小和位置，提示词字号可调。
- **速度快**：只读 PNG 文本块 / JPEG 头，不解码整张图；缩略图才解码一次。

![启动界面](docs/screenshot-empty.png)

## 系统要求

Windows 10 / 11（x64）。免安装、无运行时依赖，单个 5.5 MB 的 exe。

## 下载

到 [Releases](https://github.com/ibelwang/comfy-png-inspector/releases) 页面下载最新的
`ComfyPNG.exe`，双击即用。

## 使用说明

| 操作 | 方式 |
| --- | --- |
| 打开图片 | 拖拽、点击虚线区域（空状态）、`Ctrl+O`，或命令行传路径 |
| 粘贴文件 | `Ctrl+V` |
| 清空 | `Esc` |
| 复制参数 | 点击参数值 |
| 复制提示词 | 提示词右上角的「复制」按钮 |
| 调整提示词字号 | 「复制」旁边的 `−` / `+`（10–24，会记住） |
| 打开原图 | 双击缩略图 |
| 换一张图 | 单击缩略图 |

## 从源码构建

```powershell
cargo build --release
```

需要带 MSVC 链接器的稳定版 Rust 工具链。产物为
`target\release\comfy-png-inspector.exe`。

`build.rs` 会用 Windows SDK 的资源编译器（`rc.exe`）把 `assets/icon.ico` 编进 exe；
如果找不到 `rc.exe`，构建仍会成功，只是文件没有图标。

运行解析测试：

```powershell
cargo test
```

## 支持的元数据

| 来源 | 读取位置 | 提取内容 |
| --- | --- | --- |
| ComfyUI | `prompt` tEXt / iTXt 文本块（含 zlib 压缩的 iTXt） | 提示词（沿采样器节点图追踪）、模型 / UNET、LoRA 及强度、VAE、Clip skip、采样器、调度器、Seed、步数、CFG、降噪 |
| A1111 / WebUI | `parameters` PNG 文本块，或 JPEG EXIF `UserComment` | 正向 / 负向提示词、采样器、调度器、步数、CFG、Seed、尺寸、模型、Clip skip、`<lora:名称:权重>` |

图片没有元数据时，界面会明确提示，而不是瞎猜。

## 隐私

全部本地处理：程序不发起任何网络请求，唯一写入的是
`%LOCALAPPDATA%\ComfyPNG\settings.json`（窗口位置 / 上次目录 / 提示词字号）。

## 许可证

MIT，详见 [LICENSE](LICENSE)。

## 致谢

- 深色配色参考了 [ComfyUI](https://comfy.org) 官网。
- 本工具为第三方作品，与 ComfyUI、Automatic1111 官方无隶属或背书关系。
