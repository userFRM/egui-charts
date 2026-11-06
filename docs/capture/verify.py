#!/usr/bin/env python3
"""Check the published media against their formats and size budgets."""
import json
from pathlib import Path
import subprocess
import sys

from PIL import Image

root = Path(sys.argv[1])

for ext, codec, budget in [("mp4", "h264", 9_000_000), ("webm", "av1", 7_000_000)]:
    path = root / f"docs/site/public/media/tour-1080p.{ext}"
    info = json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)
    ]))
    stream = info["streams"][0]
    assert stream["codec_name"] == codec
    assert (stream["width"], stream["height"]) == (1920, 1080)
    assert stream["r_frame_rate"] == "60/1"
    assert stream["pix_fmt"] == "yuv420p"
    if ext == "mp4":
        assert stream["profile"] == "High"
        content = path.read_bytes()
        assert content.index(b"moov") < content.index(b"mdat"), "MP4 metadata must precede media"
    assert 25 <= float(info["format"]["duration"]) <= 30
    assert path.stat().st_size <= budget, f"{path}: size budget exceeded"

for theme in ["dark", "light"]:
    for scene in ["tour", "liquidity", "drawings", "replay"]:
        path = root / f"docs/images/{scene}-{theme}.webp"
        image = Image.open(path)
        hero = scene == "tour"
        assert image.size == ((1280, 720) if hero else (1280, 800))
        assert image.info["loop"] == 0
        assert path.stat().st_size <= (6_000_000 if hero else 3_000_000), f"{path}: size budget exceeded"
        delays = []
        for index in range(image.n_frames):
            image.seek(index)
            image.load()
            delays.append(image.info["duration"])
        duration = sum(delays)
        assert (10_000 <= duration <= 12_000) if hero else (6_000 <= duration <= 8_000)
        # WebP stores integer milliseconds and coalesces identical holds on the 30 fps grid.
        assert any(delay in (33, 34) for delay in delays)
        assert all(delay >= 33 for delay in delays)
        assert image.n_frames <= round(duration / 1000 * 30) + 1

poster = root / "docs/site/public/media/tour-poster.webp"
assert Image.open(poster).size == (2560, 1440)
assert poster.stat().st_size <= 400_000
print("Verified media")
