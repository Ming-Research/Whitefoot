#!/usr/bin/env python3
"""E6 construction check, run on hosted Linux before any timing.

This is a deliberately conservative identity checker, not an equivalence
optimizer. Unsupported dispatch shapes fail construction. ABI registers,
constants, memory offsets, branch topology and external symbol names survive
normalization. Only consistent renaming of nonvolatile integer temporaries
and caller-clobbered r10/r11 is admitted. Never collapse all registers to REG.
The exceptions are the entry-version comparison (`layout_free_difference`),
which reduces registers to a class and is recorded as weaker than identity,
and the one join reload it admits beside that (`join_reload_difference`).
"""
import argparse
import bisect
import copy
from collections import Counter
from dataclasses import dataclass
import json
import hashlib
import struct
from pathlib import Path
import re
import subprocess
import tempfile


class IdentityError(ValueError):
    pass


def manifest_path():
    """The workload manifest of the prototype runner.

    The workflow extracts that runner (frozen at one commit, never committed)
    into research/experiments/par-demand of the workspace; the checker itself
    lives on this branch so the pull request is self-contained.
    """
    for candidate in (Path(__file__).resolve().parents[1] / 'par-demand' / 'manifest.json',
                      Path(__file__).resolve().parent / 'manifest.json'):
        if candidate.exists():
            return candidate
    raise IdentityError('workload manifest not found next to the extracted prototype runner')


def canonical_symbol(name):
    name = retained_function_name(name)
    name = name.removesuffix('@plt')
    if name.startswith('wf__par_seq.'):
        return name[len('wf__par_seq.'):]
    return 'wf_' + name[len('wf__par_seq_'):] if name.startswith('wf__par_seq_') else name


def retained_function_name(name):
    # LLVM's private machine functions use temporary assembler labels. E6
    # retains them as evidence; their language-level names remain unchanged.
    return name[2:] if name.startswith(('.Lwf_', '.Lwf.')) else name


@dataclass(frozen=True)
class Instruction:
    address: int
    mnemonic: str
    operands: str


HEADER = re.compile(r'^\s*([0-9a-fA-F]+) <([^>]+)>:$')
INSN = re.compile(r'^\s*([0-9a-fA-F]+):\s+(.*)$')
TARGET = re.compile(r'(?<![\w$])([0-9a-fA-F]+)\s+<([^>]+)>')


class FunctionTable(dict):
    """Retain every machine function; duplicate spellings are not identities.

    Duplicate ordinals provide conservative seq/demand pairing candidates.
    Call targets resolve by address and their paired bodies must still compare.
    """
    def __init__(self, entries):
        self.starts, self.names, self.by_name = {}, {}, {}
        groups = {}
        for name, start, instructions in entries:
            groups.setdefault(name, []).append((start, instructions))
        for name, bodies in groups.items():
            bodies.sort(key=lambda body: body[0])
            if len({start for start, _ in bodies}) != len(bodies):
                raise IdentityError(f'duplicate disassembly identity {name} at one address')
            keys = []
            for ordinal, (start, instructions) in enumerate(bodies):
                key = name if len(bodies) == 1 else f'{name}#{ordinal}'
                if key in self:
                    raise IdentityError(f'function identity key collision: {key}')
                self[key] = instructions
                self.starts[key], self.names[key] = start, name
                keys.append(key)
            self.by_name[name] = keys

    def __getitem__(self, key):
        if key in self.by_name and len(self.by_name[key]) > 1:
            raise IdentityError(f'ambiguous function name {key}; resolve its target address')
        return super().__getitem__(key)

    def get(self, key, default=None):
        if key in self.by_name and len(self.by_name[key]) > 1:
            raise IdentityError(f'ambiguous function name {key}; resolve its target address')
        return super().get(key, default)

    def resolve(self, address, spelling):
        base, separator, offset = spelling.partition('+')
        base = retained_function_name(base)
        # objdump may print a preceding function or basic-block label rather
        # than the actual callee. The machine address chooses the function.
        exact = [key for key, start in self.starts.items() if start == address]
        if len(exact) == 1:
            return exact[0]
        if exact:
            aliases = [key for key in exact if self.names[key] == base]
            if len(aliases) == 1:
                return aliases[0]
            raise IdentityError(f'ambiguous function aliases at target address {address:x}')
        owners = [key for key, instructions in self.items()
                  if any(instruction.address == address for instruction in instructions)]
        if len(owners) == 1:
            key = owners[0]
            return f'{key}+0x{address - self.starts[key]:x}'
        if owners:
            raise IdentityError(f'ambiguous function owners at target address {address:x}')
        candidates = self.by_name.get(base, [])
        if separator:
            if not re.fullmatch(r'0x[0-9a-fA-F]+', offset):
                raise IdentityError(f'unsupported target offset {spelling}')
            begin = address - int(offset, 16)
        else:
            begin = address
        matches = [key for key in candidates if self.starts[key] == begin]
        if len(matches) == 1:
            return matches[0] + (f'+{offset}' if separator else '')
        if candidates:
            raise IdentityError(f'target {spelling} at {address:x} has no matching function identity')
        if separator:
            raise IdentityError(f'unresolved displaced machine target {spelling} at {address:x}; symbol-retaining image required')
        # PLT and other external entries need not have a bounded ELF function.
        return spelling


def parse_disassembly(text):
    entries = []
    current = None
    for line in text.splitlines():
        header = HEADER.match(line)
        if header:
            current = []
            entries.append((header[2], int(header[1], 16), current))
            continue
        match = INSN.match(line)
        if not match or current is None:
            continue
        rest = re.sub(r'^(?:[0-9a-fA-F]{2}\s+)+', '', match[2]).strip()
        if not rest or re.fullmatch(r'[0-9a-fA-F]{2}(?:\s+[0-9a-fA-F]{2})*', rest):
            continue  # a byte continuation, not an instruction
        pieces = rest.split(None, 1)
        if not re.fullmatch(r'[A-Za-z][A-Za-z0-9_.]*', pieces[0]):
            raise IdentityError(f'unparsed instruction: {line}')
        current.append(Instruction(int(match[1], 16), pieces[0], pieces[1] if len(pieces) > 1 else ''))
    if not entries:
        raise IdentityError('empty disassembly')
    return FunctionTable(entries)


class ElfImage:
    """Read Linux ELF64 evidence without executing an image or another tool."""
    def __init__(self, path):
        self.data = Path(path).read_bytes()
        if self.data[:6] != b'\x7fELF\x02\x01':
            raise IdentityError('construction identity supports little-endian ELF64 only')
        fields = struct.unpack_from('<16sHHIQQQIHHHHHH', self.data)
        self.machine = fields[2]
        self.program_headers = [struct.unpack_from('<IIQQQQQQ', self.data, fields[5] + index * fields[9])
                                for index in range(fields[10])]
        offset, stride, count = fields[6], fields[11], fields[12]
        self.sections = [struct.unpack_from('<IIQQQQIIQQ', self.data, offset + index * stride) for index in range(count)]
        section_names = self.sections[fields[13]]
        names = self.data[section_names[4]:section_names[4] + section_names[5]]
        self.named_sections = {names[section[0]:].split(b'\0', 1)[0].decode(): section for section in self.sections}
        self.symbols = {}
        self.symbol_names = {}
        self.symbol_bindings = {}
        self.symbol_sources = {}
        self.symbol_sections = {}
        records = {}
        for section in self.sections:
            if section[1] != 2:  # SHT_SYMTAB, not the incomplete dynamic table
                continue
            strings = self.sections[section[6]]
            table = self.data[strings[4]:strings[4] + strings[5]]
            file_name, file_ordinal, file_counts = '<unknown>', 0, {}
            for at in range(section[4], section[4] + section[5], section[9]):
                name, info, _, section_index, address, size = struct.unpack_from('<IBBHQQ', self.data, at)
                spelling = table[name:].split(b'\0', 1)[0].decode()
                if info & 15 == 4:  # STT_FILE identifies a local symbol's unit.
                    file_name = spelling
                    file_ordinal = file_counts.get(spelling, 0)
                    file_counts[spelling] = file_ordinal + 1
                    continue
                if info & 15 == 6 and section_index:
                    if section_index >= len(self.sections) or not self.sections[section_index][2] & 0x400:
                        raise IdentityError('defined TLS symbol is not in a TLS section')
                if spelling and section_index and (address or info & 15 == 6):
                    record = (address, size, info & 15, info >> 4, file_name, file_ordinal, section_index)
                    # Multiple symbol tables or aliases can repeat the exact
                    # record, but distinct addresses must all survive.
                    if record not in records.setdefault(spelling, []):
                        records[spelling].append(record)
        for spelling, values in records.items():
            values.sort()
            for ordinal, (address, size, kind, binding, file_name, file_ordinal, section_index) in enumerate(values):
                key = spelling if len(values) == 1 else f'{spelling}#{ordinal}'
                if key in self.symbols:
                    raise IdentityError(f'ELF symbol identity key collision: {key}')
                self.symbols[key] = (address, size, kind)
                self.symbol_names[key] = spelling
                self.symbol_bindings[key] = binding
                self.symbol_sources[key] = (file_name, file_ordinal)
                self.symbol_sections[key] = section_index
        self.relocation_slots = self.external_object_slots()
        if not self.symbols:
            raise IdentityError('ELF symbol table absent; cannot bound functions')

    def bound(self, functions):
        # Retaining temporary labels makes objdump split a function at .LBB
        # headers. Rebuild each complete body from its exact ELF range.
        flat = {}
        for instructions in functions.values():
            for instruction in instructions:
                if instruction.address in flat and flat[instruction.address] != instruction:
                    raise IdentityError(f'conflicting disassembly at {instruction.address:x}')
                flat[instruction.address] = instruction
        symbols = [(retained_function_name(getattr(self, 'symbol_names', {}).get(key, key)), address, size)
                   for key, (address, size, kind) in self.symbols.items() if kind == 2 and size]
        if isinstance(functions, FunctionTable):
            for name, keys in functions.by_name.items():
                if len(keys) > 1:
                    for key in keys:
                        start = functions.starts[key]
                        bounded_function = any(candidate == retained_function_name(name) and address == start
                                               for candidate, address, _ in symbols)
                        # The same temporary-label spelling can occur in
                        # several translation units. Admit a fragment only
                        # with an exact STT_NOTYPE record; spelling alone
                        # never supplies bounds. End labels can precede
                        # padding outside a function's exclusive ELF end.
                        retained_fragment = any(kind == 0 and address == start
                                                and getattr(self, 'symbol_names', {}).get(symbol_key, symbol_key) == name
                                                for symbol_key, (address, _, kind) in self.symbols.items())
                        if not bounded_function and not retained_fragment:
                            raise IdentityError(f'duplicate local function {name} at {functions.starts[key]:x} lacks exact ELF bounds')
        entries = []
        ranges = {}
        for name, address, size in symbols:
            identity = name, address
            if identity in ranges:
                if ranges[identity] != size:
                    raise IdentityError(f'conflicting ELF function bounds for {name} at {address:x}')
                continue
            ranges[identity] = size
            instructions = [instruction for at, instruction in sorted(flat.items()) if address <= at < address + size]
            if not instructions or instructions[0].address != address:
                raise IdentityError(f'ELF function {name} at {address:x} lacks entry disassembly')
            entries.append((name, address, instructions))
        self.functions = FunctionTable(entries)
        return self.functions

    def local_object_key(self, key):
        binding = getattr(self, 'symbol_bindings', {}).get(key, 1)
        if binding != 0:
            return canonical_symbol(key)
        source = getattr(self, 'symbol_sources', {}).get(key)
        if source is None or source[0] in ('', '<unknown>'):
            raise IdentityError('local object lacks translation-unit provenance')
        spelling = self.symbol_names[key]
        peers = sorted((start, symbol_key) for symbol_key, (start, _, kind) in self.symbols.items()
                       if kind == self.symbols[key][2] and self.symbol_names[symbol_key] == spelling
                       and self.symbol_sources[symbol_key] == source)
        ordinal = next(index for index, (_, symbol_key) in enumerate(peers) if symbol_key == key)
        suffix = f'#{ordinal}' if len(peers) > 1 else ''
        return f'{source[0]}#{source[1]}::{spelling}{suffix}'

    def external_object_slots(self):
        """Exact dynamic GLOB_DAT slots, never a nearest GOT label heuristic."""
        slots = {}
        for section in self.sections:
            if section[1] != 4:  # SHT_RELA
                continue
            symbols = self.sections[section[6]]
            if symbols[1] != 11:  # Only linked dynamic relocations are supported.
                continue
            strings = self.sections[symbols[6]]
            names = self.data[strings[4]:strings[4] + strings[5]]
            for at in range(section[4], section[4] + section[5], section[9]):
                address, info, addend = struct.unpack_from('<QQq', self.data, at)
                if info & 0xffffffff != 6:  # R_X86_64_GLOB_DAT, eight-byte slot
                    continue
                if self.machine != 62 or symbols[9] != 24:
                    raise IdentityError('unsupported dynamic object relocation ABI')
                index = info >> 32
                if index * symbols[9] >= symbols[5]:
                    raise IdentityError('dynamic object relocation symbol is absent')
                name, symbol_info, _, defined, _, _ = struct.unpack_from(
                    '<IBBHQQ', self.data, symbols[4] + index * symbols[9])
                # Internal addresses need pointee/code identity, not external
                # binding normalization. Do not erase their relocations.
                if defined or symbol_info & 15 != 1:
                    continue
                spelling = names[name:].split(b'\0', 1)[0].decode()
                if not spelling:
                    raise IdentityError('dynamic object relocation has no symbol name')
                identity = (6, spelling, symbol_info >> 4, addend,
                            self.external_version(section[6], index))
                if address in slots and slots[address] != identity:
                    raise IdentityError('conflicting dynamic object relocations')
                slots[address] = identity
        return slots

    def external_version(self, symbol_table, symbol_index):
        versions = [section for section in self.sections
                    if section[1] == 0x6fffffff and section[6] == symbol_table]
        if not versions:
            return None
        if len(versions) != 1 or (symbol_index + 1) * 2 > versions[0][5]:
            raise IdentityError('dynamic object symbol version is absent')
        version = struct.unpack_from('<H', self.data, versions[0][4] + symbol_index * 2)[0]
        if version & 0x7fff <= 1:
            return ('unversioned', version)
        matches = []
        for section in self.sections:
            if section[1] != 0x6ffffffe:  # SHT_GNU_verneed
                continue
            strings = self.sections[section[6]]
            names = self.data[strings[4]:strings[4] + strings[5]]
            at, end = section[4], section[4] + section[5]
            while at < end:
                if at + 16 > end:
                    raise IdentityError('truncated ELF version requirement')
                format_version, count, library, auxiliary, following = struct.unpack_from('<HHIII', self.data, at)
                if format_version != 1:
                    raise IdentityError('unsupported ELF version requirement')
                dependency = names[library:].split(b'\0', 1)[0].decode()
                aux = at + auxiliary
                for _ in range(count):
                    if aux < at or aux + 16 > end:
                        raise IdentityError('truncated ELF symbol version')
                    hash_value, flags, other, name, next_aux = struct.unpack_from('<IHHII', self.data, aux)
                    if other == version & 0x7fff:
                        matches.append((dependency, names[name:].split(b'\0', 1)[0].decode(),
                                        hash_value, flags, bool(version & 0x8000)))
                    aux += next_aux
                if not following:
                    break
                at += following
        if len(matches) != 1:
            raise IdentityError('unresolved dynamic object symbol version')
        return matches[0]

    def readonly_address_identity(self, address):
        objects = [(key, start, length) for key, (start, length, kind) in self.symbols.items()
                   if kind == 1 and length and start <= address < start + length]
        if not objects:
            raise IdentityError('readonly address lacks exact retained object bounds; symbol-retaining image required')
        identities = []
        for key, start, length in objects:
            section = next((section for section in self.sections if not section[2] & 0x400
                            and section[3] <= start and start + length <= section[3] + section[5]), None)
            if section is None or section[2] & 1 or section[1] == 8:
                raise IdentityError('readonly object bounds are not wholly immutable file data')
            offset = section[4] + start - section[3]
            content = self.data[offset:offset + length]
            if len(content) != length:
                raise IdentityError('readonly object data incomplete')
            identities.append((self.local_object_key(key), address - start, length, content.hex()))
        # Include all overlapping names, rather than collapsing equal strings.
        # True aliases share this set; distinct equal objects have different
        # names, and suffix aliases retain their offset within the larger one.
        return 'readonly-address:' + json.dumps(sorted(identities), separators=(',', ':'))

    def code_identity(self, address, spelling):
        if hasattr(self, 'functions'):
            return canonical_symbol(self.functions.resolve(address, spelling))
        # Object-only negative self-tests do not construct an ELF image.
        raise IdentityError('ELF function identities have not been bounded')

    def object_identity(self, address):
        objects = [(key, start, length) for key, (start, length, kind) in self.symbols.items()
                   if kind == 1 and length and start <= address < start + length]
        if not objects:
            raise IdentityError('unbounded mutable RIP object identity')
        # Overlapping symbol objects require an alias model, not a smallest
        # name heuristic that could hide different pointer relationships.
        ranges = {(start, length) for _, start, length in objects}
        if len(ranges) != 1:
            raise IdentityError('overlapping mutable ELF object identities')
        _, start, length = min(objects)
        names = sorted(self.local_object_key(key) for key, _, _ in objects)
        name = names[0] if len(names) == 1 else 'aliases:' + json.dumps(names, separators=(',', ':'))
        return name, address - start, length

    def tls_identity(self, displacement, instruction, address_only=False):
        """Resolve x86-64 variant-II local-exec offsets in the main TLS block."""
        segments = [header for header in getattr(self, 'program_headers', ()) if header[0] == 7]
        if getattr(self, 'machine', None) != 62 or len(segments) != 1:
            raise IdentityError('TLS reference requires one x86-64 PT_TLS segment')
        _, _, file_offset, address, _, file_size, memory_size, alignment = segments[0]
        # The supported hosted images have an aligned TLS template. A template
        # with a nonzero first-byte alignment offset needs a different proof.
        if alignment == 0 or alignment & (alignment - 1) or address % alignment:
            raise IdentityError('unsupported PT_TLS template alignment')
        block_size = (memory_size + alignment - 1) & -alignment
        offset = displacement + block_size
        objects = [(key, start, length) for key, (start, length, kind) in self.symbols.items()
                   if kind == 6 and length and start <= offset < start + length]
        if not objects:
            raise IdentityError(f'unbounded TLS object at thread offset {displacement}')
        if len({(start, length) for _, start, length in objects}) != 1:
            raise IdentityError('overlapping TLS object identities')
        _, start, length = objects[0]
        if start + length > memory_size or file_size > memory_size:
            raise IdentityError('TLS object exceeds PT_TLS bounds')
        if address_only:
            if instruction.mnemonic not in ('lea', 'leaq'):
                raise IdentityError('unsupported TLS address formation')
        else:
            if instruction.mnemonic in ('lea', 'leal', 'leaq'):
                raise IdentityError('unsupported address-valued TLS instruction')
            width = memory_access_width(instruction)
            if width is None or offset + width > start + length:
                raise IdentityError('TLS memory access crosses named object boundary')
        for key, _, _ in objects:
            section_index = getattr(self, 'symbol_sections', {}).get(key)
            if section_index is not None and (section_index >= len(self.sections)
                                             or not self.sections[section_index][2] & 0x400):
                raise IdentityError('TLS symbol is not in a TLS section')
        initialized = max(0, min(length, file_size - start))
        content = self.data[file_offset + start:file_offset + start + initialized]
        if len(content) != initialized:
            raise IdentityError('TLS initialized object data incomplete')
        content += bytes(length - initialized)
        names = sorted(self.local_object_key(key) for key, _, _ in objects)
        prefix = 'tls-address:' if address_only else 'tls:'
        return prefix + json.dumps((names, offset - start, length, content.hex()), separators=(',', ':'))

    def reference_identity(self, address, instruction, spelling):
        # Equal immutable contents do not imply equal pointer identity. Mutable
        # addresses use the exact bounded object and offset instead of contents.
        section = next((section for section in self.sections if not section[2] & 0x400
                        and section[3] <= address < section[3] + section[5]), None)
        if section is None:
            raise IdentityError(f'RIP address {address:x} is outside ELF data sections')
        if not section[2] & 1:  # immutable constants use bytes, not anonymous pool placement
            if instruction.mnemonic in ('lea', 'leal', 'leaq'):
                return self.readonly_address_identity(address)
            if instruction.mnemonic.startswith(('call', 'jmp')):
                raise IdentityError('unsupported address-valued RIP reference')
            content = self.reference_bytes(address, instruction).hex()
            return 'readonly:' + content
        # Mutable storage must keep an actual object identity, not a section's
        # nearest label. Source-memory aliasing is not inferred from bytes.
        slots = [(start, identity) for start, identity in getattr(self, 'relocation_slots', {}).items()
                 if start <= address < start + 8]
        if slots:
            if len(slots) != 1:
                raise IdentityError('overlapping dynamic object relocation slots')
            start, identity = slots[0]
            width = memory_access_width(instruction)
            if width is None or address + width > start + 8 or start + 8 > section[3] + section[5]:
                raise IdentityError('unsupported dynamic object slot access bounds')
            offset = section[4] + start - section[3]
            content = self.data[offset:offset + 8]
            if len(content) != 8:
                raise IdentityError('dynamic object slot data incomplete')
            return 'external-object-slot:' + json.dumps((identity, address - start, content.hex()), separators=(',', ':'))
        key, displacement, extent = self.object_identity(address)
        if instruction.mnemonic.startswith(('call', 'jmp')):
            raise IdentityError('unsupported indirect RIP call or jump')
        content = self.reference_bytes(address, instruction).hex()
        return f'{key}+0x{displacement:x}/size={extent}:' + content

    def reference_bytes(self, address, instruction):
        for section in self.sections:
            _, kind, flags, begin, offset, size, *_ = section
            if flags & 0x400:  # TLS template VAs can overlap ordinary data VAs.
                continue
            if begin <= address < begin + size:
                available = begin + size - address
                objects = [(start, length) for start, length, kind in self.symbols.values() if kind == 1 and length and start <= address < start + length]
                access = memory_access_width(instruction)
                if objects:
                    start, length = min(objects, key=lambda obj: obj[1])
                    extent = start + length - address
                    if access is not None and access > extent:
                        raise IdentityError('RIP memory access crosses named object boundary')
                else:
                    if access is None:
                        raise IdentityError('unbounded address-valued RIP reference')
                    extent = access
                if extent > available:
                    raise IdentityError('RIP object crosses section boundary')
                if kind == 8:  # SHT_NOBITS is initialized to zero, not file bytes.
                    self.object_identity(address)
                    return bytes(extent)
                at = offset + address - begin
                return self.data[at:at + extent]
        raise IdentityError(f'RIP address {address:x} is outside ELF data sections')


