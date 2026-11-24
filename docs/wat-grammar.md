# WAT grammar cheat sheet

This is the subset accepted by `vela_core::text::parse_wat`. It is intentionally
smaller than the official grammar; unsupported forms fail with a positioned
`TextError` rather than being misparsed.

## Lexical

```
input      ::= token*
token      ::= '(' | ')' | atom | string
atom       ::= (any char except whitespace, parens, '"', ';')+
string     ::= '"' (char | escape)* '"'
escape     ::= '\n' | '\t' | '\r' | '\"' | '\'' | '\\' | '\' hex hex | '\u{' hex+ '}'
comment    ::= ';;' .* newline | '(;' (nested comment | any)* ';)'
```

String tokens are byte strings. `\XX` inserts one byte; `\u{...}` inserts the
UTF-8 encoding of a scalar value. Nesting of block comments and s-expressions is
capped at 1,000.

## Module

```
module     ::= '(' 'module' id? field* ')'
field      ::= type | import | func | table | memory | global
             | export | start | elem | data
type       ::= '(' 'type' id? '(' 'func' param* result* ')' ')'
import     ::= '(' 'import' string string importdesc ')'
func       ::= '(' 'func' id? inline* typeuse? local* instr* ')'
table      ::= '(' 'table' id? inline* limits? reftype ')'
memory     ::= '(' 'memory' id? inline* limits ')'
global     ::= '(' 'global' id? inline* globaltype const ')'
export     ::= '(' 'export' string '(' kind idx ')' ')'
start      ::= '(' 'start' idx ')'
elem       ::= '(' 'elem' (id? tableuse? offset | 'declare') items ')'
data       ::= '(' 'data' (id? memuse? offset)? string* ')'
```

`inline` is `(import string string)` or `(export string)` on a definition.
Imports must appear before definitions of the same kind. `limits` is
`min max?`; `globaltype` is a value type or `(mut type)`.

## Types and immediates

```
valtype    ::= 'i32' | 'i64' | 'f32' | 'f64' | 'funcref' | 'externref'
reftype    ::= 'funcref' | 'externref' | 'func' | 'extern'
idx        ::= number | '$identifier'
blocktype  ::= '' | '(' 'result' valtype ')' | '(' 'type' idx ')'
             | '(' 'param' ... ')' '(' 'result' ... ')'
memarg     ::= ('offset=' number)? ('align=' power_of_two)?
```

Numbers accept decimal, `0x` hex and unsigned values up to the full 32/64-bit
width. Floats accept decimal, hex float (`0x1.8p1`), `inf`, `-inf`, `nan`,
`nan:0xPAYLOAD`, `-nan:0xPAYLOAD`.

## Instructions

Flat and folded forms are both accepted:

```
i32.const 1
i32.const 2
i32.add
```

```
(i32.add (i32.const 1) (i32.const 2))
```

Structured control uses labels:

```
block $out (result i32)
  loop $again
    ...
    br_if $out
    br $again
  end
end
```

Folded `if` uses `(then ...)` and optional `(else ...)`; the condition operands
precede `(then ...)`.

Supported groups:

- control: `unreachable`, `nop`, `block`, `loop`, `if`, `else`, `end`, `br`,
  `br_if`, `br_table`, `return`, `call`, `call_indirect`;
- parametric: `drop`, `select`, `select (result t)`;
- variables: `local.get/set/tee`, `global.get/set`;
- memory: all core loads/stores (i32/i64/f32/f64, 8/16/32-bit variants),
  `memory.size/grow/init/copy/fill`, `data.drop`;
- tables: `table.get/set/size/grow/fill/copy/init`, `elem.drop`;
- references: `ref.null`, `ref.is_null`, `ref.func`;
- numeric: all core i32/i64/f32/f64 arithmetic, comparisons, conversions,
  reinterpretations, sign extension and saturating truncation.

Memory instructions may name a memory (`i32.load $mem`) but multi-memory modules
are rejected by validation.

## Data and element shorthand support

Supported:

```
(data (i32.const 0) "abc")
(data (memory 0) (offset i32.const 4) "\00\ff")
(data "passive")
(elem (i32.const 0) $f $g)
(elem (table 0) (offset (i32.const 0)) funcref (ref.func $f))
(elem func $f)
(elem declare func $f)
(table funcref (elem $f $g))
```

Not supported: inline `(data ...)` on memories, `(item ...)` wrappers, custom
sections and annotations.

## Wast commands

```
(module ...)              (module binary "...")      (module quote "...")
(register "name" $mod?)   (invoke $mod? "f" args...) (get $mod? "g")
(assert_return action expected...)
(assert_trap action "msg")
(assert_exhaustion action "msg")
(assert_invalid (module ...) "msg")
(assert_malformed (module ...) "msg")
(assert_unlinkable (module ...) "msg")
(assert_return_canonical_nan action)
(assert_return_arithmetic_nan action)
```

Arguments and expected values are `i32.const`, `i64.const`, `f32.const`,
`f64.const`, `ref.null`, `ref.extern`; NaN expectations may use
`nan:canonical` or `nan:arithmetic`.
