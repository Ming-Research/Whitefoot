#!/usr/bin/env python3
"""Independent Python JSON oracle for the explicit JSON experiment.

Build the WF adapter directly with --build; binaries and logs stay in scratch.
The script is intentionally outside CI: it exercises the proposed package API.
"""
import argparse
import hashlib
import json
import random
import re
import struct
import subprocess
import sys
import time
from fractions import Fraction
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
NUMBER = re.compile(rb'-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?')
MESSAGES = {
    1: 'expected value', 2: 'expected object key', 3: 'expected colon',
    4: 'expected comma or end', 5: 'trailing input', 6: 'maximum depth exceeded',
    7: 'unterminated string', 8: 'unescaped control character', 9: 'invalid escape',
    10: 'invalid unicode escape', 11: 'lone surrogate', 12: 'invalid UTF-8',
    13: 'invalid number', 14: 'invalid literal', 16: 'writer expected key',
    17: 'writer expected value', 18: 'mismatched container end',
    19: 'writer incomplete', 20: 'buffer capacity exhausted',
}


def require(test, message):
    if not test:
        raise AssertionError(message)


def loads(data):
    return json.loads(data, parse_int=float, parse_float=float,
                      parse_constant=lambda x: (_ for _ in ()).throw(ValueError(x)))


def unpack(line):
    if line.startswith(b'ERR '):
        b = bytes.fromhex(line[4:].decode())
        require(len(b) >= 9, 'truncated diagnostic')
        return ('ERR', b[0], struct.unpack('<Q', b[1:9])[0], b[9:].decode())
    parts = line.split(b' ')
    require(len(parts) == 3 and parts[0] == b'OK', f'bad result {line!r}')
    return ('OK', bytes.fromhex(parts[1].decode()), bytes.fromhex(parts[2].decode()))


def number_ranges(source):
    """Locate numeric tokens independently, skipping whole quoted strings."""
    ranges = []
    i = 0
    while i < len(source):
        if source[i] == 34:
            i += 1
            while i < len(source) and source[i] != 34:
                i += 2 if source[i] == 92 else 1
            i += 1
        elif source[i] == 45 or 48 <= source[i] <= 57:
            match = NUMBER.match(source, i)
            require(match is not None, 'invalid numeric token in valid fixture')
            ranges.append(match.span())
            i = match.end()
        else:
            i += 1
    return ranges


def events_tree(source, events):
    """Reconstruct solely from events, and check every numeric range and bit field."""
    cursor = 0
    tags = []
    stack = []
    missing = object()
    root = missing
    strings = []
    numbers = []

    def word():
        nonlocal cursor
        require(cursor + 8 <= len(events), 'truncated event word')
        value = struct.unpack_from('<Q', events, cursor)[0]
        cursor += 8
        return value

    def attach(value):
        nonlocal root
        if not stack:
            require(root is missing, 'multiple event roots')
            root = value
        elif stack[-1][0] == 3:
            stack[-1][1].append(value)
        else:
            frame = stack[-1]
            require(frame[2] is not missing, 'object value without a key')
            frame[1][frame[2]] = value
            frame[2] = missing

    ended = False
    while cursor < len(events):
        tag = events[cursor]
        cursor += 1
        tags.append(tag)
        if tag in (1, 3):
            value = {} if tag == 1 else []
            attach(value)
            stack.append([tag, value, missing])
        elif tag in (2, 4):
            require(bool(stack) and stack[-1][0] == tag - 1, 'wrong event close')
            require(stack[-1][2] is missing, 'dangling event key')
            stack.pop()
        elif tag in (5, 6):
            length = word()
            require(cursor + length <= len(events), 'truncated event string')
            value = events[cursor:cursor+length].decode('utf-8')
            cursor += length
            strings.append((tag, value))
            if tag == 5:
                require(bool(stack) and stack[-1][0] == 1, 'key outside object')
                require(stack[-1][2] is missing, 'consecutive event keys')
                stack[-1][2] = value
            else:
                attach(value)
        elif tag == 7:
            start, end, bits = word(), word(), word()
            require(start < end <= len(source), 'bad number range')
            text = source[start:end]
            require(NUMBER.fullmatch(text), 'number range is not JSON number text')
            number = struct.unpack('<d', struct.pack('<Q', bits))[0]
            expected_bits = decimal_bits(text.decode())
            require(bits == expected_bits, f'number bits differ for {text!r}')
            numbers.append((start, end, bits))
            attach(number)
        elif tag in (8, 9, 10):
            attach({8: True, 9: False, 10: None}[tag])
        elif tag == 11:
            require(not stack and root is not missing, 'premature End event')
            require(cursor == len(events), 'events after End')
            ended = True
        else:
            raise AssertionError(f'unknown event tag {tag}')
    require(ended, 'missing End event')
    require([(a, b) for a, b, _ in numbers] == number_ranges(source), 'numeric token occurrences differ')
    return root, tags, strings, numbers


