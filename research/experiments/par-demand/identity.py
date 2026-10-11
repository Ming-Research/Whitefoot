#!/usr/bin/env python3
"""E6 construction check, run on hosted Linux before any timing.

This is a deliberately conservative identity checker, not an equivalence
optimizer. Unsupported dispatch shapes fail construction. ABI registers,
constants, memory offsets, branch topology and external symbol names survive
normalization. Only consistent renaming of nonvolatile integer temporaries
and caller-clobbered r10/r11 is admitted. Never collapse all registers to REG.
"""
import argparse
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


def canonical_symbol(name):
    name = retained_function_name(name)
    name = name.removesuffix('@plt')
    return 'wf_' + name[len('wf__par_seq_'):] if name.startswith('wf__par_seq_') else name


def retained_function_name(name):
    # LLVM's private machine functions use temporary assembler labels. E6
    # retains them as evidence; their language-level names remain unchanged.
    return name[2:] if name.startswith('.Lwf_') else name


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
        offset, stride, count = fields[6], fields[11], fields[12]
        self.sections = [struct.unpack_from('<IIQQQQIIQQ', self.data, offset + index * stride) for index in range(count)]
        section_names = self.sections[fields[13]]
        names = self.data[section_names[4]:section_names[4] + section_names[5]]
        self.named_sections = {names[section[0]:].split(b'\0', 1)[0].decode(): section for section in self.sections}
        self.symbols = {}
        self.symbol_names = {}
        self.symbol_bindings = {}
        self.symbol_sources = {}
        records = {}
        for section in self.sections:
            if section[1] != 2:  # SHT_SYMTAB, not the incomplete dynamic table
                continue
            strings = self.sections[section[6]]
            table = self.data[strings[4]:strings[4] + strings[5]]
            file_name, file_ordinal, file_counts = '<unknown>', 0, {}
            for at in range(section[4], section[4] + section[5], section[9]):
                name, info, _, _, address, size = struct.unpack_from('<IBBHQQ', self.data, at)
                spelling = table[name:].split(b'\0', 1)[0].decode()
                if info & 15 == 4:  # STT_FILE identifies a local symbol's unit.
                    file_name = spelling
                    file_ordinal = file_counts.get(spelling, 0)
                    file_counts[spelling] = file_ordinal + 1
                    continue
                if spelling and address:
                    record = (address, size, info & 15, info >> 4, file_name, file_ordinal)
                    # Multiple symbol tables or aliases can repeat the exact
                    # record, but distinct addresses must all survive.
                    if record not in records.setdefault(spelling, []):
                        records[spelling].append(record)
        for spelling, values in records.items():
            values.sort()
            for ordinal, (address, size, kind, binding, file_name, file_ordinal) in enumerate(values):
                key = spelling if len(values) == 1 else f'{spelling}#{ordinal}'
                if key in self.symbols:
                    raise IdentityError(f'ELF symbol identity key collision: {key}')
                self.symbols[key] = (address, size, kind)
                self.symbol_names[key] = spelling
                self.symbol_bindings[key] = binding
                self.symbol_sources[key] = (file_name, file_ordinal)
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
            raise IdentityError('local readonly object lacks translation-unit provenance')
        spelling = self.symbol_names[key]
        peers = sorted((start, symbol_key) for symbol_key, (start, _, kind) in self.symbols.items()
                       if kind == 1 and self.symbol_names[symbol_key] == spelling
                       and self.symbol_sources[symbol_key] == source)
        ordinal = next(index for index, (_, symbol_key) in enumerate(peers) if symbol_key == key)
        suffix = f'#{ordinal}' if len(peers) > 1 else ''
        return f'{source[0]}#{source[1]}::{spelling}{suffix}'

    def readonly_address_identity(self, address):
        objects = [(key, start, length) for key, (start, length, kind) in self.symbols.items()
                   if kind == 1 and length and start <= address < start + length]
        if not objects:
            raise IdentityError('readonly address lacks exact retained object bounds; symbol-retaining image required')
        identities = []
        for key, start, length in objects:
            section = next((section for section in self.sections if section[3] <= start and start + length <= section[3] + section[5]), None)
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
        key, start, length = min(objects)
        return canonical_symbol(key), address - start, length

    def reference_identity(self, address, instruction, spelling):
        # Equal immutable contents do not imply equal pointer identity. Mutable
        # addresses use the exact bounded object and offset instead of contents.
        section = next((section for section in self.sections if section[3] <= address < section[3] + section[5]), None)
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
        key, displacement, extent = self.object_identity(address)
        if instruction.mnemonic.startswith(('call', 'jmp')):
            raise IdentityError('unsupported indirect RIP call or jump')
        content = self.reference_bytes(address, instruction).hex()
        return f'{key}+0x{displacement:x}/size={extent}:' + content

    def reference_bytes(self, address, instruction):
        for section in self.sections:
            _, kind, flags, begin, offset, size, *_ = section
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
    registers = [register for register in registers if register != 'rip']
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


