"""Generate a test PNG whose parameters contain very long file names.

Used to check that the right-hand panel does not stretch when a LoRA or model
name is longer than the panel width.
"""

import json
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "testdata" / "long_names.png"


def chunk(kind: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def main() -> None:
    width = height = 64
    row = b"\x00" + bytes([120, 160, 210]) * width
    raw = row * height

    prompt = {
        "4": {
            "class_type": "CheckpointLoaderSimple",
            "inputs": {
                "ckpt_name": "Krea2\\yubeiKrea2Turbo_epoch_10_very_long_checkpoint_name_for_testing.safetensors"
            },
        },
        "5": {
            "class_type": "LoraLoader",
            "inputs": {
                "lora_name": "Krea2\\yubei-krea2_krea2_3321279_epoch_10_super_long_lora_name_test.safetensors",
                "strength_model": 0.8,
                "strength_clip": 0.6,
                "model": ["4", 0],
                "clip": ["4", 1],
            },
        },
        "6": {
            "class_type": "CLIPTextEncode",
            "inputs": {"text": "yubei，电影剧照质感的面部大特写，正方形构图，脸部占满整个画面。"},
        },
        "7": {
            "class_type": "ConditioningZeroOut",
            "inputs": {"conditioning": ["6", 0]},
        },
        "10": {
            "class_type": "KSampler",
            "inputs": {
                "seed": 202610182026101,
                "steps": 28,
                "cfg": 6.5,
                "sampler_name": "er_sde",
                "scheduler": "simple",
                "denoise": 1.0,
                "model": ["5", 0],
                "positive": ["6", 0],
                "negative": ["7", 0],
            },
        },
    }

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"tEXt", b"prompt\x00" + json.dumps(prompt, ensure_ascii=False).encode("utf-8"))
    png += chunk(b"IEND", b"")
    OUT.write_bytes(png)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
