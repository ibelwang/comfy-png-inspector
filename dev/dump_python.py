"""Dump the Python reference parser's output as JSON lines (ground truth).

Point COMFY_PNG_PY_REF at the folder that contains the old `comfymeta/` package
(the Python implementation this viewer was ported from).
"""

import json
import os
import sys
from pathlib import Path

OLD = Path(
    os.environ.get(
        "COMFY_PNG_PY_REF",
        Path(__file__).resolve().parent.parent / "reference-python",
    )
)
sys.path.insert(0, str(OLD))

from comfymeta import parser  # noqa: E402


def to_dict(m) -> dict:
    return {
        "path": str(m.path),
        "source": m.source,
        "positive": m.positive,
        "negative": m.negative,
        "models": m.models,
        "loras": [
            {
                "name": l.name,
                "strength_model": l.strength_model,
                "strength_clip": l.strength_clip,
                "node": l.node,
            }
            for l in m.loras
        ],
        "samplers": [
            {
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
            }
            for s in m.samplers
        ],
        "vae": m.vae,
        "clip_skip": m.clip_skip,
        "has_workflow": m.has_workflow,
        "notes": m.notes,
        "raw_parameters": m.raw_parameters,
    }


def main() -> None:
    for arg in sys.argv[1:]:
        try:
            meta = parser.load_meta(Path(arg))
        except Exception as exc:  # noqa: BLE001
            print(json.dumps({"path": arg, "error": str(exc)}, ensure_ascii=False))
            continue
        print(json.dumps(to_dict(meta), ensure_ascii=False))


if __name__ == "__main__":
    main()