def normalized(instructions, selected=None, omitted=(), image=None):
    """Preserve instruction order and label destinations, not absolute PCs."""
    selected = set(range(len(instructions))) if selected is None else set(selected)
    omitted = set(omitted)
    live = [at for at in sorted(selected) if at not in omitted]
    labels = {instructions[at].address: f'I{index}' for index, at in enumerate(live)}
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
        if insn.mnemonic in ('syscall', 'sysenter', 'int', 'int3'):
            raise IdentityError('unsupported implicit-register system instruction')
        operands, _, comment = insn.operands.partition('#')
        operands = operands.strip()
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


def project_pool(instructions, active):
    """Recognize a pool call, argument restores, test and conditional branch.

    Nondecision setup/cleanup remains in the comparison. If backend inlining
    changes the wrapper ABI or code around dispatch, that is a mismatch.
    """
    edges = assembly_edges(instructions)
    polls = [at for at, insn in enumerate(instructions) if insn.mnemonic.startswith('call') and target(insn) and target(insn)[1].removesuffix('@plt') == 'wf__par_pool_active']
    if not polls:
        return reachable(edges), set(), 0
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
    return reachable(edges), {at, test_at, branch_at}, 1


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


def check_workload(name, build):
    record = {'status': 'fail', 'covered_widths': [1], 'comparisons': [], 'failures': []}
    def failure(function, reason, difference=None):
        record['failures'].append(dict(workload=name, function=function, reason=reason, **(difference or {})))
    try:
        seq_image = ElfImage(build / 'seq' / name)
        demand_image = ElfImage(build / 'demand' / name)
        seq = seq_image.bound(parse_disassembly((build / 'seq' / f'{name}.disassembly').read_text()))
        demand = demand_image.bound(parse_disassembly((build / 'demand' / f'{name}.disassembly').read_text()))
        ir = parse_ir((build / 'demand' / f'{name}.ll').read_text())
        wrappers = [symbol for symbol in parse_ir((build / 'seq' / f'{name}.ll').read_text()) if symbol.startswith('wf_bench_') and not symbol.endswith('_release')]
        if len(wrappers) != 1:
            raise IdentityError(f'expected one timed wrapper, found {wrappers}')
        wrapper = wrappers[0]
        signature = re.search(r'define\b[^\n]*@' + re.escape(wrapper) + r'\(([^)]*)\)', (build / 'demand' / f'{name}.ll').read_text())
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
            settings = json.loads((Path(__file__).parent / 'manifest.json').read_text())[name]
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
            if 'dispatch' in record and record['dispatch']['regions']:
                if arguments is None:
                    raise IdentityError('cheap-region concrete timed scalar inputs unavailable')
                active_root, active_decisions = computation_root(demand, wrapper, True, arguments, argument_count)
                if active_root != demand_root or active_decisions.count('pool-active') != count or active_decisions.count('cheap-region') != len(record['dispatch']['regions']):
                    raise IdentityError('active timed cheap path does not select same sequential body once')
                record['actual_entry']['W2/W4/W8'] = {'computation': active_root, 'decision_trace': active_decisions, 'outside_loops': True, 'inputs': arguments}
        except IdentityError as error:
            pending = set()
            try:
                selected, omitted, count = project_pool(demand[wrapper], False)
                if 'dispatch' in record and count != record['dispatch']['pool_decisions_per_timed_call']:
                    raise IdentityError('LLVM and optimized assembly dynamic pool counts disagree')
                diff = first_difference(normalized(seq[wrapper], image=seq_image), normalized(demand[wrapper], selected, omitted, image=demand_image))
                record['comparisons'].append({'function': wrapper, 'role': 'conservative actual W1 wrapper CFG projection; all nondecision instructions retained', 'identical': diff is None})
                if diff:
                    failure(wrapper, 'normalized timed sequential wrapper differs', diff)
                else:
                    # Text equality after deleting a call cannot prove that
                    # live caller-clobbered values or flags survive that call.
                    # This projection diagnoses differences; only entry_shell
                    # supplies the state-preservation proof needed to admit.
                    failure(wrapper, f'wrapper CFG projection lacks entry-state preservation proof: {error}')
                pending.update(direct_calls(demand[wrapper], selected, demand))
            except IdentityError as fallback_error:
                failure(wrapper, str(error))
                failure(wrapper, f'wrapper CFG projection: {fallback_error}')
        # Also inspect every selected original-function clone: inlined callees
        # are covered by the wrapper; surviving calls expand this set recursively.
        # Inventory provides independent evidence even if wrapper projection fails.
        for block in ir.get(wrapper, {}).values():
            pending.update(match[1] for line in block if (match := re.search(r'\bcall\b.*?@(wf__par_seq_[\w.$]+)\(', line)))
        seen = set()
        while pending:
            function = min(pending)
            pending.remove(function)
            if function in seen:
                continue
            seen.add(function)
            original = canonical_symbol(function)
            if not function.startswith('wf_') or function == 'wf__par_pool_active':
                continue  # identical external symbols are retained in the caller
            if function not in demand or original not in seq:
                failure(function, 'missing corresponding sequential disassembly function')
                continue
            try:
                diff = first_difference(normalized(seq[original], image=seq_image), normalized(demand[function], image=demand_image))
                record['comparisons'].append({'function': function, 'seq_function': original, 'role': 'reachable sequential body/helper', 'identical': diff is None})
                if diff:
                    failure(function, 'normalized sequential body differs', diff)
                pending.update(direct_calls(demand[function], functions=demand))
            except IdentityError as error:
                failure(function, str(error))
        if 'dispatch' in record:
            regions = record['dispatch']['regions']
            inventory = {function for function, blocks in ir.items() if 'par.region.entry' in blocks}
            covered = {region['function'] for region in regions}
            record['versioned_region_inventory'] = sorted(inventory)
            if inventory != covered:
                failure(wrapper, f'versioned regions not covered: {sorted(inventory - covered)}')
            if regions:
                if 'W2/W4/W8' not in record.get('actual_entry', {}):
                    failure(wrapper, 'actual active cheap-region path remains unproved')
                else:
                    record['covered_widths'] = [1, 2, 4, 8]
                for region in regions:
                    clone = 'wf__par_seq_' + region['function'].removeprefix('wf_')
                    if clone not in seen:
                        failure(region['function'], 'cheap-region sequential body was not covered by timed-path inventory')
                # A raw-IR branch alone cannot establish that optimized dispatch
                # executes outside machine loops. Demand region asm must consist
                # of an acyclic prefix ending in a direct clone tail transfer.
                for region in regions:
                    function = region['function']
                    instructions = demand.get(function, [])
                    edges = assembly_edges(instructions)
                    clone = 'wf__par_seq_' + function.removeprefix('wf_')
                    exits = [at for at, insn in enumerate(instructions) if insn.mnemonic == 'jmp' and target(insn) and target(insn)[1] == clone]
                    if len(exits) != 1:
                        failure(function, 'unsupported optimized cheap-region shape: expected one direct clone tail transfer')
                        continue
                    exit_at = exits[0]
                    prefix = {node for node in reachable(edges) if exit_at in reachable(edges, node)}
                    if any(on_cycle(edges, node) for node in prefix):
                        failure(function, 'optimized cheap-region decision can execute inside a loop')
                    comparisons = [node for node in prefix if instructions[node].mnemonic.startswith(('cmp', 'test'))]
                    if len(comparisons) != 1:
                        failure(function, f'unsupported cheap-region predicate shape: {len(comparisons)} machine comparisons')
                    region['assembly_decisions_per_cheap_timed_call'] = len(comparisons)
                    region['assembly_prefix_instructions'] = len(prefix)
        if not record['failures']:
            record['status'] = 'pass'
    except (OSError, IdentityError, ValueError) as error:
        failure('<construction>', str(error))
    return record