def equal(actual, expected):
    # Iterative comparison also supports the 4096-container fixture.
    pending = [(actual, expected)]
    while pending:
        a, e = pending.pop()
        if isinstance(e, bool) or e is None or isinstance(e, str):
            if type(a) is not type(e) or a != e: return False
        elif isinstance(e, float):
            if type(a) is not float or struct.pack('<d', a) != struct.pack('<d', e): return False
        elif isinstance(e, list):
            if not isinstance(a, list) or len(a) != len(e): return False
            pending.extend(zip(a, e))
        else:
            if not isinstance(a, dict) or a.keys() != e.keys(): return False
            pending.extend((a[k], v) for k, v in e.items())
    return True


def check_valid(source, result, slash=False):
    require(result[0] == 'OK', f'{source[:100]!r}: {result!r}')
    output, events = result[1:]
    expected = loads(source)
    tree, tags, strings, numbers = events_tree(source, events)
    require(equal(tree, expected), f'event value differs: {source[:100]!r}')
    require(equal(loads(output), expected), f're-encode differs: {source[:100]!r}')
    if slash:
        # A slash from any decoded string must appear escaped in the writer's text.
        require(b'/' not in output.replace(b'\\/', b''), 'unescaped slash')
    return tags, strings, numbers


def check_error(result, code, offset):
    require(result == ('ERR', code, offset, MESSAGES[code]),
            f'expected {code}@{offset} ({MESSAGES[code]}), got {result!r}')


def controls(valid_source, valid_result, invalid_result, repeated_result):
    """Prove independent observations detect wrong output, payloads and diagnostics."""
    bad = [
        ('OK', b'null', valid_result[2]),  # writer differs
        ('OK', valid_result[1], b''),     # no decoder events
        ('OK', valid_result[1], valid_result[2].replace(b'word', b'ward')),
        ('ERR', 1, 0, MESSAGES[1]),       # accepted input refused
    ]
    # Mutate each numeric range word and the actual conversion bits.
    index = valid_result[2].index(b'\x07')
    for delta in (1, 9, 17):
        b = bytearray(valid_result[2]); b[index + delta] ^= 1
        bad.append(('OK', valid_result[1], bytes(b)))
    repeated = bytearray(repeated_result[2])
    # Array begin, then two 25-byte Number records: point the second at the first.
    repeated[27:43] = repeated[2:18]
    try:
        check_valid(b'[1,1]', ('OK', repeated_result[1], bytes(repeated)))
    except AssertionError:
        pass
    else:
        raise AssertionError('repeated-token range mutation escaped the oracle')
    for result in bad:
        try:
            check_valid(valid_source, result)
        except (AssertionError, ValueError, UnicodeError, struct.error):
            pass
        else:
            raise AssertionError('valid-result mutation escaped the oracle')
    for result in [('OK', b'null', b'\x0a\x0b'),
                   ('ERR', invalid_result[1] + 1, invalid_result[2], invalid_result[3]),
                   ('ERR', invalid_result[1], invalid_result[2] + 1, invalid_result[3]),
                   ('ERR', invalid_result[1], invalid_result[2], 'wrong message')]:
        try:
            check_error(result, invalid_result[1], invalid_result[2])
        except AssertionError:
            pass
        else:
            raise AssertionError('error-result mutation escaped the oracle')
    return len(bad) + 5


def document(rng, depth=0):
    alphabet = ['a', 'z', ' ', '/', '"', '\\', '\0', '\n', '\b', '\f', '\r', '\t',
                '\x1f', '\x7f', '\u0080', '\u07ff', '\u0800', '\ud7ff', '\ue000',
                '\uffff', '\U00010000', '\U0010ffff', '\u4e2d\u6587', '\U0001d11e']
    def text():
        return ''.join(rng.choice(alphabet) for _ in range(rng.randrange(15)))
    kind = rng.randrange(8 if depth < 7 else 6)
    if kind == 0: return None
    if kind == 1: return bool(rng.randrange(2))
    if kind == 2: return rng.randrange(-10**17, 10**17)
    if kind == 3: return rng.uniform(-1, 1) * 10.0**rng.randrange(-300, 300)
    if kind in (4, 5): return text()
    if kind == 6: return [document(rng, depth + 1) for _ in range(rng.randrange(7))]
    return {text(): document(rng, depth + 1) for _ in range(rng.randrange(7))}


