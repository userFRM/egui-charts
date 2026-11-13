"""Separate measurements must preserve the instruction regression decisions and output."""

import os
from pathlib import Path
import subprocess
import tempfile
import textwrap


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

# Cache identity must distinguish processor models and flags, but not the core count.
workflow = script.parents[1] / ".github/workflows/ci.yml"
measurement_job = workflow.read_text().split("\n  performance-measure:\n", 1)[1].split("\n  performance:\n", 1)[0]
cache_step = measurement_job.split("      - name: Identify measurement cache\n", 1)[1].split("\n      - ", 1)[0]
cache_script = textwrap.dedent(cache_step.split("        run: |\n", 1)[1])
with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    cpuinfo = work / "cpuinfo"
    output = work / "output"
    keys = []
    for model, flags, cores in [
        ("Processor A", "sse2 avx", 1),
        ("Processor A", "sse2 avx", 4),
        ("Processor B", "sse2 avx", 1),
        ("Processor A", "sse2", 1),
    ]:
        cpuinfo.write_text("".join(
            f"processor\t: {core}\nmodel name\t: {model}\nflags\t\t: {flags}\n\n"
            for core in range(cores)
        ))
        output.write_text("")
        subprocess.run(
            ["bash", "-euo", "pipefail", "-c", cache_script.replace("/proc/cpuinfo", str(cpuinfo))],
            env=dict(os.environ, GITHUB_OUTPUT=str(output), ImageVersion="image", WORKLOAD_HASH="workload", MEASURED_SHA="commit"),
            check=True,
        )
        fields = dict(line.split("=", 1) for line in output.read_text().splitlines())
        assert fields["processor"] == model
        keys.append(fields["key"])
    assert keys[0] == keys[1]
    assert len(set(keys)) == 3

# Head, baseline lookup and the base fallback must run in one job, keyed without prefix matching.
assert "matrix.revision" not in measurement_job
assert "key: ${{ steps.measurement.outputs.key }}-${{ github.sha }}" in measurement_job
assert "key: ${{ steps.measurement.outputs.key }}-${{ needs.changes.outputs.base }}" in measurement_job
assert "restore-keys:" not in measurement_job
print("processor measurement cache: PASS")
