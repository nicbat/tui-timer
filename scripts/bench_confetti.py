#!/usr/bin/env python3
"""Reproducible confetti benchmark; only Python stdlib and Rust are required.

Run: python3 scripts/bench_confetti.py [--frames 3000] [--skip-pty]
Results are also saved in target/qa/confetti-benchmark.txt.
All application probes use a temporary config. No personal settings are changed.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import select
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def pty_probe(binary, width, height, confetti):
    import fcntl
    import pty
    import struct
    import termios

    with tempfile.TemporaryDirectory(prefix="tui-timer-bench-") as temp:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
        args = [str(binary), "--timer", "1s", "--no-countdown", "--no-bell"]
        args += ["--confetti" if confetti else "--no-confetti"]
        process = subprocess.Popen(args, stdin=slave, stdout=slave, stderr=slave, env={
            **os.environ, "TERM": "xterm-256color", "COLORTERM": "truecolor",
            "TUI_TIMER_CONFIG": str(Path(temp) / "config.toml"),
        })

        def drain(seconds):
            deadline = time.monotonic() + seconds
            count = 0
            while time.monotonic() < deadline:
                if select.select([master], [], [], max(0, deadline - time.monotonic()))[0]:
                    count += len(os.read(master, 65536))
            if process.poll() is not None:
                raise RuntimeError(f"App exited unexpectedly: {process.returncode}")
            return count

        def cpu_seconds():
            # Linux proc includes user + system CPU for this child only.
            fields = Path(f"/proc/{process.pid}/stat").read_text().rsplit(")", 1)[1].split()
            return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")

        try:
            drain(1.15)  # Timer has completed; confetti is still active.
            before = cpu_seconds()
            start = time.monotonic()
            output = drain(4.0)
            elapsed = time.monotonic() - start
            cpu = cpu_seconds() - before
            mode = "confetti" if confetti else "complete_static"
            result = f"pty,{width}x{height},{mode},cpu_one_core_pct={100*cpu/elapsed:.2f},output_bytes={output},sample_s={elapsed:.3f}"
            if confetti:
                drain(1.2)  # Past the five-second effect lifetime.
                before = cpu_seconds()
                output = drain(1.0)
                result += f"\npty,{width}x{height},after_confetti,cpu_seconds={cpu_seconds()-before:.3f},output_bytes={output}"
            return result
        finally:
            if process.poll() is None:
                os.write(master, b"\x03")
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            os.close(master)
            os.close(slave)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frames", type=int, default=3000, help="frames per renderer per round (5 rounds)")
    parser.add_argument("--skip-pty", action="store_true", help="run deterministic renderer comparison only")
    args = parser.parse_args()
    if args.frames < 1:
        parser.error("--frames must be positive")
    build = subprocess.run(["cargo", "build", "--release", "--message-format=json"], cwd=ROOT, text=True, stdout=subprocess.PIPE, check=True)
    ratatui = None
    binary = None
    for line in build.stdout.splitlines():
        artifact = json.loads(line)
        if artifact.get("reason") != "compiler-artifact":
            continue
        if artifact["target"]["name"] == "ratatui":
            ratatui = next(Path(p) for p in artifact["filenames"] if p.endswith(".rlib"))
        if artifact["target"]["name"] == "tui-timer":
            binary = Path(artifact["executable"])
    if ratatui is None or binary is None:
        raise RuntimeError("Cannot locate release artifacts from Cargo")
    out = binary.parent.parent / "qa"
    out.mkdir(parents=True, exist_ok=True)
    probe = out / "confetti-bench"
    subprocess.run(["rustc", "--edition=2024", "-C", "opt-level=3", "-L", f"dependency={ratatui.parent}", "--extern", f"ratatui={ratatui}", str(ROOT / "scripts/confetti_bench.rs"), "-o", str(probe)], check=True)
    report = [f"Host: {platform.platform()}", subprocess.check_output(["rustc", "--version"], text=True).strip(), f"Frames per round: {args.frames}; 5 alternating-order rounds; median reported."]
    report.append(subprocess.check_output([str(probe), str(args.frames)], text=True).strip())
    if not args.skip_pty:
        if platform.system() == "Linux":
            for width, height in [(100,30), (300,100)]:
                for confetti in [False,True]:
                    result = pty_probe(binary, width, height, confetti)
                    report.append(result)
                    print(result, flush=True)
        else:
            report.append("PTY CPU probes skipped: Linux /proc required.")
    text = "\n".join(report) + "\n"
    (out / "confetti-benchmark.txt").write_text(text)
    print(text)
    print(f"Saved: {out / 'confetti-benchmark.txt'}")


if __name__ == "__main__":
    main()
