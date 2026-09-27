#!/usr/bin/env python3
"""Smoke-test an instrumented build without modifying personal settings."""
import fcntl
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import tempfile
import termios
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/metrics/release/tui-timer"
with tempfile.TemporaryDirectory(prefix="tui-timer-metrics-") as temp:
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
    report = Path(temp) / "metrics.toml"
    process = subprocess.Popen([str(BINARY), "--timer", "1s", "--no-countdown", "--confetti"],
        stdin=slave, stdout=slave, stderr=slave,
        env={**os.environ, "TERM": "xterm-256color", "TUI_TIMER_CONFIG":str(Path(temp)/"config.toml"), "TUI_TIMER_METRICS":str(report)})
    try:
        deadline = time.monotonic()+6.3
        while time.monotonic()<deadline:
            if select.select([master],[],[],max(0,min(.1,deadline-time.monotonic())))[0]:
                os.read(master,65536)
        os.write(master,b"\x03")
        process.wait(timeout=3)
        assert process.returncode == 0
        metrics = tomllib.loads(report.read_text())
        assert metrics["all_frames"]["frames"] > 100
        assert metrics["confetti_frames"]["frames"] > 50
        assert metrics["all_frames"]["frames"] > metrics["confetti_frames"]["frames"]
        for name,sample in metrics.items():
            assert sample["total_us"] >= sample["max_us"]
            print(f'{name}: {sample["frames"]} frames, mean {sample["total_us"]/sample["frames"]:.1f}µs, max {sample["max_us"]}µs')
        destination=ROOT/"target/qa/metrics-smoke.toml"
        destination.parent.mkdir(parents=True,exist_ok=True)
        destination.write_text(report.read_text())
        print(f"Metrics smoke test passed; {destination}")
    finally:
        if process.poll() is None:
            process.kill(); process.wait()
        os.close(master); os.close(slave)
