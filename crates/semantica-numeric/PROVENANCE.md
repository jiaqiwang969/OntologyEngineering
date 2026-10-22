# Shared numeric primitive provenance

This private workspace crate owns integer and Decimal primitives, including
exact binary64-to-Decimal construction and bounded reverse conversion/
binary64 scalar operations. It contains no
RDF identity, SPARQL expression interpreter, Dataset, or public query profile.

The Decimal algorithms are a Rust adaptation of Python 3.13.11's retained
`_pydecimal.py`, SHA256
`17882dbd5622aac99793a261a21ff70b3bde28e274ea1c3f0372599df164f440`.
The source is `Lib/_pydecimal.py` from the retained Python 3.13.11 distribution;
the private source-location record remains outside this distributable crate.
Its copyright notice is retained in `src/decimal.rs`. `LICENSE-PYTHON.txt` is
copied verbatim from the same retained Python distribution's `LICENSE.txt`,
SHA256 `78b12c3a81360b357002334f0e70ea0e92eebf7a9b358805c03c48484945f3bb`.

Adapted algorithms: exact construction; `_check_nans` and `_fix_nan`; unary
plus/minus; add/subtract and `_normalize`; multiply; `__truediv__` guard/sticky
remainder; `_fix` with one precision/subnormal rounding; fixed formatting.
Changes: Rust BigInt/BigUint coefficients and exponents, explicit context and
resource budgets, per-operation signal sets, typed failures, and a single
HALF_EVEN quotient primitive. Python's process/context globals, arbitrary
operator dispatch, and other Decimal functions are not reproduced. This is not
a claim that the original application used an unmodified default context, or
that the C `_decimal` backend's construction limits and complete trapped flags
have been observed. In particular, trapped signal sets are this core's explicit
contract, while independent source-derived tests constrain primary/no-result.

The HALF_EVEN div/rem/tie/carry method reuses the mathematical method of the existing
`crates/semantica-runtime/src/rdflib_literal_normalizers.rs::Dec` fragment;
its pre-extraction source SHA256 is
`f14ef7bcd690e072f1828799723709e034ff9e34eaad90c7167a04df1afec15a`.
The runtime duration adapter now calls this core for addition, subtraction,
unary minus, multiplication and the rounded division used before floor-by-12.
It retains its legacy coefficient/exponent representation (which clears the
sign of zero), str-style formatter, exact year/month quotient/remainder, and
formatter failure boundaries. Its 10,000-byte input contract uses explicit
adapter budgets; it does not inherit the core's 4,096-digit defaults. This
describes implementation wiring, not completed acceptance or qualification.

Unicode decimal-digit and character-class primitives are reused from the
workspace `semantica-json`, whose pinned Unicode table remains the single owner.
Integer lexical conversion has its own int10 grammar; Decimal lexical conversion
follows the fixed reference's strip/remove-underscore grammar. These are not
substituted for one another.

Locked dependencies: num-bigint 0.4.8, num-integer 0.1.47, num-traits 0.2.19
(each MIT OR Apache-2.0). No new native arithmetic library is introduced.
num-rational is not used as the Decimal representation because scale, signed
zero and per-operation rounding are observable.

Independent expected values were frozen before this implementation in
`nullable-next-01/NUMERIC-FROZEN-CASES-01.json`, SHA256
`17537b2c280da7ba85fe2939a05081653661564e340bb65c7ce5bd73f257a21a`.
They are source/mathematical derivations, not a new execution of the old Python
library. Do not rewrite them from candidate outputs. Additional core tests
exercise direct value transport and bounded handling of extreme exponents.

## Exact binary64 construction

`src/binary64.rs::decimal_from_binary64` adapts the reduced integer-ratio
construction in the same pinned `_pydecimal.py`, lines 586–628. It extracts the
IEEE 754 bits directly, cancels common powers of two, and constructs the exact
coefficient and exponent using BigUint/BigInt. No float display, decimal lexical
round trip or arithmetic-context rounding supplies the result. Negative zero
keeps its sign with exponent zero; integral binary64 values have exponent zero.
Finite temporaries are fixed by the input format: denominator power at most
1074, coefficient at most 767 decimal digits. The resulting representation then
passes the caller's coefficient/exponent-input budgets. The power-of-ten budget
does not impose a new power-of-five restriction, and no output is formatted.

`DecimalContext::from_binary64` adapts the float constructor at lines 570–581
and `_raise_error` at lines 3964–3985. After validating the context and its
resource admission, it records FloatOperation before exact conversion. A trap
returns no value; an untrapped result carries that one-call signal without
precision or range rounding. This API does not implement persistent Context
flags, custom ignored flags, C-backend complete flags or the distinct
`Context.create_decimal_from_float` rounding step.

The special-value interpretation was checked against official tagged CPython
v3.13.11 source, read without executing it:

