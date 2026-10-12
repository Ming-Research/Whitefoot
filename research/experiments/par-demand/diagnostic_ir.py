"""Construct the manual FIR diagnostic's common LLVM computation object.

The native LLVM path cannot combine two definitions with the same symbol. This
small reader handles the compiler's printed LLVM entities, imports only the
canonical root's dependency closure, and records every rename. It is not an
LLVM optimizer or a general IR parser. Unknown top-level forms, unresolved
dependencies, incompatible shared state and ambiguous named-type/SSA tokens
fail construction. Remove it with the par-demand investigation.
"""
from dataclasses import dataclass
import hashlib
import re

import identity


class DiagnosticIrError(ValueError):
    pass


@dataclass(frozen=True)
class Token:
    start: int
    end: int
    kind: str
    name: str


def tokens(text):
    """Skip string contents; recognize complete LLVM symbol/type/attr tokens."""
    result, at = [], 0
    while at < len(text):
        char = text[at]
        if char == ';':
            end = text.find('\n', at)
            at = len(text) if end < 0 else end + 1
            continue
        if char == '"':
            at += 1
            while at < len(text) and text[at] != '"':
                at += 3 if text[at] == '\\' else 1
            if at >= len(text):
                raise DiagnosticIrError('unterminated LLVM string')
            at += 1
            continue
        if char not in '@%#!':
            at += 1
            continue
        start, kind = at, char
        at += 1
        if at < len(text) and text[at] == '"':
            if kind not in '@%':
                raise DiagnosticIrError('unsupported quoted metadata token')
            at += 1
            name_start = at
            while at < len(text) and text[at] != '"':
                at += 3 if text[at] == '\\' else 1
            if at >= len(text):
                raise DiagnosticIrError('unterminated quoted LLVM symbol')
            spelling = text[name_start:at]
            name = re.sub(r'\\([0-9A-Fa-f]{2})', lambda match: chr(int(match[1], 16)), spelling)
            at += 1
        else:
            match = re.match(r'[-a-zA-Z$._0-9]+', text[at:])
            if not match:
                raise DiagnosticIrError(f'unsupported LLVM token at {text[start:start + 20]!r}')
            name = match[0]
            at += len(name)
        result.append(Token(start, at, kind, name))
    return result


def spelling(kind, name):
    if re.fullmatch(r'[-a-zA-Z$._][-a-zA-Z$._0-9]*|[0-9]+', name):
        return kind + name
    encoded = ''.join(char if 32 <= ord(char) < 127 and char not in ('"', '\\')
                      else f'\\{ord(char):02X}' for char in name)
    return kind + '"' + encoded + '"'


def rewrite(text, replacements):
    pieces, prior = [], 0
    for token in tokens(text):
        replacement = replacements.get((token.kind, token.name))
        if replacement is None:
            continue
        pieces.extend((text[prior:token.start], spelling(token.kind, replacement)))
        prior = token.end
    return ''.join(pieces) + text[prior:]


@dataclass(frozen=True)
class Entity:
    kind: str
    name: str
    text: str

    @property
    def header(self):
        return self.text.splitlines()[0]


