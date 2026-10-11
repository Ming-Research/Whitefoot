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


class ImageIdentityTests(unittest.TestCase):
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