def memory_access_width(instruction):
    """Bound supported scalar/vector RIP accesses; unknown widths reject."""
    opcode = instruction.mnemonic
    if opcode in ('lock', 'rep', 'repe', 'repne'):
        opcode = instruction.operands.split(None, 1)[0]
    if opcode in ('lea', 'leal', 'leaq'):
        return None  # address formation observes no bytes
    if opcode.startswith(('call', 'jmp')):
        raise IdentityError('unsupported indirect RIP call or jump')
    # Conversion names describe both input and output types. Match their
    # memory INPUT explicitly; the final suffix alone is not a load width.
    double_conversions = ('cvtsd2ss', 'cvtsd2si', 'cvtsd2siq', 'cvttsd2si',
                          'cvttsd2siq')
    single_conversions = ('cvtss2sd', 'cvtss2si', 'cvtss2siq', 'cvttss2si',
                          'cvttss2siq')
    scalar_double = ('movsd', 'addsd', 'subsd', 'mulsd', 'divsd', 'minsd',
                     'maxsd', 'sqrtsd', 'comisd', 'ucomisd', 'roundsd')
    scalar_single = ('movss', 'addss', 'subss', 'mulss', 'divss', 'minss',
                     'maxss', 'sqrtss', 'comiss', 'ucomiss', 'roundss')
    base_opcode = opcode[1:] if opcode.startswith('v') else opcode
    if base_opcode in double_conversions + scalar_double:
        return 8
    if base_opcode in single_conversions + scalar_single:
        return 4
    if opcode in ('cmpxchg16b', 'cmpxchg8b'):
        return 16 if opcode == 'cmpxchg16b' else 8
    vectors = re.findall(r'%(xmm|ymm|zmm)[0-9]+', instruction.operands)
    if vectors:
        if opcode in ('movq', 'vmovq'):
            return 8
        if opcode in ('movd', 'vmovd'):
            return 4
        packed_moves = ('movaps', 'movups', 'movapd', 'movupd', 'movdqa',
                        'movdqu', 'movdqa32', 'movdqa64', 'movdqu8',
                        'movdqu16', 'movdqu32', 'movdqu64')
        packed_arithmetic = tuple(operation + kind for operation in
                                  ('add', 'sub', 'mul', 'div', 'min', 'max',
                                   'sqrt', 'and', 'andn', 'or', 'xor',
                                   'shuf', 'unpckh', 'unpckl', 'blend')
                                  for kind in ('ps', 'pd'))
        if base_opcode in packed_moves + packed_arithmetic:
            return max({'xmm': 16, 'ymm': 32, 'zmm': 64}[vector] for vector in vectors)
        raise IdentityError(f'unsupported RIP vector access width: {opcode}')
    if opcode in ('movsbl', 'movsbw', 'movsbq', 'movzbl', 'movzbw', 'movzbq'):
        return 1
    if opcode in ('movswl', 'movswq', 'movzwl', 'movzwq'):
        return 2
    if opcode in ('movslq', 'movsxd'):
        return 4
    if opcode in ('fldl', 'fstl', 'fstpl'):
        return 8
    if opcode in ('fldt', 'fstpt'):
        return 10
    widths = {'b': 1, 'w': 2, 'l': 4, 'q': 8}
    scalar_opcodes = ('mov', 'add', 'sub', 'cmp', 'test', 'and', 'or', 'xor',
                      'xadd', 'xchg', 'cmpxchg', 'imul', 'mul', 'div', 'idiv',
                      'adc', 'sbb', 'inc', 'dec', 'neg', 'not', 'shl', 'shr',
                      'sal', 'sar', 'rol', 'ror')
    if opcode[-1:] in widths and opcode[:-1] in scalar_opcodes:
        return widths[opcode[-1]]
    registers = re.findall(r'%([A-Za-z][A-Za-z0-9]*)', instruction.operands.split('#', 1)[0])
    registers = [register for register in registers if register not in ('rip', 'fs')]
    sizes = set()
    for register in registers:
        if re.fullmatch(r'r(?:[abcd]x|[sd]i|[sb]p|[89]|1[0-5])', register):
            sizes.add(8)
        elif re.fullmatch(r'e(?:[abcd]x|[sd]i|[sb]p)|r(?:[89]|1[0-5])d', register):
            sizes.add(4)
        elif re.fullmatch(r'[abcd]x|[sd]i|[sb]p|r(?:[89]|1[0-5])w', register):
            sizes.add(2)
        elif re.fullmatch(r'[abcd][lh]|[sd]il|[sb]pl|r(?:[89]|1[0-5])b', register):
            sizes.add(1)
        else:
            raise IdentityError(f'unsupported RIP operand register width: {register}')
    # Shift/rotate count registers (usually cl) do not carry the memory
    # operand's width. Those operations require an explicit size suffix.
    inferred_opcodes = tuple(opcode for opcode in scalar_opcodes
                             if opcode not in ('shl', 'shr', 'sal', 'sar', 'rol', 'ror'))
    if len(sizes) == 1 and opcode in inferred_opcodes:
        return sizes.pop()
    raise IdentityError(f'unsupported RIP memory access width: {opcode}')


def target(insn, functions=None):
    match = TARGET.search(insn.operands.split('#', 1)[0])
    if not match:
        return None
    address, spelling = int(match[1], 16), match[2]
    return address, functions.resolve(address, spelling) if isinstance(functions, FunctionTable) else spelling


def assembly_edges(instructions):
    index = {insn.address: at for at, insn in enumerate(instructions)}
    edges = {}
    for at, insn in enumerate(instructions):
        destinations = []
        if insn.mnemonic.startswith('j'):
            dest = target(insn)
            if not dest:
                raise IdentityError(f'indirect/unresolved jump at {insn.address:x}')
            if dest[0] in index:
                destinations.append(index[dest[0]])
            elif insn.mnemonic != 'jmp':
                raise IdentityError(f'conditional jump outside function at {insn.address:x}')
        if not insn.mnemonic.startswith('ret') and insn.mnemonic != 'jmp' and at + 1 < len(instructions):
            destinations.append(at + 1)
        edges[at] = destinations
    return edges


def reachable(edges, start=0):
    seen, todo = set(), [start]
    while todo:
        node = todo.pop()
        if node in seen:
            continue
        seen.add(node)
        todo.extend(edges.get(node, ()))
    return seen


def on_cycle(edges, node):
    return any(node in reachable(edges, successor) for successor in edges.get(node, ()))


# SysV arguments/return registers, stack, flags and all vector registers stay
# pinned. RBP stays pinned when it has a frame role. Nonvolatile permutations
# otherwise preserve call effects;
# scratch r10/r11 may permute only within their caller-clobbered class.
REGISTERS = {
    'rbx': ('saved', ('rbx', 'ebx', 'bx', 'bl')),
    'rbp': ('saved', ('rbp', 'ebp', 'bp', 'bpl')),
    **{f'r{n}': ('saved', (f'r{n}', f'r{n}d', f'r{n}w', f'r{n}b')) for n in range(12, 16)},
    **{f'r{n}': ('scratch', (f'r{n}', f'r{n}d', f'r{n}w', f'r{n}b')) for n in (10, 11)},
}
ALIASES = {alias: (base, group, size) for base, (group, aliases) in REGISTERS.items() for size, alias in enumerate(aliases)}


INTEGER_REGISTER_ALIASES = {
    **{alias: 'r' + base for base, aliases in (
        ('ax', ('rax', 'eax', 'ax', 'al', 'ah')),
        ('bx', ('rbx', 'ebx', 'bx', 'bl', 'bh')),
        ('cx', ('rcx', 'ecx', 'cx', 'cl', 'ch')),
        ('dx', ('rdx', 'edx', 'dx', 'dl', 'dh')),
        ('si', ('rsi', 'esi', 'si', 'sil')),
        ('di', ('rdi', 'edi', 'di', 'dil')),
        ('bp', ('rbp', 'ebp', 'bp', 'bpl')),
        ('sp', ('rsp', 'esp', 'sp', 'spl')),
    ) for alias in aliases},
    **{alias: f'r{number}' for number in range(8, 16)
       for alias in (f'r{number}', f'r{number}d', f'r{number}w', f'r{number}b')},
}
FULL_INTEGER_REGISTER = re.compile(r'%(r[abcd]x|r[sd]i|rbp|rsp|r(?:[89]|1[0-5]))$')
TLS_LEA_ADDRESS = re.compile(r'(-?0x[0-9a-fA-F]+|-?\d+)?\(%([A-Za-z][A-Za-z0-9]*)\)$')


def thread_pointer_states(instructions):
    """Must-provenance of Linux x86-64 FS:0 thread-pointer register values.

    Analyze the original CFG, including instructions omitted by dispatch
    projection. Only exact full-width copies/address formation add facts;
    joins retain facts agreed by every path. Unsupported effects clear facts.
    """
    if not any(re.search(r'%fs:(?:0x0|0),%', insn.operands.replace(' ', ''))
               for insn in instructions):
        return {}
    indexes = {insn.address: at for at, insn in enumerate(instructions)}
    if any(insn.mnemonic.startswith('loop') for insn in instructions):
        raise IdentityError('unsupported loop control flow in TLS provenance proof')

    def successors(at):
        insn = instructions[at]
        result = []
        if insn.mnemonic.startswith('j'):
            dest = target(insn)
            if dest is None:
                raise IdentityError('unresolved control flow in TLS provenance proof')
            if dest[0] in indexes:
                result.append(indexes[dest[0]])
        if (not insn.mnemonic.startswith('ret') and insn.mnemonic != 'jmp'
                and at + 1 < len(instructions)):
            result.append(at + 1)
        return result

    def transfer(insn, incoming):
        state = dict(incoming)
        opcode = insn.mnemonic
        operands = insn.operands.split('#', 1)[0].strip()
        if opcode == 'lock':
            parts = operands.split(None, 1)
            opcode, operands = parts[0], parts[1] if len(parts) == 2 else ''
        parts = [part.strip() for part in re.split(r',(?![^()]*\))', operands)]

        def forget(operand):
            if re.fullmatch(r'%[A-Za-z][A-Za-z0-9]*', operand):
                state.pop(INTEGER_REGISTER_ALIASES.get(operand[1:]), None)

        if (opcode.startswith(('j', 'ret', 'nop'))
                or opcode in ('cmp', 'cmpb', 'cmpw', 'cmpl', 'cmpq',
                              'test', 'testb', 'testw', 'testl', 'testq')):
            return state
        if opcode in ('pause', 'endbr64'):
            return state
        if opcode.startswith('call'):
            return {}  # Conservative even for nonvolatile registers.
        if opcode in ('mov', 'movq', 'movabs', 'movabsq') and len(parts) == 2:
            source, destination = parts
            forget(destination)
            full = FULL_INTEGER_REGISTER.fullmatch(destination)
            if full:
                if source in ('%fs:0', '%fs:0x0'):
                    state[full[1]] = 0
                elif FULL_INTEGER_REGISTER.fullmatch(source) and source[1:] in incoming:
                    state[full[1]] = incoming[source[1:]]
            return state
        if opcode in ('lea', 'leaq') and len(parts) == 2:
            address, destination = parts
            forget(destination)
            full, base = FULL_INTEGER_REGISTER.fullmatch(destination), TLS_LEA_ADDRESS.fullmatch(address)
            if full and base and FULL_INTEGER_REGISTER.fullmatch('%' + base[2]) and base[2] in incoming:
                offset = int(base[1], 0) if base[1] and '0x' in base[1] else int(base[1] or '0')
                state[full[1]] = incoming[base[2]] + offset
            return state
        if opcode in ('xadd', 'xaddb', 'xaddw', 'xaddl', 'xaddq', 'xchg', 'xchgb', 'xchgw', 'xchgl', 'xchgq'):
            for operand in parts:
                forget(operand)
            return state
        extensions = ('movb', 'movw', 'movl', 'movsbl', 'movsbw', 'movsbq',
                      'movzbl', 'movzbw', 'movzbq', 'movswl', 'movswq',
                      'movzwl', 'movzwq', 'movslq', 'movsxd')
        conditions = ('a', 'ae', 'b', 'be', 'c', 'e', 'g', 'ge', 'l', 'le',
                      'na', 'nae', 'nb', 'nbe', 'nc', 'ne', 'ng', 'nge',
                      'nl', 'nle', 'no', 'np', 'ns', 'nz', 'o', 'p', 'pe',
                      'po', 's', 'z')
        cmoves = tuple('cmov' + condition + suffix for condition in conditions for suffix in ('', 'w', 'l', 'q'))
        sets = tuple('set' + condition for condition in conditions)
        if ((opcode in extensions + cmoves and len(parts) == 2)
                or (opcode in sets and len(parts) == 1)):
            forget(parts[-1])
            return state
        ordinary = ('add', 'sub', 'and', 'or', 'xor', 'adc', 'sbb', 'shl', 'shr', 'sal', 'sar', 'rol', 'ror', 'inc', 'dec', 'neg', 'not')
        if opcode in ordinary or any(opcode == family + suffix for family in ordinary for suffix in 'bwlq'):
            forget(parts[-1])
            return state
        if opcode in ('push', 'pushq'):
            state.pop('rsp', None)
            return state
        if opcode in ('pop', 'popq'):
            forget(parts[-1])
            state.pop('rsp', None)
            return state
        return {}  # Implicit/unknown register writes must not leave stale facts.

    incoming, outgoing, pending = {0: {}}, {}, [0]
    while pending:
        at = pending.pop()
        after = transfer(instructions[at], incoming[at])
        if outgoing.get(at) == after:
            continue
        outgoing[at] = after
        for successor in successors(at):
            before = incoming.get(successor)
            merged = after if before is None else {name: value for name, value in before.items()
                                                   if after.get(name) == value}
            if before != merged:
                incoming[successor] = dict(merged)
                pending.append(successor)
    return incoming


