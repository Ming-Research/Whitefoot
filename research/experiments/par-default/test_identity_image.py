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


def tls_fixture(memory_size=16, symbol='counter', symbol_offset=0, initial=b'abcd'):
    """An aligned PT_TLS template with initialized bytes and zero-fill tail."""
    data = bytearray(0x400)
    ident = b'\x7fELF\x02\x01\x01' + bytes(9)
    struct.pack_into('<16sHHIQQQIHHHHHH', data, 0,
                     ident, 2, 62, 1, 0, 0x40, 0x200, 0, 64, 56, 1, 64, 4, 0)
    struct.pack_into('<IIQQQQQQ', data, 0x40,
                     7, 4, 0x80, 0x300, 0x300, len(initial), memory_size, 8)
    data[0x80:0x80 + len(initial)] = initial
    strings = b'\0' + symbol.encode() + b'\0'
    data[0x180:0x180 + len(strings)] = strings
    struct.pack_into('<IBBHQQ', data, 0x100, 1, 0x16, 0, 1, symbol_offset, 8)
    sections = [(0,) * 10,
                (0, 1, 0x403, 0x300, 0x80, memory_size, 0, 0, 8, 0),
                (0, 2, 0, 0, 0x100, 24, 3, 0, 8, 24),
                (0, 3, 0, 0, 0x180, len(strings), 0, 0, 1, 0)]
    for index, section in enumerate(sections):
        struct.pack_into('<IIQQQQIIQQ', data, 0x200 + index * 64, *section)
    return bytes(data)


def external_slot_fixture(address=0x300, symbol='stderr', version='GLIBC_2.2.5'):
    """A real RELA/dynsym/versym/verneed-backed external object GOT slot."""
    data = bytearray(0x700)
    ident = b'\x7fELF\x02\x01\x01' + bytes(9)
    struct.pack_into('<16sHHIQQQIHHHHHH', data, 0,
                     ident, 3, 62, 1, 0, 0, 0x400, 0, 64, 0, 0, 64, 8, 0)
    strings, offsets = b'\0', {}
    for spelling in (symbol, 'libc.so.6', version, 'bounded'):
        offsets[spelling] = len(strings)
        strings += spelling.encode() + b'\0'
    data[0x200:0x200 + len(strings)] = strings
    struct.pack_into('<IBBHQQ', data, 0x100, offsets[symbol], 0x11, 0, 0, 0, 8)
    struct.pack_into('<QQq', data, 0x130, address, 6, 0)
    struct.pack_into('<H', data, 0x150, 2)
    struct.pack_into('<HHIII', data, 0x160, 1, 1, offsets['libc.so.6'], 16, 0)
    struct.pack_into('<IHHII', data, 0x170, 0, 0, 2, offsets[version], 0)
    struct.pack_into('<IBBHQQ', data, 0x190, offsets['bounded'], 0x11, 0, 1, address + 8, 8)
    sections = [(0,) * 10,
                (0, 1, 3, address, 0x80, 16, 0, 0, 8, 0),
                (0, 11, 0, 0, 0x100, 24, 4, 0, 8, 24),
                (0, 4, 0, 0, 0x130, 24, 2, 0, 8, 24),
                (0, 3, 0, 0, 0x200, len(strings), 0, 0, 1, 0),
                (0, 0x6fffffff, 0, 0, 0x150, 2, 2, 0, 2, 2),
                (0, 0x6ffffffe, 0, 0, 0x160, 32, 4, 1, 4, 0),
                (0, 2, 0, 0, 0x190, 24, 4, 0, 8, 24)]
    for index, section in enumerate(sections):
        struct.pack_into('<IIQQQQIIQQ', data, 0x400 + index * 64, *section)
    return bytes(data)


