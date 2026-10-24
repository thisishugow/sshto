#!/usr/bin/env python3
"""Verify real PTY interactions using an isolated HOME; never establish SSH connections."""
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import re
import select
import shutil
import signal
import struct
import subprocess
import sys
import tarfile
import tempfile
import termios
import time
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
with (ROOT / "Cargo.toml").open("rb") as manifest:
    VERSION = tomllib.load(manifest)["package"]["version"]
BIN = Path(os.environ.get('SSHTO_TEST_BIN', str(ROOT / 'target/debug/sshto')))
if 'SSHTO_TEST_BIN' not in os.environ:
    subprocess.run(['cargo', 'build'], cwd=ROOT, check=True)
ANSI = re.compile(r'\x1b\[[0-?]*[ -/]*[@-~]|\x1b\([^)]|\x1b[=>]')
UP, DOWN, LEFT, RIGHT = b'\x1b[A', b'\x1b[B', b'\x1b[D', b'\x1b[C'

class Session:
    def __init__(self, env, rows=30, cols=160):
        self.data = b''
        self.status = None
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execve(str(BIN), [str(BIN)], env)
        self.resize(rows, cols, notify=False)
        self.read(0.35)

    def resize(self, rows, cols, notify=True):
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack('HHHH', rows, cols, 0, 0))
        if notify:
            os.kill(self.pid, signal.SIGWINCH)
            self.read(0.15)

    def read(self, seconds=0.12):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.fd], [], [], max(0, deadline - time.monotonic()))
            if not ready:
                break
            try:
                chunk = os.read(self.fd, 65536)
            except OSError as exc:
                if exc.errno == errno.EIO:
                    break
                raise
            if not chunk:
                break
            self.data += chunk
        return self.data.decode('utf-8', 'replace')

    def send(self, data):
        os.write(self.fd, data)
        return self.read()

    def frame(self):
        text = self.read(0.02)
        # Inspect only the latest frame, excluding output from previously selected groups.
        starts = list(re.finditer(r'\x1b\[(?:H|1;1H)(?:\x1b\[(?:2?J))?', text))
        if starts:
            text = text[starts[-1].end():]
        text = re.sub(r'\x1b\[\d+;1H', '\n', text)
        return ANSI.sub('', text).replace('\r', '')

    def wait(self, seconds=3):
        deadline = time.monotonic() + seconds
        while self.status is None and time.monotonic() < deadline:
            self.read(0.05)
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                self.status = os.waitstatus_to_exitcode(status)
        if self.status is None:
            raise AssertionError('TUI did not exit before the deadline')
        return self.status

    def close(self):
        if self.status is None:
            try:
                os.kill(self.pid, signal.SIGKILL)
                os.waitpid(self.pid, 0)
            except ProcessLookupError:
                pass
        os.close(self.fd)


class MenuTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='sshto-test-')
        self.root = Path(self.tmp.name)
        self.sessions = []
        (self.root / '.ssh').mkdir()
        (self.root / 'bin').mkdir()
        (self.root / '.config/sshto').mkdir(parents=True)
        self.config = self.root / '.config/sshto/config'
        self.config.write_text('''group|Tailscale
group|工作
group|Personal
match|工作|hostname|192.0.2.*
match|Tailscale|alias|*-TS
match|Personal|either|home-*
exclude|github.*
''')
        self.hosts = {
            'office-ts': '192.0.2.10', 'cloud-ts': '198.51.100.1',
            'home-b': '203.0.113.1', 'loose': '203.0.113.2',
            'work-a': '192.0.2.11', 'work-b': '192.0.2.11',
            'github.example': 'example.com',
        }
        (self.root / '.ssh/config').write_text(''.join(
            f'Host {host}\n HostName {ip}\n User tester\n' for host, ip in self.hosts.items()
        ) + '  hOsT work-a work-b *.example !blocked # comment\n')
        self.log = self.root / 'connections.jsonl'
        self.env = dict(os.environ, HOME=str(self.root), TERM='xterm-256color', LC_ALL='C',
                        PATH=str(self.root / 'bin') + ':' + os.environ.get('PATH', ''),
                        SSHTO_TEST_LOG=str(self.log), SSHTO_TEST_MAP=json.dumps(self.hosts))
        self.env.pop('SSHTO_CONFIG', None)
        self.env.pop('XDG_CONFIG_HOME', None)
        stub = self.root / 'bin/ssh'
        stub.write_text('#!' + sys.executable + '\n' + '''import json, os, sys, select, tty
args = sys.argv[1:]
if args and args[0] == '-G':
    print('hostname ' + json.loads(os.environ['SSHTO_TEST_MAP'])[args[-1]])
    print('user tester')
else:
    if os.isatty(0):
        tty.setraw(0)
        if select.select([0], [], [], 0.08)[0]:
            raise SystemExit('SSH received leftover mouse events')
    with open(os.environ['SSHTO_TEST_LOG'], 'a') as out:
        out.write(json.dumps(args) + '\\n')
''')
        stub.chmod(0o755)

    def tearDown(self):
        for session in self.sessions:
            session.close()
        self.tmp.cleanup()

    def start(self, ready=True, **kwargs):
        session = Session(self.env, **kwargs)
        self.sessions.append(session)
        if ready:
            deadline = time.monotonic() + 8
            while b'\x1b[?25l' not in session.data and time.monotonic() < deadline:
                session.read(0.1)
            self.assertIn(b'\x1b[?25l', session.data, session.data.decode())
            session.read(0.15)
        return session

    def connections(self):
        return [json.loads(s) for s in self.log.read_text().splitlines()] if self.log.exists() else []

    def finish(self, s, key=b'q', code=0):
        s.send(key)
        self.assertEqual(s.wait(), code, s.data.decode())
        for event in (b'\x1b[?1000l', b'\x1b[?1006l', b'\x1b[?25h', b'\x1b[?1049l'):
            self.assertIn(event, s.data)
        self.assertTrue(termios.tcgetattr(s.fd)[3] & termios.ECHO)

    def test_config_order_priority_and_all_navigation(self):
        s = self.start()
        frame = s.frame()
        self.assertIn('0:All(6)', frame)
        order = ['cloud-ts', 'office-ts', 'work-a', 'work-b', 'home-b', 'loose']
        self.assertEqual(sorted(order, key=frame.index), order)
        self.assertNotIn('github.example', frame)
        self.assertEqual(frame.count('work-a'), 1)
        s.send(b'2')
        self.assertIn('office-ts', s.frame())
        self.assertNotIn('cloud-ts', s.frame())
        s.send(b'0' + LEFT)
        self.assertIn('loose', s.frame())
        self.assertNotIn('work-a', s.frame())
        s.send(RIGHT + b'\t')
        self.assertIn('cloud-ts', s.frame())
        self.assertNotIn('loose', s.frame())
        self.finish(s)

    def test_default_xdg_and_explicit_config(self):
        text = self.config.read_text()
        self.config.unlink()
        custom = self.root / 'custom/sshto'
        custom.mkdir(parents=True)
        (custom / 'config').write_text(text)
        s = self.start()
        self.assertIn('1:Other(7)', s.frame())
        self.assertNotIn('2:', s.frame().splitlines()[0])
        self.finish(s)
        self.env['XDG_CONFIG_HOME'] = str(custom.parent)
        s = self.start()
        self.assertIn('0:All(6)', s.frame())
        self.finish(s)
        self.env.pop('XDG_CONFIG_HOME')
        self.env['SSHTO_CONFIG'] = str(custom / 'config')
        s = self.start()
        self.assertIn('工作(3)', s.frame())
        self.finish(s)
        self.env.pop('SSHTO_CONFIG')

    def test_reject_invalid_config_and_never_execute_data(self):
        for text in ['group|All\n', 'group|X\ngroup|X\n', 'match|Missing|alias|*\n',
                     'group|X\nmatch|X|unknown|*\n', 'group|X|bad\n', 'group|X\x1b\n']:
            self.config.write_text(text)
            s = self.start(ready=False)
            self.assertEqual(s.wait(), 1, s.data.decode())
            self.assertNotIn(b'\x1b[?1049h', s.data)
        self.env['SSHTO_CONFIG'] = str(self.root / 'missing')
        s = self.start(ready=False)
        self.assertEqual(s.wait(), 1)
        self.env.pop('SSHTO_CONFIG')
        marker = self.root / 'executed'
        self.config.write_text(f'group|$(touch {marker})\nmatch|$(touch {marker})|alias|*\n')
        s = self.start()
        self.assertFalse(marker.exists())
        self.finish(s)

    def test_more_than_nine_groups_and_resize(self):
        self.config.write_text(''.join(f'group|G{i}\n' for i in range(11)) + 'match|G10|alias|*\n')
        s = self.start(rows=8, cols=40)
        s.send(b'0' + LEFT + LEFT)
        self.assertIn('11:G10 (7)', s.frame())
        s.resize(1, 40)
        s.read(1.2)
        self.assertIn('cloud-ts', s.frame())
        s.resize(8, 40)
        s.read(1.2)
        self.assertIn('11:G10', s.frame())
        self.finish(s)

    @staticmethod
    def click(x, y):
        return f'\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m'.encode()

    def test_unicode_tab_hitboxes_and_double_click_release(self):
        s = self.start()
        # All takes 10 cells and Tailscale 16; the wide-character group starts at column 27.
        s.send(self.click(29, 1))
        self.assertIn('office-ts', s.frame())
        self.assertNotIn('cloud-ts', s.frame())
        s.send(self.click(5, 4))  # Second host: work-a
        self.assertIn('> work-a', s.frame())
        s.send(DOWN + UP)  # Reset click timing while preserving the selection.
        before = len(self.connections())
        # Queue a fresh click pair without frame assertions or fixed waits between clicks.
        # Otherwise a busy CI runner can exhaust the real 450 ms double-click window.
        os.write(s.fd, self.click(5, 4) + b'\x1b[<0;5;4M')
        s.read(0.02)
        self.assertEqual(len(self.connections()), before)
        s.send(b'\x1b[<0;5;4m')
        self.assertEqual(s.wait(), 0, s.data.decode())
        self.assertEqual(self.connections()[-1], ['work-a'])
        self.assertIn(b'\x1b[?1000l', s.data)

    def test_headers_scroll_keyboard_and_cleanup(self):
        s = self.start(rows=7, cols=40)
        s.send(self.click(4, 3) * 2)  # Group heading
        self.assertFalse(self.connections())
        s.send(b'\x1b[<65;5;4M' * 5)
        self.assertIn('loose', s.frame())
        s.send(self.click(4, 5))
        time.sleep(0.5)
        s.send(self.click(4, 5))
        self.assertFalse(self.connections())
        self.finish(s)
        for key, code in [(b'\x03', 130), (b'\x04', 0), (b'\x1b', 0)]:
            s = self.start()
            self.finish(s, key, code)
        s = self.start()
        s.send(DOWN + b'\r')
        self.assertEqual(s.wait(), 0, s.data.decode())
        self.assertEqual(self.connections()[-1], ['office-ts'])
        self.log.unlink()

    def test_cli_and_passthrough(self):
        argv = ['-p', '2222', 'my-host', 'printf', 'space value', 'literal;$HOME']
        result = subprocess.run([str(BIN), *argv], env=self.env, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertEqual(self.connections()[-1], argv)
        for arg, expected in [('--version', f'sshto {VERSION}'), ('--help', 'SSHTO_CONFIG')]:
            result = subprocess.run([str(BIN), arg], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0)
            self.assertIn(expected, result.stdout)


class PackageTests(unittest.TestCase):
    def test_archive_allowlist_metadata_and_formula_checksum(self):
        with tempfile.TemporaryDirectory(prefix='sshto-package-test-') as temp:
            root = Path(temp) / 'source'
            shutil.copytree(ROOT, root, ignore=shutil.ignore_patterns('__pycache__', '.git', 'dist', 'target'))
            (root / 'personal-secret').write_text('PRIVATE_SENTINEL_NOT_FOR_RELEASE')
            out = Path(temp) / 'release'
            subprocess.run([sys.executable, str(root / 'tools/package.py'), str(out),
                            '--url', f'https://example.com/sshto-{VERSION}.tar.gz',
                            '--homepage', 'https://example.com/sshto'], check=True, capture_output=True)
            archive = out / f'sshto-{VERSION}.tar.gz'
            with tarfile.open(archive) as tar:
                self.assertEqual(set(tar.getnames()), {
                    f'sshto-{VERSION}/' + name for name in [
                        'Cargo.toml', 'Cargo.lock', 'src/cli.rs', 'src/config.rs', 'src/hosts.rs',
                        'src/main.rs', 'src/menu.rs', 'src/ssh.rs', 'src/ui.rs', 'README.md', 'PACKAGING.md', 'CHANGELOG.md', 'CONTRIBUTING.md', 'LICENSE', 'assets/sshto.svg', 'examples/config']})
                for entry in tar:
                    self.assertTrue(entry.isfile())
                    self.assertEqual((entry.mode, entry.mtime), (0o644, 0))
                    self.assertEqual((entry.uid, entry.gid, entry.uname, entry.gname), (0, 0, '', ''))
                    self.assertNotIn(b'PRIVATE_SENTINEL_NOT_FOR_RELEASE', tar.extractfile(entry).read())
            formula = (out / 'Formula/sshto.rb').read_text()
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            self.assertIn(digest, formula)
            self.assertEqual((out / "SHA256SUMS").read_text(), f"{digest}  {archive.name}\n")
            self.assertIn('depends_on "rust" => :build', formula)
            self.assertIn('system "cargo", "install", *std_cargo_args', formula)
            self.assertNotIn("depends_on :macos", formula)
            self.assertNotIn('@@', formula)
            ruby = shutil.which('ruby')
            if ruby:
                subprocess.run([ruby, '-c', str(out / 'Formula/sshto.rb')], check=True, capture_output=True)


if __name__ == '__main__':
    unittest.main(verbosity=2)