def normalized(instructions, selected=None, omitted=(), image=None, forced=None):
    """Preserve instruction order and label destinations, not absolute PCs.

    `forced` maps an omitted branch to the one destination a projection
    keeps; one that is not the next kept instruction becomes a `jmp` there,
    so the rows keep the projection's control flow.
    """
    selected = set(range(len(instructions))) if selected is None else set(selected)
    omitted = set(omitted)
    live = [at for at in sorted(selected) if at not in omitted]
    jumps = {}
    for branch, destination in (forced or {}).items():
        following = next((at for at in live if at > branch), None)
        if branch not in omitted or destination not in live:
            raise IdentityError('a forced edge must leave an omitted branch for a kept instruction')
        if destination != following:
            jumps[branch] = destination
    live = sorted(set(live) | set(jumps))
    labels = {instructions[at].address: f'I{index}' for index, at in enumerate(live)}
    tls_states = thread_pointer_states(instructions)
    renaming = {}
    counts = {'saved': 0, 'scratch': 0}
    # A backend can allocate RBP as an ordinary nonvolatile temporary. It is
    # not interchangeable with saved registers when frame instructions give
    # it an implicit role or when it transports the stack pointer. Inspect
    # the whole function, even when normalizing a projected subset.
    frame_pointer = any(
        re.search(r'(?:^|\s)(?:enter|leave)[wlq]?(?:\s|$)',
                  insn.mnemonic + ' ' + insn.operands.split('#', 1)[0])
        or (re.search(r'%(?:rbp|ebp|bp|bpl)\b', insn.operands)
            and re.search(r'%(?:rsp|esp|sp|spl)\b', insn.operands))
        for insn in instructions)

    def register(match):
        name = match[1]
        if name not in ALIASES:
            return '%' + name
        base, group, size = ALIASES[name]
        if base == 'rbp' and frame_pointer:
            return '%' + name
        if base not in renaming:
            renaming[base] = f'{group}{counts[group]}'
            counts[group] += 1
        return '%' + renaming[base] + f'.{size}'

    rows = []
    for at in live:
        insn = instructions[at]
        if at in jumps:
            rows.append(f'jmp {labels[instructions[jumps[at]].address]}')
            continue
        if insn.mnemonic in ('syscall', 'sysenter', 'int', 'int3'):
            raise IdentityError('unsupported implicit-register system instruction')
        operands, _, comment = insn.operands.partition('#')
        operands = operands.strip()
        if insn.mnemonic in ('lea', 'leaq'):
            parts = [part.strip() for part in re.split(r',(?![^()]*\))', operands)]
            address = TLS_LEA_ADDRESS.fullmatch(parts[0]) if len(parts) == 2 else None
            state = tls_states.get(at, {})
            if address and address[2] in state and FULL_INTEGER_REGISTER.fullmatch(parts[1]):
                if image is None:
                    raise IdentityError('TLS address formation requires ELF data evidence')
                offset = int(address[1], 0) if address[1] and '0x' in address[1] else int(address[1] or '0')
                identity = image.tls_identity(state[address[2]] + offset, insn, address_only=True)
                operands = f'[{identity}](%{address[2]}),{parts[1]}'
        dest = target(insn)
        if dest:
            address, symbol = dest
            replacement = labels.get(address)
            if replacement is None:
                # A local target must remain a local instruction identity. Never
                # hide a changed edge behind the containing function's name.
                if image is not None:
                    replacement = image.code_identity(address, symbol)
                elif '+' in symbol:
                    base, offset = symbol.rsplit('+', 1)
                    if base == symbol or not re.fullmatch(r'0x[0-9a-fA-F]+', offset):
                        raise IdentityError(f'unsupported target {symbol}')
                    replacement = canonical_symbol(base) + '+' + offset
                else:
                    replacement = canonical_symbol(symbol)
            operands = TARGET.sub(replacement, operands)
        if '%rip' in operands:
            reference = TARGET.search(comment)
            if not reference:
                raise IdentityError(f'unresolved RIP reference at {insn.address:x}')
            # Preserve the named object AND its offset. A changed constant-pool
            # offset is conservatively a mismatch, never erased as placement.
            reference_name = canonical_symbol(reference[2])
            if image is None:
                raise IdentityError('RIP reference requires ELF data evidence')
            reference_name = image.reference_identity(int(reference[1], 16), insn, reference_name)
            operands = re.sub(r'[-+]?0x[0-9a-fA-F]+\(%rip\)|[-+]?\d+\(%rip\)', f'[{reference_name}](%rip)', operands)
        tls = re.search(r'%fs:(-?0x[0-9a-fA-F]+|-?\d+)(?=,|\s|$)', operands)
        if '%fs:' in operands and tls is None:
            raise IdentityError('unsupported indexed or symbolic TLS reference')
        if tls:
            displacement = int(tls[1], 0) if '0x' in tls[1] else int(tls[1])
            if displacement < -(1 << 63) or displacement >= 1 << 64:
                raise IdentityError('TLS displacement is outside x86-64 address bounds')
            if displacement >= 1 << 63:
                displacement -= 1 << 64
            if displacement < 0:
                if image is None:
                    raise IdentityError('TLS reference requires ELF data evidence')
                identity = image.tls_identity(displacement, insn)
                operands = operands[:tls.start()] + f'%fs:[{identity}]' + operands[tls.end():]
        operands = re.sub(r'%([A-Za-z][A-Za-z0-9]*)', register, operands)
        operands = re.sub(r'\s+', ' ', operands)
        rows.append(f'{insn.mnemonic} {operands}'.rstrip())
    return rows


def first_difference(left, right):
    for index in range(max(len(left), len(right))):
        a = left[index] if index < len(left) else '<end>'
        b = right[index] if index < len(right) else '<end>'
        if a != b:
            return {'instruction': index + 1, 'seq': a, 'demand': b}
    return None


# Entry-version shapes of the default --par (stage 3 step 6).
#
# Choosing the version once at each function's entry puts the sequential path
# and the parallel path in one machine function, so the register allocator
# gives the sequential path other saved registers, spill slots and frame size
# than the canonical sequential function has, and the optimizer may inline a
# cheap region's sequential clone into that function and lay out its blocks
# in another order. The strict comparison above can then never be textually
# equal. `layout_free_difference` compares what the allocator and the block
# layout may not change: opcodes, immediates, memory shapes, call targets and
# branch topology. It is deliberately weaker than machine identity; callers
# record that.

def _split_operands(text):
    parts, depth, current = [], 0, ''
    for char in text:
        depth += (char == '(') - (char == ')')
        if char == ',' and depth == 0:
            parts.append(current)
            current = ''
        else:
            current += char
    return parts + [current] if current else parts


def _register(operand):
    return re.fullmatch(r'%[a-z][\w.]*', operand) is not None and operand not in ('%rsp', '%rip')


def _slot(operand):
    return re.fullmatch(r'(?:-?0x[0-9a-f]+)?\(%rsp\)', operand) is not None


NOISE = re.compile(r'(?:(?:data16|cs|ds) )*(?:nop[wlq]?|xchg %ax,%ax|endbr64)(?: |$)')
LOCAL_TARGET = re.compile(r'I(\d+)')
# Each condition with the one that holds exactly when it does not.
INVERSE_CONDITION = {}
for _left, _right in (('e', 'ne'), ('z', 'nz'), ('a', 'be'), ('ae', 'b'), ('g', 'le'), ('ge', 'l'),
                      ('s', 'ns'), ('o', 'no'), ('p', 'np')):
    INVERSE_CONDITION['j' + _left], INVERSE_CONDITION['j' + _right] = 'j' + _right, 'j' + _left


def _plumbing(line):
    """Allocator and layout plumbing the shape comparison leaves out.

    Callee-saved pushes and pops, constant stack-pointer adjustments,
    register-to-register moves, register/stack-slot moves, padding, and an
    unconditional jump to a local instruction, which is block placement.
    """
    head, _, rest = line.partition(' ')
    operands = _split_operands(rest.strip())
    if NOISE.match(line):
        return True
    if head == 'jmp' and LOCAL_TARGET.fullmatch(rest.strip()):
        return True
    if head in ('push', 'pop') and len(operands) == 1 and _register(operands[0]):
        return True
    if head in ('sub', 'add') and len(operands) == 2 and operands[1] == '%rsp' and operands[0].startswith('$'):
        return True
    if head == 'mov' and len(operands) == 2:
        first, second = operands
        if (_register(first) and (_register(second) or _slot(second))) or (_slot(first) and _register(second)):
            return True
    if head in ('movaps', 'movapd', 'movdqa', 'movups', 'movupd', 'movdqu') and len(operands) == 2:
        first, second = operands
        if ((first.startswith('%xmm') and (second.startswith('%xmm') or _slot(second)))
                or (_slot(first) and second.startswith('%xmm'))):
            return True
    return False


def _shape(line):
    """Register names reduced to a class, stack offsets erased; the rest kept.

    A zero displacement is dropped: an RBP or R13 base must encode one, so
    objdump prints `0x0(` for exactly the address another base prints as `(`.
    """
    text = re.sub(r'(?<![\w$])0x0\(', '(', line)
    text = re.sub(r'%[xy]mm\d+', '%X', text)
    text = re.sub(r'%(?!rsp\b|rip\b|X\b)[a-z][\w.]*', '%R', text)
    return re.sub(r'(?:-?0x[0-9a-f]+)?\(%rsp', 'S(%rsp', text)


def line_successors(lines, at):
    """Successor line indices of one normalized line, a branch's taken edge first.

    A conditional branch's missing fall-through is None. A jump that names no
    local instruction leaves the compared code (a tail transfer); an indirect
    one has unknown destinations and is refused.
    """
    head, _, rest = lines[at].partition(' ')
    local = LOCAL_TARGET.fullmatch(rest.strip())
    following = at + 1 if at + 1 < len(lines) else None
    if head.startswith('j'):
        if rest.strip().startswith('*'):
            raise IdentityError(f'indirect jump in compared code: {lines[at]}')
        if head == 'jmp':
            return [int(local[1])] if local else []
        if not local:
            raise IdentityError(f'conditional branch leaves the compared code: {lines[at]}')
        return [int(local[1]), following]
    if head.startswith('ret') or head in ('ud2', 'hlt'):
        return []
    return [following]


def line_reach(lines, roots):
    seen, todo = set(), [root for root in roots if root is not None]
    while todo:
        at = todo.pop()
        if at in seen:
            continue
        seen.add(at)
        todo.extend(successor for successor in line_successors(lines, at) if successor is not None)
    return seen


def _computation(lines, at):
    """The first compared instruction on the one path from `at`, or None at an exit."""
    passed = set()
    while at is not None and _plumbing(lines[at]):
        if at in passed:
            raise IdentityError('a cycle of allocator plumbing only')
        passed.add(at)
        successors = line_successors(lines, at)
        at = successors[0] if successors else None
    return at


def layout_free_difference(left, right, left_root=0, right_root=0, pairs=None):
    """None when two normalized functions have the same rooted shape graph.

    A walk from both roots pairs compared instructions one to one: each pair
    has equal `_shape` text and its successors pair in order. A conditional
    branch also pairs with its inverse condition when its two successors are
    swapped, so block order is not compared; plumbing is skipped. A `pairs`
    dict receives the pairing of left lines to right lines.
    """
    forward, backward = {}, {}
    pending = [(_computation(left, left_root), _computation(right, right_root))]
    while pending:
        a, b = pending.pop()

        def difference(reason):
            return {'reason': reason,
                    'seq_instruction': None if a is None else a + 1, 'demand_instruction': None if b is None else b + 1,
                    'seq': '<exit>' if a is None else left[a], 'demand': '<exit>' if b is None else right[b],
                    'paired_instructions': len(forward)}
        if a is None or b is None:
            if a is None and b is None:
                continue
            return difference('one path exits where the other continues')
        if a in forward or b in backward:
            if forward.get(a) == b and backward.get(b) == a:
                continue
            return difference('paths join at different instructions')
        forward[a], backward[b] = b, a
        head_a, head_b = left[a].partition(' ')[0], right[b].partition(' ')[0]
        successors_a, successors_b = line_successors(left, a), line_successors(right, b)
        if head_a in INVERSE_CONDITION and head_b in INVERSE_CONDITION:
            if head_b == INVERSE_CONDITION[head_a]:
                successors_b = successors_b[::-1]
            elif head_a != head_b:
                return difference('branch conditions differ')
        elif _shape(left[a]) != _shape(right[b]) or len(successors_a) != len(successors_b):
            return difference('instruction shapes differ')
        pending.extend((None if x is None else _computation(left, x), None if y is None else _computation(right, y))
                       for x, y in reversed(list(zip(successors_a, successors_b))))
    if pairs is not None:
        pairs.update(forward)
    return None


# A join reload (stage 3, the mandelbrot timed wrapper).
#
# An entry-version function's sequential path can rejoin a path from the
# parallel decision. There the optimizer may read a value from memory that the
# canonical function kept in a register: in the canonical function the value
# was stored to that address on one path and loaded from it, just before the
# join, on another, so it never loads at the join. `join_reload_difference`
# admits exactly that one load and nothing else: see its docstring.
DIRECT_MEMORY = re.compile(r'(-?0x[0-9a-f]+)?\((%[a-z][\w.]*)\)')
INDEXED_MEMORY = re.compile(r'(-?0x[0-9a-f]+)?\((%[a-z][\w.]*)?,%[a-z][\w.]*(?:,\d)?\)')
CALLER_SAVED = ('rax', 'rcx', 'rdx', 'rsi', 'rdi', 'r8', 'r9', 'r10', 'r11')
# Instructions with an implicit register result or an implicit memory
# operand; a function holding one is never admitted as a join reload.
IMPLICIT_EFFECTS = re.compile(r'(?:(?:mul|div|idiv)[bwlq]?|cqto|cqo|cltq|cltd|cdq|cdqe|cwtl|xchg|cmpxchg\w*|xadd|rep\w*|stos\w*|movs[bwlq]?'
                              r'|cmps\w*|scas\w*|lods\w*|cpuid|rdtsc\w*|leave\w*|enter\w*|pushf\w*|popf\w*)')
COMPARE_ONLY = ('cmp', 'test', 'ucomisd', 'ucomiss', 'comisd', 'comiss', 'bt')
JOIN_RELOAD_QUALIFICATION = ('the entry-version function loads, once at the join outside any loop, the value the canonical function '
                             'kept in a register that holds that address\'s last stored or loaded value on every other path; '
                             'whether a call or an indexed store between may change that address is the canonical build\'s own '
                             'alias reasoning, not checked here; all other instructions have entry-version strength')


def _family(operand):
    """The register an operand names, whatever its width or renaming."""
    name = operand[1:]
    renamed = re.fullmatch(r'(saved|scratch)(\d+)\.\d+', name)
    if renamed:
        return renamed[1] + renamed[2]
    return INTEGER_REGISTER_ALIASES.get(name, name)


def _full_width(operand):
    return (re.fullmatch(r'%(?:saved|scratch)\d+\.0', operand) is not None
            or FULL_INTEGER_REGISTER.fullmatch(operand) is not None)


def _displacement(text):
    return int(text, 16) if text and '0x' in text else int(text or 0)


def _written(line):
    """The register family an instruction writes explicitly, or None."""
    head, _, rest = line.partition(' ')
    operands = _split_operands(rest.strip())
    if (NOISE.match(line) or head.startswith(('j', 'ret', 'push', 'call')) or head in COMPARE_ONLY
            or not operands or not _register(operands[-1])):
        return None
    return _family(operands[-1])