def self_test(clang='clang', objdump='objdump'):
    """CI-only compiled mutation and retained private-symbol controls."""
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
    manifest = json.loads((Path(__file__).parent / 'manifest.json').read_text())
    records = {name: check_workload(name, args.build) for name in manifest}
    # small_split is a research coverage assertion, not a compiler dispatch.
    if not records.get('small_split', {}).get('dispatch', {}).get('regions'):
        records['small_split']['status'] = 'fail'
        records['small_split']['failures'].append({'workload': 'small_split', 'function': 'wf_workload', 'reason': 'required cheap-region dispatch coverage missing'})
    hashes = {}
    for name in manifest:
        for arm in ('seq', 'demand'):
            for suffix in ('', '.ll', '.o', '.disassembly'):
                relative = f'{arm}/{name}{suffix}'
                path = args.build / relative
                if path.exists():
                    hashes[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    result = {'hashes': hashes, 'schema': 1, 'status': 'pass' if all(record['status'] == 'pass' for record in records.values()) else 'fail', 'workloads': records, 'scope': 'all manifest W1 cells; every discovered cheap region at W1/W2/W4/W8; normalized actual timed wrapper and reachable sequential bodies; conservative CFG dispatch proof'}
    output = args.output or args.build / 'sequential-identity.json'
    output.write_text(json.dumps(result, indent=2) + '\n')
    for record in records.values():
        for failure in record['failures']:
            print(f"{failure['workload']} / {failure['function']}: {failure['reason']}" + (f"; instruction {failure['instruction']}: seq={failure['seq']!r}, demand={failure['demand']!r}" if 'instruction' in failure else ''))
    if result['status'] != 'pass':
        raise SystemExit(1)


if __name__ == '__main__':
    main()
