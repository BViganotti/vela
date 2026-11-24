//! Honest micro-benchmarks for the Vela core. Run with:
//!
//! ```sh
//! cargo run --release -p vela-core --bin vela-bench
//! ```
//!
//! Numbers are measured on the machine running the benchmark; nothing is
//! fabricated. The goal is regression visibility, not competition with JITs.

use std::time::Instant;

use vela_core::binary::{decode_module, encode_module};
use vela_core::instr::{Instr, MemArg, StoreKind};
use vela_core::module::{Export, ExportDesc, Func, FuncBody, Module};
use vela_core::runtime::memory::MemoryInst;
use vela_core::runtime::{instantiate, invoke, invoke_export, Limits, Machine, Registry, Store};
use vela_core::text::parse_wat;
use vela_core::types::{FuncType, Limits as WLimits, MemoryType, ValType};

fn measure<F: FnMut()>(name: &str, iters: u32, unit: &str, mut f: F) {
    f(); // warm up
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    let per = elapsed / iters;
    let rate = if per.as_nanos() > 0 {
        1_000_000_000.0 / per.as_nanos() as f64
    } else {
        0.0
    };
    println!(
        "{name:<32} {iters:>7} iters  {:>10.2} us/op  {rate:>12.0} {unit}",
        per.as_secs_f64() * 1_000_000.0
    );
}

fn sum_module() -> Module {
    let mut m = Module::default();
    m.types.push(FuncType {
        params: vec![ValType::I32],
        results: vec![ValType::I32],
    });
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![ValType::I32, ValType::I32],
            body: vec![
                Instr::Block(vela_core::types::BlockType::Empty),
                Instr::Loop(vela_core::types::BlockType::Empty),
                Instr::LocalGet(1),
                Instr::LocalGet(0),
                Instr::I32GeU,
                Instr::BrIf(1),
                Instr::LocalGet(2),
                Instr::LocalGet(1),
                Instr::I32Add,
                Instr::LocalSet(2),
                Instr::LocalGet(1),
                Instr::I32Const(1),
                Instr::I32Add,
                Instr::LocalSet(1),
                Instr::Br(0),
                Instr::End,
                Instr::End,
                Instr::LocalGet(2),
                Instr::End,
            ],
        },
    });
    m.exports.push(Export {
        name: "sum".into(),
        desc: ExportDesc::Func(0),
    });
    m
}

fn fib_wat() -> Module {
    parse_wat(
        r#"
        (module
          (func $fib (export "fib") (param $n i32) (result i32)
            (if (result i32)
              (i32.lt_s (local.get $n) (i32.const 2))
              (then (local.get $n))
              (else
                (i32.add
                  (call $fib (i32.sub (local.get $n) (i32.const 1)))
                  (call $fib (i32.sub (local.get $n) (i32.const 2))))))))
        "#,
    )
    .expect("fib parses")
}

fn memory_module() -> Module {
    let mut m = Module::default();
    m.types.push(FuncType {
        params: vec![ValType::I32],
        results: vec![],
    });
    m.memories.push(MemoryType {
        limits: WLimits::new(16, Some(64)),
    });
    // fill(addr) writes 64 KiB in 4-byte stores via a loop.
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![ValType::I32],
            body: vec![
                Instr::I32Const(0),
                Instr::LocalSet(1),
                Instr::Block(vela_core::types::BlockType::Empty),
                Instr::Loop(vela_core::types::BlockType::Empty),
                Instr::LocalGet(1),
                Instr::LocalGet(0),
                Instr::I32GeU,
                Instr::BrIf(1),
                Instr::LocalGet(1),
                Instr::I32Const(0x5a5a),
                Instr::Store {
                    kind: StoreKind::I32,
                    mem: 0,
                    arg: MemArg::new(2, 0),
                },
                Instr::LocalGet(1),
                Instr::I32Const(4),
                Instr::I32Add,
                Instr::LocalSet(1),
                Instr::Br(0),
                Instr::End,
                Instr::End,
                Instr::End,
            ],
        },
    });
    m.exports.push(Export {
        name: "fill".into(),
        desc: ExportDesc::Func(0),
    });
    m
}