def _cell_flow(lines, preds):
    """Per line, the register values and address cells after it executes.

    A value is a symbol: an entry register, an instruction's result, or a
    join's merge. A register holds a cell (base value, displacement) when its
    full-width value was last stored to or loaded from that direct address
    with no store to an overlapping direct address of the same base since.
    Calls clobber caller-saved registers. Returns None when the flow does not
    settle or the function has an implicit effect.
    """
    for line in lines:
        head, _, rest = line.partition(' ')
        if not NOISE.match(line) and (IMPLICIT_EFFECTS.fullmatch(head)
                                      or head.startswith('imul') and len(_split_operands(rest.strip())) == 1):
            return None
    reach = line_reach(lines, [0])

    def transfer(state, at):
        values, cells = dict(state[0]), {family: set(held) for family, held in state[1].items()}

        def value(family):
            return values.get(family, ('entry', family))

        def write(family):
            values[family] = ('def', at, family)
            cells.pop(family, None)

        def overwrite(base, displacement):
            for family in cells:
                cells[family] = {cell for cell in cells[family]
                                 if not (cell[0] == base and abs(cell[1] - displacement) < 8)}

        line = lines[at]
        head, _, rest = line.partition(' ')
        operands = _split_operands(rest.strip())
        if head.startswith('call'):
            for family in CALLER_SAVED:
                write(family)
            for family in list(values) + list(cells):
                if family.startswith(('scratch', 'xmm')):
                    write(family)
            return values, cells
        if head == 'mov' and len(operands) == 2:
            source, destination = operands
            loaded, stored = DIRECT_MEMORY.fullmatch(source), DIRECT_MEMORY.fullmatch(destination)
            if _register(source) and _register(destination) and _full_width(source) and _full_width(destination):
                values[_family(destination)] = value(_family(source))
                cells[_family(destination)] = set(cells.get(_family(source), ()))
                return values, cells
            if loaded and _register(destination) and loaded[2] != '%rsp':
                cell = (value(_family(loaded[2])), _displacement(loaded[1]))
                write(_family(destination))
                if _full_width(destination):
                    cells[_family(destination)] = {cell}
                return values, cells
            if stored and stored[2] != '%rsp':
                cell = (value(_family(stored[2])), _displacement(stored[1]))
                overwrite(*cell)
                if _register(source) and _full_width(source):
                    cells.setdefault(_family(source), set()).add(cell)
                return values, cells
        if operands and (stored := DIRECT_MEMORY.fullmatch(operands[-1])) and stored[2] != '%rsp' \
                and head not in COMPARE_ONLY and not head.startswith('push'):
            overwrite(value(_family(stored[2])), _displacement(stored[1]))
        written = _written(line)
        if written:
            write(written)
        return values, cells

    def merge(states, at):
        families = set().union(*(state[0] for state in states))
        values = {}
        for family in families:
            seen = {state[0].get(family, ('entry', family)) for state in states}
            values[family] = seen.pop() if len(seen) == 1 else ('join', at, family)
        cells = {}
        for family in set().union(*(state[1] for state in states)):
            common = set.intersection(*(set(state[1].get(family, ())) for state in states))
            if common:
                cells[family] = common
        return values, cells

    after = {}
    for _ in range(64):
        changed = False
        for at in sorted(reach):
            incoming = [after[p] for p in preds[at] if p in after] + ([({}, {})] if at == 0 else [])
            if not incoming:
                continue
            state = transfer(incoming[0] if len(incoming) == 1 else merge(incoming, at), at)
            if after.get(at) != state:
                after[at], changed = state, True
        if not changed:
            return after
    return None


def _predecessors(lines):
    preds = {at: [] for at in range(len(lines))}
    for at in line_reach(lines, [0]):
        for successor in line_successors(lines, at):
            if successor is not None and at not in preds[successor]:
                preds[successor].append(at)
    return preds


def _block_head(lines, preds, at):
    """The first line of the block whose first compared instruction is `at`."""
    while len(preds[at]) == 1 and _plumbing(lines[preds[at][0]]) and line_successors(lines, preds[at][0]) == [at]:
        at = preds[at][0]
    return at


def _block_lines(lines, preds, head):
    """The straight-line lines from a block head to its branch, exit or the next join."""
    block, at = [head], head
    while True:
        successors = line_successors(lines, at)
        if len(successors) != 1 or successors[0] is None or len(preds[successors[0]]) != 1:
            return block
        at = successors[0]
        block.append(at)


def join_reload_difference(left, right):
    """Evidence that `right` differs from `left` by one reload at a join; else None.

    The right function holds a direct load `mov D(%B),%R` that is the first
    compared instruction of a block two or more paths enter, outside any loop.
    The left function holds the same-shaped load as the last instruction of
    one path into the corresponding join, also outside any loop. With both
    loads removed the two functions have the same rooted shape graph, and in
    the join block the right base and destination registers stand exactly
    where the left ones do until either is written. On every other path into
    the left join, the left destination register holds the value last stored
    to or loaded from D(%B), for that path's value of %B (`_cell_flow`).
    """
    left_preds, right_preds = _predecessors(left), _predecessors(right)
    flow = None
    found = []
    for s, seq_load in enumerate(left):
        seq_match = re.fullmatch(r'mov (\S+),(%[a-z][\w.]*)', seq_load)
        seq_address = DIRECT_MEMORY.fullmatch(seq_match[1]) if seq_match else None
        if (not seq_address or seq_address[2] in ('%rsp', '%rip') or not _full_width(seq_match[2])
                or _family(seq_address[2]) == _family(seq_match[2]) or s not in line_reach(left, [0])):
            continue
        join = line_successors(left, s)
        if len(join) != 1 or join[0] is None or len(left_preds[join[0]]) < 2 or s in line_reach(left, join):
            continue
        join = join[0]
        seq_base, seq_value = _family(seq_address[2]), _family(seq_match[2])
        seq_block = _block_lines(left, left_preds, join)
        for d, demand_load in enumerate(right):
            if _shape(demand_load) != _shape(seq_load) or d not in line_reach(right, [0]):
                continue
            demand_match = re.fullmatch(r'mov (\S+),(%[a-z][\w.]*)', demand_load)
            demand_base, demand_value = _family(DIRECT_MEMORY.fullmatch(demand_match[1])[2]), _family(demand_match[2])
            head = _block_head(right, right_preds, d)
            if (demand_base == demand_value or len(right_preds[head]) < 2 or _computation(right, head) != d
                    or d in line_reach(right, [x for x in line_successors(right, d) if x is not None])):
                continue
            left_without = left[:s] + ['nop'] + left[s + 1:]
            right_without = right[:d] + ['nop'] + right[d + 1:]
            pairs = {}
            if layout_free_difference(left_without, right_without, pairs=pairs) is not None:
                continue
            if pairs.get(_computation(left_without, join)) != _computation(right_without, head):
                continue
            # The base and the loaded value stand in the same places in the join block.
            roles = {seq_base: demand_base, seq_value: demand_value}
            demand_block = _block_lines(right, right_preds, head)
            corresponding, used = True, set()
            for a in seq_block:
                if roles and not _plumbing(left[a]):
                    b = pairs.get(a)
                    if b is None or b not in demand_block or any(_written(right[x]) in roles.values()
                                                                for x in demand_block if x < b and x != d):
                        corresponding = False
                        break
                    for x, y in zip(re.findall(r'%[a-z][\w.]*', left[a]), re.findall(r'%[a-z][\w.]*', right[b])):
                        for seq_role, demand_role in roles.items():
                            if (_family(x) == seq_role) != (_family(y) == demand_role):
                                corresponding = False
                            elif _family(x) == seq_role:
                                used.add(seq_role)
                written = _written(left[a])
                if written in roles:
                    roles = {}
            if not corresponding or used != {seq_base, seq_value}:
                continue
            # Every other path into the join carries the loaded cell in the register.
            if flow is None:
                flow = _cell_flow(left, left_preds)
            if flow is None:
                return None
            others = [p for p in left_preds[join] if p != s]
            if all(p in flow and (flow[p][0].get(seq_base, ('entry', seq_base)), _displacement(seq_address[1]))
                   in flow[p][1].get(seq_value, ()) for p in others):
                found.append({'seq_load': s + 1, 'demand_load': d + 1, 'seq_join': join + 1, 'demand_join': head + 1,
                              'address': seq_address[0], 'carried_on': [p + 1 for p in others]})
    return found[0] if len(found) == 1 else None


SPLIT_CALL = re.compile(r'call wf__par_split_')


def inlined_clone_root(seq_lines, demand_lines):
    """Where a cheap region's inlined sequential clone begins in its function.

    The decision and the parallel path are the lines from which a call of a
    parallel split is reachable. Each line outside them that one of them can
    enter is a candidate; exactly one must have the canonical sequential
    function's whole shape graph. Returns that line and the decision lines
    that reach it.
    """
    splits = [at for at, line in enumerate(demand_lines) if SPLIT_CALL.match(line)]
    if not splits:
        raise IdentityError('cheap region without a parallel split call')
    parallel = {at for at in range(len(demand_lines)) if line_reach(demand_lines, [at]) & set(splits)}
    entered = line_reach(demand_lines, [0])
    candidates = sorted({successor for at in parallel & entered
                         for successor in line_successors(demand_lines, at)
                         if successor is not None and successor not in parallel})
    matches = {_computation(demand_lines, at): at for at in candidates
               if layout_free_difference(seq_lines, demand_lines, 0, at) is None}
    if len(matches) != 1:
        raise IdentityError(f'{len(matches)} of {len(candidates)} sequential entries have the canonical shape')
    root = next(iter(matches.values()))
    decision = {at for at in parallel & entered if root in line_reach(demand_lines, [at])}
    return root, decision


def clone_transfer(instructions, clone):
    """The one direct transfer to the sequential clone, as (index, kind).

    A `jmp` is a tail transfer. A `call` is accepted only when everything
    reachable after it is register, stack and result-store plumbing: no
    further call, comparison, conditional branch or loop.
    """
    transfers = [(at, insn.mnemonic) for at, insn in enumerate(instructions)
                 if insn.mnemonic in ('jmp', 'call') and target(insn)
                 and target(insn)[1].removesuffix('@plt') == clone]
    if len(transfers) != 1:
        raise IdentityError('expected one direct clone transfer')
    at, kind = transfers[0]
    if kind == 'call':
        edges = assembly_edges(instructions)
        after = reachable(edges, at + 1) if at + 1 < len(instructions) else set()
        for node in after:
            mnemonic = instructions[node].mnemonic
            if (mnemonic.startswith(('call', 'cmp', 'test')) or (mnemonic.startswith('j') and mnemonic != 'jmp')
                    or on_cycle(edges, node)):
                raise IdentityError('work after the clone call is not result plumbing')
    return at, kind


def entry_compare_bound(blocks):
    """How many machine comparisons the entry block's IR can explain.

    A comparison, a min/max or abs, an overflow-checked operation (its flag
    is tested where a select reads it) and a division by a non-constant
    (whose quotient the backend may bound by a compare) each explain one.
    """
    return sum(1 for line in blocks.get('par.region.entry', [])
               if re.search(r'\bicmp\b', line) or re.search(r'@llvm\.(?:[us](?:max|min)|abs)\b', line)
               or re.search(r'@llvm\.[us](?:add|sub|mul)\.with\.overflow\b', line)
               or re.search(r'=\s*[us](?:div|rem)\b[^,]*,\s*%', line))


def ir_call_reach(ir, root):
    """Every function the root can reference, a superset of what it can call."""
    seen, todo = set(), [root]
    while todo:
        function = todo.pop()
        if function in seen or function not in ir:
            continue
        seen.add(function)
        for lines in ir[function].values():
            for line in lines:
                todo.extend(re.findall(r'@([\w.$]+)', line))
    return seen


def project_pool(instructions, active):
    """Recognize a pool call, argument restores, test and conditional branch.

    Nondecision setup/cleanup remains in the comparison. If backend inlining
    changes the wrapper ABI or code around dispatch, that is a mismatch.
    Returns the selected and omitted instructions, the pool decision count and
    the branch's one kept edge.
    """
    edges = assembly_edges(instructions)
    polls = [at for at, insn in enumerate(instructions) if insn.mnemonic.startswith('call') and target(insn) and target(insn)[1].removesuffix('@plt') == 'wf__par_pool_active']
    if not polls:
        return reachable(edges), set(), 0, {}
    if len(polls) != 1:
        raise IdentityError(f'expected at most one pool dispatch, found {len(polls)}')
    at = polls[0]
    test_at = at + 1
    # LLVM may restore saved integer arguments before testing the pool result.
    # These flag-preserving moves remain computation in the comparison. Only
    # full-width callee-saved-to-argument moves are supported: they cannot
    # change eax, the stack, or the flags consumed by the recognized test.
    while test_at < len(instructions):
        restore = instructions[test_at]
        operands = re.sub(r'\s', '', restore.operands)
        match = re.fullmatch(r'%([a-z0-9]+),%([a-z0-9]+)', operands)
        if (restore.mnemonic not in ('mov', 'movq') or match is None
                or match[1] not in CALLEE_SAVED or match[2] not in ABI_ARGUMENTS):
            break
        test_at += 1
    branch_at = test_at + 1
    if branch_at >= len(instructions):
        raise IdentityError('truncated pool dispatch')
    test, branch = instructions[test_at:branch_at + 1]
    if test.mnemonic not in ('test', 'testl') or re.sub(r'\s', '', test.operands) != '%eax,%eax' or branch.mnemonic not in ('je', 'jne', 'jz', 'jnz'):
        raise IdentityError('unsupported pool dispatch: expected call/test eax/je-or-jne')
    if on_cycle(edges, at) or on_cycle(edges, branch_at):
        raise IdentityError('pool dispatch is inside an assembly cycle')
    # Every return/tail exit must be dominated by the decision, not simply
    # one static occurrence. Removing its node must disconnect every exit.
    without = {node: [succ for succ in successors if succ != at] for node, successors in edges.items()}
    bypass = reachable(without) if at != 0 else set()
    if any(not edges[node] for node in bypass):
        raise IdentityError('a timed entry exit bypasses pool dispatch')
    destination = target(branch)[0]
    index = {insn.address: node for node, insn in enumerate(instructions)}
    taken = bool(active) if branch.mnemonic in ('jne', 'jnz') else not active
    edges[branch_at] = [index[destination] if taken else branch_at + 1]
    return reachable(edges), {at, test_at, branch_at}, 1, {branch_at: edges[branch_at][0]}


ABI_ARGUMENTS = ('rdi', 'rsi', 'rdx', 'rcx', 'r8', 'r9')
CALLEE_SAVED = ('rbx', 'rbp', 'r12', 'r13', 'r14', 'r15')


def entry_shell(instructions, active, arguments=None, argument_count=6, functions=None):
    """Prove a supported x86 entry shell restores ABI before a tail transfer.

    Only register moves and private push/pop stack saves may surround dispatch.
    No source-memory operation is erased. Follow concrete pool and cheap-region
    conditions, rejecting unknown branches, loops or unrecognized instructions.
    """
    registers = {name: name for name in ABI_ARGUMENTS + CALLEE_SAVED + ('rax', 'r10', 'r11')}
    if arguments:
        registers.update(arguments)
    initial = dict(registers)
    stack, sp, flags, at, visited, decisions = {}, 0, None, 0, set(), []
    addresses = {instruction.address: index for index, instruction in enumerate(instructions)}

    def read(operand):
        operand = operand.strip()
        if operand.startswith('$'):
            return int(operand[1:], 0)
        if operand == '%eax':
            value = registers['rax']
            if not isinstance(value, int):
                raise IdentityError('unknown or overwritten pool result alias eax')
            return value & 0xffffffff
        if operand.startswith('%') and operand[1:] in registers:
            return registers[operand[1:]]
        raise IdentityError(f'unsupported dispatch-shell operand {operand}')

    while at < len(instructions):
        if at in visited:
            raise IdentityError('entry dispatch executes inside an optimized loop')
        visited.add(at)
        instruction = instructions[at]
        mnemonic = instruction.mnemonic.rstrip('q')
        operands = re.sub(r'\s+', '', instruction.operands.split('#', 1)[0])
        if mnemonic.startswith('nop') or mnemonic in ('endbr64',):
            pass
        elif mnemonic == 'push':
            sp -= 8
            stack[sp] = read(operands)
        elif mnemonic == 'pop':
            if sp not in stack or operands[1:] not in registers:
                raise IdentityError('unsupported dispatch-shell pop')
            registers[operands[1:]] = stack.pop(sp)
            sp += 8
            if sp > 0:
                raise IdentityError('dispatch shell crosses caller stack boundary')
        elif mnemonic in ('sub', 'add') and operands.endswith(',%rsp'):
            amount = read(operands.split(',')[0])
            if not isinstance(amount, int):
                raise IdentityError('nonconstant dispatch stack adjustment')
            sp += amount if mnemonic == 'add' else -amount
            if sp > 0:
                raise IdentityError('dispatch shell crosses caller stack boundary')
            flags = None
        elif mnemonic == 'mov':
            source, destination = [operand.strip() for operand in operands.split(',')]
            if not destination.startswith('%') or destination[1:] not in registers:
                raise IdentityError('dispatch shell writes source memory or an unsupported register')
            registers[destination[1:]] = read(source)
        elif mnemonic in ('test', 'testl', 'cmp', 'cmpl'):
            left, right = [operand.strip() for operand in operands.split(',')]
            left, right = read(left), read(right)
            if not isinstance(left, int) or not isinstance(right, int):
                raise IdentityError('dispatch condition not determined by timed inputs')
            width = 32 if instruction.mnemonic.endswith('l') or '%eax' in operands else 64
            if width == 32 and '%eax' not in operands:
                raise IdentityError('unsupported narrow dispatch comparison operands')
            mask = (1 << width) - 1
            left, right = left & mask, right & mask
            flags = (right == left, right < left, right <= left) if mnemonic.startswith('cmp') else ((left & right) == 0, False, (left & right) == 0)
        elif mnemonic.startswith('call'):
            destination = target(instruction, functions)
            if not destination or destination[1].removesuffix('@plt') != 'wf__par_pool_active':
                raise IdentityError('unsupported nontail computation in dispatch shell')
            decisions.append('pool-active')
            # The call's return address and callee stack may overwrite every
            # deallocated slot below the current stack pointer. Such stale
            # saves cannot prove argument restoration after the call.
            stack = {address: value for address, value in stack.items() if address >= sp}
            for register in ABI_ARGUMENTS + ('rax', 'r10', 'r11'):
                registers[register] = f'clobbered:{register}'
            registers['rax'] = int(active)
            flags = None
        elif mnemonic.startswith('j'):
            destination = target(instruction, functions)
            if not destination:
                raise IdentityError('indirect entry-shell branch')
            if mnemonic == 'jmp':
                taken = True
            else:
                if flags is None:
                    raise IdentityError('entry-shell branch lacks known flags')
                equal, below, below_equal = flags
                conditions = {'je': equal, 'jz': equal, 'jne': not equal, 'jnz': not equal, 'ja': not below_equal, 'jae': not below, 'jb': below, 'jbe': below_equal}
                if mnemonic not in conditions:
                    raise IdentityError(f'unsupported entry-shell condition {mnemonic}')
                taken = conditions[mnemonic]
                if not decisions or decisions[-1] != 'pool-active':
                    decisions.append('cheap-region')
            if taken:
                if destination[0] in addresses:
                    at = addresses[destination[0]]
                    continue
                if mnemonic != 'jmp':
                    raise IdentityError('conditional entry transfer crosses function boundary')
                if sp != 0 or any(registers[register] != initial[register] for register in ABI_ARGUMENTS[:argument_count] + CALLEE_SAVED):
                    raise IdentityError('entry dispatch does not restore arguments, saved registers and stack before computation')
                return destination[1], decisions, initial
        else:
            raise IdentityError(f'unsupported entry-shell instruction {instruction.mnemonic} {operands}')
        at += 1
    raise IdentityError('entry shell has no direct computation tail transfer')