- [`Objects/floatobject.c`](https://raw.githubusercontent.com/python/cpython/v3.13.11/Objects/floatobject.c),
  SHA256 `a41f721536339d6df470d03b6d9c7efe8995f64348bb0a6093632d463ac55753`,
  lines 375–389 (`float_repr`) and 1572–1620 (`as_integer_ratio`).
- [`Python/pystrtod.c`](https://raw.githubusercontent.com/python/cpython/v3.13.11/Python/pystrtod.c),
  SHA256 `4f630125a735cf92b7d24a200fe038ce3957e34ce18152c8648d50f7c1126a0c`,
  lines 862–873 and 1004–1037: Infinity retains sign; NaN text omits sign/payload.

These C files are reference evidence, not copied implementation or a claim
about an installed C runtime. The selected negative quiet NaN therefore becomes
a positive quiet Decimal NaN with empty payload. Arbitrary signaling-NaN
hardware behavior and host floating-point exception state remain unqualified.
The adapted Python method uses the same retained PSF license above.

Sixteen source/mathematical expectations were frozen before the candidate in
`BINARY64-FROZEN-01.json`, SHA256
`f06930161fcbe451e0fcdfa6e118c46d44af6f5aeb059b10dcc97266efa92892`.
The public derivative `tests/fixtures/source-derived-binary64.json`, SHA256
`660068dab41ece37f53a8a26f61f624d5125273fe5abf54f8cd925708e81eeb3`, removes
private source locations while retaining cases, contexts and interface budgets
exactly. `tests/binary64_frozen.rs` checks all 16: 14 explicit conversions and
two constructor boundaries. Its separate active resource test checks a tiny
coefficient budget, trap-before-conversion, and zero pow10/output budgets;
that test is not counted among the 16 source-derived cases. The locked-host
crate smoke run passed 15 Rust tests (12 existing units, the 70-case adapter,
the 16-case adapter, and the separate active test). Final source qualification,
query integration, mixed arithmetic and formal release remain separate gates.


## Mixed binary64 scalar increment

`binary64_from_integer` and `binary64_from_decimal` share exact positive-ratio
rounding to nearest binary64 with ties to even. BigInt quotient/remainder and
bit assembly supply the result; neither Python execution, float string parsing
nor an intermediate floating-point approximation supplies the expected value.
Integer overflow is an `IntegerOverflow` fault, whereas finite Decimal overflow
returns signed Infinity. Decimal sNaN conversion is `SignalingNaNConversion`;
quiet NaN loses its decimal payload. Reverse conversions do not take a Decimal
context or add Decimal signals. `apply_binary64_binary` owns scalar four-operation
rules, including rejection of either signed zero denominator before division;
`apply_binary64_unary` owns plus/minus. Finite host arithmetic assumes ordinary
binary64 nearest rounding with gradual underflow and no reassociation or FMA
combining source steps. Hardware NaN payload/sign, signaling-NaN host behavior,
non-default fenv/FTZ/DAZ and other platforms need separate receiving evidence.

The mathematical ratio implementation follows the behavior specified by these
additional official tagged CPython v3.13.11 sources, read without executing them:

- [`Objects/longobject.c`](https://raw.githubusercontent.com/python/cpython/v3.13.11/Objects/longobject.c),
  SHA256 `d841531666d7daf12d90500b4e491f6873daaf498527a7a37f0b694fbf41d27a`,
  lines 3383–3544 (`_PyLong_Frexp` / `PyLong_AsDouble`).
- [`Modules/_decimal/_decimal.c`](https://raw.githubusercontent.com/python/cpython/v3.13.11/Modules/_decimal/_decimal.c),
  SHA256 `81293c0fcebd2820c96d85690c41460adc032cc42f771a87f5b86acadbb21dbf`,
  lines 3833–3862 (`PyDec_AsFloat`).
- The already pinned `Objects/floatobject.c`, lines 589–635 for binary
  operations and 856/1114 for unary behavior, and `Python/pystrtod.c`, lines
  274–332 for conversion overflow and the previously recorded special formatting.
- The same pinned `_pydecimal.py`, lines 1523–1533 for Decimal float conversion.

The source references constrain behavior; the new positive-ratio rounding is a
mathematical Rust implementation, not a copied C runtime. Existing source and
license records remain intact.

`binary64-conversion-budget-v1` is an explicit active resource contract rather
than an assertion about Python resource limits. Integer coefficient admission
comes first and does not impose the integer text-output limit. Decimal inputs
first pass `NumericLimits::check_value`, including coefficient, exponent and
special payload checks, before shortcuts. Admitted zero/special values and
adjusted decimal exponent at least 309 or at most -325 require no power expansion.
Inside that range, the original finite representation's absolute exponent must
fit `max_materialized_power_of_ten` before power allocation; removing decimal
zeros or renaming an expansion pow5 cannot bypass that check. Finite ratio
work is bounded by admitted inputs/power plus fixed binary64 shifts. Core output
is f64, so it does not consume formatted-output budget; query RDF output still
does. The existing default limits and the opposite-direction 16-case contract
are unchanged. A structured core resource fault stays a resource fault.

Independent `QUERY-BINARY64-FROZEN-01.json`, SHA256
`69974deeacb163228a361b5b3d03de93ee2fe437c1d5a95361b7d7de62ed34dd`, freezes
42 core cases (37 source/mathematical cases, four active-resource cases and
one source-overflow case with an active no-string-conversion control),
21 query families/54 runs and two old-capability controls. Exact rational
midpoint witnesses check supplied finite expected bits with integer arithmetic;
they are neither original Python runs nor candidate-derived results. The public
core derivative is `tests/fixtures/binary64_mixed_cases.json`, SHA256
`a2c7dc917dcdc3b883bf276f22944cc3352b0b0c4802d4fd2d151c7ed6b80915`, received by
`tests/binary64_mixed_frozen.rs`. Its publication and current execution identity
are separate from the original 70 and 16 cases, which remain active unchanged.
The query derivative and actual source-order/catch/cache consumers belong to
spareval. The core has no capability gate; query sessions explicitly select
`mixed-binary64-v1`, while default `integer-decimal-v1` retains prior arithmetic
refusals. No public `/5` or completed full Semantica qualification follows from
this source implementation record.

## Two-item source sum and integer true division

`python_sum_binary64_pair(first, second)` is a Rust adaptation of the fixed
CPython 3.13.11 ordinary `#ifndef SLOW_SUM` path for exactly two float items
with the default integer-zero start. The source is
[`Python/bltinmodule.c`](https://raw.githubusercontent.com/python/cpython/v3.13.11/Python/bltinmodule.c),
95,929 bytes, SHA256
`707c3c3e599941828b79d5cfb4d355ad6a97ea72835bdf4510d2a627913b53f5`,
lines 2569–2764. The adaptation retains `0.0 + first`, one Neumaier correction
step for the second float, and the final nonzero/finite correction guard.
It does not implement arbitrary Python objects, integer items or the SLOW_SUM
build. Query aggregates restart the two-item sum for each update; this primitive
is not a compensation state to carry across rows. Source operation order,
nearest binary64 rounding and gradual underflow remain explicit target
assumptions; hardware NaN sign/payload and non-default floating environments
remain outside qualification. No Decimal signal or allocation is introduced.
The existing verbatim `LICENSE-PYTHON.txt` has the same 13,809-byte SHA as the
official tag's LICENSE and covers this adaptation; no duplicate license is added.

`binary64_divide_integers` reuses the existing positive-ratio mathematical
rounding owner for the behavior specified by `long_true_divide` in the already
pinned `Objects/longobject.c`, lines 4493–4754. It does not convert each operand
to f64 first, so a finite quotient of individually overflowing integers remains
computable and only one ties-to-even rounding occurs. Both inputs undergo
coefficient admission, in left/right order, before denominator-zero detection.
Zero division and a final infinite result become the existing ZeroDivision and
IntegerOverflow faults; zero and underflow retain the quotient sign, including
zero divided by a negative integer. The ratio has no Decimal context, text
intermediary or power-of-ten materialization; coefficient admission may use the
existing bounded digit-count check. Text/format/pow10 limits do not become
additional scalar division limits. Input budget priority is an active resource
contract, distinct from original Python acceptance.

Independent `QUERY-SUM-AVG-FROZEN-01.json`, SHA256
`e58a335cd7dc736112164c520f11f3fbd399aeffeb9581476818636a8d204c98`,
was fixed before these two Rust entry points. It contains 16 core cases and
12 query families/57 runs. The core's first eight cases describe the pair sum;
the next eight describe integer division, with ID08 separately marked as an
active budget-before-zero-division policy. Core expected payload SHA256 is
`8e75e871b8842f560227ad6e7d715326cec0cc25946d84c64a981da49cf9b482`.
The current public derivative is `tests/fixtures/sum_avg_cases.json`, consumed by
`tests/sum_avg_frozen.rs`, SHA256
`e0549f69c1ee5f80f218cba125102488f62b8c8d6b6741f3fc51d6ebcde9d46e`.
Its provenance now binds `QUERY-SUM-AVG-FROZEN-03.json`, SHA256
`f86864839e50f1902e18ee89a9315377ead0577ab3065fdc6407d73f25246566`;
the core payload is unchanged. Freeze02, SHA256
`7d0b63709f2e0d8d8694fd550b4669277bbb9d6b5a14a52f96ffbfff115b2b47`,
remains preserved. The retained correction record SHA256
`4eb2751f9bb454fe77fb5dc412baea2252b9df8e4a6031ea8bdb9d9629aec897`
changes only SA12-06's active Dataset fault ordinal and expected call count
from 2 to 4, plus its wiring explanation: Group result roundtrip, alias read,
alias roundtrip, then final decoder. This is an implementation-informed static
wiring correction approved before candidate execution, not a blind oracle
claim. No rows, caches, numeric values, signals or failure kinds changed.
Original freeze01 and both source01 public derivatives remain preserved in the
private evidence records; no original source data becomes a public build input.
Revision03 separately corrects only SA12-03's active resource operation label
from `output_characters` to `integer coefficient or output digits`, as the
pre-existing integer formatter rejects before the later generic text check.
Correction record SHA256
`e1fe7c5b9793b62d0398974ddacf251d23e4cfbda7d19054a6736fdb19f662e0`
retains the failed run and prior02 public bytes. This exposed source-reviewed
case is development replay, not blind acceptance; all16 core cases and other56
query runs are unchanged.
The fixture is source/mathematical evidence, not an original Python execution
or an execution receipt for this Rust implementation. Query state, catches,
DISTINCT, Group scheduling and output caches belong to the existing spareval
owner and require their own actual receiving.

The separately retained libmpdec source is used for source interpretation of
specific C-backend trapped flags; neither of these two primitives copies its
implementation. Its BSD notice must not be relabelled as PSF if a later change
actually adapts that code. The present frozen finish-failure control uses the
single FloatOperation boundary, and does not revise older `_pydecimal`
trapped-flag uncertainty or claim complete C-backend parity.

## Exact numeric relations

`src/compare.rs` owns `DecimalContext::compare_numeric` for Equal, Less and
Greater. It is an independently written mathematical implementation using the
existing BigInt/BigUint representation: signs and zero, adjusted decimal
exponents and virtual coefficient padding, or binary64 significand/exponent
comparison. It neither subtracts rounded values nor casts an integer to f64.
No exponent-sized power or padded decimal string is allocated by comparison.
The existing exact `decimal_from_binary64` supplies Decimal/float conversion;
no new numeric parser or formatting engine is introduced.

The already pinned CPython 3.13.11 `Objects/floatobject.c:407–542` and
`Modules/_decimal/_decimal.c:3178–3245,4656–4713,5969–5980` are reference-only
behavior evidence for exact mixed relations, FloatOperation/NaN stages and the
FloatOperation TypeError base class. The retained libmpdec `mpdecimal.c`, SHA256
`4f89b8095e408a18deff79cfb605299e615bae747898eb105d8936064f7fb626`, is also
reference-only: its NaN status behavior was checked, but its C comparison
implementation was not copied or translated. Its separate Stefan Krah
two-clause license is not relabelled as PSF; no additional libmpdec code is
distributed by this change.

The relation result includes reached signals even if a later step fails.
Context and input admission occur first, in left/right order. For admitted
Decimal/float operands, FloatOperation is reached before exact conversion;
EQ records but does not trap it, while LT/GT may trap before any NaN check.
NaN equality/order then follows the operation-specific InvalidOperation rules.
Comparison does not apply arithmetic precision/exponent rounding. Existing
Decimal representation equality remains distinct from these numeric relations.

Coefficient/exponent admission and exact-conversion budgets are explicit
active resource contracts. Binary64/integer comparison uses at most a fixed
1024-bit temporary, independent of a materialized decimal power; numeric output
is bool and consumes no text-output budget. A session must merge result signals
before handling its success or failure. The API is stateless and does not own
RDF identities, Literal rich comparison/reflection, Group, or sorting.

Independent `SOURCE-EXTREMA-FROZEN-01.json`, SHA256
`9cb28bb8ab35653f2490037f627925fbe8c4b92b0fffa51563f8c1c9367fb497`, predates
this implementation. It separates source/mathematical relations from active
budgets and query consumers. These are source-derived expectations, not runs of
the original Python library or qualification receipts for this Rust increment.

The current public projections bind `SOURCE-EXTREMA-FROZEN-02.json`, SHA256
`6df0965cedb36486e13c20876eab75315fb77e42c5af66a0b4278e3dbe5e55fa`.
Revision02 corrects only the query active case EA01's two projected group-variable
`ill_typed` fields from null to false: the original integer token passes through
implicit SAMPLE and retains its constructor facts, unlike an extremum's new
Literal copy. The first Rust run exposed that oracle-author mistake; independent
source review then confirmed it. The correction review SHA256 is
`7380cbe6034ba9e4e7a7325d5f32f5e49b1f8703c5fc90de4a10f7dafd43ed62`.
Original freeze01, public01 bytes and the failed run remain preserved. This is
post-exposure development replay for EA01. All core inputs/expectations, all34
source/math cases, four historical MM cases and the other10 active cases are
unchanged; no production algorithm or resource boundary changed for this fix.
