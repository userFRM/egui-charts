#!/usr/bin/env bash
# Rebuilds assets/emoji/Noto-COLRv1.ttf, the colour emoji font of the `color-emoji` feature, from the
# Apache 2.0 emoji SVGs and the public-domain region flags of googlefonts/noto-emoji.
#
#   tools/emoji-font.sh [work-dir]
#
# Fetches only the two SVG directories of commit NOTO_COMMIT (tag v2026-09-24-unicode18_0), installs the
# pinned nanoemoji into a virtual environment inside the work directory (default: a new directory under
# ${TMPDIR:-/tmp}; never the repository or the system), compiles every SVG into one COLRv1 font and copies
# it into assets/emoji. Needs git and Python 3.12 (`PYTHON` overrides the interpreter), about 500 MB of
# disk and about an hour on four cores (`JOBS` sets the parallel jobs, default all).
# assets/emoji/LICENSE-noto-emoji.txt and src/drawings/emoji_set.rs (scripts/emoji-set.py) are not touched.
#
# Reproducible: with the pinned versions and SOURCE_DATE_EPOCH below, rebuilding the font from the same
# intermediate files gave identical bytes, and without that variable only the timestamps in the `head` table
# differed. A complete rebuild on another machine has not been compared byte for byte.
set -euo pipefail

NOTO_COMMIT=e20cbc2bbec1926686be9f9bee7d1d2cfa1fea0e
# Commit time of NOTO_COMMIT, so the font's timestamps do not depend on the day of the build.
export SOURCE_DATE_EPOCH=1790271300
REQUIREMENTS="nanoemoji==0.16.0 picosvg==0.23.0 fonttools==4.69.1 skia-pathops==0.9.2 ufo2ft==3.11.0
ufoLib2==0.19.1 booleanOperations==0.10.0 fontMath==0.10.0 cffsubr==0.4.0 absl-py==2.5.1 attrs==26.1.0
lxml==6.1.3 ninja==1.13.2 pillow==13.0.0 pngquant-cli==3.0.3 pyclipper==1.4.0 regex==2026.11.25
resvg-cli==0.44.0 toml==0.10.2 zopfli==0.4.3"

root=$(cd "$(dirname "$0")/.." && pwd)
work=${1:-$(mktemp -d "${TMPDIR:-/tmp}/emoji-font.XXXXXX")}
mkdir -p "$work"
cd "$work"

if [ ! -d noto ]; then
    git init -q noto
    git -C noto remote add origin https://github.com/googlefonts/noto-emoji.git
    git -C noto fetch -q --depth 1 --filter=blob:none origin "$NOTO_COMMIT"
    git -C noto sparse-checkout set --no-cone /2D/svg/ /third_party/region-flags/waved-svg/
    git -C noto checkout -q FETCH_HEAD
fi

if [ ! -x venv/bin/nanoemoji ]; then
    "${PYTHON:-python3.12}" -m venv venv
    # shellcheck disable=SC2086 # one requirement per word
    venv/bin/pip install -q --disable-pip-version-check $REQUIREMENTS
fi
export PATH="$work/venv/bin:$PATH"

# The same font configuration upstream uses for its COLRv1 build, over every emoji SVG and waved flag.
{
    printf 'family = "Noto Color Emoji"\noutput_file = "NotoColorEmoji.ttf"\n'
    printf 'color_format = "glyf_colr_1"\nclipbox_quantization = 32\n\n'
    printf '[axis.wght]\nname = "Weight"\ndefault = 400\n\n[master.regular]\nstyle_name = "Regular"\nsrcs = [\n'
    (ls noto/2D/svg/*.svg noto/third_party/region-flags/waved-svg/*.svg) | LC_ALL=C sort | sed 's/.*/    "&",/'
    printf ']\n\n[master.regular.position]\nwght = 400\n'
} > all.toml
nanoemoji --noexec_ninja --build_dir build all.toml
ninja -C build ${JOBS:+-j "$JOBS"}

cp build/NotoColorEmoji.ttf "$root/assets/emoji/Noto-COLRv1.ttf"
sha256sum "$root/assets/emoji/Noto-COLRv1.ttf"