def decimal_bits(text):
    """Round an exact rational to binary64 using integer division, ties to even."""
    m = re.fullmatch(r'(-?)([0-9]+)(?:\.([0-9]+))?(?:[eE]([+-]?[0-9]+))?', text)
    require(m is not None, 'invalid rational oracle input')
    sign, whole, fraction, exponent = m.groups()
    fraction = fraction or ''
    coefficient = int(whole + fraction)
    sign_bit = (1 << 63) if sign else 0
    if coefficient == 0: return sign_bit
    power = int(exponent or '0') - len(fraction)
    adjusted = len(str(coefficient)) - 1 + power
    if adjusted > 309: return sign_bit | (0x7ff << 52)
    if adjusted < -325: return sign_bit
    numerator, denominator = coefficient, 1
    if power >= 0: numerator *= 10**power
    else: denominator = 10**(-power)
    e = numerator.bit_length() - denominator.bit_length()
    if e >= 0:
        if numerator < denominator * 2**e: e -= 1
    elif numerator * 2**(-e) < denominator: e -= 1
    shift = 1074 if e < -1022 else 52 - e
    if shift >= 0: numerator <<= shift
    else: denominator <<= -shift
    quotient, remainder = divmod(numerator, denominator)
    if remainder * 2 > denominator or (remainder * 2 == denominator and quotient & 1):
        quotient += 1
    if e < -1022: return sign_bit | quotient
    if quotient == 1 << 53:
        quotient >>= 1
        e += 1
    if e > 1023: return sign_bit | (0x7ff << 52)
    return sign_bit | ((e + 1023) << 52) | (quotient - (1 << 52))


def finite_decimal(value):
    """Exact base-10 expansion of a dyadic Fraction, with no float formatter."""
    sign = '-' if value < 0 else ''
    value = abs(value)
    power = value.denominator.bit_length() - 1
    require(value.denominator == 2**power, 'not dyadic')
    digits = str(value.numerator * 5**power).rjust(power + 1, '0')
    return sign + (digits[:-power] + '.' + digits[-power:] if power else digits)


