import json
import os

for line in open("dev/python-out.jsonl", encoding="utf-8"):
    d = json.loads(line)
    name = os.path.basename(d["path"])
    print("==", name, "| source", d["source"])
    print("  pos_len", len(d["positive"]), "neg_len", len(d["negative"]))
    print("  models", d["models"])
    print("  loras", [(l["name"], l["strength_model"], l["strength_clip"]) for l in d["loras"]])
    print("  vae", d["vae"], "clip_skip", d["clip_skip"])
    for s in d["samplers"]:
        print("  sampler", s["class_type"], s["sampler_name"], s["scheduler"], s["seed"], s["steps"], s["cfg"], s["denoise"])
    print("  pos_head", d["positive"][:80].replace("\n", " | "))
