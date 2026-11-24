# Embedding Vela

`vela-core` and `vela-replay` are ordinary Rust libraries with no dependencies.
Add them by path (the workspace is not published to crates.io):

```toml
[dependencies]
vela-core = { path = "path/to/vela/crates/vela-core" }
vela-replay = { path = "path/to/vela/crates/vela-replay" }
```

## Decode or parse

```rust
use vela_core::binary::decode_module;
use vela_core::text::parse_wat;
use vela_core::validate::validate_module;

let module = decode_module(&wasm_bytes)?;          // DecodeError
let module = parse_wat(wat_source)?;               // TextError
validate_module(&module)?;                         // ValidationError
```

`instantiate` validates again before touching the store, so passing an
unvalidated module is safe; it returns `InstantiateError::Validation`.

## Instantiate with host imports

```rust
use std::rc::Rc;
use vela_core::runtime::{instantiate, invoke, HostEnv, Registry, Store, Value, Trap};
use vela_core::types::{FuncType, ValType};

let mut store = Store::new();
let mut registry = Registry::new();

registry.define_host(
    "env",
    "log",
    FuncType { params: vec![ValType::I32], results: vec![] },
    Rc::new(|env: &mut HostEnv, args: &[Value]| {
        // Deterministic: use env.now_millis() / env.random_u64().
        println!("log({}) at {}", args[0], env.now_millis());
        Ok(vec![])
    }),
);
registry.install_hosts(&mut store);

let instance = instantiate(&mut store, module, &registry)?;
```

Host functions are `Fn(&mut HostEnv, &[Value]) -> Result<Vec<Value>, Trap>`.
Arguments and results are type-checked at the call boundary. A returned `Err`
becomes a typed trap in the module.

## Call

```rust
use vela_core::runtime::{invoke_export, Limits};

let out = invoke_export(
    &mut store,
    instance,
    "add",
    vec![Value::I32(2), Value::I32(3)],
    Limits::default(),
)?;
```

`invoke_export` resolves the export and returns `InstantiateError` for missing
exports or traps. `invoke` takes a raw function address and returns `Trap`.

## Step and replay

```rust
use vela_core::runtime::{ExecState, Machine, StepResult, Limits};
use vela_replay::{Session, Breakpoint};

let mut session = Session::new(store, func_addr, args, Limits::default(), 64)?;
session.add_breakpoint(Breakpoint::Instruction { func_addr, offset: 3 });
match session.run() {
    vela_replay::RunStop::Breakpoint(_) => { /* inspect */ }
    vela_replay::RunStop::Trapped(t) => { /* t.code() */ }
    _ => {}
}
session.resume();
let trace = session.trace_hash();
let state = session.state_hash();
session.step_back()?;         // checkpoint + deterministic replay
session.rewind_to(10)?;       // arbitrary backward target
```

For manual control without replay, use `Machine::start`, `step` and `run`. The
machine is resumable because call frames are explicit.

## Inspecting

- `Instance::exports` maps names to `ExternVal`.
- `Store::func/table/memory/global/instance` returns references.
- `Session::locals`, `current_instruction`, `func_name`, `state_hash`.
- `Trace::events` is the instruction log; `Trace::opcode_counts` summarises it.

## Limits and failures

`Limits` controls steps, call depth and value stack. `Store::max_memory_bytes`
and `Store::max_table_entries` bound allocations. `Machine` returns typed
`Trap`s; `Session` wraps construction errors in `SessionError`.

The crate forbids `unsafe` at the workspace level. Malformed modules are
rejected; they do not panic the embedder.

## Studio

The Studio is a binary and a library (`vela_studio::run_server`). Embedding the
server is optional; most users will run the library at the API level.