def fixtures(samples, seed):
    cases = []
    def valid(data, depth=128, op=1):
        cases.append((op, data, depth, ('valid',)))
    def invalid(data, code, offset, depth=128, op=1):
        cases.append((op, data, depth, ('error', code, offset)))
    valid(b'{"word":[1.25,"word",true,false,null]}')  # mutation-control fixture
    invalid(b'[1,]', 1, 3)                              # error-control fixture
    valid(b'[1,1]')                                    # repeated-token range control
    for data in [b'null', b'true', b'false', b'0', b'-0', b'1.0', b'-0.0',
                 b'1e400', b'-1e400', b'1e-9999', b'-1e-9999',
                 b'[]', b'{}', b' [ ] \r\n\t', b'{"a":1,"a":2}',
                 b'"\\\"\\\\\\/\\b\\f\\n\\r\\t\\u0000\\u001f"',
                 b'"\\uD834\\uDD1E"', b'"\\udbff\\udfff"',
                 b'"\\ud800\\udc00"', b'"\\ud7ff\\ue000"',
                 b'"\\u0080\\u07ff\\u0800\\uffff"',
                 json.dumps('\U0010ffff\U00010000\u0080\ud7ff\ue000', ensure_ascii=False).encode()]:
        valid(data)
    # Every scalar boundary, byte control, slash mode, and mixed nesting.
    for i in range(32): valid(json.dumps(chr(i)).encode())
    for data in [b'"/"', b'{"/key":["a/b",{"k":"/\\/"}]}']:
        valid(data, op=1); valid(data, op=2)
    valid(b'0', depth=0)
    valid(b'['*128 + b'{\"x\":0}' + b']'*128, depth=129)
    valid(b'{\"x\":'*512 + b'0' + b'}'*512, depth=512)
    valid(json.dumps('a/'*16000).encode(), op=2)
    for n in (1, 2, 16, 256, 4096):
        valid(b'['*n + b'0' + b']'*n, depth=n)
        invalid(b'['*n + b'0' + b']'*n, 6, n-1, depth=n-1)
    valid(b'{"a":[{"b":[]}]}', depth=4)
    invalid(b'{"a":[{"b":[]}]}', 6, 11, depth=3)
    # Diagnostic expectations follow grammar, independent of adapter output.
    bad = [
        (b'',1,0), (b' ',1,1), (b'NaN',1,0), (b'Infinity',1,0),
        (b'+1',1,0), (b'//x',1,0), (b'\xef\xbb\xbfnull',1,0),
        (b'[',1,1), (b'[,]',1,1), (b'[1,,2]',1,3), (b'{"x":}',1,5),
        (b'{',2,1), (b'{1:2}',2,1), (b'{"x":1,}',2,7), (b'{,}',2,1),
        (b'{"x" 1}',3,5), (b'{"x"',3,4),
        (b'[1 2]',4,3), (b'[1',4,2), (b'[1}',4,2),
        (b'{"a":1 "b":2}',4,7), (b'{"a":1',4,6),
        (b'null false',5,5), (b'{}x',5,2), (b'null\0',14,4),
        (b'"',7,1), (b'"abc',7,4), (b'"a\0"',8,2),
        (b'"a\n"',8,2), (b'"\\x"',9,2), (b'"\\',9,2),
        (b'"\\u12x4"',10,5), (b'"\\u12',10,5),
        (b'"\\ud800"',11,3), (b'"\\udc00"',11,3),
        (b'"\\ud800x"',11,3), (b'"\\ud800\\u0041"',11,9),
        (b'"\\ud800\\ud800"',11,9), (b'"\\ud800\\uZZZZ"',10,9),
        (b'"\xff"',12,1), (b'"\xc0\x80"',12,1), (b'"\xed\xa0\x80"',12,1),
        (b'"\xf4\x90\x80\x80"',12,1), (b'"\xc2"',12,1), (b'"\xe0\x80\x80"',12,1),
        (b'"\xf0\x80\x80\x80"',12,1), (b'"\x80"',12,1), (b'"\xf5\x80\x80\x80"',12,1),
        (b'01',13,1), (b'-01',13,2), (b'-',13,1), (b'1.',13,2),
        (b'1e',13,2), (b'1e+',13,3), (b'1e-',13,3), (b'1.e2',13,2),
        (b'0x1',13,1), (b'[1x]',13,2), (b'-Infinity',13,1),
        (b'tru',14,3), (b'truex',14,4), (b'falSe',14,3), (b'nul',14,3),
    ]
    for data, code, offset in bad: invalid(data, code, offset)
    # Writer caller text is validated even though formatting is caller-owned.
    for data, offset in [(b'',0),(b'01',1),(b'+1',0),(b'1e',2),(b'NaN',0),
                         (b'Infinity',0),(b' 1',0),(b'1 ',1),(b'1,2',1)]:
        invalid(data,13,offset,op=6)
    for data in [b'"\xff"', b'"\xed\xa0\x80"', b'"\xf4\x90\x80\x80"']:
        invalid(data[1:-1],12,0,op=5)
    for data in ['hello/\0\n"\\', '\U0010ffff', ''.join(map(chr, range(32)))]:
        cases.append((5,data.encode(),0,('string',data)))
    cases.append((4,b'',0,('writer',)))
    for data, start, end, error, valid_scan in [
        (b'',0,0,0,0), (b'1',1,1,1,0), (b'1',2,1,1,0),
        (b'1',2**64-1,1,1,0), (b'12x',0,2,2,1),
        (b'x-2E+3,',1,6,6,1), (b'-',0,1,1,0), (b'00',0,1,1,0),
    ]:
        cases.append((7,data,start,('scan',struct.pack('<QQB',end,error,valid_scan))))
    # Total public quoted-string helper offsets at/past EOF.
    for start in (1, 2, 2**64-1): invalid(b'"',1,1,depth=start,op=8)
    invalid(b'',1,0,depth=2**64-1,op=8)
    cases.append((8,b'x"a"tail',1,('scan',struct.pack('<Q',4)+b'a')))
    # Exact decimal halfway values, and tiny changes on either side.
    numbers = ['0','-0','1','-1','9007199254740993','18446744073709551615',
               '2.2250738585072014e-308','2.2250738585072012e-308',
               '4.9406564584124654e-324','2.4703282292062327e-324',
               '1.7976931348623157e308','1.7976931348623159e308',
               '1e99999999999999999999999','-1e-999999999999999999999',
               '0.'+'0'*2000+'1','1.'+'0'*2000+'1']
    for bits in [0,1,2,0x000fffffffffffff,0x0010000000000000,
                 0x3fefffffffffffff,0x3ff0000000000000,0x3ff0000000000001,
                 0x4340000000000000,0x7feffffffffffffe]:
        a = Fraction.from_float(struct.unpack('<d',struct.pack('<Q',bits))[0])
        b = Fraction.from_float(struct.unpack('<d',struct.pack('<Q',bits+1))[0])
        half = (a+b)/2
        for value in (half-(b-a)/1024,half,half+(b-a)/1024):
            text=finite_decimal(value)
            numbers.extend((text,'-'+text))
    # A nonzero digit beyond the 800 retained decimal digits must break an exact tie.
    tie = finite_decimal(Fraction(1) + Fraction(1,2**53))
    numbers += [tie, tie+'0'*1000+'1', '-'+tie, '-'+tie+'0'*1000+'1']
    rng = random.Random(seed)
    for _ in range(samples):
        value=document(rng)
        text=json.dumps(value,ensure_ascii=bool(rng.randrange(2)),
                        separators=rng.choice([(',',':'),(', ',': ')]),allow_nan=False)
        valid(text.encode(),op=1+rng.randrange(2))
        digits=''.join(str(rng.randrange(10)) for _ in range(rng.randrange(1,120)))
        number=rng.choice(['','-'])+str(rng.randrange(1,10))+'.'+digits+'e'+str(rng.randrange(-400,400))
        numbers.append(number)
    for number in numbers:
        bits = decimal_bits(number)
        require(struct.pack('<Q', bits) == struct.pack('<d', float(number)), 'Python float and exact oracle differ')
        cases.append((3,number.encode(),0,('number',struct.pack('<Q',bits))))
    return cases


