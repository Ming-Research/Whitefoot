"""Image identity regressions; synthetic ELF evidence, no compiler invocation."""
from pathlib import Path
import struct
import tempfile
import unittest

from identity import (ElfImage, FunctionTable, IdentityError, Instruction,
                      direct_calls, first_difference, normalized,
                      parse_disassembly, target, memory_access_width)


def image_fixture():
    """Two local functions of the same name and one bounded zero-fill object."""
    data = bytearray(0x400)
    ident = b'\x7fELF\x02\x01\x01' + bytes(9)
    struct.pack_into('<16sHHIQQQIHHHHHH', data, 0,
                     ident, 2, 62, 1, 0, 0, 0x200, 0, 64, 0, 0, 64, 5, 0)
    strings = b'\0wf_local\0counter\0'
    data[0xf0:0xf0 + len(strings)] = strings
    symbols = [(0, 0, 0, 0, 0, 0),
               (1, 2, 0, 1, 0x100, 5),
               (1, 2, 0, 1, 0x110, 5),
               (10, 1, 0, 4, 0x300, 16)]
    for index, symbol in enumerate(symbols):
        struct.pack_into('<IBBHQQ', data, 0x90 + index * 24, *symbol)
    sections = [(0,) * 10,
                (0, 1, 6, 0x100, 0x80, 0x20, 0, 0, 16, 0),
                (0, 2, 0, 0, 0x90, 96, 3, 3, 8, 24),
                (0, 3, 0, 0, 0xf0, len(strings), 0, 0, 1, 0),
                (0, 8, 3, 0x300, 0, 16, 0, 0, 8, 0)]
    for index, section in enumerate(sections):
        struct.pack_into('<IIQQQQIIQQ', data, 0x200 + index * 64, *section)
    return bytes(data)


def readonly_local_fixture():
    """Equal local spellings from different STT_FILE translation units."""
    data = bytearray(0x600)
    ident = b'\x7fELF\x02\x01\x01' + bytes(9)
    struct.pack_into('<16sHHIQQQIHHHHHH', data, 0,
                     ident, 2, 62, 1, 0, 0, 0x400, 0, 64, 0, 0, 64, 4, 0)
    strings, offsets = b'\0', {}
    for spelling in ('first.c', 'second.c', '.L.str'):
        offsets[spelling] = len(strings)
        strings += spelling.encode() + b'\0'
    data[0x80:0x8a] = b'same\0same\0'
    data[0x200:0x200 + len(strings)] = strings
    symbols = [(0, 0, 0, 0, 0, 0),
               (offsets['first.c'], 4, 0, 0xfff1, 0, 0),
               (offsets['.L.str'], 1, 0, 1, 0x300, 5),
               (offsets['second.c'], 4, 0, 0xfff1, 0, 0),
               (offsets['.L.str'], 1, 0, 1, 0x305, 5)]
    for index, symbol in enumerate(symbols):
        struct.pack_into('<IBBHQQ', data, 0x100 + index * 24, *symbol)
    sections = [(0,) * 10,
                (0, 1, 2, 0x300, 0x80, 10, 0, 0, 1, 0),
                (0, 2, 0, 0, 0x100, len(symbols) * 24, 3, 5, 8, 24),
                (0, 3, 0, 0, 0x200, len(strings), 0, 0, 1, 0)]
    for index, section in enumerate(sections):
        struct.pack_into('<IIQQQQIIQQ', data, 0x400 + index * 64, *section)
    return bytes(data)


