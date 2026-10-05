"""Compiler flags shared by explicitly invoked Halo and codec experiments.

Keep this helper while these runners share incremental-build policy; remove it
when its last consumer is retired. Python owns runner orchestration here, not
compiler semantics or verification.
"""
import os
import sys
from pathlib import Path


def default_cache(experiment):
    base = os.environ.get('WHITEFOOT_CACHE') or str(
        Path(os.environ.get('XDG_CACHE_HOME') or Path.home() / '.cache') / 'whitefoot')
    return Path(base).expanduser() / experiment


def add_arguments(parser, experiment, *, timing=False):
    parser.add_argument('--cache', type=Path, default=default_cache(experiment),
                        help='persistent compiler cache (default: %(default)s)')
    parser.add_argument('--no-cache', action='store_true', help='disable compiler caching')
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--full-lto', dest='full_lto', action='store_true',
                      help='whole-build LTO; incompatible with compiler caching')
    mode.add_argument('--incremental', dest='full_lto', action='store_false',
                      help='cached native build; runtime timings have unvalidated LTO differences')
    parser.set_defaults(full_lto=timing, runtime_timings=timing)


def flags(args, *, check=False):
    if args.full_lto and not check:
        return ['--full-lto']
    if args.no_cache:
        return []
    cache = args.cache.expanduser().resolve()
    root = Path(__file__).resolve().parents[2]
    if cache.is_relative_to(root):
        raise ValueError('compiler cache must be outside the repository')
    if args.runtime_timings and not check:
        print('Cached build: runtime timings have unvalidated differences from full LTO.', file=sys.stderr)
    return ['--cache', str(cache)]
