# Imports and linking

Vela supports the core import/export model with shared functions, tables,
memories and globals. There is no WASI and no ambient OS access.

## Resolution model

`Store` holds addressable arrays:

```
funcs:     Vec<FuncInst>    // wasm or host
tables:    Vec<TableInst>
memories:  Vec<MemoryInst>
globals:   Vec<GlobalInst>
instances: Vec<Instance>
```

`ExternVal` is `Func(addr) | Table(addr) | Memory(addr) | Global(addr)`.
An `Instance` contains parallel address vectors for the module's index spaces
(imports first, then definitions), an export `BTreeMap` and retained passive
segment contents.

`ImportResolver` maps `(module_name, field_name)` to an `ExternVal`.
`Registry` is the simple implementation: register individual values, register
all exports of an instance under a name, and install host functions.

```rust
let mut registry = Registry::new();
registry.register_instance("lib", &store.instances[a as usize]);
registry.define_host("env", "tick", ty, Rc::new(host_fn));
registry.install_hosts(&mut store);
let b = instantiate(&mut store, module_b, &registry)?;
```

## Type matching

- Functions: exact `FuncType` equality.
- Tables: same element type and compatible limits.
- Memories: compatible limits.
- Globals: same value type and mutability.

Limit compatibility (`limits_match(expected, actual)`): the provided minimum
must be at least the expected minimum; if the expected type declares a maximum,
the provided type must declare one no larger.

Instantiating an imported memory/table/global shares the underlying object by
address. Writing through either module is visible to both, as required by the
specification.

## Instantiation order

1. Validate the module.
2. Resolve imports and check types.
3. Allocate defined functions, tables and memories (with caps).
4. Evaluate and allocate globals in order (only imported globals are readable).
5. Build the export map.
6. Apply active element segments, then active data segments.
7. Run the start function.
8. Return the instance address.

On failure, allocations made in steps 3–8 are rolled back. Mutations applied to
*imported* memories/tables by failing active segments are not undone; this is
documented in `LIMITATIONS.md`.

## Segments

- Active segments are applied at instantiation and then marked dropped.
- Passive segments remain available to `memory.init`/`table.init` until
  explicitly dropped.
- Declarative element segments are dropped immediately.
- Reusing a dropped segment traps (`DataSegmentDropped` /
  `ElemSegmentDropped`).

## Host functions

Host functions are store entries with an exact signature and a Rust closure.
They receive `&mut HostEnv` for deterministic clock/RNG access. Their arguments
and results are type-checked at the interpreter boundary; an `Err` becomes a
`Trap::Host`.

Host functions are shared through `Rc`, so checkpoints and store clones do not
duplicate them. They must be pure with respect to everything outside
`HostEnv` for replay determinism to hold.

## What is not supported

- WASI and OS imports.
- Multi-memory and shared memories.
- Cross-instance mutable globals with mismatched mutability.
- The component model.