def computation_root(functions, root, active, arguments=None, argument_count=6):
    """Expand only ABI-preserving tail shells; ordinary computation stays whole."""
    current, decisions, seen = root, [], set()
    while current not in seen:
        seen.add(current)
        instructions = functions[current]
        has_pool = any(target(insn) and target(insn)[1].removesuffix('@plt') == 'wf__par_pool_active' for insn in instructions if insn.mnemonic.startswith('call'))
        addresses = {instruction.address for instruction in instructions}

        def interfunction_tail(insn):
            destination = target(insn, functions)
            return (insn.mnemonic == 'jmp' and destination is not None
                    and destination[0] not in addresses
                    and destination[1].split('+', 1)[0] != current)

        # An ordinary entry selects its own sequential clone: canonical names
        # deliberately coincide, but these are distinct machine functions.
        # A sequential function's recursive backedge remains computation.
        has_clone_tail = not current.startswith('wf__par_seq_') and any(
            interfunction_tail(insn) and target(insn)[1].startswith('wf__par_seq_')
            for insn in instructions)
        # Recognize a simple seq adapter transfer, but never strip a local or
        # self-recursive jump merely because the body has one instruction.
        simple_tail = len(instructions) == 1 and interfunction_tail(instructions[0])
        if not (has_pool or has_clone_tail or simple_tail):
            return current, decisions
        if argument_count > 6:
            raise IdentityError('unsupported stack-argument dispatch-shell ABI')
        destination, counted, arguments = entry_shell(instructions, active, arguments, argument_count, functions)
        if destination not in functions:
            raise IdentityError(f'entry computation function missing: {destination}')
        decisions.extend(counted)
        current = destination
    raise IdentityError('cyclic entry-shell function transfers')


def parse_ir(text):
    functions, name, blocks, block = {}, None, None, None
    for raw in text.splitlines():
        line = raw.split(';', 1)[0].strip()
        match = re.match(r'define\b.*?@([\w.$]+)\(', line)
        if match:
            name, blocks, block = match[1], {}, 'entry'
            blocks[block] = []
        elif name and line == '}':
            functions[name] = blocks
            name = None
        elif name:
            label = re.match(r'^([\w.$-]+):', line)
            if label:
                block = label[1]
                # An explicitly named first block replaces the implicit entry.
                if blocks == {'entry': []} and block != 'entry':
                    blocks = {}
                blocks.setdefault(block, [])
            elif line:
                blocks[block].append(line)
    return functions


def ir_edges(blocks):
    edges = {}
    for name, lines in blocks.items():
        if not lines:
            raise IdentityError(f'empty LLVM block {name}')
        terminator = lines[-1]
        if terminator == ']':
            # LLVM prints a switch as several lines. Its final bracket is not
            # a separate terminator, and every case edge belongs in the CFG.
            starts = [at for at, line in enumerate(lines) if line.startswith('switch ')]
            if len(starts) != 1:
                raise IdentityError(f'malformed LLVM switch in {name}')
            switch_lines = lines[starts[0]:]
            header = re.fullmatch(r'switch (i[1-9][0-9]*) (%[\w.$]+|-?\d+|true|false), label %([\w.$-]+) \[', switch_lines[0])
            if header is None:
                raise IdentityError(f'malformed LLVM switch header in {name}')
            successors, values = [header[3]], set()
            width = int(header[1][1:])
            if width > 64:
                raise IdentityError(f'unsupported LLVM switch integer width in {name}')
            selector = header[2]
            if not selector.startswith('%'):
                if selector in ('true', 'false') and width != 1:
                    raise IdentityError(f'malformed LLVM switch selector in {name}')
                value = {'true': 1, 'false': 0}.get(selector)
                if value is None:
                    value = int(selector)
                if not -(1 << (width - 1)) <= value < 1 << width:
                    raise IdentityError(f'out-of-range LLVM switch selector in {name}')
            for case in switch_lines[1:-1]:
                match = re.fullmatch(re.escape(header[1]) + r' (-?\d+|true|false), label %([\w.$-]+)', case)
                if match is None or match[1] in ('true', 'false') and width != 1:
                    raise IdentityError(f'malformed or duplicate LLVM switch case in {name}: {case}')
                value = {'true': 1, 'false': 0}.get(match[1])
                if value is None:
                    value = int(match[1])
                if not -(1 << (width - 1)) <= value < 1 << width:
                    raise IdentityError(f'out-of-range LLVM switch case in {name}: {case}')
                value %= 1 << width
                if value in values:
                    raise IdentityError(f'malformed or duplicate LLVM switch case in {name}: {case}')
                values.add(value)
                successors.append(match[2])
            edges[name] = successors
        elif terminator.startswith('switch '):
            # The compiler currently emits multiline switches. Unknown forms
            # must not lose their case edges by falling through to a branch.
            raise IdentityError(f'unsupported LLVM switch form in {name}')
        elif terminator.startswith('br '):
            edges[name] = re.findall(r'label %([\w.$-]+)', terminator)
        elif terminator.startswith(('ret ', 'unreachable')):
            edges[name] = []
        else:
            raise IdentityError(f'unsupported LLVM terminator: {terminator}')
        if any(succ not in blocks for succ in edges[name]):
            raise IdentityError(f'missing LLVM successor in {name}')
    return edges


def ir_reachable(edges):
    return reachable(edges, next(iter(edges)))


def dispatch_proof(ir, wrapper):
    """Count dynamic decisions on every entry-to-return path, using CFGs.

    Binder pool selection must be a unique dominating, acyclic entry decision.
    Version selection must be at a function's first block; its only reachable
    caller is a single acyclic wrapper call. Recursive/repeated dispatch or an
    unknown calling shape fails closed. This proves placement, not code identity.
    """
    if wrapper not in ir:
        raise IdentityError(f'missing timed LLVM wrapper {wrapper}')
    root = ir[wrapper]
    root_edges = ir_edges(root)
    root_reached = ir_reachable(root_edges)
    pools = [(block, line) for block in root_reached for line in root[block] if '@wf__par_pool_active(' in line]
    if len(pools) > 1:
        raise IdentityError(f'timed wrapper has {len(pools)} pool decisions')
    if pools:
        point = pools[0][0]
        if on_cycle(root_edges, point):
            raise IdentityError('pool decision lies in an LLVM cycle')
        if point != next(iter(root)):
            without = {node: [succ for succ in successors if succ != point] for node, successors in root_edges.items()}
            if any(not root_edges[node] for node in ir_reachable(without)):
                raise IdentityError('LLVM return bypasses pool decision')
        line = pools[0][1]
        result = re.match(r'(%[\w.$]+) = call i32 @wf__par_pool_active\(\)', line)
        if not result or not any(re.search(r'icmp ne i32 ' + re.escape(result[1]) + r', 0$', candidate) for candidate in root[point]):
            raise IdentityError('pool result does not drive the recognized entry decision')
    calls = [(block, match[1]) for block in root_reached for line in root[block] if (match := re.search(r'\bcall\b.*?@([\w.$]+)\(', line))]
    seen, todo, regions = set(), [callee for _, callee in calls if callee in ir], []
    while todo:
        function = todo.pop()
        if function in seen:
            continue
        seen.add(function)
        blocks = ir[function]
        if 'par.region.entry' not in blocks:
            if any('@wf__par_pool_active(' in line for lines in blocks.values() for line in lines):
                raise IdentityError(f'nested pool decision in {function}')
            continue
        edges = ir_edges(blocks)
        reached = ir_reachable(edges)
        if any('@wf__par_pool_active(' in line for block in reached for line in blocks[block]):
            raise IdentityError(f'nested pool decision in {function}')
        if 'par.region.entry' in reached:
            if next(iter(blocks)) != 'par.region.entry' or on_cycle(edges, 'par.region.entry'):
                raise IdentityError(f'region selection is not acyclic function entry: {function}')
            entry = blocks['par.region.entry']
            if not entry[-1].startswith('br i1 ') or 'label %par.region.sequential' not in entry[-1]:
                raise IdentityError(f'unrecognized cheap-region branch: {function}')
            # The emitter puts private local slots in the function's entry
            # block to preserve promotability. An unused alloca is not an
            # observation in the cheap predicate. Its result must have no use
            # anywhere in that predicate, including the deciding branch.
            predicate = []
            slots = []
            for line in entry:
                if re.search(r'\balloca\b', line):
                    slot = re.fullmatch(r'(%[\w.$]+) = alloca (?:ptr|i[1-9][0-9]*), align [1-9][0-9]*', line)
                    if slot is None:
                        raise IdentityError(f'unsupported private entry allocation: {function}')
                    slots.append(slot[1])
                else:
                    predicate.append(line)
            if any(re.search(re.escape(slot) + r'(?![\w.$])', line)
                   for slot in slots for line in predicate):
                raise IdentityError(f'cheap predicate uses private entry allocation: {function}')
            if any('call ' in line and '@llvm.' not in line for line in predicate[:-1]) or any(re.search(r'\b(load|store)\b', line) for line in predicate[:-1]):
                raise IdentityError(f'cheap predicate observes memory or performs work: {function}')
            callers = [(caller, block) for caller, body in ir.items() for block, lines in body.items() for line in lines if re.search(r'\bcall\b.*?@' + re.escape(function) + r'\(', line)]
            timed_callers = [(caller, block) for caller, block in callers if caller == wrapper or caller in seen]
            if timed_callers != [(wrapper, next(block for block, callee in calls if callee == function))]:
                raise IdentityError(f'region is not called once directly by timed wrapper: {function}')
            call_block = timed_callers[0][1]
            if on_cycle(root_edges, call_block):
                raise IdentityError(f'region call repeats in timed wrapper: {function}')
            regions.append({'function': function, 'block': 'par.region.entry', 'decisions_per_active_timed_call': 1, 'outside_loops': True})
        # Regions are required to be direct wrapper calls. Other parallel bodies
        # do not affect this placement proof; their selected seq computation
        # and surviving helpers are checked from optimized machine code.
    return {'pool_decisions_per_timed_call': len(pools), 'outside_loops': True, 'regions': regions, 'proof': 'LLVM CFG dominance/cycle and direct-call proof; assembly pool CFG dominance/cycle proof'}


def direct_calls(instructions, selected=None, functions=None):
    """Discover reachable helper transfers, including interfunction branches."""
    selected = range(len(instructions)) if selected is None else sorted(selected)
    result = set()
    addresses = {instruction.address for instruction in instructions}
    for at in selected:
        insn = instructions[at]
        if insn.mnemonic.startswith(('call', 'j', 'loop')):
            destination = target(insn, functions)
            if not destination:
                raise IdentityError(f'unresolved helper transfer at {insn.address:x}')
            if destination[0] not in addresses:
                result.add(destination[1].removesuffix('@plt'))
    return result


def latched_writer_evidence(function, seq_ir, demand_ir, demand, functions, seq_machine=None, seq_functions=None,
                            seq_headers=None, demand_headers=None):
    """Attribute the known cold writer difference without declaring it equal.

    A name is only a candidate: the IR must have precisely the shared latch
    addition, preserving every write/abort block, and the actual image must
    retain the latch accessor call and atomic acquisition.
    """
    if canonical_symbol(function) != 'wf_resource_record_abort':
        return None
    original = seq_ir.get('wf_resource_record_abort', {})
    changed = demand_ir.get(function, {})
    entry = [
        '%latch = call ptr @wf__floor_record_latch()',
        '%acquired = cmpxchg ptr %latch, i32 0, i32 1 seq_cst seq_cst',
        '%won = extractvalue { i32, i1 } %acquired, 1',
        'br i1 %won, label %write.loop, label %park',
    ]
    park = ['%parked = load volatile i32, ptr %latch, align 4', 'br label %park']
    common = {block: lines for block, lines in original.items() if block != 'entry'}
    if (original.get('entry') != ['br label %write.loop']
            or changed.get('entry') != entry or changed.get('park') != park
            or not common or common != {block: lines for block, lines in changed.items() if block not in ('entry', 'park')}
            or common.get('abort') != ['call void @abort()', 'unreachable']
            or not any('@wf_resource_write(' in line for lines in common.values() for line in lines)):
        return None
    # A private write helper may disappear entirely through inlining. Its exact
    # source closure remains an obligation of the cold-boundary exception.
    try:
        original_helpers, original_headers = writer_helper_closure(seq_ir, seq_headers or {})
        changed_helpers, changed_headers = writer_helper_closure(demand_ir, demand_headers or {})
    except IdentityError:
        return None
    if original_helpers != changed_helpers or original_headers != changed_headers:
        return None
    calls = transfer_inventory(demand, functions)
    original_calls = transfer_inventory(seq_machine, seq_functions) if seq_machine is not None else []
    atomics = [instruction.address for instruction in demand
               if instruction.mnemonic == 'lock' and instruction.operands.startswith('cmpxchg')]
    if ('abort' not in original_calls
            or not writer_machine_frontier('wf_resource_write', original_helpers, set(original_calls))
            or Counter(calls) != Counter(original_calls) + Counter({'wf__floor_record_latch': 1})
            or sum(1 for instruction in demand if instruction.mnemonic in ('call', 'callq')
                   and target(instruction, functions)
                   and target(instruction, functions)[1] == 'wf__floor_record_latch') != 1
            or len(atomics) != 1):
        return None
    return {'ir': 'identical write/abort CFG; only shared cmpxchg entry and volatile loser park added',
            'original_blocks': original, 'demand_blocks': changed,
            'helpers': {'original_blocks': original_helpers, 'demand_blocks': changed_helpers,
                        'original_headers': original_headers, 'demand_headers': changed_headers},
            'machine': {'latch_accessor': 'wf__floor_record_latch', 'atomic_acquisition_addresses': atomics,
                        'original_calls': sorted(original_calls), 'demand_calls': sorted(calls),
                        'original_instructions': instruction_snapshot(seq_machine, seq_functions),
                        'demand_instructions': instruction_snapshot(demand, functions)},
            'qualification': 'cold support exception: exact common IR and call inventory, not common writer machine-body identity'}


def writer_helper_closure(ir, headers):
    """Retain definitions and signatures of the common write-helper closure."""
    if 'wf_resource_write' not in ir:
        raise IdentityError('missing common write-helper IR definition')
    pending, blocks, signatures = {'wf_resource_write'}, {}, {}
    while pending:
        function = pending.pop()
        if function in signatures:
            continue
        header = headers.get(function)
        if not isinstance(header, str) or not header:
            raise IdentityError(f'missing common writer-helper signature: {function}')
        signatures[function] = header
        if function not in ir:
            if not header.startswith('declare '):
                raise IdentityError(f'missing common writer-helper definition: {function}')
            continue
        if not header.startswith('define '):
            raise IdentityError(f'unsupported common writer-helper signature: {function}')
        blocks[function] = ir[function]
        for lines in ir[function].values():
            for line in lines:
                if re.search(r'\bcall\b', line):
                    callee = re.search(r'\bcall\b.*?@([\w.$]+)\(', line)
                    if not callee:
                        raise IdentityError('unsupported indirect common writer-helper call')
                    pending.add(callee[1])
    return blocks, signatures


def writer_machine_frontier(function, helpers, calls, visiting=None):
    """Require every common helper leaf, or its surviving native ancestor.

    Merely dropping a helper-name requirement would admit images where both
    writers lost the write operation. An inlined helper must expose its actual
    call leaves in the common native inventory instead.
    """
    if function in calls:
        return True
    visiting = set() if visiting is None else visiting
    if function not in helpers or function in visiting:
        return False
    callees = {match[1] for lines in helpers[function].values() for line in lines
               if (match := re.search(r'\bcall\b.*?@([\w.$]+)\(', line))}
    return bool(callees) and all(writer_machine_frontier(callee, helpers, calls, visiting | {function})
                                 for callee in callees)


def ir_function_headers(text):
    return {match[2]: match[1] for match in re.finditer(
        r'^((?:define|declare)\b[^\n]*@([\w.$]+)\([^\n]*)$', text, re.M)}


def transfer_inventory(instructions, functions=None):
    addresses = {instruction.address for instruction in instructions}
    calls = []
    for instruction in instructions:
        if instruction.mnemonic.startswith(('call', 'j', 'loop')):
            destination = target(instruction, functions)
            if destination is None:
                raise IdentityError('unresolved writer machine transfer')
            if destination[0] not in addresses:
                calls.append(destination[1].removesuffix('@plt'))
    return calls


def instruction_snapshot(instructions, functions=None):
    """Retain actual instruction addresses and exact resolved transfer names."""
    result = []
    for instruction in instructions:
        operands = instruction.operands
        destination = target(instruction, functions)
        if destination and (instruction.mnemonic.startswith('j') or instruction.mnemonic in ('call', 'callq')):
            operands = f'{destination[0]:x} <{destination[1]}>'
        result.append({'address': instruction.address, 'mnemonic': instruction.mnemonic, 'operands': operands})
    return result


def snapshot_instructions(rows):
    if not isinstance(rows, list) or not rows:
        raise IdentityError('missing actual machine instruction snapshot')
    return [Instruction(row['address'], row['mnemonic'], row['operands']) for row in rows]


def verified_writer_evidence(function, evidence):
    try:
        machine = evidence['machine']
        replay = latched_writer_evidence(function,
            {'wf_resource_record_abort': evidence['original_blocks'], **evidence['helpers']['original_blocks']},
            {function: evidence['demand_blocks'], **evidence['helpers']['demand_blocks']},
            snapshot_instructions(machine['demand_instructions']), None,
            snapshot_instructions(machine['original_instructions']), None,
            evidence['helpers']['original_headers'], evidence['helpers']['demand_headers'])
        return replay is not None and replay == evidence
    except (KeyError, TypeError, ValueError):
        return False