class Module:
    def __init__(self, text):
        self.items, self.entities, self.targets = [], {}, {}
        lines, at = text.splitlines(keepends=True), 0
        while at < len(lines):
            line = lines[at]
            if line.startswith('target '):
                directive = re.fullmatch(r'target (triple|datalayout) = ("[^"\\\n]*")\s*', line)
                if directive is None or directive[1] in self.targets:
                    raise DiagnosticIrError('unsupported or duplicate LLVM target directive')
                self.targets[directive[1]] = directive[2]
                self.items.append(line)
                at += 1
                continue
            if not line.strip() or line.startswith(';') or line.startswith('source_filename '):
                self.items.append(line)
                at += 1
                continue
            if line.startswith('define '):
                end = at + 1
                while end < len(lines) and lines[end].strip() != '}':
                    end += 1
                if end == len(lines):
                    raise DiagnosticIrError('unclosed LLVM function')
                entity_text = ''.join(lines[at:end + 1])
                kind, name = 'function', next((token.name for token in tokens(line) if token.kind == '@'), None)
                at = end + 1
            else:
                entity_text = line
                first = tokens(line)
                if line.startswith('declare '):
                    kind, name = 'declaration', next((token.name for token in first if token.kind == '@'), None)
                elif first and first[0].kind == '@' and re.search(r'\s=\s', line):
                    kind, name = 'global', first[0].name
                elif first and first[0].kind == '%' and re.search(r'\s=\stype\b', line):
                    kind, name = 'type', first[0].name
                elif line.startswith('attributes ') and first and first[0].kind == '#':
                    kind, name = 'attributes', first[0].name
                else:
                    raise DiagnosticIrError(f'unsupported LLVM top-level form: {line.strip()[:100]}')
                at += 1
            if name is None:
                raise DiagnosticIrError('missing printed LLVM entity name')
            key = (self.token_kind(kind), name)
            if key in self.entities:
                raise DiagnosticIrError(f'duplicate LLVM entity: {key}')
            entity = Entity(kind, name, entity_text)
            self.entities[key] = entity
            self.items.append(entity)

    @staticmethod
    def token_kind(kind):
        return '%' if kind == 'type' else '#' if kind == 'attributes' else '@'

    def render(self):
        return ''.join(item.text if isinstance(item, Entity) else item for item in self.items)


def dependencies(entity, module):
    result = set()
    locals_ = local_names(entity)
    for token in tokens(entity.text):
        key = token.kind, token.name
        if token.kind == '!':
            raise DiagnosticIrError(f'unsupported metadata dependency: !{token.name}')
        if token.kind == '%' and token.name in locals_:
            if key in module.entities:
                raise DiagnosticIrError(f'ambiguous named type/local value: %{token.name}')
            continue  # Function-local SSA value or block.
        if key not in module.entities:
            raise DiagnosticIrError(f'missing LLVM dependency: {token.kind}{token.name}')
        result.add(key)
    return result


def argument_parts(entity):
    header = entity.header
    symbol = next((token for token in tokens(header) if token.kind == '@'), None)
    if symbol is None or header[symbol.end:symbol.end + 1] != '(':
        raise DiagnosticIrError('unsupported LLVM function header')
    start, at, depth = symbol.end + 1, symbol.end + 1, 0
    result = []
    while at < len(header):
        char = header[at]
        if char in '([{':
            depth += 1
        elif char in ')]}':
            if char == ')' and depth == 0:
                if header[start:at].strip():
                    result.append(header[start:at].strip())
                return result
            depth -= 1
        elif char == ',' and depth == 0:
            result.append(header[start:at].strip())
            start = at + 1
        at += 1
    raise DiagnosticIrError('unclosed LLVM argument list')


def local_names(entity):
    if entity.kind not in ('function', 'declaration'):
        return set()
    result = set()
    for argument in argument_parts(entity):
        refs = tokens(argument)
        if refs and refs[-1].kind == '%' and argument[:refs[-1].start].strip():
            result.add(refs[-1].name)
    for line in entity.text.splitlines()[1:]:
        stripped = line.strip()
        if stripped.endswith(':'):
            label = tokens('%' + stripped[:-1])
            if len(label) != 1:
                raise DiagnosticIrError('unsupported LLVM block label')
            result.add(label[0].name)
        elif stripped.startswith('%'):
            refs = tokens(stripped)
            if refs and stripped[refs[0].end:].lstrip().startswith('='):
                result.add(refs[0].name)
    return result


def closure(module, roots, stop=frozenset()):
    pending, reached = set(roots), set()
    while pending:
        key = pending.pop()
        if key in reached or key in stop:
            continue
        if key not in module.entities:
            raise DiagnosticIrError(f'missing LLVM root/dependency: {key}')
        reached.add(key)
        pending.update(dependencies(module.entities[key], module) - reached)
    return reached


