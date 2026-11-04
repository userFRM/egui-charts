"""Separate measurements must preserve the instruction regression decisions and output."""

import os
from pathlib import Path
import subprocess
import tempfile


with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    script = Path(__file__).with_name("check-regressions.sh").resolve()
    valgrind = work / "valgrind"
    valgrind.write_text("""#!/usr/bin/env python3
import os
from pathlib import Path
import sys

assert sys.argv[1:5] == ['--tool=cachegrind', '--cache-sim=no', '--branch-sim=no', '--cachegrind-out-file=/dev/null']
assert os.environ['MALLOC_MMAP_THRESHOLD_'] == '131072'
binary, flag, scenario = sys.argv[5:]
assert flag == '--regression'
assert binary.endswith(('/base/frame_time', '/head/frame_time'))
samples = int(os.environ['FRAME_TIME_FRAMES'])
assert samples in (0, 1 if scenario == 'trade-replay' else 20)
delta = samples * (400000 if scenario == 'tpo-1600' else 100000)
if samples and Path(binary).read_text() == 'head':
    if scenario == 'frame-1000':
        delta += int(os.environ['TEST_GROWTH'])
    if scenario == 'tpo-1600':
        delta += int(os.environ['TEST_SCALING'])
print(f'I refs: {1000 + delta:,}', file=sys.stderr)
""")
    valgrind.chmod(0o755)
    for revision in ("base", "head"):
        (work / revision).write_text(revision)
    env = dict(os.environ, PATH=f"{work}:{os.environ['PATH']}")

    def run(*args):
        return subprocess.run(
            ["bash", str(script), *map(str, args)],
            env=env, capture_output=True, text=True, check=False,
        )

    # 100001 / 2000000 is above 5% even though the printed change rounds to +5.00%.
    cases = [
        (0, 0, ""),
        (100000, 400000, ""),
        (100001, 0, "::error title=Performance regression::frame-1000 grew by +5.00% (tolerance +5%)"),
        (0, 400001, "::error title=TPO scaling regression::Instructions per session grew by more than 5% from 400 to 1,600 sessions"),
    ]
    for growth, scaling, error in cases:
        env.update(TEST_GROWTH=str(growth), TEST_SCALING=str(scaling))
        direct = run(work / "base", work / "head")
        for revision in ("base", "head"):
            measured = run("--measure", work / revision)
            assert measured.returncode == 0, measured.stderr
            assert [row.split()[0] for row in measured.stdout.splitlines()] == [
                row.split("|")[1].strip() for row in direct.stdout.splitlines()[2:]
            ]
            (work / f"{revision}.txt").write_text(measured.stdout)
        compared = run("--compare", work / "base.txt", work / "head.txt")
        assert compared.returncode == direct.returncode == bool(error), compared
        assert compared.stdout == direct.stdout
        assert compared.stderr == direct.stderr
        assert not error or error in compared.stderr

    # Incomplete or invalid job outputs must fail the required comparison check.
    for counts in ("", "frame-1000 invalid 2001000\n", "frame-1000 1000 1000\n"):
        (work / "head.txt").write_text(counts)
        compared = run("--compare", work / "base.txt", work / "head.txt")
        assert compared.returncode == 1
        assert "::error::Invalid instruction delta for frame-1000" in compared.stderr

print("instruction comparison: PASS")
