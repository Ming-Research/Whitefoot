#!/bin/sh
# One harness: Python's standard library handles binary RESP2 and fixture metadata.
set -eu
ORACLE_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec python3 - "$ORACLE_DIR" "$@" <<'PY'
import argparse
import json
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(sys.argv[1])
parser = argparse.ArgumentParser(description="Generate or compare Halo's typed Redis EVAL oracle.")
mode = parser.add_mutually_exclusive_group()
mode.add_argument('--generate', action='store_true', help='write replies from a fresh Redis 7.0.15 (default)')
mode.add_argument('--check', action='store_true', help='compare replies with expected files; never rewrite them')
parser.add_argument('--port', type=int, help='compare an existing server; requires --check')
parser.add_argument('--stdio', type=Path, help='in-process firn request driver; requires --check')
parser.add_argument('--host', default='127.0.0.1', help='existing server host (default: 127.0.0.1)')
parser.add_argument('--server', default='/private/tmp/wf-redis-7.0.15/src/redis-server')
parser.add_argument('--output', type=Path, help='generation directory (default: expected/)')
parser.add_argument('--expected', type=Path, default=ROOT / 'expected', help='comparison directory')
parser.add_argument('--filter', help='one group or group/name, without .lua, for a small sample')
args = parser.parse_args(sys.argv[2:])
if args.stdio is not None and (not args.check or args.port is not None):
    parser.error('--stdio requires --check and excludes --port')
if args.port is not None and not args.check:
    parser.error('--port requires --check: expected replies may only be generated with pinned Redis')
if args.check and args.output is not None:
    parser.error('--output is only for generation')
if args.port is not None and not 1 <= args.port <= 65535:
    parser.error('--port must be between 1 and 65535')


def read_case(path):
    fields = {'setup': []}
    for line in path.read_text().splitlines():
        if not line.startswith('-- '):
            break
        name, sep, value = line[3:].partition(': ')
        if sep and name in ('KEYS', 'ARGV', 'setup', 'error'):
            v = json.loads(value)
            if name == 'setup':
                fields[name].append(v)
            elif name in fields:
                raise ValueError(f'{path}: duplicate {name}')
            else:
                fields[name] = v
    for name in ('KEYS', 'ARGV'):
        if name not in fields or not isinstance(fields[name], list) or not all(isinstance(x, str) for x in fields[name]):
            raise ValueError(f'{path}: {name} must be a JSON string array')
    for command in fields['setup']:
        if not isinstance(command, list) or not command or not all(isinstance(x, str) for x in command):
            raise ValueError(f'{path}: setup must be a nonempty JSON string array')
        if len(command) < 2 or command[1] not in fields['KEYS']:
            raise ValueError(f'{path}: setup key must be declared in KEYS')
    if 'error' in fields and not isinstance(fields['error'], bool):
        raise ValueError(f'{path}: error must be a JSON boolean')
    return path, fields


