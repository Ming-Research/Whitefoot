#!/usr/bin/env python3
"""Run the getting started guide's own commands as the package acceptance.

A guide that the acceptance never runs stays correct only until the next edit.
check-package.sh installs the Linux and macOS packages with the `sh` blocks of
docs/guide/getting-started.md and builds the guide's programs with the
installed compiler. The Windows runner reads the same guide for its PowerShell
blocks with a reader of its own (check-package-windows.sh), which the item
wf-guide-reader-single replaces by this one.

  guide-steps.py prepare GUIDE PLATFORM DOWNLOADS PROGRAMS TOOLS
      Writes TOOLS/install.sh, which runs the guide's blocks for PLATFORM
      (linux or macos) in the guide's order, each block's output and exit
      status kept in TOOLS/blocks; the guide's programs into PROGRAMS, the
      directory install.sh runs in; and TOOLS/expect.json, which says what each
      block must have produced. Two things differ from a reader at a terminal:
      the two `curl -LO` lines become copies from DOWNLOADS of the candidate
      archive and of the release's SHA256SUMS, since the candidate is not
      published yet, and the macOS runner does not run `xcode-select
      --install`, since it already holds the Command Line Tools and the command
      would open an installer window; check-package.sh shows the SDK and the
      system linker they provide. On Linux `sudo` runs its command directly (the
      container's user is root, and holds no sudo) and `apt-get` answers yes and
      installs no recommended package, which is stricter than the guide's
      command: nothing the guide does not list comes along.
  guide-steps.py verify TOOLS
      Reads what install.sh recorded and fails unless every block ran, each
      succeeded, the checksum block reported its archive `OK`, the block that
      runs `whitefootc --help` printed the usage, each program ran to the exit
      status the guide states, and the rejected program was refused with the
      text the guide shows. The compiler reports its usage as a usage error, so
      the `--help` block's own exit status is not required; the item
      wf-cli-version-flag replaces that command with `--version` in the guide.

The container and the macOS sandbox hold no Rust, so the gate's documentation
example test cannot run the package's compiler there and this reader stands in
for it. It reads only column-zero fences of exactly three backticks, and stops
at any other fence form (tildes, four or more backticks, indentation, a list or
quote marker) rather than skipping a block the guide's convention allows.
"""
import argparse
import json
import pathlib
import re
import shlex
import sys

ARCHIVES = {
    'linux': 'whitefootc-linux-x86_64-bundled.tar.gz',
    'macos': 'whitefootc-macos-arm64-bundled.tar.gz',
}
HEADINGS = {'linux': 'Linux x86-64', 'macos': 'macOS arm64', 'windows': 'Windows x86-64'}
# What a user's download looks like; the tag is the user's, so it stays a variable.
DOWNLOAD = re.compile(
    r'curl -LO "https://github\.com/Ming-Research/Whitefoot/releases/download/'
    r'\$\{release_tag\}/([A-Za-z0-9._-]+)"')
CHECKSUM = re.compile(r'(?:sha256sum|shasum -a 256) -c ')
CHECK = re.compile(r'whitefootc --check (\S+)')


def fail(message):
    sys.exit('guide-steps: ' + message)


def read_blocks(guide):
    """The guide's fenced blocks: (section, info string, body, line number).

    The section is the block's `###` heading; a `##` or `#` heading ends it.
    """
    fence_like = re.compile(r'[\s>]*(?:[-*+]|\d+[.)])?\s*(?:`{3,}|~{3,})')
    blocks, info, body, start, section = [], None, [], 0, None
    for number, line in enumerate(guide.read_text().splitlines(), 1):
        if info is None:
            opening = re.fullmatch(r'```(?!`)(.*)', line)
            if opening:
                info, body, start = opening.group(1).strip(), [], number
            elif fence_like.match(line):
                fail('unsupported fence form at line %d of the guide: %r' % (number, line))
            else:
                heading = re.match(r'(#{1,6}) (.*)', line)
                if heading:
                    section = heading.group(2).strip() if len(heading.group(1)) == 3 else None
        elif line == '```':
            blocks.append((section, info, '\n'.join(body) + '\n', start))
            info = None
        elif re.match(r'`{3,}', line):
            fail('unsupported fence form at line %d of the guide: %r' % (number, line))
        else:
            body.append(line)
    if info is not None:
        fail('unclosed fence at line %d of the guide' % start)
    return blocks


