"""Compare Python and Rust parser output line by line."""

import json
import sys
from pathlib import Path

EPS = 1e-9


def load(path: str) -> dict[str, dict]:
    out: dict[str, dict] = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        data = json.loads(line)
        out[Path(data["path"]).name] = data
    return out


def diff(py, rs, key: str, problems: list[str]) -> None:
    if isinstance(py, dict) and isinstance(rs, dict):
        for k in sorted(set(py) | set(rs)):
            if k == "path":
                continue  # Windows vs Unix separators only
            diff(py.get(k), rs.get(k), f"{key}.{k}", problems)
        return
    if isinstance(py, list) and isinstance(rs, list):
        if len(py) != len(rs):
            problems.append(f"{key}: list length {len(py)} vs {len(rs)}")
            return
        for i, (a, b) in enumerate(zip(py, rs)):
            diff(a, b, f"{key}[{i}]", problems)
        return
    if isinstance(py, (int, float)) and isinstance(rs, (int, float)) and not isinstance(py, bool):
        if abs(float(py) - float(rs)) > EPS:
            problems.append(f"{key}: {py!r} vs {rs!r}")
        return
    if py != rs:
        problems.append(f"{key}: {py!r} vs {rs!r}")


def main() -> int:
    py = load(sys.argv[1])
    rs = load(sys.argv[2])
    problems_total = 0
    for name in sorted(py):
        if name not in rs:
            print(f"[MISSING] {name}")
            problems_total += 1
            continue
        problems: list[str] = []
        diff(py[name], rs[name], name, problems)
        if problems:
            problems_total += len(problems)
            print(f"[DIFF] {name}")
            for problem in problems:
                print(f"    {problem}")
        else:
            print(f"[OK]   {name}")
    return 1 if problems_total else 0


if __name__ == "__main__":
    sys.exit(main())