def main():
    sys.setrecursionlimit(10000)  # Python JSON oracle must admit the deep fixture.
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--compiler',type=Path,required=True)
    ap.add_argument('--binary',type=Path,required=True)
    ap.add_argument('--build',action='store_true')
    ap.add_argument('--samples',type=int,default=4000)
    ap.add_argument('--seed',type=int,default=8259)
    ap.add_argument('--report',type=Path)
    args=ap.parse_args()
    build_seconds=None
    if args.build:
        start=time.monotonic()
        subprocess.run([str(args.compiler),'--graph',str(ROOT/'research/experiments/json/modules.wfg'),
                        '--entry','check','-o',str(args.binary)],cwd=ROOT,check=True)
        build_seconds=time.monotonic()-start
    cases=fixtures(args.samples,args.seed)
    framed=b''.join(struct.pack('<BQQ',op,len(data),depth)+data for op,data,depth,_ in cases)+bytes(17)
    start=time.monotonic()
    p=subprocess.run([str(args.binary)],input=framed,capture_output=True,timeout=60)
    execution_seconds=time.monotonic()-start
    require(p.returncode==0,f'adapter exit {p.returncode}: {p.stderr!r}')
    lines=p.stdout.splitlines()
    require(len(lines)==len(cases),f'{len(lines)} replies for {len(cases)} records')
    results=list(map(unpack,lines))
    counts={name:0 for name in ['valid','error','string','writer','number','scan']}
    for index,((op,data,depth,expected),result) in enumerate(zip(cases,results)):
        try:
            kind=expected[0]
            if kind=='valid':
                tags,strings,numbers=check_valid(data,result,op==2)
                if data==b'{"a":1,"a":2}':
                    require(strings==[(5,'a'),(5,'a')], 'duplicate key events lost')
                if data==b'-0': require(numbers[0][2]==2**63,'negative zero lost')
            elif kind=='error': check_error(result,*expected[1:])
            elif kind=='writer': require(result==('OK',bytes(8),b''),'writer state checks failed')
            elif kind=='string':
                require(result[0]=='OK' and json.loads(result[1])==expected[1], 'raw string writer differs')
            elif kind=='scan': require(result==('OK',expected[1],b''), 'scanner boundary differs')
            elif kind=='number':
                require(result==('OK',expected[1],b''),f'number bits differ for {data[:100]!r}')
            counts[kind]+=1
        except Exception as error:
            raise AssertionError(f'case {index} op={op} depth={depth} input={data[:100]!r}: {error}') from error
    mutations=controls(cases[0][1],results[0],results[1],results[2])
    metadata={'counts':counts,'random_documents':args.samples,'random_numbers':args.samples,
              'mutation_controls':mutations,'seed':args.seed,'build_seconds':build_seconds,
              'execution_seconds':execution_seconds,
              'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
              'compiler_sha256':hashlib.sha256(args.compiler.read_bytes()).hexdigest(),
              'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              'package_sha256':hashlib.sha256(b''.join(str(p.relative_to(ROOT)).encode()+b'\0'+p.read_bytes()
                  for p in sorted((ROOT/'lib/json').rglob('*')) if p.is_file())).hexdigest()}
    print(json.dumps(metadata,indent=2))
    if args.report: args.report.write_text(json.dumps(metadata,indent=2)+'\n')


if __name__=='__main__': main()