def programs(blocks):
    """The guide's programs in order: (kind, file name, source, exit status or
    the rejection text the guide shows)."""
    found = []
    for position, (_, info, body, line) in enumerate(blocks):
        run = re.fullmatch(r'whitefoot run exit=(\d+) file=(\S+)', info)
        reject = re.fullmatch(r'whitefoot reject (\S+) file=(\S+)', info)
        if run:
            found.append(('run', run.group(2), body, run.group(1)))
        elif reject:
            following = blocks[position + 1] if position + 1 < len(blocks) else None
            if following is None or following[1] != 'whitefoot-output':
                fail('the rejected program at line %d is not followed by its output' % line)
            found.append(('reject', reject.group(2), body, following[2]))
    return found


def prepare(args):
    platform = args.platform
    archive = ARCHIVES[platform]
    blocks = read_blocks(args.guide)
    others = {name for key, name in HEADINGS.items() if key != platform}
    steps = [block for block in blocks if block[1] == 'sh' and block[0] not in others]
    found = programs(blocks)
    downloads, programs_dir, tools = (pathlib.Path(p).resolve() for p in
                                      (args.downloads, args.programs, args.tools))
    quote = shlex.quote
    script = ['#!/bin/bash',
              '# Generated by guide-steps.py from the getting started guide; see there.',
              'mkdir -p %s' % quote(str(tools / 'blocks'))]
    if platform == 'linux':
        script += ['sudo() { "$@"; }',
                   'apt-get() { command apt-get -y --no-install-recommends "$@"; }']
    expect, skipped, copied, runs = [], [], set(), iter([p for p in found if p[0] == 'run'])
    rejects = iter([p for p in found if p[0] == 'reject'])
    needles = {'linux': ('apt-get install', 'sha256sum -c', 'tar -xzf ' + archive),
               'macos': ('shasum -a 256 -c', 'tar -xzf ' + archive)}[platform]
    everything = '\n'.join(body for _, _, body, _ in steps)
    for needle in needles + ('export PATH=', 'whitefootc --help'):
        if needle not in everything:
            fail('the guide has no %s block with %r' % (platform, needle))
    for _, _, body, line in steps:
        if platform == 'macos' and 'xcode-select --install' in body:
            skipped.append({'line': line, 'command': body.strip()})
            script.append('# Not run (guide line %d): %s' % (line, body.strip()))
            continue
        lines = []
        for text in body.splitlines():
            download = DOWNLOAD.fullmatch(text)
            if download:
                if download.group(1) not in (archive, 'SHA256SUMS'):
                    fail('the guide downloads %r, not the candidate files' % download.group(1))
                copied.add(download.group(1))
                text = 'cp %s .' % quote(str(downloads / download.group(1)))
            elif 'curl' in text:
                fail('the guide line %d downloads in a form this reader does not know' % line)
            lines.append(text)
        index = len(expect) + 1
        record = {'index': index, 'line': line}
        check = next((m for m in map(CHECK.fullmatch, lines) if m), None)
        if check:
            _, name, _, text = next(rejects, (None,) * 4)
            if name != check.group(1):
                fail('the guide line %d checks %r, not the rejected program' % (line, check.group(1)))
            record.update(kind='reject', output=text)
        elif 'whitefootc --help' in lines:
            record.update(kind='usage')
        elif 'echo "$?"' in lines:
            _, name, _, code = next(runs, (None,) * 4)
            if name is None or ('whitefootc %s ' % name) not in body:
                fail('the guide line %d does not build the next program' % line)
            record.update(kind='run', code=code)
        elif CHECKSUM.search(body):
            record.update(kind='checksum')
        else:
            record.update(kind='plain')
        expect.append(record)
        stem = quote(str(tools / 'blocks' / ('%02d' % index)))
        script += ['{', '\n'.join(lines), '} > %s.txt 2>&1' % stem,
                   '__guide_status=$?',
                   'echo "$__guide_status" > %s.status' % stem,
                   'cat %s.txt' % stem,
                   'echo "--- guide block %d (line %d of the guide): status $__guide_status"'
                   % (index, line)]
    if copied != {archive, 'SHA256SUMS'}:
        fail('the guide downloads %s, not the archive and SHA256SUMS' % sorted(copied))
    if next(runs, None) or next(rejects, None):
        fail('the guide has a program that no block builds')
    if platform == 'macos' and len(skipped) != 1:
        fail('the macOS section has %d xcode-select blocks, not one' % len(skipped))
    # Source text, byte for byte: a program's line ends are not the platform's.
    programs_dir.mkdir(parents=True, exist_ok=True)
    for _, name, source, _ in found:
        (programs_dir / name).write_bytes(source.encode())
    tools.mkdir(parents=True, exist_ok=True)
    (tools / 'install.sh').write_text('\n'.join(script) + '\n')
    (tools / 'expect.json').write_text(json.dumps(
        {'platform': platform, 'archive': archive, 'blocks': expect, 'skipped': skipped}, indent=2))
    print('Guide:', args.guide, '- blocks run:', len(expect), '- not run:', len(skipped),
          '- programs:', ' '.join(name for _, name, _, _ in found))