fn bench_fib_stepping() {
    let module = fib_wat();
    let mut store = Store::new();
    let inst = instantiate(&mut store, module, &Registry::new()).unwrap();
    let addr = match store.instances[inst as usize].exports["fib"] {
        vela_core::runtime::ExternVal::Func(a) => a,
        _ => unreachable!(),
    };
    let machine = Machine::new(Limits {
        max_steps: 100_000_000,
        ..Limits::default()
    });
    let mut st = machine
        .start(&mut store, addr, vec![vela_core::runtime::Value::I32(20)])
        .unwrap();
    let start = Instant::now();
    let mut steps = 0u64;
    while !st.is_finished() {
        machine.step(&mut store, &mut st);
        steps += 1;
    }
    let elapsed = start.elapsed();
    let rate = steps as f64 / elapsed.as_secs_f64();
    println!(
        "{:<32} {:>7} steps  fib(20)={:?}  {:>12.0} steps/s",
        "interpreter stepping", steps, st.results, rate
    );
}

fn bench_memory_raw() {
    let mut mem = MemoryInst::new(WLimits::new(16, Some(64)));
    let buf = vec![0x5a; 64 * 1024];
    let iters = 2_000u32;
    let start = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(&mut mem);
        mem.write(0, 0, &buf).unwrap();
    }
    let elapsed = start.elapsed();
    assert_eq!(mem.data()[0], 0x5a);
    let bytes = iters as f64 * buf.len() as f64;
    println!(
        "{:<32} {:>7} iters  {:>10.2} us/op  {:>12.1} MiB/s",
        "memory write 64 KiB",
        iters,
        elapsed.as_secs_f64() * 1e6 / iters as f64,
        bytes / elapsed.as_secs_f64() / (1024.0 * 1024.0)
    );
}

fn main() {
    println!("Vela core benchmarks");
    println!(
        "target: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!();

    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/factorial.wat"),
    )
    .unwrap_or_else(|_| {
        "(module (func (export \"fact\") (param i32) (result i32) local.get 0))".to_string()
    });
    let module = parse_wat(&text).expect("benchmark module parses");
    let bytes = encode_module(&module);

    measure("decode factorial module", 20_000, "modules/s", || {
        let _ = decode_module(&bytes).unwrap();
    });
    measure("validate factorial module", 20_000, "modules/s", || {
        let m = decode_module(&bytes).unwrap();
        vela_core::validate::validate_module(&m).unwrap();
    });
    measure("instantiate + destroy", 5_000, "modules/s", || {
        let m = parse_wat(&text).unwrap();
        let mut store = Store::new();
        let _ = instantiate(&mut store, m, &Registry::new()).unwrap();
    });

    let sum = sum_module();
    let mut store = Store::new();
    let inst = instantiate(&mut store, sum, &Registry::new()).unwrap();
    let addr = match store.instances[inst as usize].exports["sum"] {
        vela_core::runtime::ExternVal::Func(a) => a,
        _ => unreachable!(),
    };
    measure("invoke sum(10_000)", 20, "calls/s", || {
        let out = invoke(
            &mut store,
            addr,
            vec![vela_core::runtime::Value::I32(10_000)],
            Limits::default(),
        )
        .unwrap();
        assert_eq!(out[0], vela_core::runtime::Value::I32(49_995_000));
    });

    let memmod = memory_module();
    let mut store2 = Store::new();
    let inst2 = instantiate(&mut store2, memmod, &Registry::new()).unwrap();
    measure("fill 64 KiB via wasm loop", 100, "fills/s", || {
        let _ = invoke_export(
            &mut store2,
            inst2,
            "fill",
            vec![vela_core::runtime::Value::I32(64 * 1024)],
            Limits::default(),
        )
        .unwrap();
    });

    bench_fib_stepping();
    bench_memory_raw();
    println!("\ndone");
}
