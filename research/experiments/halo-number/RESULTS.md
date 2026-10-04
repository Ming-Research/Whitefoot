# Halo number oracle results

Run UTC: 2026-10-04T12:23:10+00:00.

Host: macOS-26.6.2-arm64-arm-64bit-Mach-O; Python 3.14.7.

Base revision at run: `14726fb50afc330b057bc7ad611c2c9f9bba6415`. Number source/test SHA-256: `0b8cebfb59a3e6c19a5ef89c6ae1358b3ea2d7ba6d0b9e032e24e6113da0f251`.

Compiler SHA-256: `58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11`.

Lua SHA-256: `dd2f2bb469b8c423292e23a5ab0dea2c4f3ea23658b76d55cd5f296d5d94e91e`.

Reference liblua.a SHA-256: `40c361395e506d975b9c9b4fa17f117db801ee57723379d16605a1d706f715e0`.

Seed: `0x48414c4f0902`; random samples per primary operation: 10000.

Corpus SHA-256 (Lua input): `cb39956a6185b26c26cf26cd3304b7bdda94b8c81d8f4321c7de9a033ccc18e1`.

| Group | Cases | Bit/text mismatches | NaN bit mismatches | Other nonfinite mismatches | Maximum finite ULP |
|---|---:|---:|---:|---:|---:|
| format | 13199 | 0 | 0 | 0 | 0 |
| parse | 165 | 0 | 0 | 0 | 0 |
| parse-decimal | 2000 | 0 | 0 | 0 | 0 |
| parse-hex | 2000 | 0 | 0 | 0 | 0 |
| pow-random | 10000 | 11 | 0 | 0 | 1 |
| pow-special | 841 | 3 | 0 | 0 | 1 |
| fmod | 2255 | 0 | 0 | 0 | 0 |
| floor | 2000 | 0 | 0 | 0 | 0 |
| ceil | 2000 | 0 | 0 | 0 | 0 |

Build: 1.696 s; Lua execution: 0.308 s; Halo execution: 0.361 s. These are sizing observations, not a performance comparison.

The reference executable also ran all 34460 cases in 0.076 s and agreed with the archive host on every finite bit, parse verdict and format byte, and every NaN classification. NaN signs and payloads are extracted only by the archive host.

Every returned line and all process exit codes were checked. Comparator controls detect a changed format byte, parse verdict, power bit and missing record.

Finite mismatch ULP histograms:

```json
{
  "pow-random": {
    "1": 11
  },
  "pow-special": {
    "1": 3
  }
}
```

First mismatches per group (inputs are hexadecimal IEEE bits, except S inputs are hexadecimal bytes):

```json
[
  {
    "group": "pow-random",
    "op": "P",
    "x": "4032c537f4fadac3",
    "y": "c0498dbadc38dcde",
    "oracle": "326bca636c9cfe53",
    "halo": "326bca636c9cfe54",
    "ulp": 1
  },
  {
    "group": "pow-random",
    "op": "P",
    "x": "4011b16c4601b5b2",
    "y": "c05259e438e3968b",
    "oracle": "36173f853511f981",
    "halo": "36173f853511f982",
    "ulp": 1
  },
  {
    "group": "pow-random",
    "op": "P",
    "x": "400cea39a800c3fc",
    "y": "404c1fcc5ef782e0",
    "oracle": "46734b788d242dd3",
    "halo": "46734b788d242dd4",
    "ulp": 1
  },
  {
    "group": "pow-random",
    "op": "P",
    "x": "40168c23afa98690",
    "y": "c0521966f30b26fa",
    "oracle": "34a4c99b2570d477",
    "halo": "34a4c99b2570d478",
    "ulp": 1
  },
  {
    "group": "pow-random",
    "op": "P",
    "x": "401a67a47f1fbe10",
    "y": "4040000000000000",
    "oracle": "45617a27f929a5e9",
    "halo": "45617a27f929a5ea",
    "ulp": 1
  },
  {
    "group": "pow-random",
    "op": "P",
    "x": "403377f6b469eb89",
    "y": "c049d6701c270e81",
    "oracle": "3219781f4c28546c",
    "halo": "3219781f4c28546d",
    "ulp": 1
  },
  {
    "group": "pow-random",
    "op": "P",
    "x": "40143892763a82be",
    "y": "405142b6826b052e",
    "oracle": "4a0533453697f461",
    "halo": "4a0533453697f462",
    "ulp": 1
  },
  {
    "group": "pow-random",
    "op": "P",
    "x": "3ffd27b2983cecda",
    "y": "c048f414864bc1e7",
    "oracle": "3d3bcc1ca02704ee",
    "halo": "3d3bcc1ca02704ef",
    "ulp": 1
  },
  {
    "group": "pow-special",
    "op": "P",
    "x": "0010000000000001",
    "y": "3fe0000000000000",
    "oracle": "2000000000000000",
    "halo": "2000000000000001",
    "ulp": 1
  },
  {
    "group": "pow-special",
    "op": "P",
    "x": "7fefffffffffffff",
    "y": "3fe0000000000000",
    "oracle": "5ff0000000000000",
    "halo": "5fefffffffffffff",
    "ulp": 1
  },
  {
    "group": "pow-special",
    "op": "P",
    "x": "433fffffffffffff",
    "y": "bff0000000000000",
    "oracle": "3ca0000000000000",
    "halo": "3ca0000000000001",
    "ulp": 1
  }
]
```