def entry_adapter_evidence(seq_ir, demand_ir, wrapper, seq_header, demand_header):
    """Recognize the exact pool diamond's inactive IR provenance.

    The selected source call may be inlined in the actual wrapper. This proof
    preserves every adapter operation, including output stores, and establishes
    only which canonical IR entrance was compiled, never its machine identity
    or preservation of live state across the pool call.
    """
    if not seq_header or seq_header != demand_header:
        return None
    original, changed = seq_ir.get(wrapper, {}), demand_ir.get(wrapper, {})
    if len(original) != 1 or len(changed) != 4:
        return None
    entry = next(iter(changed.values()))
    if len(entry) != 3:
        return None
    pool = re.fullmatch(r'(%[\w.$]+) = call i32 @wf__par_pool_active\(\)', entry[0])
    active = re.fullmatch(r'(%[\w.$]+) = icmp ne i32 (%[\w.$]+), 0', entry[1])
    branch = re.fullmatch(r'br i1 (%[\w.$]+), label %([\w.$]+), label %([\w.$]+)', entry[2])
    if not pool or not active or not branch or active[2] != pool[1] or branch[1] != active[1]:
        return None
    par, seq = changed.get(branch[2], []), changed.get(branch[3], [])
    if len(par) != 2 or len(seq) != 2 or par[1] != seq[1]:
        return None
    join_edge = re.fullmatch(r'br label %([\w.$]+)', seq[1])
    join = changed.get(join_edge[1], []) if join_edge else []
    call_pattern = r'(%[\w.$]+) = call (i[0-9]+|ptr) @([\w.$]+)\((.*)\)'
    par_call, seq_call = re.fullmatch(call_pattern, par[0]), re.fullmatch(call_pattern, seq[0])
    phi = re.fullmatch(r'(%[\w.$]+) = phi (i[0-9]+|ptr) \[ (%[\w.$]+), %([\w.$]+) \], \[ (%[\w.$]+), %([\w.$]+) \]', join[0]) if join else None
    if (not par_call or not seq_call or not phi or not seq_call[3].startswith('wf__par_seq_')
            or par_call[2] != seq_call[2] or par_call[4] != seq_call[4]
            or par_call[3] != canonical_symbol(seq_call[3])
            or phi[2] != seq_call[2] or (phi[3], phi[4]) != (par_call[1], branch[2])
            or (phi[5], phi[6]) != (seq_call[1], branch[3])):
        return None
    projected = [f'{phi[1]} = call {seq_call[2]} @{canonical_symbol(seq_call[3])}({seq_call[4]})', *join[1:]]
    if projected != next(iter(original.values())):
        return None
    return {'inactive_ir_root': seq_call[3], 'projected_adapter_ir_identical': True,
            'projected_ir_sha256': hashlib.sha256('\n'.join(projected).encode()).hexdigest(),
            'original_blocks': original, 'demand_blocks': changed,
            'original_header': seq_header, 'demand_header': demand_header,
            'qualification': 'canonical source IR entrance provenance; selected computation may be inlined and its wrapper machine code still differs'}


def verified_adapter_evidence(wrapper, evidence, compared, canonical_machine_closure=()):
    try:
        replay = entry_adapter_evidence({wrapper: evidence['original_blocks']},
            {wrapper: evidence['demand_blocks']}, wrapper,
            evidence['original_header'], evidence['demand_header'])
        projection = evidence['optimized_pool_projection']
        instructions = snapshot_instructions(projection['instructions'])
        selected, omitted, count, _ = project_pool(instructions, False)
        closure, declarations = adapter_ir_inventory(evidence['canonical_ir_closure'],
            evidence['inactive_ir_root'], set(evidence['canonical_ir_declarations']))
        allowed = set(closure) | declarations
        # LLVM may lower a referenced memory intrinsic to its native ABI call.
        # This adds only the specific intrinsic's implementation, not all IR
        # declarations and never a parallel/original counterpart of a clone.
        for declaration in declarations:
            memory = re.fullmatch(r'llvm\.(memcpy|memmove)\.p[0-9]+\.p[0-9]+\.i(?:32|64)|llvm\.(memset)\.p[0-9]+\.i(?:32|64)', declaration)
            if memory:
                allowed.add(memory[1] or memory[2])
        # LLVM also recognizes scalar fill/copy loops without an intrinsic in
        # the raw IR. Admit that native transfer only when its exact canonical
        # function is in this IR closure and both normalized machine bodies
        # contain it identically. A label or another function's call proves
        # nothing, and this never admits a parallel counterpart of a clone.
        for row in canonical_machine_closure:
            left, right = row.get('seq_instructions'), row.get('demand_instructions')
            if (row.get('function') in closure and row.get('function') in compared
                    and row.get('role') == 'reachable sequential body/helper'
                    and row.get('seq_function') == canonical_symbol(row['function'])
                    and isinstance(left, list) and left and isinstance(right, list) and right
                    and all(isinstance(line, str) for line in left + right)
                    and first_difference(left, right) is None):
                allowed.update(match[1] for line in right
                    if (match := re.fullmatch(r'(?:call|jmp) (memcpy|memmove|memset)', line)))
        actual_calls = direct_calls(instructions, selected - omitted)
        return (replay is not None and replay == {key: value for key, value in evidence.items()
                    if key not in ('optimized_pool_projection', 'canonical_ir_closure', 'canonical_ir_declarations')}
                and closure == evidence['canonical_ir_closure']
                and declarations == set(evidence['canonical_ir_declarations'])
                and actual_calls <= allowed
                and replay['inactive_ir_root'] in compared
                and projection['outside_loops'] is True
                and count == projection['pool_decisions_per_timed_call'] == 1
                and projection['selected_instruction_addresses'] == [instructions[at].address for at in sorted(selected)])
    except (KeyError, TypeError, ValueError, StopIteration):
        return False


def adapter_ir_inventory(ir, root, declarations):
    """Exact inactive entrance's raw IR dependency closure, before inlining."""
    if root not in ir:
        raise IdentityError('canonical adapter entrance has no IR definition')
    pending, reached, external = {root}, {}, set()
    while pending:
        function = pending.pop()
        if function in reached:
            continue
        if function not in ir:
            if function not in declarations:
                raise IdentityError(f'canonical adapter IR dependency missing: {function}')
            external.add(function)
            continue
        reached[function] = ir[function]
        pending.update(match[1] for lines in ir[function].values() for line in lines
                       if (match := re.search(r'\bcall\b.*?@([\w.$]+)\(', line)))
    return reached, external


def classify_construction(record, wrapper, selected_roots, writer_evidence, inventory_roots=None):
    """Apply the owner's D boundary; retain every observed first difference."""
    record['admission_inputs'] = {'wrapper': wrapper, 'selected_roots': sorted(selected_roots),
        'inventory_roots': sorted(selected_roots if inventory_roots is None else inventory_roots),
        'writer_evidence': writer_evidence}
    _classify_construction(record, wrapper, selected_roots, writer_evidence, inventory_roots)


def observed_difference(failure):
    return (type(failure.get('instruction')) is int and failure['instruction'] > 0
            and isinstance(failure.get('seq'), str) and isinstance(failure.get('demand'), str)
            and failure['seq'] != failure['demand'])


PROJECTION_ROLE = 'conservative actual W1 wrapper CFG projection; all nondecision instructions retained'
ADMITTED_CATEGORIES = ('latched-writer', 'entry-adapter', 'entry-version', 'join-reload')
INLINED_ROLE = 'cheap-region clone inlined into its entry-version function; layout-free shape graph from the clone root'
ENTRY_VERSION_QUALIFICATION = ('default --par picks the version once at function entry, so the allocator re-assigns saved registers, '
                               'spill slots and frame and the optimizer may inline the clone and reorder its blocks; opcodes, immediates, '
                               'memory shapes, call targets and branch topology are identical, machine identity is not proved')


def entry_version_rows(record, wrapper, failure):
    """Replay: the wrapper's retained rows whose strict difference is exactly the stored one, or None."""
    evidence = record.get('entry_version_evidence')
    if (not isinstance(evidence, dict) or evidence.get('wrapper') != wrapper
            or evidence.get('wrapper_pool_calls') != record.get('dispatch', {}).get('pool_decisions_per_timed_call')
            or failure['function'] != wrapper or failure['reason'] != 'normalized timed sequential wrapper differs'
            or not observed_difference(failure)):
        return None
    for row in record['comparisons']:
        if row['function'] == wrapper and row['role'] == PROJECTION_ROLE and row.get('identical') is False:
            seq, demand = row.get('seq_instructions'), row.get('demand_instructions')
            if not (isinstance(seq, list) and seq and isinstance(demand, list) and demand
                    and all(isinstance(line, str) for line in seq + demand)):
                return None
            strict = first_difference(seq, demand)
            if strict is None or any(failure.get(key) != value for key, value in strict.items()):
                return None
            return seq, demand
    return None


def verified_entry_version(record, wrapper, failure):
    """Replay: the stored strict difference, and shape graphs that agree."""
    rows = entry_version_rows(record, wrapper, failure)
    return rows is not None and layout_free_difference(*rows) is None


def verified_join_reload(record, wrapper, failure):
    """Replay: the stored strict difference, and one admitted join reload; its evidence or None."""
    rows = entry_version_rows(record, wrapper, failure)
    return None if rows is None else join_reload_difference(*rows)


def _classify_construction(record, wrapper, selected_roots, writer_evidence, inventory_roots=None):
    writer_evidence = {function: evidence for function, evidence in writer_evidence.items()
                       if verified_writer_evidence(function, evidence)}
    closure = [row for row in record['comparisons'] if row['role'] == 'reachable sequential body/helper']
    compared = {row['function'] for row in closure}
    inventory_roots = selected_roots if inventory_roots is None else inventory_roots | selected_roots
    unknown = []
    for failure in record['failures']:
        failure.pop('evidence', None)
        evidence = writer_evidence.get(failure['function'])
        if evidence and observed_difference(failure) and failure['reason'] == 'normalized sequential body differs':
            failure.update(category='latched-writer', evidence=evidence)
        elif verified_entry_version(record, wrapper, failure):
            failure.update(category='entry-version', evidence={
                'timed_wrapper': wrapper, 'qualification': ENTRY_VERSION_QUALIFICATION})
        elif (reload := verified_join_reload(record, wrapper, failure)) is not None:
            failure.update(category='join-reload', evidence={
                'timed_wrapper': wrapper, 'reload': reload, 'qualification': JOIN_RELOAD_QUALIFICATION})
        else:
            failure['category'] = 'ordinary'
            if failure['function'] != wrapper:
                unknown.append(failure)
    comparison_complete = all(isinstance(row.get('seq_instructions'), list) and bool(row['seq_instructions'])
        and isinstance(row.get('demand_instructions'), list) and bool(row['demand_instructions'])
        and all(isinstance(line, str) for line in row['seq_instructions'] + row['demand_instructions'])
        and row.get('seq_function') == canonical_symbol(row['function'])
        and row['identical'] is (first_difference(row['seq_instructions'], row['demand_instructions']) is None)
        and (row['identical'] or any(failure['function'] == row['function']
             and all(failure.get(key) == value for key, value in first_difference(row['seq_instructions'], row['demand_instructions']).items())
             for failure in record['failures'])) for row in closure)
    closure_complete = (comparison_complete and bool(inventory_roots) and inventory_roots <= compared
                        and bool(closure) and not unknown
                        and all(row['identical'] is True or row['function'] in writer_evidence
                                and any(failure['function'] == row['function'] and failure['category'] == 'latched-writer'
                                        for failure in record['failures']) for row in closure))
    adapter = record.get('entry_adapter_provenance', {})
    proven_ir_entrance = verified_adapter_evidence(wrapper, adapter, compared, closure)
    for failure in record['failures']:
        observed_adapter_difference = any(row['function'] == wrapper
            and row['role'] == 'conservative actual W1 wrapper CFG projection; all nondecision instructions retained'
            and row.get('identical') is False
            and isinstance(row.get('seq_instructions'), list) and bool(row['seq_instructions'])
            and isinstance(row.get('demand_instructions'), list) and bool(row['demand_instructions'])
            and all(isinstance(line, str) for line in row['seq_instructions'] + row['demand_instructions'])
            and (difference := first_difference(row['seq_instructions'], row['demand_instructions'])) is not None
            and all(failure.get(key) == value for key, value in difference.items())
            for row in record['comparisons'])
        if (failure['function'] == wrapper and 'dispatch' in record and closure_complete
                and record['dispatch']['pool_decisions_per_timed_call'] == 1
                and record['dispatch'].get('outside_loops') is True
                and not record['dispatch']['regions'] and proven_ir_entrance
                and observed_difference(failure) and observed_adapter_difference
                and failure['reason'] == 'normalized timed sequential wrapper differs'):
            failure.update(category='entry-adapter', evidence={
                'timed_wrapper': wrapper, 'selected_sequential_roots': sorted(selected_roots),
                'closure_functions': sorted(compared),
                'dispatch': record['dispatch'],
                'ir_entrance_provenance': adapter or None,
                'qualification': 'owner-admitted entry boundary: exact IR adapter projection and canonical closure; wrapper machine identity/state preservation are not proved'})
    record['computation_status'] = ('pass' if closure_complete else
                                    'fail' if unknown or any(not row['identical'] and row['function'] not in writer_evidence for row in closure)
                                    else 'unverified')
    record['computation_scope'] = {'selected_sequential_roots': sorted(selected_roots),
                                   'inventoried_sequential_roots': sorted(inventory_roots),
                                   'compared_closure_functions': sorted(compared),
                                   'shared_support_exceptions': sorted(writer_evidence),
                                   'scope': 'canonical sequential body/helper identity, except the evidenced shared cold writer; entry adapter is a separate admitted boundary',
                                   'complete': closure_complete}
    record['timed_selection_status'] = ('pass' if 'W1' in record.get('actual_entry', {})
                                        and 'dispatch' in record
                                        and (not record['dispatch']['regions'] or 'W2/W4/W8' in record['actual_entry'])
                                        else 'unverified')
    record['timed_selection_scope'] = ('ABI-preserving concrete W1 and covered active cheap-path body selection only; '
                                        'adapter identity and overall dispatch coverage remain separate construction obligations')
    record['construction_differences'] = [failure for failure in record['failures']
                                         if failure['category'] != 'ordinary']
    dispatch = record.get('dispatch', {})
    regions = dispatch.get('regions', [])
    entry = record.get('actual_entry', {})
    pool = dispatch.get('pool_decisions_per_timed_call')
    accounting = record.get('versioned_region_accounting', {})
    accounted = {function for function, kind in accounting.items()
                 if kind in ('outside-timed-path', 'clone-inlined-into-compared-body')
                 or kind == 'indirect-clone-compared'
                 and 'wf__par_seq_' + function.removeprefix('wf_') in compared}
    path = record.get('entry_version_active_path', {})
    entry_version_path = (path.get('verified') is True
                          and path.get('regions') == sorted(region['function'] for region in regions))
    # Replay each inlined clone's shape comparison from its retained rows.
    inlined_shapes = all(any(row['function'] == region['function'] and row['role'] == INLINED_ROLE
                             and type(row.get('demand_root')) is int and row['demand_root'] + 1 == region.get('inlined_clone_root')
                             and isinstance(row.get('seq_instructions'), list) and bool(row['seq_instructions'])
                             and isinstance(row.get('demand_instructions'), list) and bool(row['demand_instructions'])
                             and all(isinstance(line, str) for line in row['seq_instructions'] + row['demand_instructions'])
                             and layout_free_difference(row['seq_instructions'], row['demand_instructions'],
                                                        0, row['demand_root']) is None
                             for row in record['comparisons'])
                         for region in regions if region.get('clone_transfer') == 'inlined')
    decisions_complete = (type(pool) is int and pool in (0, 1) and dispatch.get('outside_loops') is True
        and (proven_ir_entrance or entry.get('W1', {}).get('outside_loops') is True
             and entry['W1'].get('decision_trace', []).count('pool-active') == pool
             or pool == 0 and record.get('entry_version_evidence', {}).get('wrapper_pool_calls') == 0)
        and set(record.get('versioned_region_inventory', [])) - accounted == {region['function'] for region in regions}
        and all(type(region.get('decisions_per_active_timed_call')) is int and region['decisions_per_active_timed_call'] == 1
                and type(region.get('assembly_decisions_per_cheap_timed_call')) is int
                and 1 <= region['assembly_decisions_per_cheap_timed_call'] <= region.get('ir_comparison_bound', 1)
                and region.get('outside_loops') is True for region in regions)
        and (not regions or entry_version_path
             or entry.get('W2/W4/W8', {}).get('outside_loops') is True
             and entry['W2/W4/W8'].get('computation') == entry.get('W1', {}).get('computation')
             and isinstance(entry['W2/W4/W8'].get('inputs'), dict) and bool(entry['W2/W4/W8']['inputs'])
             and all(type(value) is int for value in entry['W2/W4/W8']['inputs'].values())
             and entry['W2/W4/W8'].get('decision_trace', []).count('pool-active') == pool
             and entry['W2/W4/W8'].get('decision_trace', []).count('cheap-region') == len(regions))
        and inlined_shapes
        and all(type(width) is int for width in record.get('covered_widths', []))
        and {1, *([2, 4, 8] if regions else [])} <= set(record.get('covered_widths', [])))
    categories = {category: [failure for failure in record['failures'] if failure['category'] == category]
                  for category in ADMITTED_CATEGORIES}
    ordinary = [failure for failure in record['failures'] if failure['category'] == 'ordinary']
    admitted = closure_complete and decisions_complete and not ordinary
    record['admission'] = {'policy': 'experiment-6-option-D', 'status': 'pass' if admitted else 'fail',
        'admitted_differences': categories, 'category_counts': {key: len(value) for key, value in categories.items()},
        'unadmitted_failures': ordinary, 'cold_support_boundary': 'wf_resource_record_abort',
        'entry_decisions': dispatch, 'decision_proof_complete': decisions_complete,
        'qualification': 'writer common IR/call inventory and adapter IR entrance are proved; full boundary machine identity is explicitly excluded'}
    record['status'] = record['admission']['status']