class ImageIdentityTests(unittest.TestCase):
    def test_retained_cleanup_helper_bounds_and_canonical_namespace_agree(self):
        def image_at(name, address):
            image = ElfImage.__new__(ElfImage)
            image.symbols = {'wf_caller': (0x80, 6, 2), name: (address, 5, 2)}
            table = image.bound(parse_disassembly(f'''80 <wf_caller>:
 80: call {address:x} <{name}>
 85: ret
{address:x} <{name}>:
 {address:x}: mov $7,%eax
 {address + 4:x}: ret
'''))
            return image, table
        seq_image, seq = image_at('.Lwf.drop.t.cell', 0x100)
        clone_image, clone = image_at('.Lwf__par_seq.wf.drop.t.cell', 0x200)
        self.assertIn('wf.drop.t.cell', seq)
        self.assertIn('wf__par_seq.wf.drop.t.cell', clone)
        self.assertEqual(direct_calls(seq['wf_caller'], functions=seq), {'wf.drop.t.cell'})
        self.assertEqual(direct_calls(clone['wf_caller'], functions=clone), {'wf__par_seq.wf.drop.t.cell'})
        self.assertIsNone(first_difference(normalized(seq['wf_caller'], image=seq_image),
                                          normalized(clone['wf_caller'], image=clone_image)))
        self.assertIsNone(first_difference(normalized(seq['wf.drop.t.cell']),
                                          normalized(clone['wf__par_seq.wf.drop.t.cell'])))

    def test_nearest_label_call_resolves_exact_private_function_and_checks_body(self):
        def image_at(address, value):
            image = ElfImage.__new__(ElfImage)
            image.symbols = {'frame_dummy': (0x100, 4, 2),
                             '.Lwf_resource_abort': (address, 5, 2)}
            text = f'''100 <frame_dummy>:
 100: ret
{address:x} <.LBB0_0>:
 {address:x}: mov ${value},%eax
 {address + 4:x}: ret
'''
            table = image.bound(parse_disassembly(text))
            caller = [Instruction(0x80, 'call', f'{address:x} <frame_dummy+0x{address - 0x100:x}>')]
            return image, table, caller
        seq_image, seq, seq_caller = image_at(0x140, 7)
        moved_image, moved, moved_caller = image_at(0x180, 7)
        self.assertEqual(direct_calls(seq_caller, functions=seq), {'wf_resource_abort'})
        self.assertIsNone(first_difference(normalized(seq_caller, image=seq_image),
                                          normalized(moved_caller, image=moved_image)))
        self.assertIsNone(first_difference(normalized(seq['wf_resource_abort']),
                                          normalized(moved['wf_resource_abort'])))
        _, changed, _ = image_at(0x180, 9)
        self.assertIsNotNone(first_difference(normalized(seq['wf_resource_abort']),
                                             normalized(changed['wf_resource_abort'])))

    def test_retained_basic_block_headers_do_not_truncate_elf_bodies(self):
        image = ElfImage.__new__(ElfImage)
        image.symbols = {'.Lwf_resource_record_abort': (0x100, 8, 2),
                         '.Lwf_resource_abort': (0x110, 6, 2)}
        functions = image.bound(parse_disassembly('''100 <.Lwf_resource_record_abort>:
 100: mov $7,%eax
104 <.LBB0_1>:
 104: ret
 108: nop
110 <.Lwf_resource_abort>:
 110: call 100 <.Lwf_resource_record_abort>
115 <.LBB1_1>:
 115: ret
'''))
        self.assertEqual([row.address for row in functions['wf_resource_record_abort']], [0x100, 0x104])
        self.assertEqual([row.address for row in functions['wf_resource_abort']], [0x110, 0x115])
        self.assertEqual(direct_calls(functions['wf_resource_abort'], functions=functions),
                         {'wf_resource_record_abort'})

    def test_duplicate_retained_notype_fragments_reconstruct_enclosing_functions(self):
        for spelling in ('.LBB0_1', 'arbitrary_fragment'):
            with self.subTest(spelling=spelling):
                image = ElfImage.__new__(ElfImage)
                image.symbols = {'wf_first': (0x100, 8, 2), 'wf_second': (0x200, 8, 2),
                                 'fragment#0': (0x104, 0, 0), 'fragment#1': (0x204, 0, 0)}
                image.symbol_names = {'wf_first': 'wf_first', 'wf_second': 'wf_second',
                                      'fragment#0': spelling, 'fragment#1': spelling}
                disassembly = parse_disassembly(f'''100 <wf_first>:
 100: mov $7,%eax
104 <{spelling}>:
 104: ret
200 <wf_second>:
 200: mov $9,%eax
204 <{spelling}>:
 204: ret
''')
                functions = image.bound(disassembly)
                self.assertEqual(set(functions), {'wf_first', 'wf_second'})
                self.assertEqual([row.address for row in functions['wf_first']], [0x100, 0x104])
                self.assertEqual([row.address for row in functions['wf_second']], [0x200, 0x204])
                del image.symbols['fragment#1']
                with self.assertRaisesRegex(IdentityError, 'lacks exact ELF bounds'):
                    image.bound(disassembly)

    def test_duplicate_retained_end_labels_do_not_include_alignment_padding(self):
        image = ElfImage.__new__(ElfImage)
        image.symbols = {'wf_first': (0x100, 5, 2), 'wf_second': (0x200, 5, 2),
                         'end#0': (0x105, 0, 0), 'end#1': (0x205, 0, 0)}
        image.symbol_names = {'wf_first': 'wf_first', 'wf_second': 'wf_second',
                              'end#0': '.Lfunc_end0', 'end#1': '.Lfunc_end0'}
        functions = image.bound(parse_disassembly('''100 <wf_first>:
 100: mov $7,%eax
 104: ret
105 <.Lfunc_end0>:
 105: nop
200 <wf_second>:
 200: mov $9,%eax
 204: ret
205 <.Lfunc_end0>:
 205: nop
'''))
        self.assertEqual(set(functions), {'wf_first', 'wf_second'})
        self.assertEqual([row.address for row in functions['wf_first']], [0x100, 0x104])
        self.assertEqual([row.address for row in functions['wf_second']], [0x200, 0x204])

    def test_interfunction_conditional_branch_checks_relocated_private_body(self):
        def image_at(address, constant):
            image = ElfImage.__new__(ElfImage)
            image.symbols = {'wf_caller': (0x80, 8, 2),
                             '.Lwf_private_cold': (address, 5, 2)}
            functions = image.bound(parse_disassembly(f'''80 <wf_caller>:
 80: jne {address:x} <.Lwf_private_cold>
 86: ret
{address:x} <.Lwf_private_cold>:
 {address:x}: mov ${constant},%eax
 {address + 4:x}: ret
'''))
            return image, functions
        seq_image, seq = image_at(0x100, 7)
        moved_image, moved = image_at(0x200, 7)
        _, changed = image_at(0x200, 9)
        self.assertIsNone(first_difference(normalized(seq['wf_caller'], image=seq_image),
                                          normalized(moved['wf_caller'], image=moved_image)))
        targets = direct_calls(seq['wf_caller'], functions=seq)
        self.assertEqual(targets, {'wf_private_cold'})
        for helper in targets:
            self.assertIsNone(first_difference(normalized(seq[helper]), normalized(moved[helper])))
            self.assertIsNotNone(first_difference(normalized(seq[helper]), normalized(changed[helper])))

    def test_unresolved_displaced_private_target_requires_retained_symbols(self):
        functions = FunctionTable([('wf_public', 0x100, [Instruction(0x100, 'ret', '')])])
        with self.assertRaisesRegex(IdentityError, 'symbol-retaining image required'):
            target(Instruction(0x80, 'call', '140 <frame_dummy+0x40>'), functions)

    def test_named_readonly_address_identity_survives_layout_but_not_alias_changes(self):
        def image_at(address, content=b'same\0same\0'):
            image = ElfImage.__new__(ElfImage)
            image.data = content
            image.sections = [(0, 1, 2, address, 0, len(content), 0, 0, 1, 0)]
            image.symbols = {'A': (address, 5, 1), 'B': (address + 5, 5, 1)}
            return image
        instruction = Instruction(0x100, 'lea', '(%rip),%rax')
        seq, moved = image_at(0x300), image_at(0x500)
        self.assertEqual(seq.reference_identity(0x300, instruction, 'A'),
                         moved.reference_identity(0x500, instruction, 'A'))
        seq_body = [Instruction(0x100, 'lea', '0x1f9(%rip),%rax # 300 <A>')]
        moved_body = [Instruction(0x200, 'lea', '0x2f9(%rip),%rax # 500 <A>')]
        self.assertIsNone(first_difference(normalized(seq_body, image=seq),
                                          normalized(moved_body, image=moved)))
        self.assertNotEqual(seq.reference_identity(0x300, instruction, 'A'),
                            seq.reference_identity(0x305, instruction, 'B'))
        moved.symbols['B'] = (0x500, 5, 1)
        self.assertNotEqual(seq.reference_identity(0x300, instruction, 'A'),
                            moved.reference_identity(0x500, instruction, 'A'))
        changed = image_at(0x500, b'diff\0same\0')
        self.assertNotEqual(seq.reference_identity(0x300, instruction, 'A'),
                            changed.reference_identity(0x500, instruction, 'A'))

    def test_readonly_suffix_alias_offsets_and_translation_units_are_preserved(self):
        image = ElfImage.__new__(ElfImage)
        image.data = b'longsame\0same\0'
        image.sections = [(0, 1, 2, 0x300, 0, len(image.data), 0, 0, 1, 0)]
        image.symbols = {'.L.str#0': (0x300, 9, 1), '.L.str#1': (0x304, 5, 1),
                         '.L.str#2': (0x309, 5, 1)}
        image.symbol_names = {key: '.L.str' for key in image.symbols}
        image.symbol_bindings = {key: 0 for key in image.symbols}
        image.symbol_sources = {'.L.str#0': ('first.c', 0), '.L.str#1': ('second.c', 0),
                               '.L.str#2': ('third.c', 0)}
        instruction = Instruction(0x100, 'lea', '(%rip),%rax')
        suffix = image.reference_identity(0x304, instruction, 'nearest')
        separate = image.reference_identity(0x309, instruction, 'nearest')
        self.assertNotEqual(suffix, separate)
        self.assertIn('first.c#0::.L.str', suffix)
        self.assertIn('second.c#0::.L.str', suffix)
        self.assertIn(',4,9,', suffix)
        self.assertIn('third.c#0::.L.str', separate)

    def test_elf_stt_file_provenance_keeps_equal_local_readonly_names_distinct(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'fixture'
            path.write_bytes(readonly_local_fixture())
            image = ElfImage(path)
            self.assertEqual(image.symbol_sources['.L.str#0'], ('first.c', 0))
            self.assertEqual(image.symbol_sources['.L.str#1'], ('second.c', 0))
            self.assertEqual(image.local_object_key('.L.str#0'), 'first.c#0::.L.str')
            self.assertEqual(image.local_object_key('.L.str#1'), 'second.c#0::.L.str')
            self.assertNotEqual(image.readonly_address_identity(0x300),
                                image.readonly_address_identity(0x305))

    def test_duplicate_local_functions_are_retained_and_resolved_by_address(self):
        functions = parse_disassembly('''100 <wf_local>:
 100: ret
110 <wf_local>:
 110: mov $7,%eax
 114: ret
''')
        self.assertEqual(set(functions), {'wf_local#0', 'wf_local#1'})
        self.assertEqual(target(Instruction(0x80, 'call', '110 <wf_local>'), functions),
                         (0x110, 'wf_local#1'))
        with self.assertRaisesRegex(IdentityError, 'ambiguous'):
            functions['wf_local']
        with self.assertRaisesRegex(IdentityError, 'no matching'):
            target(Instruction(0x80, 'call', '120 <wf_local>'), functions)

    def test_elf_retains_and_bounds_each_duplicate_local_record(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'fixture'
            path.write_bytes(image_fixture())
            image = ElfImage(path)
            self.assertEqual(image.symbols['wf_local#0'], (0x100, 5, 2))
            self.assertEqual(image.symbols['wf_local#1'], (0x110, 5, 2))
            functions = image.bound(parse_disassembly('''100 <wf_local>:
 100: ret
 105: nop
110 <wf_local>:
 110: mov $7,%eax
 114: ret
 115: nop
'''))
            self.assertEqual([row.address for row in functions['wf_local#0']], [0x100])
            self.assertEqual([row.address for row in functions['wf_local#1']], [0x110, 0x114])
            self.assertEqual(image.code_identity(0x110, 'wf_local'), 'wf_local#1')

    def test_missing_exact_bound_for_a_duplicate_body_rejects(self):
        image = ElfImage.__new__(ElfImage)
        image.symbols = {'wf_local': (0x100, 5, 2)}
        functions = parse_disassembly('''100 <wf_local>:
 100: ret
110 <wf_local>:
 110: ret
''')
        with self.assertRaisesRegex(IdentityError, 'lacks exact ELF bounds'):
            image.bound(functions)

    def test_generated_ordinal_cannot_overwrite_an_actual_symbol_name(self):
        with self.assertRaisesRegex(IdentityError, 'key collision'):
            FunctionTable([('wf_local', 0x100, []),
                           ('wf_local', 0x110, []),
                           ('wf_local#0', 0x120, [])])

    def test_duplicate_callee_mutation_cannot_hide_behind_same_spelling(self):
        def table(first, second):
            return FunctionTable([
                ('wf_caller', 0x80, [Instruction(0x80, 'call', '100 <wf_local>')]),
                ('wf_local', 0x100, [Instruction(0x100, 'mov', f'${first},%eax')]),
                ('wf_local', 0x110, [Instruction(0x110, 'mov', f'${second},%eax')])])
        seq, changed = table(7, 9), table(9, 7)
        callees = direct_calls(seq['wf_caller'], functions=seq)
        self.assertEqual(callees, {'wf_local#0'})
        for callee in callees:
            self.assertIsNotNone(first_difference(normalized(seq[callee]), normalized(changed[callee])))
        self.assertEqual(target(Instruction(0x80, 'call', '110 <wf_local>'), seq)[1], 'wf_local#1')

    def test_bounded_zero_fill_keeps_object_and_offset_identity(self):
        image = ElfImage.__new__(ElfImage)
        image.data = b'not zero file bytes'
        image.sections = [(0, 8, 3, 0x300, 0, 16, 0, 0, 8, 0)]
        image.symbols = {'counter': (0x300, 8, 1), 'other': (0x308, 8, 1)}
        instruction = Instruction(0x100, 'movq', '(%rip),%rax')
        self.assertEqual(image.reference_bytes(0x300, instruction), bytes(8))
        a = image.reference_identity(0x300, instruction, 'wrong_nearest_name')
        b = image.reference_identity(0x308, instruction, 'wrong_nearest_name')
        offset = image.reference_identity(0x304, Instruction(0x100, 'movl', '(%rip),%eax'), 'counter+0x4')
        self.assertNotEqual(a, b)
        self.assertNotEqual(a, offset)
        self.assertTrue(a.startswith('counter+0x0/size=8:'))
        self.assertTrue(offset.startswith('counter+0x4/size=8:'))
        with self.assertRaisesRegex(IdentityError, 'crosses named object'):
            image.reference_identity(0x304, instruction, 'counter+0x4')

    def test_runtime_lock_xadd_has_four_byte_memory_access(self):
        instruction = Instruction(0x100, 'lock', 'xadd %eax,0x25588c(%rip) # 300 <counter>')
        self.assertEqual(memory_access_width(instruction), 4)
        self.assertEqual(memory_access_width(Instruction(0x100, 'sub', '%rax,(%rip)')), 8)
        self.assertEqual(memory_access_width(Instruction(0x100, 'imul', '(%rip),%rax')), 8)

    def test_conversion_uses_memory_input_width_and_rejects_overrun(self):
        image = ElfImage.__new__(ElfImage)
        image.data = b''
        image.sections = [(0, 8, 3, 0x300, 0, 8, 0, 0, 8, 0)]
        image.symbols = {'counter': (0x300, 8, 1)}
        for opcode in ('cvtsd2ss', 'vcvtsd2ss'):
            instruction = Instruction(0x100, opcode, '(%rip),%xmm0')
            self.assertEqual(memory_access_width(instruction), 8)
            with self.assertRaisesRegex(IdentityError, 'crosses named object'):
                image.reference_bytes(0x304, instruction)
        self.assertEqual(memory_access_width(Instruction(0x100, 'cvtss2sd', '(%rip),%xmm0')), 4)

    def test_cmpxchg_special_widths_and_unknown_opcode_rejection(self):
        image = ElfImage.__new__(ElfImage)
        image.data = b''
        image.sections = [(0, 8, 3, 0x300, 0, 8, 0, 0, 8, 0)]
        image.symbols = {'counter': (0x300, 8, 1)}
        instruction = Instruction(0x100, 'lock', 'cmpxchg16b (%rip)')
        self.assertEqual(memory_access_width(instruction), 16)
        with self.assertRaisesRegex(IdentityError, 'crosses named object'):
            image.reference_bytes(0x300, instruction)
        self.assertEqual(memory_access_width(Instruction(0x100, 'cmpxchg8b', '(%rip)')), 8)
        for opcode in ('unknownq', 'unknownb', 'unknownsd', 'unknownss'):
            with self.assertRaisesRegex(IdentityError, 'unsupported'):
                memory_access_width(Instruction(0x100, opcode, '(%rip),%rax'))

    def test_shift_count_register_does_not_determine_memory_width(self):
        for opcode in ('shl', 'shr', 'sal', 'sar', 'rol', 'ror'):
            with self.assertRaisesRegex(IdentityError, 'unsupported'):
                memory_access_width(Instruction(0x100, opcode, '%cl,(%rip)'))
        self.assertEqual(memory_access_width(Instruction(0x100, 'shlq', '%cl,(%rip)')), 8)

    def test_named_mutable_addresses_preserve_alias_identity(self):
        image = ElfImage.__new__(ElfImage)
        image.data = bytes(16)
        image.sections = [(0, 8, 3, 0x300, 0, 16, 0, 0, 8, 0)]
        image.symbols = {'counter': (0x300, 8, 1), 'other': (0x308, 8, 1)}
        instruction = Instruction(0x100, 'lea', '(%rip),%rax')
        self.assertNotEqual(image.reference_identity(0x300, instruction, 'counter'),
                            image.reference_identity(0x308, instruction, 'other'))
        with self.assertRaisesRegex(IdentityError, 'outside ELF'):
            image.reference_identity(0x400, instruction, 'unknown')

    def test_unbounded_zero_fill_reference_is_not_accepted(self):
        image = ElfImage.__new__(ElfImage)
        image.data = b''
        image.sections = [(0, 8, 3, 0x300, 0, 16, 0, 0, 8, 0)]
        image.symbols = {}
        with self.assertRaisesRegex(IdentityError, 'unbounded mutable'):
            image.reference_bytes(0x300, Instruction(0x100, 'movq', '(%rip),%rax'))


if __name__ == '__main__':
    unittest.main()
