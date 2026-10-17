"""Regenerates golden/*.png with tools/compose.py of silica-icons (the reference implementation).

usage: python gen.py ICONS_DIR      (ICONS_DIR: a silica-icons checkout on amalfi-icons)
Needs pillow, numpy, resvg-py. Dynamic layers are dropped (the host draws them live), exactly as
the daemon does. Tinted goldens use the accent #d6336c.
"""
import json
import shutil
import sys
import tempfile
from pathlib import Path

icons = Path(sys.argv[1]).resolve()
sys.path.insert(0, str(icons / "tools"))
import compose  # noqa: E402

here = Path(__file__).resolve().parent
out = here / "golden"
out.mkdir(exist_ok=True)
ACCENT = "#d6336c"
LOOKS = [("light", "light", False), ("dark", "light", True), ("clear", "clear", False),
         ("clear-dark", "clear", True), ("tinted", "tinted", False), ("tinted-dark", "tinted", True)]


def static_copy(src: Path, tmp: Path) -> Path:
    dst = tmp / src.name
    shutil.copytree(src, dst)
    manifest = json.loads((dst / "icon.json").read_text())
    manifest["layers"] = [l for l in manifest["layers"] if "dynamic" not in l]
    (dst / "icon.json").write_text(json.dumps(manifest))
    return dst


jobs = [(here / "synthetic", 128, LOOKS), (here / "synthetic", 64, LOOKS[:1])]
for app in ("mustache", "terminal", "clock"):
    jobs.append((icons / "apps" / app, 128, LOOKS))
with tempfile.TemporaryDirectory() as t:
    for src, size, looks in jobs:
        d = static_copy(src, Path(t) / f"{src.name}-{size}")
        for name, appearance, dark in looks:
            img = compose.compose(d, size, dark, [], appearance=appearance, accent=ACCENT)
            img.save(out / f"{src.name}-{size}-{name}.png", optimize=True)
