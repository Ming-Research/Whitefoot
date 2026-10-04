#!/usr/bin/env python3
"""Print poly_body.h: vm.c's k_poly body as C statements over r0..r7.

The xorshift sequence and operand selection follow vm.c's k_poly exactly;
equal checksums from vm.c and kernels.c confirm the transcription.
"""
M = (1 << 64) - 1
OPS = ["ADD", "SUB", "XOR", "AND", "SHL", "SHRU", "MUL", "MOV", "ADDI"]
seed = 0x9E3779B97F4A7C15
for _ in range(1536):
    seed ^= (seed << 13) & M
    seed ^= seed >> 7
    seed ^= (seed << 17) & M
    op = OPS[seed % 9]
    a, b, c = (seed >> 8) & 7, (seed >> 11) & 7, (seed >> 14) & 7
    imm = ((seed >> 20) & 0xFF) + 1
    rhs = {
        "ADD": f"r{a} + r{b}", "SUB": f"r{a} - r{b}", "XOR": f"r{a} ^ r{b}",
        "AND": f"r{a} & r{b}", "SHL": f"r{a} << (r{b} & 63)",
        "SHRU": f"r{a} >> (r{b} & 63)", "MUL": f"r{a} * r{b}", "MOV": f"r{a}",
        "ADDI": f"r{a} + {imm}u",
    }[op]
    print(f"r{c} = {rhs};")