def expanded_attributes(entity, module):
    replacements = {}
    for token in tokens(entity.text):
        if token.kind == '#':
            attribute = module.entities.get(('#', token.name))
            if attribute is None:
                raise DiagnosticIrError(f'missing attribute group: #{token.name}')
            replacements[token.start] = (token.end, attribute.text.split('=', 1)[1].strip())
    out, prior = [], 0
    for start, (end, value) in sorted(replacements.items()):
        out.extend((entity.text[prior:start], value))
        prior = end
    return (''.join(out) + entity.text[prior:]).strip()


def shared_cold_boundary(base, canonical):
    """Keep the existing common latched writer; never substitute another one."""
    roots = {('@', 'wf_resource_abort'), ('@', 'wf_resource_record_abort'), ('@', 'wf_resource_write')}
    base_closure, canonical_closure = closure(base, roots), closure(canonical, roots)
    base_names = {key for key in base_closure if key[0] != '#'}
    canonical_names = {key for key in canonical_closure if key[0] != '#'}
    if base_names != canonical_names:
        raise DiagnosticIrError('shared cold-support dependency closures differ')
    for key in base_names:
        if expanded_attributes(base.entities[key], base) != expanded_attributes(canonical.entities[key], canonical):
            raise DiagnosticIrError(f'shared cold-support entity differs: {key}')
    writer = identity.parse_ir(base.render()).get('wf_resource_record_abort', {})
    if writer.get('entry') != [
            '%latch = call ptr @wf__floor_record_latch()',
            '%acquired = cmpxchg ptr %latch, i32 0, i32 1 seq_cst seq_cst',
            '%won = extractvalue { i32, i1 } %acquired, 1',
            'br i1 %won, label %write.loop, label %park'] or writer.get('park') != [
            '%parked = load volatile i32, ptr %latch, align 4', 'br label %park']:
        raise DiagnosticIrError('common writer is not the evidenced shared latched writer')
    return canonical_names, {'status': 'pass', 'kind': 'identical existing latched IR closure',
        'symbols': sorted(name for kind, name in base_closure if kind == '@'),
        'sha256': {kind + name: hashlib.sha256(base.entities[kind, name].text.encode()).hexdigest()
                   for kind, name in sorted(base_closure)},
        'qualification': 'common IR support only; no new cold writer and no machine-equivalence waiver'}


def noinline(entity):
    if entity.kind != 'function':
        raise DiagnosticIrError(f'noinline root is not a definition: {entity.name}')
    header = entity.header
    symbol = next(token for token in tokens(header) if token.kind == '@')
    at = symbol.end
    if at >= len(header) or header[at] != '(':
        raise DiagnosticIrError('unsupported function-header spacing')
    depth = 1
    at += 1
    while at < len(header) and depth:
        depth += (header[at] == '(') - (header[at] == ')')
        at += 1
    if depth or re.search(r'\balwaysinline\b', header[at:]):
        raise DiagnosticIrError('unsupported/conflicting noinline root header')
    if re.search(r'\bnoinline\b', header[at:]):
        return entity
    changed = header[:at] + ' noinline' + header[at:]
    return Entity(entity.kind, entity.name, changed + entity.text[len(header):])


def replace_entities(module, replacements):
    """Apply a token-exact export rename across all references, not substrings."""
    items = []
    for item in module.items:
        if isinstance(item, Entity):
            key = module.token_kind(item.kind), item.name
            item = Entity(item.kind, replacements.get(key, item.name), rewrite(item.text, replacements))
        items.append(item)
    module.items = items
    module.entities = {(module.token_kind(item.kind), item.name): item for item in items if isinstance(item, Entity)}