class RESP:
    def __init__(self, host, port):
        self.sock = socket.create_connection((host, port), timeout=5)
        self.file = self.sock.makefile('rb')

    @classmethod
    def stdio(cls, executable):
        self = cls.__new__(cls)
        self.process = subprocess.Popen([str(executable.resolve())], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.sock = None
        self.file = self.process.stdout
        return self

    def close(self):
        self.file.close()
        if self.sock is not None:
            self.sock.close()
        else:
            self.process.stdin.close()
            try:
                code = self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
                raise
            error = self.process.stderr.read()
            self.process.stderr.close()
            if code != 0:
                raise RuntimeError(f'firn driver exit {code}: {error!r}')

    def command(self, *parts):
        parts = [p if isinstance(p, bytes) else str(p).encode('utf-8') for p in parts]
        wire = b'*%d\r\n' % len(parts)
        wire += b''.join(b'$%d\r\n' % len(p) + p + b'\r\n' for p in parts)
        if self.sock is not None:
            self.sock.sendall(wire)
        else:
            self.process.stdin.write(wire)
            self.process.stdin.flush()
        return self.reply()

    def line(self):
        line = self.file.readline()
        if not line.endswith(b'\r\n'):
            raise ValueError('Truncated RESP line')
        return line[:-2]

    @staticmethod
    def byte_string(kind, data):
        # Each Unicode code point 0..255 denotes exactly one wire byte.
        return {'type': kind, 'bytes': data.decode('latin-1')}

    def reply(self):
        prefix = self.file.read(1)
        if prefix in (b'+', b'-'):
            return self.byte_string('status' if prefix == b'+' else 'error', self.line())
        if prefix == b':':
            return {'type': 'integer', 'value': int(self.line())}
        if prefix == b'$':
            n = int(self.line())
            if n == -1:
                return {'type': 'nil', 'kind': 'bulk'}
            if n < 0:
                raise ValueError('Invalid bulk length')
            data = self.file.read(n)
            if len(data) != n or self.file.read(2) != b'\r\n':
                raise ValueError('Truncated RESP bulk')
            return self.byte_string('bulk', data)
        if prefix == b'*':
            n = int(self.line())
            if n == -1:
                return {'type': 'nil', 'kind': 'array'}
            if n < 0:
                raise ValueError('Invalid array length')
            return {'type': 'array', 'items': [self.reply() for _ in range(n)]}
        raise ValueError(f'Unknown RESP prefix: {prefix!r}')


def successful(reply, context):
    if reply['type'] == 'error':
        raise RuntimeError(f'{context}: {reply["bytes"]}')
    return reply


def start_server(scratch):
    version = subprocess.check_output([args.server, '--version'], text=True)
    if not version.startswith('Redis server v=7.0.15 '):
        raise RuntimeError(f'Requires Redis 7.0.15, got {version.strip()}')
    # The small bind/start race is handled by retrying with a new ephemeral port.
    for _ in range(10):
        with socket.socket() as probe:
            probe.bind(('127.0.0.1', 0))
            port = probe.getsockname()[1]
        log_path = Path(scratch) / 'server.log'
        with log_path.open('wb') as log:
            process = subprocess.Popen([args.server, '--bind', '127.0.0.1', '--port', str(port),
                '--save', '', '--appendonly', 'no', '--daemonize', 'no', '--dir', scratch],
                cwd=scratch, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 5
            while process.poll() is None and time.monotonic() < deadline:
                try:
                    connection = RESP('127.0.0.1', port)
                    try:
                        ready = connection.command('PING') == {'type': 'status', 'bytes': 'PONG'}
                    except BaseException:
                        connection.close()
                        raise
                    if ready and process.poll() is None:
                        return process, connection
                    connection.close()
                except OSError:
                    pass
                time.sleep(0.01)
        except BaseException:
            process.terminate()
            process.wait(timeout=5)
            raise
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
        if 'Address already in use' not in log_path.read_text():
            raise RuntimeError('Redis startup failed:\n' + log_path.read_text())
    raise RuntimeError('Could not start Redis on a free local port')


def interrupted(signum, frame):
    raise InterruptedError(f'Interrupted by signal {signum}')


signal.signal(signal.SIGTERM, interrupted)


def main():
    all_cases = [read_case(p) for p in sorted((ROOT / 'scripts').glob('*/*.lua'))]
    cases = [(p, m) for p, m in all_cases if args.filter is None or
             p.relative_to(ROOT / 'scripts').with_suffix('').as_posix() == args.filter or
             p.parent.name == args.filter]
    if not cases:
        raise RuntimeError('No scripts selected')
    names = {p.relative_to(ROOT / 'scripts').with_suffix('.txt') for p, _ in cases}
    if args.check:
        if args.filter is None:
            existing = {p.relative_to(args.expected) for p in args.expected.glob('*/*.txt')}
            if names != existing:
                raise RuntimeError(f'Expected file inventory differs: missing={sorted(names-existing)}, extra={sorted(existing-names)}')
        for name in names:
            path = args.expected / name
            if not path.is_file() or path.stat().st_size == 0:
                raise RuntimeError(f'Missing or empty expected file: {path}')
    corpus_keys = sorted({k for _, m in all_cases for k in m['KEYS']})
    process = connection = None
    # Explicit system temp root: never leave Redis files in the repository.
    with tempfile.TemporaryDirectory(prefix='halo-oracle-', dir='/private/tmp') as scratch:
        try:
            if args.stdio is not None:
                connection = RESP.stdio(args.stdio)
                successful(connection.command('PING'), 'PING')
            elif args.port is None:
                process, connection = start_server(scratch)
            else:
                connection = RESP(args.host, args.port)
                successful(connection.command('PING'), 'PING')

            def flush():
                if args.port is None and args.stdio is None:
                    successful(connection.command('FLUSHDB'), 'FLUSHDB')
                elif corpus_keys:
                    # Firn has DEL but no FLUSHDB. All corpus writes use declared keys.
                    successful(connection.command('DEL', *corpus_keys), 'DEL corpus keys')

            replies = {}
            failures = []
            counts = {}
            for path, metadata in cases:
                name = path.relative_to(ROOT / 'scripts').with_suffix('.txt')
                flush()
                for command in metadata['setup']:
                    successful(connection.command(*command), f'{path}: setup {command[0]}')
                reply = connection.command('EVAL', path.read_bytes(), len(metadata['KEYS']),
                                           *metadata['KEYS'], *metadata['ARGV'])
                is_error = reply['type'] == 'error'
                if is_error != metadata.get('error', False):
                    failures.append(f'{name}: unexpected reply error state: {reply}')
                text = json.dumps(reply, ensure_ascii=True, indent=2) + '\n'
                replies[name] = text
                if args.check and (args.expected / name).read_text() != text:
                    failures.append(f'{name}: reply differs from {args.expected / name}')
                counts[path.parent.name] = counts.get(path.parent.name, 0) + 1
                flush()
            if failures:
                raise RuntimeError('\n'.join(failures))
            if not args.check:
                output = args.output or ROOT / 'expected'
                if args.filter is None:
                    extra = {p.relative_to(output) for p in output.glob('*/*.txt')} - names
                    if extra:
                        raise RuntimeError(f'Stale expected files (remove deliberately): {sorted(extra)}')
                for name, text in replies.items():
                    destination = output / name
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_text(text)
            print(('Checked' if args.check else 'Generated') + f' {len(cases)} replies: ' +
                  ', '.join(f'{g}={n}' for g, n in sorted(counts.items())))
        finally:
            if connection is not None:
                connection.close()
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


try:
    main()
except (Exception, KeyboardInterrupt) as e:
    print(f'halo-oracle: {e}', file=sys.stderr)
    sys.exit(1)
PY