Independent comparison with the original local musl C FMA path (same power inputs). NaN payload priority intentionally follows the macOS oracle; the musl C comparison additionally confirms every non-NaN result bit of the port:

```json
{
  "source_sha256": "1b7dc6685992628866c97011310cf4969a9592d431fad55cbd68613e948052f7",
  "seconds": 0.3587712086737156,
  "groups": {
    "pow-random": {
      "count": 10000,
      "mismatches": 0,
      "nan_bit_mismatches": 0,
      "nonfinite_mismatches": 0,
      "max_ulp": 0,
      "ulp_histogram": {}
    },
    "pow-special": {
      "count": 841,
      "mismatches": 22,
      "nan_bit_mismatches": 22,
      "nonfinite_mismatches": 0,
      "max_ulp": 0,
      "ulp_histogram": {}
    }
  },
  "examples": [
    {
      "group": "pow-special",
      "op": "P",
      "x": "7ff8000000000000",
      "y": "fff8000000000000",
      "oracle": "7ff8000000000000",
      "halo": "fff8000000000000",
      "ulp": null
    },
    {
      "group": "pow-special",
      "op": "P",
      "x": "7ff8000000000000",
      "y": "7ff8000000004321",
      "oracle": "7ff8000000000000",
      "halo": "7ff8000000004321",
      "ulp": null
    },
    {
      "group": "pow-special",
      "op": "P",
      "x": "fff8000000000000",
      "y": "7ff8000000000000",
      "oracle": "fff8000000000000",
      "halo": "7ff8000000000000",
      "ulp": null
    },
    {
      "group": "pow-special",
      "op": "P",
      "x": "fff8000000000000",
      "y": "7ff8000000004321",
      "oracle": "fff8000000000000",
      "halo": "7ff8000000004321",
      "ulp": null
    },
    {
      "group": "pow-special",
      "op": "P",
      "x": "fff8000000000000",
      "y": "bff0000000000000",
      "oracle": "7ff8000000000000",
      "halo": "fff8000000000000",
      "ulp": null
    },
    {
      "group": "pow-special",
      "op": "P",
      "x": "fff8000000000000",
      "y": "3ff0000000000000",
      "oracle": "7ff8000000000000",
      "halo": "fff8000000000000",
      "ulp": null
    },
    {
      "group": "pow-special",
      "op": "P",
      "x": "fff8000000000000",
      "y": "c090cc0000000000",
      "oracle": "7ff8000000000000",
      "halo": "fff8000000000000",
      "ulp": null
    },
    {
      "group": "pow-special",
      "op": "P",
      "x": "fff8000000000000",
      "y": "433fffffffffffff",
      "oracle": "7ff8000000000000",
      "halo": "fff8000000000000",
      "ulp": null
    }
  ]
}
```
