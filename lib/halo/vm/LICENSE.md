# Library implementation licenses

The base, string and table algorithms follow Redis 7.0.15's bundled Lua
5.1.5 (`lbaselib.c`, `lstrlib.c`, `ltablib.c`, `lmathlib.c`). In particular,
`library-sort.wf` preserves `auxsort`'s observable operation order.

Copyright (C) 1994-2012 Lua.org, PUC-Rio.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to
deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

The log/exp kernels in `library-libm.wf` use the musl Arm `pow.c` kernels
already ported in `pkg::number`; the internal helpers and their tables are
kept here because this task does not change that package's interface.
`library-decimal.wf` similarly uses the number formatter's exact decimal
operations for `string.format`'s variable precision. These copies should be
replaced by shared package exports when changes to `pkg::number` are in scope.

Copyright (c) 2018, Arm Limited. SPDX-License-Identifier: MIT.

The MIT permission and disclaimer above apply to these Arm portions also.

The sine, cosine, tangent, arctangent, arcsine, arccosine and expm1 kernels
are ports of musl's Sun/FreeBSD algorithms. `library-trig.wf` uses musl's
24-bit 2/pi digits with a full significand multiplication and two-double
Payne-Hanek remainder, rather than musl's adaptive convolution. Floating
exception flags and non-nearest rounding modes are outside Whitefoot's
floating-operation interface. These algorithms need not match a different
host libm's last bit; the experiment compares their Lua text results.
Hyperbolic functions follow musl's expm1 formulas and scale the exponential
near overflow. `log10` multiplies the compensated natural logarithm by log10(e).

Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
Copyright 2004 Sun Microsystems, Inc. All Rights Reserved.

Developed at SunPro and SunSoft, a Sun Microsystems, Inc. business.
Permission to use, copy, modify, and distribute this software is freely
granted, provided that this notice is preserved.

Redis random uses the 48-bit recurrence and initialization from `src/rand.c`
and the result mapping from `src/script_lua.c`; VM instances hold independent
states. Invocation availability is supplied by the embedding running a VM.
This file stays with the library code while any of these adaptations remain.

Redis random portions:

Copyright (c) 2010-2012, Salvatore Sanfilippo <antirez at gmail dot com>.
Copyright (c) 2009-2021, Redis Ltd.
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

- Redistributions of source code must retain the above copyright notice,
  this list of conditions and the following disclaimer.
- Redistributions in binary form must reproduce the above copyright notice,
  this list of conditions and the following disclaimer in the documentation
  and/or other materials provided with the distribution.
- Neither the name of Redis nor the names of its contributors may be used to
  endorse or promote products derived from this software without specific
  prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