def validate_admission(record):
    """Replay D evidence; consumer labels, counts and status are never authority.

    Image hashes bind the report in the hosted consumer. This pure replay checks
    retained IR/machine snapshots and every ordinary difference without tools.
    """
    try:
        inputs = record['admission_inputs']
        replay = copy.deepcopy(record)
        _classify_construction(replay, inputs['wrapper'], set(inputs['selected_roots']),
                               inputs['writer_evidence'], set(inputs['inventory_roots']))
        for key in ('failures', 'construction_differences', 'computation_status', 'computation_scope',
                    'timed_selection_status', 'timed_selection_scope', 'admission', 'status'):
            if replay[key] != record[key]:
                raise IdentityError(f'inconsistent D admission field: {key}')
        if replay['status'] != 'pass':
            raise IdentityError('D admission has ordinary, incomplete computation or dispatch evidence')
        return replay['admission']
    except (KeyError, TypeError, AttributeError) as error:
        raise IdentityError(f'incomplete D admission evidence: {error}') from error


def check_workload(name, build):
    record = {'status': 'fail', 'computation_status': 'unverified', 'covered_widths': [1], 'comparisons': [], 'failures': []}
    selected_roots, inventory_roots, writer_evidence = set(), set(), {}
    def failure(function, reason, difference=None):
        record['failures'].append(dict(workload=name, function=function, reason=reason, **(difference or {})))
    try:
        seq_image = ElfImage(build / 'seq' / name)
        demand_image = ElfImage(build / 'demand' / name)
        seq = seq_image.bound(parse_disassembly((build / 'seq' / f'{name}.disassembly').read_text()))
        demand = demand_image.bound(parse_disassembly((build / 'demand' / f'{name}.disassembly').read_text()))
        demand_text = (build / 'demand' / f'{name}.ll').read_text()
        seq_text = (build / 'seq' / f'{name}.ll').read_text()
        ir, seq_ir = parse_ir(demand_text), parse_ir(seq_text)
        wrappers = [symbol for symbol in seq_ir if symbol.startswith('wf_bench_') and not symbol.endswith('_release')]
        if len(wrappers) != 1:
            raise IdentityError(f'expected one timed wrapper, found {wrappers}')
        wrapper = wrappers[0]
        signature = re.search(r'define\b[^\n]*@' + re.escape(wrapper) + r'\(([^)]*)\)', demand_text)
        if not signature or any(not re.match(r'(?:i[0-9]+|ptr)\b', argument.strip()) for argument in signature[1].split(',') if argument.strip()):
            raise IdentityError('unsupported noninteger/pointer timed-wrapper ABI')
        argument_count = len([argument for argument in signature[1].split(',') if argument.strip()])
        try:
            record['dispatch'] = dispatch_proof(ir, wrapper)
        except (IdentityError, StopIteration) as error:
            failure(wrapper, f'dispatch proof: {error}')
        if wrapper not in demand or wrapper not in seq:
            raise IdentityError(f'missing disassembled timed wrapper {wrapper}')
        try:
            settings = json.loads(manifest_path().read_text())[name]
            arguments = {'rdi': settings['repetitions'], 'rsi': settings['extent']} if wrapper == 'wf_bench_micro' and 'extent' in settings else None
            seq_root, _ = computation_root(seq, wrapper, False, arguments, argument_count)
            demand_root, decisions = computation_root(demand, wrapper, False, arguments, argument_count)
            count = decisions.count('pool-active')
            if 'dispatch' in record and count != record['dispatch']['pool_decisions_per_timed_call']:
                raise IdentityError('LLVM and optimized assembly dynamic pool counts disagree')
            left = normalized(seq[seq_root], image=seq_image)
            right = normalized(demand[demand_root], image=demand_image)
            diff = first_difference(left, right)
            record['comparisons'].append({'function': wrapper, 'seq_computation': seq_root, 'demand_computation': demand_root, 'role': 'actual timed W1 computation after ABI-preserving entry shell', 'identical': diff is None})
            record['actual_entry'] = {'W1': {'computation': demand_root, 'decision_trace': decisions, 'outside_loops': True}}
            if diff:
                failure(wrapper, 'normalized timed sequential computation differs', diff)
            pending = direct_calls(demand[demand_root], functions=demand) | {demand_root}
            if demand_root.startswith('wf__par_seq_') or diff is None and not decisions:
                selected_roots.add(demand_root)
            if 'dispatch' in record and record['dispatch']['regions']:
                if arguments is None:
                    raise IdentityError('cheap-region concrete timed scalar inputs unavailable')
                active_root, active_decisions = computation_root(demand, wrapper, True, arguments, argument_count)
                if active_root != demand_root or active_decisions.count('pool-active') != count or active_decisions.count('cheap-region') != len(record['dispatch']['regions']):
                    raise IdentityError('active timed cheap path does not select same sequential body once')
                record['actual_entry']['W2/W4/W8'] = {'computation': active_root, 'decision_trace': active_decisions, 'outside_loops': True, 'inputs': arguments}
        except IdentityError as error:
            record['entry_state_proof'] = {'status': 'unverified', 'reason': str(error)}
            pending = set()
            try:
                selected, omitted, count, forced = project_pool(demand[wrapper], False)
                if 'dispatch' in record and count != record['dispatch']['pool_decisions_per_timed_call']:
                    raise IdentityError('LLVM and optimized assembly dynamic pool counts disagree')
                left = normalized(seq[wrapper], image=seq_image)
                right = normalized(demand[wrapper], selected, omitted, image=demand_image, forced=forced)
                diff = first_difference(left, right)
                record['comparisons'].append({'function': wrapper,
                    'seq_instructions': left, 'demand_instructions': right,
                    'role': 'conservative actual W1 wrapper CFG projection; all nondecision instructions retained', 'identical': diff is None})
                record['entry_version_evidence'] = {'wrapper': wrapper, 'wrapper_pool_calls': count}
                headers = [re.search(r'^define\b[^\n]*@' + re.escape(wrapper) + r'\([^\n]*$', text, re.M)
                           for text in (seq_text, demand_text)]
                provenance = entry_adapter_evidence(seq_ir, ir, wrapper,
                    headers[0][0] if headers[0] else None, headers[1][0] if headers[1] else None)
                if provenance:
                    closure, declarations = adapter_ir_inventory(ir, provenance['inactive_ir_root'],
                        set(re.findall(r'^declare\b[^\n]*@([\w.$]+)\(', demand_text, re.M)))
                    provenance['canonical_ir_closure'] = closure
                    provenance['canonical_ir_declarations'] = sorted(declarations)
                    provenance['optimized_pool_projection'] = {
                        'pool_decisions_per_timed_call': count,
                        'outside_loops': True,
                        'instructions': instruction_snapshot(demand[wrapper], demand),
                        'selected_instruction_addresses': [demand[wrapper][at].address for at in sorted(selected)],
                        'state_preservation': 'unverified; CFG selection does not establish ABI preservation'}
                    record['entry_adapter_provenance'] = provenance
                if diff:
                    shape_diff = layout_free_difference(left, right)
                    failure(wrapper, 'normalized timed sequential wrapper differs',
                            dict(diff, **({'shape_difference': shape_diff} if shape_diff else {})))
                else:
                    # Text equality after deleting a call cannot prove that
                    # live caller-clobbered values or flags survive that call.
                    # This projection diagnoses differences; only entry_shell
                    # supplies the state-preservation proof needed to admit.
                    failure(wrapper, f'wrapper CFG projection lacks entry-state preservation proof: {error}')
                pending.update(direct_calls(demand[wrapper], selected, demand))
                selected_roots.update(function for function in pending if function.startswith('wf__par_seq_'))
            except IdentityError as fallback_error:
                failure(wrapper, str(error))
                failure(wrapper, f'wrapper CFG projection: {fallback_error}')
        # Also inspect every selected original-function clone: inlined callees
        # are covered by the wrapper; surviving calls expand this set recursively.
        # Inventory provides independent evidence even if wrapper projection fails.
        for block in ir.get(wrapper, {}).values():
            inventory_roots.update(match[1] for line in block if (match := re.search(r'\bcall\b.*?@(wf__par_seq_[\w.$]+)\(', line)))
        pending.update(inventory_roots)
        seen = set()

        def compare_closure(pending):
            while pending:
                function = min(pending)
                pending.remove(function)
                if function in seen:
                    continue
                seen.add(function)
                compare_body(function, pending)

        def compare_body(function, pending):
            original = canonical_symbol(function)
            if not function.startswith(('wf_', 'wf.')) or function == 'wf__par_pool_active':
                return  # identical external symbols are retained in the caller
            if function not in demand or original not in seq:
                failure(function, 'missing corresponding sequential disassembly function')
                return
            try:
                left, right = normalized(seq[original], image=seq_image), normalized(demand[function], image=demand_image)
                diff = first_difference(left, right)
                record['comparisons'].append({'function': function, 'seq_function': original,
                    'seq_instructions': left, 'demand_instructions': right,
                    'role': 'reachable sequential body/helper', 'identical': diff is None})
                if diff:
                    failure(function, 'normalized sequential body differs', diff)
                    evidence = latched_writer_evidence(function, seq_ir, ir, demand[function], demand,
                                                       seq[original], seq,
                                                       ir_function_headers(seq_text), ir_function_headers(demand_text))
                    if evidence:
                        writer_evidence[function] = evidence
                pending.update(direct_calls(demand[function], functions=demand))
            except IdentityError as error:
                failure(function, str(error))

        compare_closure(pending)
        if 'dispatch' in record:
            regions = record['dispatch']['regions']
            inventory = {function for function, blocks in ir.items() if 'par.region.entry' in blocks}
            covered = {region['function'] for region in regions}
            record['versioned_region_inventory'] = sorted(inventory)
            # Every function now has its own entry version. One the timed
            # wrapper does not call directly is accounted for only when it is
            # unreachable from the wrapper, or reachable with its sequential
            # clone compared above; anything else stays a failure.
            reach = ir_call_reach(ir, wrapper)
            machine_calls = set()
            for function in {wrapper, *seen}:
                if function in demand:
                    machine_calls |= direct_calls(demand[function], functions=demand)
            accounting = {}
            for function in sorted(inventory - covered):
                clone = 'wf__par_seq_' + function.removeprefix('wf_')
                if function not in reach:
                    accounting[function] = 'outside-timed-path'
                elif clone in seen:
                    accounting[function] = 'indirect-clone-compared'
                elif clone not in machine_calls:
                    # Reachable in IR, but no compared function calls its clone:
                    # the optimizer inlined it into a compared body.
                    accounting[function] = 'clone-inlined-into-compared-body'
            record['versioned_region_accounting'] = accounting
            if inventory != covered | set(accounting):
                failure(wrapper, f'versioned regions not covered: {sorted(inventory - covered - set(accounting))}')
            if regions:
                if 'W2/W4/W8' in record.get('actual_entry', {}):
                    record['covered_widths'] = [1, 2, 4, 8]
                shapes_ok = True
                # A raw-IR branch alone cannot establish that optimized dispatch
                # executes outside machine loops. Demand region asm must consist
                # of an acyclic decision prefix leading to the sequential path:
                # one direct transfer to the clone (a tail jump, or a call whose
                # continuation is only result plumbing), or the clone inlined,
                # whose shape graph must then be the canonical function's. The
                # machine comparisons must be explained by the entry block's IR.
                for region in regions:
                    function = region['function']
                    instructions = demand.get(function, [])
                    edges = assembly_edges(instructions)
                    clone = 'wf__par_seq_' + function.removeprefix('wf_')
                    transfers = [insn for insn in instructions if insn.mnemonic in ('jmp', 'call') and target(insn)
                                 and target(insn)[1].removesuffix('@plt') == clone]
                    try:
                        if transfers:
                            exit_at, kind = clone_transfer(instructions, clone)
                            prefix = {node for node in reachable(edges) if exit_at in reachable(edges, node)}
                            if clone not in seen:
                                failure(function, 'cheap-region sequential body was not covered by timed-path inventory')
                                shapes_ok = False
                        else:
                            kind = 'inlined'
                            if function not in seq:
                                raise IdentityError('missing canonical sequential function of the inlined clone')
                            left = normalized(seq[function], image=seq_image)
                            right = normalized(instructions, image=demand_image)
                            root, prefix = inlined_clone_root(left, right)
                            record['comparisons'].append({'function': function, 'seq_function': function,
                                'seq_instructions': left, 'demand_instructions': right, 'demand_root': root,
                                'role': INLINED_ROLE, 'identical': False})
                            region['inlined_clone_root'] = root + 1
                            region['sequential_identity'] = ENTRY_VERSION_QUALIFICATION
                            if any(instructions[node].mnemonic.startswith('call') for node in prefix):
                                raise IdentityError('the decision before the inlined clone calls a function')
                            compare_closure(direct_calls(instructions, line_reach(right, [root]), demand))
                    except IdentityError as error:
                        addresses = {insn.address for insn in instructions}
                        failure(function, f'unsupported optimized cheap-region shape: {error}', {'diagnostic': {
                            'clone': clone, 'instruction_count': len(instructions),
                            'external_transfers': sorted({f'{insn.mnemonic} {target(insn)[1]}' for insn in instructions
                                                          if insn.mnemonic in ('jmp', 'call') and target(insn)
                                                          and target(insn)[0] not in addresses}),
                            'instructions': [f'{insn.mnemonic} {insn.operands}'[:90] for insn in instructions[:120]]}})
                        shapes_ok = False
                        continue
                    if any(on_cycle(edges, node) for node in prefix):
                        failure(function, 'optimized cheap-region decision can execute inside a loop')
                        shapes_ok = False
                    comparisons = [node for node in prefix if instructions[node].mnemonic.startswith(('cmp', 'test'))]
                    bound = max(1, entry_compare_bound(ir.get(function, {})))
                    if not 1 <= len(comparisons) <= bound:
                        failure(function, f'unsupported cheap-region predicate shape: {len(comparisons)} machine comparisons, IR explains {bound}')
                        shapes_ok = False
                    region['assembly_decisions_per_cheap_timed_call'] = len(comparisons)
                    region['ir_comparison_bound'] = bound
                    region['clone_transfer'] = kind
                    region['assembly_prefix_instructions'] = len(prefix)
                if 'W2/W4/W8' not in record.get('actual_entry', {}):
                    if shapes_ok:
                        # No concrete entry-shell walk exists for this shape; the
                        # active path is the region entries just checked.
                        record['entry_version_active_path'] = {
                            'verified': True, 'regions': sorted(region['function'] for region in regions),
                            'basis': 'each region function is an acyclic cheap prefix leading to its one clone transfer '
                                     'or to its inlined clone, whose shape graph is the canonical function\'s'}
                        record['covered_widths'] = [1, 2, 4, 8]
                    else:
                        failure(wrapper, 'actual active cheap-region path remains unproved')
        classify_construction(record, wrapper, selected_roots, writer_evidence, inventory_roots)
    except (OSError, IdentityError, ValueError) as error:
        failure('<construction>', str(error))
        for item in record['failures']:
            item.setdefault('category', 'ordinary')
        record['computation_status'] = 'fail'
    return record


