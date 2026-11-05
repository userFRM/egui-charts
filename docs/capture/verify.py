#!/usr/bin/env python3
"""Check published media and make review sheets outside the repository."""
import json
from pathlib import Path
import subprocess
import sys

from PIL import Image, ImageDraw

root, work = map(Path, sys.argv[1:])
review = work / "review"
review.mkdir(exist_ok=True)
report = []


def sheet(name, frames):
    canvas = Image.new("RGB", (1920, 4 * 360 + 4 * 28), "#20242b")
    draw = ImageDraw.Draw(canvas)
    for index, (label, frame) in enumerate(frames):
        frame = frame.convert("RGB")
        frame.thumbnail((640, 360), Image.Resampling.LANCZOS)
        x, y = (index % 3) * 640, (index // 3) * 388
        canvas.paste(frame, (x, y + 28))
        draw.text((x + 8, y + 7), label, fill="white")
    canvas.save(review / f"{name}.jpg", quality=92)


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
    frames = []
    for index, second in enumerate([0, 3, 6, 9, 12, 15, 18, 21, 24, 28, 29.7, 29.95]):
        still = review / f"{ext}-{index:02}.png"
        subprocess.run([
            "ffmpeg", "-v", "error", "-y", "-threads", "3", "-ss", str(second), "-i", str(path),
            "-frames:v", "1", "-threads", "3", str(still)
        ], check=True)
        frames.append((f"{second}s", Image.open(still).copy()))
    sheet(ext, frames)
    report.append({"asset": str(path.relative_to(root)), "bytes": path.stat().st_size, "stream": stream,
                   "duration": info["format"]["duration"]})

for theme in ["dark", "light"]:
    for scene in ["tour", "liquidity", "drawings", "replay"]:
        path = root / f"docs/images/{scene}-{theme}.webp"
        image = Image.open(path)
        hero = scene == "tour"
        assert image.size == ((1280, 720) if hero else (1280, 800))
        assert image.info["loop"] == 0
        assert path.stat().st_size <= (6_000_000 if hero else 3_000_000), f"{path}: size budget exceeded"
        frames, duration, delays = [], 0, []
        selected = {round(i * (image.n_frames - 1) / 11) for i in range(12)}
        for index in range(image.n_frames):
            image.seek(index)
            image.load()
            if index in selected:
                frames.append((f"frame {index} / {duration / 1000:.2f}s", image.copy()))
            delays.append(image.info["duration"])
            duration += image.info["duration"]
        assert (10_000 <= duration <= 12_000) if hero else (6_000 <= duration <= 8_000)
        # WebP stores integer milliseconds and coalesces identical holds on the 30 fps grid.
        assert any(delay in (33, 34) for delay in delays)
        assert all(delay >= 33 for delay in delays)
        assert image.n_frames <= round(duration / 1000 * 30) + 1
        sheet(f"{scene}-{theme}", frames)
        report.append({"asset": str(path.relative_to(root)), "bytes": path.stat().st_size,
                       "frames": image.n_frames, "duration_ms": duration})

poster = root / "docs/site/public/media/tour-poster.webp"
assert Image.open(poster).size == (2560, 1440)
assert poster.stat().st_size <= 400_000
(work / "media.json").write_text(json.dumps(report, indent=2) + "\n")
print(f"Verified media; review sheets: {review}")