class ImageIdentityTests(unittest.TestCase):
    def read_image(self, data):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'image'
            path.write_bytes(data)
            return ElfImage(path)

    def test_tls_layout_movement_uses_object_identity_including_offset_zero(self):
        seq = self.read_image(tls_fixture())
        moved = self.read_image(tls_fixture(memory_size=40))
        def rows(displacement, image):
            return normalized([Instruction(0x100, 'movq', f'%fs:0x{displacement % (1 << 64):x},%rax')], image=image)
        self.assertEqual(seq.symbols['counter'], (0, 8, 6))
        self.assertEqual(rows(-16, seq), rows(-40, moved))
        self.assertNotEqual(rows(-16, seq), rows(-40, self.read_image(tls_fixture(memory_size=40, symbol='other'))))
        self.assertNotEqual(rows(-16, seq), rows(-40, self.read_image(tls_fixture(memory_size=40, initial=b'abce'))))
        with self.assertRaisesRegex(IdentityError, 'unbounded TLS'):
            rows(-16, self.read_image(tls_fixture(symbol_offset=8)))
        with self.assertRaisesRegex(IdentityError, 'ELF data evidence'):
            rows(-16, None)

    def test_tls_within_object_offsets_and_access_bounds_survive_normalization(self):
        image = self.read_image(tls_fixture())
        a = normalized([Instruction(0x100, 'movl', '%fs:-16,%eax')], image=image)
        b = normalized([Instruction(0x100, 'movl', '%fs:-12,%eax')], image=image)
        self.assertNotEqual(a, b)
        with self.assertRaisesRegex(IdentityError, 'crosses named object'):
            normalized([Instruction(0x100, 'movq', '%fs:-12,%rax')], image=image)
        with self.assertRaisesRegex(IdentityError, 'unsupported indexed'):
            normalized([Instruction(0x100, 'movq', '%fs:-16(%rax),%rax')], image=image)
        wrong_section = bytearray(tls_fixture())
        struct.pack_into('<H', wrong_section, 0x106, 3)
        with self.assertRaisesRegex(IdentityError, 'not in a TLS section'):
            self.read_image(wrong_section)

    def test_thread_pointer_lea_matches_relocated_tls_across_realistic_branches(self):
        def instructions(displacement):
            return [Instruction(0, 'mov', f'%fs:{displacement},%rax'),
                    Instruction(1, 'test', '%rax,%rax'),
                    Instruction(2, 'jne', 'c <done>'),
                    Instruction(3, 'mov', '%fs:0x0,%rcx'),
                    Instruction(4, 'mov', '$1,%eax'),
                    Instruction(5, 'lock', 'xadd %eax,(%rdi)'),
                    Instruction(6, 'cmp', '$0x81,%eax'),
                    Instruction(7, 'jae', 'd <error>'),
                    Instruction(8, 'lea', f'{displacement}(%rcx),%rcx'),
                    Instruction(9, 'mov', '%eax,%edx'),
                    Instruction(10, 'shl', '$6,%rdx'),
                    Instruction(11, 'mov', '%rax,(%rcx)'),
                    Instruction(12, 'ret', ''), Instruction(13, 'ret', '')]
        seq, moved = self.read_image(tls_fixture()), self.read_image(tls_fixture(memory_size=40))
        self.assertEqual(normalized(instructions(-16), image=seq), normalized(instructions(-40), image=moved))
        self.assertIn('tls-address:', normalized(instructions(-16), image=seq)[8])

    def test_thread_pointer_lea_preserves_object_offset_bytes_and_missing_identity(self):
        def instructions(displacement):
            return [Instruction(0, 'movq', '%fs:0,%rcx'),
                    Instruction(1, 'lea', f'{displacement}(%rcx),%rdx')]
        seq = self.read_image(tls_fixture())
        self.assertNotEqual(normalized(instructions(-16), image=seq), normalized(instructions(-12), image=seq))
        for different in (tls_fixture(symbol='other'), tls_fixture(initial=b'abce')):
            self.assertNotEqual(normalized(instructions(-16), image=seq),
                                normalized(instructions(-16), image=self.read_image(different)))
        with self.assertRaisesRegex(IdentityError, 'unbounded TLS'):
            normalized(instructions(-16), image=self.read_image(tls_fixture(symbol_offset=8)))
        with self.assertRaisesRegex(IdentityError, 'ELF data evidence'):
            normalized(instructions(-16))

    def test_thread_pointer_copies_and_derived_pointer_lea_keep_offsets(self):
        def instructions(displacement):
            return [Instruction(0, 'movq', '%fs:0,%rcx'),
                    Instruction(1, 'movq', '%rcx,%r10'),
                    Instruction(2, 'lea', f'{displacement}(%r10),%r11'),
                    Instruction(3, 'movq', '%r11,%rcx'),
                    Instruction(4, 'leaq', '4(%rcx),%rdx')]
        seq, moved = self.read_image(tls_fixture()), self.read_image(tls_fixture(memory_size=40))
        rows = normalized(instructions(-16), image=seq)
        self.assertEqual(rows, normalized(instructions(-40), image=moved))
        self.assertIn('tls-address:', rows[4])
        self.assertNotEqual(rows[2], rows[4])

    def test_thread_pointer_clobbers_leave_ordinary_lea_displacements_unchanged(self):
        clobbers = [Instruction(1, 'movl', '$0,%ecx'),
                    Instruction(1, 'movb', '$0,%cl'),
                    Instruction(1, 'pop', '%rcx'),
                    Instruction(1, 'add', '$4,%rcx'),
                    Instruction(1, 'xchg', '%rcx,%rax'),
                    Instruction(1, 'call', '20 <external>'),
                    Instruction(1, 'movsb', ''),
                    Instruction(1, 'cmpsq', ''),
                    Instruction(1, 'mul', '%rdi')]
        for clobber in clobbers:
            with self.subTest(clobber=clobber):
                instructions = [Instruction(0, 'movq', '%fs:0,%rcx'), clobber,
                                Instruction(2, 'lea', '-16(%rcx),%rdx')]
                # No image supplied: stale provenance would try ELF lookup
                # and raise instead of retaining this non-proven displacement.
                self.assertEqual(normalized(instructions)[2], 'lea -16(%rcx),%rdx')

    def test_thread_pointer_cfg_join_requires_provenance_from_every_path(self):
        image = self.read_image(tls_fixture())
        missing = [Instruction(0, 'test', '%eax,%eax'),
                   Instruction(1, 'jne', '3 <join>'),
                   Instruction(2, 'movq', '%fs:0,%rcx'),
                   Instruction(3, 'lea', '-16(%rcx),%rdx')]
        self.assertEqual(normalized(missing, image=image)[3], 'lea -16(%rcx),%rdx')
        agreed = [Instruction(0, 'movq', '%fs:0,%rcx'),
                  Instruction(1, 'test', '%eax,%eax'),
                  Instruction(2, 'jne', '4 <copy>'),
                  Instruction(3, 'jmp', '5 <join>'),
                  Instruction(4, 'movq', '%rcx,%rcx'),
                  Instruction(5, 'lea', '-16(%rcx),%rdx')]
        self.assertIn('tls-address:', normalized(agreed, image=image)[5])

    def test_thread_pointer_projection_cannot_omit_a_clobber_from_its_proof(self):
        instructions = [Instruction(0, 'movq', '%fs:0,%rcx'),
                        Instruction(1, 'movl', '$0,%ecx'),
                        Instruction(2, 'lea', '-16(%rcx),%rdx')]
        self.assertEqual(normalized(instructions, selected={0, 2})[1], 'lea -16(%rcx),%rdx')
        self.assertEqual(normalized(instructions, omitted={1})[1], 'lea -16(%rcx),%rdx')

    def test_thread_pointer_loop_backedge_clobber_invalidates_join_provenance(self):
        instructions = [Instruction(0, 'movq', '%fs:0,%rcx'),
                        Instruction(1, 'lea', '-16(%rcx),%rdx'),
                        Instruction(2, 'movl', '$0,%ecx'),
                        Instruction(3, 'jne', '1 <repeat>')]
        self.assertEqual(normalized(instructions)[1], 'lea -16(%rcx),%rdx')
        instructions[3] = Instruction(3, 'loop', '1 <repeat>')
        with self.assertRaisesRegex(IdentityError, 'unsupported loop control'):
            normalized(instructions)

    def test_ordinary_lea_does_not_gain_thread_pointer_provenance(self):
        instructions = [Instruction(0, 'mov', '%rdi,%rcx'),
                        Instruction(1, 'lea', '-16(%rcx),%rdx')]
        self.assertEqual(normalized(instructions)[1], 'lea -16(%rcx),%rdx')

    def test_truncated_tls_address_formation_keeps_the_raw_offset(self):
        instructions = [Instruction(0, 'movq', '%fs:0,%rcx'),
                        Instruction(1, 'lea', '-16(%rcx),%edx')]
        self.assertEqual(normalized(instructions)[1], 'lea -16(%rcx),%edx')

    def test_external_got_slot_relocation_preserves_symbol_version_and_offset(self):
        seq = self.read_image(external_slot_fixture())
        moved = self.read_image(external_slot_fixture(address=0x500))
        def rows(address, image, opcode='movq', register='rax'):
            return normalized([Instruction(0x100, opcode, f'0x20(%rip),%{register} # {address:x} <misleading>')], image=image)
        self.assertEqual(rows(0x300, seq), rows(0x500, moved))
        self.assertNotEqual(rows(0x300, seq), rows(0x300, self.read_image(external_slot_fixture(symbol='stdout'))))
        self.assertNotEqual(rows(0x300, seq), rows(0x300, self.read_image(external_slot_fixture(version='GLIBC_2.34'))))
        changed_bytes = bytearray(external_slot_fixture(address=0x500))
        changed_bytes[0x80] = 1
        self.assertNotEqual(rows(0x300, seq), rows(0x500, self.read_image(changed_bytes)))
        changed_addend = bytearray(external_slot_fixture())
        struct.pack_into('<q', changed_addend, 0x140, 1)
        self.assertNotEqual(rows(0x300, seq), rows(0x300, self.read_image(changed_addend)))
        self.assertNotEqual(rows(0x300, seq, 'movl', 'eax'), rows(0x304, seq, 'movl', 'eax'))
        with self.assertRaisesRegex(IdentityError, 'slot access bounds'):
            rows(0x304, seq)
        del moved.relocation_slots[0x500]
        with self.assertRaisesRegex(IdentityError, 'unbounded mutable'):
            rows(0x500, moved)

    def test_mutable_local_object_provenance_and_layout_are_both_checked(self):
        def image_at(address, source='first.c'):
            image = ElfImage.__new__(ElfImage)
            image.data = bytes(8)
            image.sections = [(0, 8, 3, address, 0, 8, 0, 0, 8, 0)]
            image.symbols = {'counter': (address, 8, 1)}
            image.symbol_names = {'counter': 'counter'}
            image.symbol_bindings = {'counter': 0}
            image.symbol_sources = {'counter': (source, 0)}
            return image
        instruction = Instruction(0x100, 'movl', '(%rip),%eax')
        seq, moved, different = image_at(0x300), image_at(0x500), image_at(0x300, 'second.c')
        self.assertEqual(seq.reference_identity(0x304, instruction, 'nearest'),
                         moved.reference_identity(0x504, instruction, 'nearest'))
        self.assertNotEqual(seq.reference_identity(0x300, instruction, 'nearest'),
                            seq.reference_identity(0x304, instruction, 'nearest'))
        self.assertNotEqual(seq.reference_identity(0x300, instruction, 'nearest'),
                            different.reference_identity(0x300, instruction, 'nearest'))
        with self.assertRaisesRegex(IdentityError, 'crosses named object'):
            moved.reference_identity(0x504, Instruction(0x100, 'movq', '(%rip),%rax'), 'nearest')

    def test_mutable_alias_names_cannot_disappear_in_content_normalization(self):
        image = ElfImage.__new__(ElfImage)
        image.data = bytes(8)
        image.sections = [(0, 8, 3, 0x300, 0, 8, 0, 0, 8, 0)]
        image.symbols = {'counter': (0x300, 8, 1), 'alias': (0x300, 8, 1)}
        instruction = Instruction(0x100, 'movq', '(%rip),%rax')
        aliased = image.reference_identity(0x300, instruction, 'nearest')
        del image.symbols['alias']
        self.assertNotEqual(aliased, image.reference_identity(0x300, instruction, 'nearest'))

    def test_rip_data_is_not_read_from_an_overlapping_tls_template_address(self):
        image = ElfImage.__new__(ElfImage)
        image.data = b'\x07\0\0\0\0\0\0\0'
        image.sections = [(0, 8, 0x403, 0x300, 0, 8, 0, 0, 8, 0),
                          (0, 1, 3, 0x300, 0, 8, 0, 0, 8, 0)]
        image.symbols = {'counter': (0x300, 8, 1), 'tls_counter': (0, 8, 6)}
        self.assertEqual(image.reference_bytes(0x300, Instruction(0x100, 'movq', '(%rip),%rax')),
                         image.data)

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