def verify(args):
    tools = pathlib.Path(args.tools)
    expect = json.loads((tools / 'expect.json').read_text())
    problems = []
    for block in expect['blocks']:
        stem = tools / 'blocks' / ('%02d' % block['index'])
        where = 'guide block %d (line %d)' % (block['index'], block['line'])
        if not stem.with_suffix('.status').is_file():
            problems.append('%s did not run' % where)
            continue
        status = int(stem.with_suffix('.status').read_text())
        output = stem.with_suffix('.txt').read_bytes()
        kind = block['kind']
        if kind == 'reject':
            if status == 0:
                problems.append('%s: the rejected program was accepted' % where)
            if output != block['output'].encode():
                problems.append('%s: the rejection text differs from the guide\'s' % where)
        elif kind == 'usage':
            if b'usage: whitefootc' not in output:
                problems.append('%s did not print the usage (status %d)' % (where, status))
        elif status != 0:
            problems.append('%s failed with status %d' % (where, status))
        elif kind == 'run':
            last = output.decode(errors='replace').splitlines()[-1:]
            if last != [block['code']]:
                problems.append('%s printed %r, the guide says %s' % (where, last, block['code']))
        elif kind == 'checksum':
            if ('%s: OK' % expect['archive']).encode() not in output.splitlines():
                problems.append('%s did not report %s: OK' % (where, expect['archive']))
    if problems:
        fail('\n'.join(problems))
    print('Guide blocks verified:', len(expect['blocks']), '- exit statuses:',
          ' '.join(b['code'] for b in expect['blocks'] if b['kind'] == 'run'))


parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
commands = parser.add_subparsers(dest='command', required=True)
prepare_parser = commands.add_parser('prepare')
prepare_parser.add_argument('guide', type=pathlib.Path)
prepare_parser.add_argument('platform', choices=sorted(ARCHIVES))
prepare_parser.add_argument('downloads')
prepare_parser.add_argument('programs')
prepare_parser.add_argument('tools')
prepare_parser.set_defaults(run=prepare)
verify_parser = commands.add_parser('verify')
verify_parser.add_argument('tools')
verify_parser.set_defaults(run=verify)
arguments = parser.parse_args()
arguments.run(arguments)