def rename_fixture_entries(module):
    replacements = {('@', old): new for old, new in (
        ('main', 'wf_diagnostic_fixture_main'), ('wf__main_body', 'wf_diagnostic_fixture_body'))
        if ('@', old) in module.entities}
    if any(('@', new) in module.entities for new in replacements.values()):
        raise DiagnosticIrError('diagnostic fixture export collision')
    replace_entities(module, replacements)


def mark_root(module, name):
    key = '@', name
    entity = module.entities.get(key)
    if entity is None:
        raise DiagnosticIrError(f'missing computation root: {name}')
    if 'alwaysinline' in expanded_attributes(Entity('function', name, entity.header), module):
        raise DiagnosticIrError(f'conflicting alwaysinline computation root: {name}')
    changed = noinline(entity)
    module.items = [changed if item is entity else item for item in module.items]
    module.entities[key] = changed


def import_canonical(base, canonical, root, shared):
    # A weak fallback is a native link contract, not a canonical private copy.
    # Require its existing closure to agree and stop at that shared boundary.
    for key in closure(canonical, {('@', root)}, shared):
        entity = canonical.entities[key]
        if entity.kind == 'function' and re.search(r'^define\s+weak\b', entity.header):
            weak_closure = closure(canonical, {key})
            for dependency in weak_closure:
                if dependency[0] == '#':
                    continue  # Attribute indices are remapped by value below.
                existing = base.entities.get(dependency)
                if existing is None or expanded_attributes(existing, base) != expanded_attributes(canonical.entities[dependency], canonical):
                    raise DiagnosticIrError(f'incompatible shared weak ABI closure: {dependency}')
            shared = shared | {dependency for dependency in weak_closure if dependency[0] != '#'}
    reached = closure(canonical, {('@', root)}, shared)
    replacements, imported, reused = {}, [], sorted(shared)
    # Attributes are module-local indices, not SSA or globally linked names.
    next_attribute = max((int(name) for kind, name in base.entities if kind == '#'), default=-1) + 1
    for key in sorted(reached):
        entity = canonical.entities[key]
        if entity.kind == 'attributes':
            value = entity.text.split('=', 1)[1].strip()
            existing = next((name for (kind, name), candidate in base.entities.items()
                if kind == '#' and candidate.text.split('=', 1)[1].strip() == value), None)
            if existing is None:
                existing, next_attribute = str(next_attribute), next_attribute + 1
            replacements[key] = existing
        elif entity.kind == 'function':
            if re.search(r'^define\s+weak\b', entity.header):
                candidate = base.entities.get(key)
                if candidate is None or expanded_attributes(candidate, base) != expanded_attributes(entity, canonical):
                    raise DiagnosticIrError(f'incompatible shared weak ABI helper: @{entity.name}')
                reused.append(key)
            else:
                replacements[key] = 'wf_diagnostic_canonical.' + entity.name
        elif entity.kind == 'type':
            replacements[key] = 'wf.t.diagnostic_canonical.' + entity.name
        elif entity.kind == 'global':
            # Preserve a matching object identity whenever its references stay
            # shared. Readonly dependencies of imported functions can be private.
            replacements[key] = 'wf_diagnostic_canonical.' + entity.name
        elif entity.kind == 'declaration':
            replacements[key] = entity.name
    # No function-local token may alias a named type being rewritten.
    for kind, name in reached:
        if kind != '@' or canonical.entities[kind, name].kind != 'function':
            continue
        text = canonical.entities[kind, name].text
        for type_kind, type_name in replacements:
            if type_kind == '%' and (re.search(re.escape(spelling('%', type_name)) + r'\s*=', text)
                    or re.search(re.escape(spelling('%', type_name)) + r'\s*[,)]', canonical.entities[kind, name].header)):
                raise DiagnosticIrError(f'ambiguous named type/local value: %{type_name}')
    # Reuse identical types/globals only after function-reference renaming has
    # settled. Repeat so dependencies of reused types/globals can also reuse.
    changed = True
    while changed:
        changed = False
        for key in sorted(reached):
            entity, candidate = canonical.entities[key], base.entities.get(key)
            if entity.kind not in ('type', 'global') or candidate is None or replacements.get(key) == key[1]:
                continue
            trial = dict(replacements)
            trial[key] = key[1]
            if rewrite(entity.text, trial).strip() == candidate.text.strip():
                replacements[key], changed = key[1], True
    for key in sorted(reached):
        entity = canonical.entities[key]
        if key in reused:
            continue
        name = replacements.get(key, entity.name)
        new_key = key[0], name
        rewritten = rewrite(entity.text, replacements)
        candidate = base.entities.get(new_key)
        if candidate is not None:
            if rewritten.strip() != candidate.text.strip():
                raise DiagnosticIrError(f'conflicting imported LLVM entity: {new_key}')
            reused.append(key)
            continue
        if entity.kind == 'global' and ('@', entity.name) in base.entities and not re.search(r'\bconstant\b', entity.text):
            raise DiagnosticIrError(f'mutable object identity conflict: @{entity.name}')
        if entity.kind == 'declaration' and ('@', entity.name) in base.entities:
            raise DiagnosticIrError(f'external declaration conflict: @{entity.name}')
        added = Entity(entity.kind, name, rewritten)
        base.items.extend(('\n', added))
        base.entities[new_key] = added
        imported.append((entity.kind, entity.name, name))
    return replacements, imported, reused