def shape_self_test():
    """Entry-version shapes: the accepted allocator-only difference, and wrong shapes that must fail."""
    seq = ['push %saved0.0', 'sub $0x18,%rsp', 'mov %r9,%saved0.0', 'sub %r8,%saved0.0', 'jne I7',
           'mov $0x14,%rdi', 'call wf__heap_take', 'add $0x18,%rsp', 'pop %saved0.0', 'ret']
    demand = ['push %saved0.0', 'push %saved1.0', 'sub $0x28,%rsp', 'mov %r9,%saved1.0', 'mov %saved1.0,0x10(%rsp)',
              'sub %r8,%saved1.0', 'jne I10', 'mov $0x14,%rdi', 'call wf__heap_take', 'nopl 0x0(%rax)',
              'add $0x28,%rsp', 'pop %saved1.0', 'pop %saved0.0', 'ret']
    if first_difference(seq, demand) is None:
        raise IdentityError('shape self-test fixture is not strictly different')
    if layout_free_difference(seq, demand) is not None:
        raise IdentityError('shape self-test rejected an allocator-only difference')
    wrong = {
        'immediate': [line.replace('$0x14', '$0x15') for line in demand],
        'extra call': demand[:9] + ['call wf__heap_take'] + demand[9:],
        'other callee': [line.replace('wf__heap_take', 'wf__heap_change') for line in demand],
        'branch topology': [line.replace('jne I10', 'jne I8') for line in demand],
        'dropped operation': [line for line in demand if not line.startswith('sub %r8')],
        'inverted condition': [line.replace('jne I10', 'je I10') for line in demand],
    }
    # The same function with the taken block placed first: the inverted
    # branch and a placement jump are layout, not computation.
    reordered = ['push %saved0.0', 'sub $0x18,%rsp', 'mov %r9,%saved0.0', 'sub %r8,%saved0.0', 'je I8',
                 'add $0x18,%rsp', 'pop %saved0.0', 'ret', 'mov $0x14,%rdi', 'call wf__heap_take', 'jmp I5']
    if layout_free_difference(seq, reordered) is not None:
        raise IdentityError('shape self-test rejected a block reordering')
    wrong['reordered without inversion'] = [line.replace('je I8', 'jne I8') for line in reordered]
    # Paths that join in the canonical function must join here too.
    wrong['duplicated join'] = reordered[:-1] + ['add $0x18,%rsp', 'pop %saved0.0', 'ret']
    for name, lines in wrong.items():
        if layout_free_difference(seq, lines) is None:
            raise IdentityError(f'shape self-test accepted a wrong shape: {name}')

    if layout_free_difference(['movaps %xmm0,%xmm1', 'addsd %xmm1,%xmm2', 'movaps %xmm2,0x10(%rsp)', 'ret'],
                              ['addsd %xmm0,%xmm3', 'ret']) is not None:
        raise IdentityError('shape self-test rejected vector register copies')
    if layout_free_difference(['movaps %xmm0,%xmm1', 'addsd %xmm1,%xmm2', 'ret'],
                              ['mulsd %xmm0,%xmm3', 'ret']) is None:
        raise IdentityError('shape self-test accepted a changed vector operation')

    # A zero displacement an RBP or R13 base must encode is the same address.
    if layout_free_difference(['movsd 0x0(%saved0.0,%saved1.0,8),%xmm1', 'ret'],
                              ['movsd (%saved1.0,%saved0.0,8),%xmm1', 'ret']) is not None:
        raise IdentityError('shape self-test rejected an encoded zero displacement')
    if layout_free_difference(['movsd 0x0(%saved0.0,%saved1.0,8),%xmm1', 'ret'],
                              ['movsd 0x8(%saved1.0,%saved0.0,8),%xmm1', 'ret']) is None:
        raise IdentityError('shape self-test accepted a changed displacement')

    # A join reload: the canonical function stores the length it keeps in a
    # register on the allocating path and loads it on the empty path; the
    # entry-version function loads it once where the two paths join.
    empty = 'lea [readonly-address:empty](%rip),%saved0.0'
    allocating = ['push %saved0.0', 'push %saved1.0', 'test %rdi,%rdi', 'je I10', 'mov %rdi,%saved1.0',
                  'call wf__heap_take', 'mov %rax,%saved0.0', 'mov %saved1.0,(%rax)', 'call memset']
    returning = ['lea 0x8(%saved0.0),%rax', 'pop %saved1.0', 'pop %saved0.0', 'ret']
    kept = allocating + ['jmp I12', empty, 'mov (%saved0.0),%saved1.0', 'mov %saved1.0,(%rsi)'] + returning
    reloaded = allocating + ['jmp I11', empty, 'mov (%saved0.0),%rcx', 'mov %rcx,(%rsi)'] + returning
    if layout_free_difference(kept, reloaded) is None:
        raise IdentityError('shape self-test join reload fixture has no shape difference')
    if join_reload_difference(kept, reloaded) is None:
        raise IdentityError('shape self-test rejected a join reload')

    def looped(lines):
        # A back edge from the join block to the test makes both loads loop.
        return lines[:-3] + ['jne I2'] + lines[-3:]

    def replaced(lines, at, line):
        return lines[:at] + [line] + lines[at + 1:]
    reload_wrong = {
        'join reload in a loop': (looped(kept), looped(reloaded)),
        # The load ends one path instead of starting the join block, so the
        # empty path reaches the store without it.
        'join reload off the join': (kept, replaced(allocating, 3, 'je I11')
                                     + ['mov (%saved0.0),%rcx', 'jmp I12', empty, 'mov %rcx,(%rsi)'] + returning),
        'join reload at another offset': (kept, replaced(reloaded, 11, 'mov 0x8(%saved0.0),%rcx')),
        'join reload from another base': (kept, replaced(reloaded, 11, 'mov (%saved2.0),%rcx')),
        'join reload of an unstored value': (replaced(kept, 7, 'mov %rdx,(%rax)'), replaced(reloaded, 7, 'mov %rdx,(%rax)')),
        'join reload after an overwrite': (replaced(kept, 8, 'movq $0x0,(%saved0.0)'),
                                           replaced(reloaded, 8, 'movq $0x0,(%saved0.0)')),
    }
    for name, (seq_lines, demand_lines) in reload_wrong.items():
        if join_reload_difference(seq_lines, demand_lines) is not None:
            raise IdentityError(f'shape self-test accepted a wrong join reload: {name}')
        wrong[name] = demand_lines

    # An inlined clone: a decision prefix, the parallel path through a split
    # call, and the canonical function's code entered from the decision.
    prefix = ['push %saved0.0', 'cmp $0x2,%rdi', 'jb I{clone}']
    parallel = ['mov $0x1,%rsi', 'call wf__par_split_f.0', 'pop %saved0.0', 'ret']
    def inlined(body):
        clone = len(prefix) + len(parallel)
        lines = [line.replace('{clone}', str(clone)) for line in prefix] + parallel
        return lines + [re.sub(r'I(\d+)', lambda match: f'I{int(match[1]) + clone}', line) for line in body]
    region = inlined(demand)
    found, decision = inlined_clone_root(seq, region)
    if found != len(prefix) + len(parallel) or decision != {0, 1, 2}:
        raise IdentityError('shape self-test missed an inlined clone')
    for name in ('immediate', 'other callee', 'branch topology'):
        try:
            inlined_clone_root(seq, inlined(wrong[name]))
        except IdentityError:
            continue
        raise IdentityError(f'shape self-test accepted a wrong inlined clone: {name}')
    try:
        inlined_clone_root(seq, [line for line in region if not line.startswith('call wf__par_split_')])
    except IdentityError:
        pass
    else:
        raise IdentityError('shape self-test accepted a cheap region without a parallel split')

    # A projection whose kept edge is a taken branch keeps that edge.
    projected = [Instruction(0x10, 'call', '400 <wf__par_pool_active>'), Instruction(0x15, 'test', '%eax,%eax'),
                 Instruction(0x17, 'je', '20 <wf_f+0x10>'), Instruction(0x19, 'call', '500 <wf_g>'),
                 Instruction(0x1e, 'ret', ''), Instruction(0x20, 'mov', '$0x1,%eax'), Instruction(0x25, 'ret', '')]
    rows = normalized(projected, {0, 1, 2, 4, 5, 6}, {0, 1, 2}, forced={2: 5})
    if rows != ['jmp I2', 'ret', 'mov $0x1,%eax', 'ret']:
        raise IdentityError(f'shape self-test lost a forced projection edge: {rows}')
    if normalized(projected, {0, 1, 2, 3, 4}, {0, 1, 2}, forced={2: 3}) != ['call wf_g', 'ret']:
        raise IdentityError('shape self-test added a jump for a fall-through projection edge')

    def code(*rows):
        return [Instruction(address, mnemonic, operands) for address, mnemonic, operands in rows]
    clone = 'wf__par_seq_f'
    tail = code((0x10, 'cmp', '$0x2,%rdi'), (0x14, 'jb', '1c <wf_f+0xc>'), (0x16, 'jmp', f'500 <{clone}>'), (0x1c, 'ret', ''))
    if clone_transfer(tail, clone) != (2, 'jmp'):
        raise IdentityError('shape self-test rejected a direct tail transfer')
    stored = code((0x10, 'cmp', '$0x2,%rdi'), (0x14, 'jb', '20 <wf_f+0x10>'), (0x16, 'call', f'500 <{clone}>'),
                  (0x1b, 'mov', '%rax,(%rbx)'), (0x1e, 'pop', '%rbx'), (0x1f, 'ret', ''), (0x20, 'ret', ''))
    if clone_transfer(stored, clone) != (2, 'call'):
        raise IdentityError('shape self-test rejected a call followed by result plumbing')
    bad = {
        'work after the call': code((0x10, 'call', f'500 <{clone}>'), (0x15, 'call', '600 <wf_other>'), (0x1a, 'ret', '')),
        'two transfers': code((0x10, 'cmp', '$0x2,%rdi'), (0x14, 'jb', '19 <wf_f+0x9>'),
                              (0x16, 'jmp', f'500 <{clone}>'), (0x19, 'jmp', f'500 <{clone}>')),
        'no transfer': code((0x10, 'cmp', '$0x2,%rdi'), (0x14, 'ret', '')),
        'branch after the call': code((0x10, 'call', f'500 <{clone}>'), (0x15, 'test', '%rax,%rax'),
                                      (0x18, 'je', '1a <wf_f+0xa>'), (0x1a, 'ret', '')),
    }
    for name, rows in bad.items():
        try:
            clone_transfer(rows, clone)
        except IdentityError:
            continue
        raise IdentityError(f'shape self-test accepted a wrong clone transfer: {name}')
    blocks = {'par.region.entry': ['%a = icmp ult i64 %x, 2', '%b = call i64 @llvm.umax.i64(i64 %y, i64 1)',
                                   '%c = call { i64, i1 } @llvm.umul.with.overflow.i64(i64 13, i64 %y)',
                                   '%d = udiv i64 149999, %b', '%e = udiv i64 %b, 16',
                                   '%f = call i64 @llvm.uadd.sat.i64(i64 %b, i64 3)', 'br i1 %a, label %p, label %q']}
    if entry_compare_bound(blocks) != 4:
        raise IdentityError('shape self-test miscounted the entry block comparisons')
    print(json.dumps({'entry_version_shape_self_test': 'pass', 'wrong_shapes_rejected': sorted(wrong) + sorted(bad)}))


def self_test(clang='clang', objdump='objdump'):
    """CI-only compiled mutation and retained private-symbol controls."""
    shape_self_test()
    with tempfile.TemporaryDirectory(prefix='wf-e6-identity-') as directory:
        directory = Path(directory)
        bodies = []
        for arm, constant in [('seq', 7), ('mutated', 9)]:
            source = directory / f'{arm}.c'
            obj = directory / f'{arm}.o'
            source.write_text(f'unsigned long wf_witness(unsigned long x) {{ return x + {constant}UL; }}\n')
            subprocess.run([clang, '-O2', '-c', str(source), '-o', str(obj)], check=True)
            disassembly = subprocess.check_output([objdump, '-d', '--no-show-raw-insn', str(obj)], text=True)
            bodies.append(normalized(parse_disassembly(disassembly)['wf_witness']))
        difference = first_difference(*bodies)
        if not difference:
            raise IdentityError('negative construction self-test accepted mutated sequential body')
        print(json.dumps({'negative_self_test': 'pass', 'function': 'wf_witness', 'detected': difference}))
        images, functions = {}, {}
        for arm, constant, retain in [('discarded', 7, False), ('retained', 7, True), ('changed', 9, True)]:
            source, executable = directory / f'{arm}.ll', directory / arm
            c_source, c_object = directory / f'{arm}_strings.c', directory / f'{arm}_strings.o'
            c_source.write_text('const char *wf_identity_c_string(void) { return "identity retained local string"; }\n')
            # These address-significant objects must remain distinct despite
            # equal bytes. Private functions use the same naming/bounds path
            # as the allocator failure helpers that motivated this control.
            source.write_text(f'''source_filename = "identity_private_fixture.ll"
@wf_identity_readonly_a = private constant [5 x i8] c"same\\00"
@wf_identity_readonly_b = private constant [5 x i8] c"same\\00"
define private i64 @wf_identity_private(i64 %x) noinline {{
entry:
  %result = add i64 %x, {constant}
  ret i64 %result
}}
define i64 @wf_identity_use(i64 %x) {{
entry:
  %result = call i64 @wf_identity_private(i64 %x)
  ret i64 %result
}}
define ptr @wf_identity_address_a() {{
entry:
  ret ptr @wf_identity_readonly_a
}}
define ptr @wf_identity_address_b() {{
entry:
  ret ptr @wf_identity_readonly_b
}}
define i32 @main() {{
entry:
  ret i32 0
}}
''')
            flags = ['-Wa,-L', '-Wl,--discard-none'] if retain else []
            subprocess.run([clang, '-O2', *(['-Wa,-L'] if retain else []), '-c', str(c_source), '-o', str(c_object)], check=True)
            subprocess.run([clang, '-O2', '-Wno-override-module', *flags, str(source), str(c_object), '-o', str(executable)], check=True)
            images[arm] = ElfImage(executable)
            disassembly = subprocess.check_output([objdump, '-d', '--no-show-raw-insn', str(executable)], text=True)
            functions[arm] = images[arm].bound(parse_disassembly(disassembly))
        if 'wf_identity_private' not in functions['retained'] or 'wf_identity_private' in functions['discarded']:
            raise IdentityError('native retention self-test did not retain the private function exclusively in the retained image')
        def executable_sections(image):
            return {name: image.data[section[4]:section[4] + section[5]]
                    for name, section in image.named_sections.items() if section[1] == 1 and section[2] & 4}
        if executable_sections(images['discarded']) != executable_sections(images['retained']):
            raise IdentityError('native symbol-retention flags changed executable section bytes')
        private_difference = first_difference(normalized(functions['retained']['wf_identity_private']),
                                              normalized(functions['changed']['wf_identity_private']))
        if private_difference is None:
            raise IdentityError('native retention self-test accepted a changed private callee body')
        actual_calls = direct_calls(functions['retained']['wf_identity_use'], functions=functions['retained'])
        if actual_calls != {'wf_identity_private'}:
            raise IdentityError('native retention self-test failed to resolve the actual private callee')
        image = images['retained']
        local_strings = [key for key, (_, _, kind) in image.symbols.items()
                         if kind == 1 and image.symbol_names[key] == '.L.str'
                         and image.symbol_bindings[key] == 0
                         and image.symbol_sources[key] == ('retained_strings.c', 0)]
        if len(local_strings) != 1:
            raise IdentityError('native retention self-test lacks final-ELF local .L.str STT_FILE provenance')
        string_key = local_strings[0]
        string_address, string_size, _ = image.symbols[string_key]
        if string_size != len(b'identity retained local string\0') or 'retained_strings.c#0::.L.str' not in image.readonly_address_identity(string_address):
            raise IdentityError('native retention self-test has incorrect bounded C string identity')
        normalized(functions['retained']['wf_identity_c_string'], image=image)
        addresses = []
        for suffix in ('a', 'b'):
            candidates = [address for key, (address, size, kind) in image.symbols.items()
                          if kind == 1 and size == 5 and retained_function_name(image.symbol_names[key]) == f'wf_identity_readonly_{suffix}']
            if len(candidates) != 1:
                raise IdentityError('native retention self-test lacks exact private readonly object bounds')
            addresses.append(candidates[0])
        if addresses[0] == addresses[1] or image.readonly_address_identity(addresses[0]) == image.readonly_address_identity(addresses[1]):
            raise IdentityError('native retention self-test erased distinct equal-content readonly address identities')
        address_difference = first_difference(normalized(functions['retained']['wf_identity_address_a'], image=image),
                                              normalized(functions['retained']['wf_identity_address_b'], image=image))
        if address_difference is None:
            raise IdentityError('native retention self-test accepted a changed readonly address target')
        print(json.dumps({'native_retention_self_test': 'pass', 'executable_bytes_identical': True,
                          'private_callee_mutation': private_difference, 'readonly_address_mutation': address_difference}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--clang', default='clang')
    parser.add_argument('--objdump', default='objdump')
    args = parser.parse_args()
    if args.self_test:
        self_test(args.clang, args.objdump)
        return
    if args.build is None:
        parser.error('--build is required unless --self-test')
    manifest = json.loads(manifest_path().read_text())
    records = {name: check_workload(name, args.build) for name in manifest}
    # small_split is a research coverage assertion, not a compiler dispatch.
    if not records.get('small_split', {}).get('dispatch', {}).get('regions'):
        records['small_split']['status'] = 'fail'
        records['small_split']['computation_status'] = 'fail'
        records['small_split']['failures'].append({'workload': 'small_split', 'function': 'wf_workload', 'category': 'ordinary', 'reason': 'required cheap-region dispatch coverage missing'})
    hashes = {}
    for name in manifest:
        for arm in ('seq', 'demand'):
            for suffix in ('', '.ll', '.o', '.disassembly'):
                relative = f'{arm}/{name}{suffix}'
                path = args.build / relative
                if path.exists():
                    hashes[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    computation = [record['computation_status'] for record in records.values()]
    for record in records.values():
        try:
            validate_admission(record)
        except IdentityError:
            record['status'] = 'fail'
    categories = {category: [failure for record in records.values() for failure in record['failures']
                             if failure.get('category') == category]
                  for category in ADMITTED_CATEGORIES}
    result = {'hashes': hashes, 'schema': 1, 'status': 'pass' if all(record['status'] == 'pass' for record in records.values()) else 'fail',
              'computation_status': 'pass' if all(status == 'pass' for status in computation) else 'fail' if 'fail' in computation else 'unverified',
              'admission_policy': 'experiment-6-option-D', 'admitted_differences': categories,
              'category_counts': {category: len(rows) for category, rows in categories.items()},
              'cold_support_boundary': 'wf_resource_record_abort',
              'entry_decisions': {name: record.get('dispatch') for name, record in records.items()},
              'unadmitted_failures': [failure for record in records.values() for failure in record['failures'] if failure.get('category') == 'ordinary'],
              'computation_scope': 'canonical sequential body/helper closure with evidenced cold-writer and entry-adapter boundaries under option D',
              'workloads': records, 'scope': 'all manifest W1 cells; every discovered cheap region at W1/W2/W4/W8; normalized actual timed wrapper and reachable sequential bodies; conservative CFG dispatch proof'}
    output = args.output or args.build / 'sequential-identity.json'
    output.write_text(json.dumps(result, indent=2) + '\n')
    for record in records.values():
        for failure in record['failures']:
            print(f"{failure['workload']} / {failure['function']} [{failure['category']}]: {failure['reason']}" + (f"; instruction {failure['instruction']}: seq={failure['seq']!r}, demand={failure['demand']!r}" if 'instruction' in failure else '')
                  + (f"; shape difference {failure['shape_difference']}" if 'shape_difference' in failure else '')
                  + (f"; transfers {failure['diagnostic']['external_transfers']}, {failure['diagnostic']['instruction_count']} instructions" if 'diagnostic' in failure else ''))
    for category, rows in categories.items():
        print(f"{category}: {len(rows)} evidenced differences")
    print('Cold support boundary: wf_resource_record_abort; common IR/call inventory proved, writer machine identity excluded')
    print(f"Covered canonical sequential body/helper closure: {result['computation_status']}; option D admission: {result['status']}")
    if result['status'] != 'pass':
        raise SystemExit(1)


if __name__ == '__main__':
    main()
