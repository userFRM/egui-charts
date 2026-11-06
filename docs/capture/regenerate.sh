#!/usr/bin/env bash
# Linux software-renderer capture. Exact encoding commands follow the frame comparison.
# Encoders: FFmpeg 6.1.1 and libwebp 1.6.0 (cwebp and img2webp).
set -euo pipefail
root=$(realpath "$(dirname "$0")/../..")
work=${1:?usage: regenerate.sh /absolute/temporary-directory}
work=$(realpath -m "$work")
[[ "$work/" != "$root/"* ]] || { echo "Frames must live outside the repository" >&2; exit 2; }
mkdir -p "$work"
[[ ! -e "$work/frames" && ! -e "$work/repeat" ]] || { echo "Use a fresh temporary directory" >&2; exit 2; }
RUSTUP_TOOLCHAIN=stable cargo build --release --manifest-path "$root/docs/capture/Cargo.toml"
capture=$root/docs/capture/target/release/capture
cc -shared -fPIC -O2 -Wall -Wextra -Werror "$root/docs/capture/clock.c" -o "$work/clock.so"
export CAPTURE_CLOCK="$work/clock.txt"
printf '1793129390 0\n' > "$CAPTURE_CLOCK"
export LD_PRELOAD="$work/clock.so" LP_NUM_THREADS=3
export WGPU_BACKEND=vulkan
"$capture" "$work/frames" --animated
"$capture" "$work/repeat" --animated
diff --brief --recursive "$work/frames" "$work/repeat"
unset LD_PRELOAD
mkdir -p "$root/docs/site/public/media"
# The last 0.35 seconds dissolve into the opening 0.35 seconds, always playing forwards.
seam='split=2[body][head];[head]trim=duration=0.35,setpts=PTS-STARTPTS[opening];[body][opening]xfade=transition=fade:duration=0.35:offset='
ffmpeg -filter_complex_threads 3 -hide_banner -loglevel error -y -threads 3 -framerate 60 -i "$work/frames/tour-dark/%04d.png" \
    -filter_complex "${seam}29.65,scale=1920:1080:flags=lanczos:out_color_matrix=bt709:out_range=tv,format=yuv420p" \
    -c:v libx264 -profile:v high -preset slow -crf 22 -threads 3 -movflags +faststart -colorspace bt709 -color_primaries bt709 -color_trc bt709 -color_range tv -an \
    "$root/docs/site/public/media/tour-1080p.mp4"
ffmpeg -filter_complex_threads 3 -hide_banner -loglevel error -y -threads 3 -framerate 60 -i "$work/frames/tour-dark/%04d.png" \
    -filter_complex "${seam}29.65,scale=1920:1080:flags=lanczos:out_color_matrix=bt709:out_range=tv,format=yuv420p" \
    -c:v libsvtav1 -preset 4 -crf 32 -svtav1-params lp=3 -colorspace bt709 -color_primaries bt709 -color_trc bt709 -color_range tv -an \
    "$root/docs/site/public/media/tour-1080p.webm"
cwebp -quiet -q 85 -m 6 "$work/frames/tour-dark/0000.png" -o "$root/docs/site/public/media/tour-poster.webp"
webp() {
    local scene=$1 theme=$2 seconds=$3 height=$4 quality=$5
    local frames_dir="$work/encode-$scene-$theme" index=0 delay frame
    local offset="$((seconds - 1)).65"
    local -a frames=()
    mkdir -p "$frames_dir"
    ffmpeg -filter_complex_threads 3 -hide_banner -loglevel error -y -threads 3 -framerate 60 -i "$work/frames/$scene-$theme/%04d.png" \
        -filter_complex "trim=duration=$seconds,${seam}$offset,scale=1280:$height:flags=lanczos,fps=30" \
        -compression_level 1 -threads 3 "$frames_dir/%04d.png"
    for frame in "$frames_dir"/*.png; do
        delay=33
        if ((index % 3 == 2)); then delay=34; fi
        frames+=(-d "$delay" "$frame")
        index=$((index + 1))
    done
    img2webp -loop 0 -mixed -q "$quality" -m 6 "${frames[@]}" -o "$root/docs/images/$scene-$theme.webp"
}
for theme in dark light; do
    webp tour "$theme" 11 720 72
    for scene in liquidity drawings replay; do webp "$scene" "$theme" 7 800 75; done
done
"${PYTHON:-python3}" "$root/docs/capture/verify.py" "$root"