FIR_ARGUMENTS = 'ptr %input, i64 %input_len, ptr %taps, i64 %tap_len, i64 %first, i64 %end, i64 %last_tap'


def require_target(module):
    if set(module.targets) != {'triple', 'datalayout'}:
        raise DiagnosticIrError('missing LLVM target triple or datalayout')


def require_fir_boundary(module, name):
    """The diagnostic bridges support only the compiler's default seven-arg ABI.

    Domain/alias annotations do not change argument passing. All other header
    or parameter qualifiers are unsupported here, including calling convention,
    sign/zero extension, byval, sret and address-space forms. Do not infer their
    ABI from the leading ptr/i64 type alone.
    """
    entity = module.entities.get(('@', name))
    if entity is None or entity.kind != 'function':
        raise DiagnosticIrError(f'missing FIR definition: {name}')
    symbol = next(token for token in tokens(entity.header) if token.kind == '@')
    prefix = entity.header[:symbol.start].strip()
    qualifiers = ('private', 'internal', 'external', 'weak', 'weak_odr',
                  'linkonce', 'linkonce_odr', 'available_externally',
                  'dso_local', 'dso_preemptable', 'default', 'hidden', 'protected', 'ccc')
    pattern = r'define(?: (?:' + '|'.join(qualifiers) + r'))* ptr'
    if not re.fullmatch(pattern, prefix):
        raise DiagnosticIrError(f'unsupported FIR boundary ABI: {name}')
    arguments = argument_parts(entity)
    expected = ['ptr', 'i64', 'ptr', 'i64', 'i64', 'i64', 'i64']
    if len(arguments) != len(expected):
        raise DiagnosticIrError(f'unsupported FIR boundary signature: {name}')
    for argument, type_ in zip(arguments, expected):
        formal = tokens(argument)
        if (len(formal) != 1 or formal[0].kind != '%'
                or argument[formal[0].end:].strip()):
            raise DiagnosticIrError(f'unsupported FIR boundary signature: {name}')
        words = argument[:formal[0].start].split()
        if not words or words[0] != type_:
            raise DiagnosticIrError(f'unsupported FIR boundary signature: {name}')
        allowed = {'noundef', 'noalias', 'nonnull', 'nocapture'} if type_ == 'ptr' else {'noundef'}
        if any(word not in allowed for word in words[1:]):
            raise DiagnosticIrError(f'unsupported FIR boundary ABI: {name}')


def bridge(name, root):
    return f'define ptr @{name}({FIR_ARGUMENTS}) noinline {{\nentry:\n  %r = tail call ptr @{root}({FIR_ARGUMENTS})\n  ret ptr %r\n}}\n'


