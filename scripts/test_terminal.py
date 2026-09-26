"""PTY integration test. Run: uv run --with pyte --with pillow scripts/test_terminal.py"""
import codecs
import fcntl
import os
from pathlib import Path
import pty
import select
import signal
import statistics
import struct
import subprocess
import tempfile
import termios
import time
import tomllib

import pyte
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / 'target/release/tui-timer'
OUT = ROOT / 'target/qa'
OUT.mkdir(parents=True, exist_ok=True)

class Terminal:
    def __init__(self, config, *args):
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 30, 100, 0, 0))
        self.original = termios.tcgetattr(slave)
        self.slave = slave
        self.screen = pyte.Screen(100, 30)
        self.stream = pyte.Stream(self.screen)
        self.decoder = codecs.getincrementaldecoder('utf-8')()
        self.started = time.perf_counter()
        self.process = subprocess.Popen([str(BINARY), *args], stdin=slave, stdout=slave, stderr=slave,
            env={**{k: v for k, v in os.environ.items() if k != 'NO_COLOR'}, 'TERM': 'xterm-256color', 'COLORTERM': 'truecolor', 'TUI_TIMER_CONFIG': str(config)})

    def read(self, seconds=0.12):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if select.select([self.master], [], [], max(0, deadline-time.monotonic()))[0]:
                data = os.read(self.master, 65536)
                self.stream.feed(self.decoder.decode(data))
        return '\n'.join(self.screen.display)

    def expect(self, value, timeout=3):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if value in self.read(.015):
                return
        raise AssertionError(f'Missing {value!r}:\n' + '\n'.join(self.screen.display))

    def send(self, keys):
        os.write(self.master, keys.encode())
        self.read()

    def capture(self, name):
        (OUT / f'{name}.txt').write_text('\n'.join(self.screen.display))
        font_path = subprocess.check_output(['fc-match', '-f', '%{file}', 'monospace']).decode()
        font = ImageFont.truetype(font_path, 18)
        cell_w, cell_h = round(font.getlength('M')), 24
        image = Image.new('RGB', (100*cell_w, 30*cell_h), '#0f1319')
        draw = ImageDraw.Draw(image)
        for y in range(30):
            for x in range(100):
                cell = self.screen.buffer[y][x]
                color = '#' + cell.fg if len(cell.fg) == 6 else '#dee5ed'
                bounds = (x*cell_w, y*cell_h, (x+1)*cell_w-1, (y+1)*cell_h-1)
                if len(cell.bg) == 6: draw.rectangle(bounds, fill='#'+cell.bg)
                if cell.data == '█': draw.rectangle(bounds, fill=color)
                else: draw.text((x*cell_w,y*cell_h),cell.data,font=font,fill=color)
        image.save(OUT / f'{name}.png')

    def close(self):
        self.send('\x03')
        self.process.wait(timeout=3)
        assert self.process.returncode == 0
        assert termios.tcgetattr(self.slave) == self.original, 'Terminal modes not restored'
        os.close(self.master)
        os.close(self.slave)

with tempfile.TemporaryDirectory() as temp:
    config = Path(temp) / 'config.toml'
    terminal = Terminal(config)
    terminal.expect('T I M E R')
    terminal.capture('setup')
    terminal.send('l')
    terminal.expect('--hang')
    terminal.send('\r')
    terminal.expect('Preset: --hang')
    terminal.send('\x1b[Be3m\r')
    terminal.send('p')
    terminal.expect('hang')
    terminal.send('\r')
    terminal.expect('Saved. Launch with tui-timer --hang')
    assert tomllib.loads(config.read_text())['presets']['hang']['seconds'] == 180
    terminal.close()

    terminal = Terminal(config, '--hang', '--timer', '2s', '--countdown', '1s')
    terminal.expect('GET READY')
    terminal.expect('TIME REMAINING')
    terminal.send(' ')
    terminal.expect('P A U S E D')
    terminal.read(2.2)
    terminal.expect('P A U S E D')
    terminal.send(' ')
    terminal.expect('COMPLETE')
    terminal.capture('complete')
    terminal.send('\r')
    terminal.expect('GET READY')
    terminal.close()

    terminal = Terminal(config, '--hang')
    terminal.expect('GET READY')
    terminal.expect('TIME REMAINING', timeout=7)
    terminal.capture('running')
    fcntl.ioctl(terminal.slave, termios.TIOCSWINSZ, struct.pack('HHHH', 12, 36, 0, 0))
    os.kill(terminal.process.pid, signal.SIGWINCH)
    terminal.read(.2)
    assert terminal.process.poll() is None
    terminal.close()

    terminal = Terminal(config, '--stopwatch', '--no-countdown')
    terminal.expect('STOPWATCH')
    terminal.send(' ')
    terminal.expect('P A U S E D')
    terminal.send('\x1b')
    terminal.expect('T I M E R')
    terminal.close()

    timings = []
    for _ in range(10):
        terminal = Terminal(config)
        terminal.expect('T I M E R')
        timings.append((time.perf_counter()-terminal.started)*1000)
        terminal.close()
    print(f'PTY flows passed. First setup frame median: {statistics.median(timings):.1f}ms (15ms polling resolution).')
    print(f'Screenshots: {OUT}')