def build_fir_module(seq_text, demand_text, par_text):
    seq, canonical, base = Module(seq_text), Module(demand_text), Module(par_text)
    for module in (seq, canonical, base):
        require_target(module)
    if seq.targets != canonical.targets or seq.targets != base.targets:
        raise DiagnosticIrError('LLVM target triple/datalayout differs across diagnostic modules')
    root, seq_root = 'wf__par_seq_filter', 'wf_filter'
    if ('@', root) not in canonical.entities or ('@', seq_root) not in seq.entities:
        raise DiagnosticIrError('missing canonical/source FIR computation root')
    for module, name in ((seq, seq_root), (canonical, root), (base, root)):
        require_fir_boundary(module, name)
    restore = {('@', name): identity.canonical_symbol(name) for kind, name in canonical.entities if kind == '@'}
    restored_root = Entity('function', seq_root, rewrite(canonical.entities['@', root].text, restore))
    if expanded_attributes(restored_root, canonical) != expanded_attributes(seq.entities['@', seq_root], seq):
        raise DiagnosticIrError('canonical FIR root differs from independent sequential source IR')
    shared, cold_proof = shared_cold_boundary(base, canonical)
    # The source oracle is an independent Off module, not the earlier demand
    # image. Compare every reached definition/declaration/global after the
    # compiler's exact canonical-clone name restoration, before importing it.
    independent = []
    for key in sorted(closure(canonical, {('@', root)}, shared)):
        entity = canonical.entities[key]
        if key[0] == '#':
            continue
        original_name = restore.get(key, key[1])
        candidate = seq.entities.get((key[0], original_name))
        restored = Entity(entity.kind, original_name, rewrite(entity.text, restore))
        if candidate is None or expanded_attributes(restored, canonical) != expanded_attributes(candidate, seq):
            raise DiagnosticIrError(f'canonical closure differs from independent sequential IR: {key}')
        independent.append(key[0] + original_name)
    replacements, imported, reused = import_canonical(base, canonical, root, shared)
    actual = replacements['@', root]
    mark_root(base, root)
    mark_root(base, actual)
    rename_fixture_entries(base)
    exposed = {'canonical': 'wf_diagnostic_canonical_filter', 'legacy': 'wf_diagnostic_legacy_filter'}
    for label, target in (('canonical', actual), ('legacy', root)):
        name = exposed[label]
        if ('@', name) in base.entities:
            raise DiagnosticIrError(f'diagnostic bridge collision: @{name}')
        base.items.extend(('\n', Entity('function', name, bridge(name, target))))
    metadata = {'canonical_actual_root': actual, 'legacy_actual_root': root, 'seq_actual_root': seq_root,
        'exposed_roots': exposed,
        'canonical_symbol_map': {new: identity.canonical_symbol(old) for (kind, old), new in replacements.items() if kind == '@'},
        'canonical_type_map': {new: old for (kind, old), new in replacements.items() if kind == '%'},
        'imported_closure': [{'kind': kind, 'original': old, 'imported': new} for kind, old, new in imported],
        'reused_closure': [kind + name for kind, name in reused],
        'shared_symbols': sorted({name for kind, name in shared | set(reused) if kind == '@'}),
        'shared_cold_boundary_proof': cold_proof,
        'independent_sequential_ir_closure': independent,
        'noinline_roots': [root, actual, *exposed.values()],
        'input_ir_sha256': {arm: hashlib.sha256(text.encode()).hexdigest() for arm, text in (
            ('seq', seq_text), ('demand', demand_text), ('par', par_text))}}
    return base.render(), metadata


def build_seq_noinline_module(seq_text):
    module = Module(seq_text)
    require_target(module)
    require_fir_boundary(module, 'wf_filter')
    mark_root(module, 'wf_filter')
    rename_fixture_entries(module)
    return module.render()
