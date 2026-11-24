//! Interpreter tests: control flow, memory, tables, traps and limits.

use vela_core::instr::{BrTable, Instr, LoadKind, MemArg, StoreKind};
use vela_core::module::{
    Data, DataMode, Element, ElementMode, Export, ExportDesc, Func, FuncBody, Global, Import,
    ImportDesc, Module,
};
use vela_core::runtime::{
    instantiate, invoke, invoke_export, ExternVal, Limits, Registry, Store as RuntimeStore, Trap, Value,
};
use vela_core::types::{BlockType, FuncType, GlobalType, Limits as WLimits, MemoryType, TableType, ValType};
use Instr::*;

fn build(params: Vec<ValType>, results: Vec<ValType>, locals: Vec<ValType>, mut body: Vec<Instr>) -> Module {
    let mut m = Module::default();
    let type_idx = m.types.len() as u32;
    m.types.push(FuncType { params, results });
    body.push(End);
    m.funcs.push(Func { type_idx, body: FuncBody { locals, body } });
    m
}

fn export(m: &mut Module, name: &str, func: u32) {
    m.exports.push(Export { name: name.into(), desc: ExportDesc::Func(func) });
}

fn run(m: Module, name: &str, args: Vec<Value>) -> Result<Vec<Value>, Trap> {
    run_limited(m, name, args, Limits::default())
}

fn run_limited(m: Module, name: &str, args: Vec<Value>, limits: Limits) -> Result<Vec<Value>, Trap> {
    let mut store = RuntimeStore::new();
    let inst = instantiate(&mut store, m, &Registry::new()).expect("instantiate");
    let ext = store.export(inst, name).expect("export");
    let addr = match ext {
        ExternVal::Func(a) => a,
        _ => panic!("not a function"),
    };
    invoke(&mut store, addr, args, limits)
}

#[test]
fn add_returns_sum() {
    let mut m = build(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
        vec![],
        vec![LocalGet(0), LocalGet(1), I32Add],
    );
    export(&mut m, "add", 0);
    assert_eq!(
        run(m, "add", vec![Value::I32(2), Value::I32(40)]).unwrap(),
        vec![Value::I32(42)]
    );
}

#[test]
fn factorial_loop() {
    let body = vec![
        I32Const(1),
        LocalSet(1),
        Block(BlockType::Value(ValType::I32)),
        Loop(BlockType::Empty),
        LocalGet(1),
        LocalGet(0),
        I32Mul,
        LocalSet(1),
        LocalGet(0),
        I32Const(1),
        I32Sub,
        LocalTee(0),
        BrIf(0),
        End,
        LocalGet(1),
        End,
    ];
    let mut m = build(
        vec![ValType::I32],
        vec![ValType::I32],
        vec![ValType::I32],
        body,
    );
    export(&mut m, "fact", 0);
    assert_eq!(run(m, "fact", vec![Value::I32(5)]).unwrap(), vec![Value::I32(120)]);
}

#[test]
fn recursive_fib() {
    let body = vec![
        LocalGet(0),
        I32Const(2),
        I32LtS,
        If(BlockType::Value(ValType::I32)),
        LocalGet(0),
        Else,
        LocalGet(0),
        I32Const(1),
        I32Sub,
        Call(0),
        LocalGet(0),
        I32Const(2),
        I32Sub,
        Call(0),
        I32Add,
        End,
    ];
    let mut m = build(vec![ValType::I32], vec![ValType::I32], vec![], body);
    export(&mut m, "fib", 0);
    assert_eq!(run(m, "fib", vec![Value::I32(10)]).unwrap(), vec![Value::I32(55)]);
}

#[test]
fn memory_load_store() {
    let mut m = build(
        vec![],
        vec![ValType::I32],
        vec![],
        vec![
            I32Const(16),
            I32Const(0x1234),
            Store { kind: StoreKind::I32, mem: 0, arg: MemArg::new(2, 0) },
            I32Const(16),
            Load { kind: LoadKind::I32, mem: 0, arg: MemArg::new(2, 0) },
        ],
    );
    m.memories.push(MemoryType { limits: WLimits::new(1, None) });
    export(&mut m, "read", 0);
    assert_eq!(run(m, "read", vec![]).unwrap(), vec![Value::I32(0x1234)]);
}

#[test]
fn memory_grow_and_size() {
    let mut m = build(
        vec![],
        vec![ValType::I32, ValType::I32],
        vec![],
        vec![I32Const(2), MemoryGrow(0), MemorySize(0)],
    );
    m.memories.push(MemoryType { limits: WLimits::new(1, Some(4)) });
    export(&mut m, "grow", 0);
    assert_eq!(
        run(m, "grow", vec![]).unwrap(),
        vec![Value::I32(1), Value::I32(3)]
    );
}

#[test]
fn out_of_bounds_load_traps() {
    let mut m = build(
        vec![],
        vec![ValType::I32],
        vec![],
        vec![
            I32Const(-4),
            Load { kind: LoadKind::I32, mem: 0, arg: MemArg::new(2, 0) },
        ],
    );
    m.memories.push(MemoryType { limits: WLimits::new(1, None) });
    export(&mut m, "bad", 0);
    assert_eq!(run(m, "bad", vec![]), Err(Trap::MemoryOutOfBounds));
}

#[test]
fn integer_traps() {
    fn div(a: i32, b: i32) -> Result<Vec<Value>, Trap> {
        let mut m = build(
            vec![ValType::I32, ValType::I32],
            vec![ValType::I32],
            vec![],
            vec![LocalGet(0), LocalGet(1), I32DivS],
        );
        export(&mut m, "div", 0);
        run(m, "div", vec![Value::I32(a), Value::I32(b)])
    }
    assert_eq!(div(7, 0), Err(Trap::IntegerDivideByZero));
    assert_eq!(div(i32::MIN, -1), Err(Trap::IntegerOverflow));
    assert_eq!(div(7, 2).unwrap(), vec![Value::I32(3)]);
    assert_eq!(div(-7, 2).unwrap(), vec![Value::I32(-3)]);
}

#[test]
fn call_indirect_and_tables() {
    let mut m = Module::default();
    m.types.push(FuncType { params: vec![ValType::I32], results: vec![ValType::I32] });
    // func 0: identity
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody { locals: vec![], body: vec![LocalGet(0), End] },
    });
    // func 1: double
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![],
            body: vec![LocalGet(0), I32Const(2), I32Mul, End],
        },
    });
    // func 2: indirect call
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![],
            body: vec![
                LocalGet(0),
                I32Const(1),
                CallIndirect { type_idx: 0, table_idx: 0 },
                End,
            ],
        },
    });
    m.tables.push(TableType { elem: ValType::FuncRef, limits: WLimits::new(2, None) });
    m.elems.push(Element {
        mode: ElementMode::Active { table: 0, offset: vec![I32Const(0), End] },
        elem_type: ValType::FuncRef,
        init: vec![
            vec![RefFunc(0), End],
            vec![RefFunc(1), End],
        ],
    });
    m.exports.push(Export { name: "id".into(), desc: ExportDesc::Func(0) });
    m.exports.push(Export { name: "double".into(), desc: ExportDesc::Func(1) });
    m.exports.push(Export { name: "call".into(), desc: ExportDesc::Func(2) });
    export(&mut m, "indirect", 2);
    assert_eq!(run(m, "indirect", vec![Value::I32(21)]).unwrap(), vec![Value::I32(42)]);

    // Calling a null table slot traps.
    let mut m2 = Module::default();
    m2.types.push(FuncType { params: vec![], results: vec![] });
    m2.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![],
            body: vec![I32Const(0), CallIndirect { type_idx: 0, table_idx: 0 }, End],
        },
    });
    m2.tables.push(TableType { elem: ValType::FuncRef, limits: WLimits::new(1, None) });
    export(&mut m2, "null", 0);
    assert_eq!(run(m2, "null", vec![]), Err(Trap::UninitializedElement));
}

#[test]
fn bulk_memory_init_copy_fill() {
    let mut m = Module::default();
    m.types.push(FuncType { params: vec![], results: vec![ValType::I32] });
    m.memories.push(MemoryType { limits: WLimits::new(1, None) });
    m.datas.push(Data { mode: DataMode::Passive, bytes: vec![1, 2, 3, 4] });
    m.data_count = Some(1);
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![],
            body: vec![
                // memory.init 0 at address 100, then copy 100 -> 200, fill 300 with 9
                I32Const(100),
                I32Const(0),
                I32Const(4),
                MemoryInit { data: 0, mem: 0 },
                I32Const(200),
                I32Const(100),
                I32Const(4),
                MemoryCopy { dst: 0, src: 0 },
                I32Const(300),
                I32Const(9),
                I32Const(3),
                MemoryFill(0),
                DataDrop(0),
                // load the copied word
                I32Const(200),
                Load { kind: LoadKind::I32, mem: 0, arg: MemArg::new(2, 0) },
                End,
            ],
        },
    });
    export(&mut m, "bulk", 0);
    assert_eq!(run(m, "bulk", vec![]).unwrap(), vec![Value::I32(0x04030201)]);
}

#[test]
fn dropped_data_segment_traps() {
    let mut m = Module::default();
    m.types.push(FuncType::default());
    m.memories.push(MemoryType { limits: WLimits::new(1, None) });
    m.datas.push(Data { mode: DataMode::Passive, bytes: vec![1] });
    m.data_count = Some(1);
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![],
            body: vec![
                DataDrop(0),
                I32Const(0),
                I32Const(0),
                I32Const(1),
                MemoryInit { data: 0, mem: 0 },
                End,
            ],
        },
    });
    export(&mut m, "f", 0);
    assert_eq!(run(m, "f", vec![]), Err(Trap::DataSegmentDropped));
}

#[test]
fn globals_read_write() {
    let mut m = build(
        vec![],
        vec![ValType::I32],
        vec![],
        vec![
            GlobalGet(0),
            GlobalGet(1),
            I32Add,
            GlobalSet(0),
            GlobalGet(0),
        ],
    );
    m.globals.push(Global {
        ty: GlobalType { val: ValType::I32, mutable: true },
        init: vec![I32Const(40), End],
    });
    m.globals.push(Global {
        ty: GlobalType { val: ValType::I32, mutable: false },
        init: vec![I32Const(2), End],
    });
    export(&mut m, "f", 0);
    assert_eq!(run(m, "f", vec![]).unwrap(), vec![Value::I32(42)]);
}

#[test]
fn stack_overflow_is_a_trap() {
    let mut m = build(vec![], vec![], vec![], vec![Call(0)]);
    export(&mut m, "recurse", 0);
    let limits = Limits { max_call_depth: 32, ..Limits::default() };
    assert_eq!(run_limited(m, "recurse", vec![], limits), Err(Trap::StackOverflow));
}

#[test]
fn fuel_limit_stops_infinite_loop() {
    let mut m = build(vec![], vec![], vec![], vec![Loop(BlockType::Empty), Br(0), End]);
    export(&mut m, "spin", 0);
    let limits = Limits { max_steps: 500, ..Limits::default() };
    assert_eq!(run_limited(m, "spin", vec![], limits), Err(Trap::OutOfFuel));
}

#[test]
fn start_function_runs_at_instantiation() {
    let mut m = Module::default();
    m.types.push(FuncType::default());
    m.memories.push(MemoryType { limits: WLimits::new(1, None) });
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![],
            body: vec![I32Const(8), I32Const(77), Store { kind: StoreKind::I32, mem: 0, arg: MemArg::new(2, 0) }, End],
        },
    });
    m.start = Some(0);
    m.exports.push(Export { name: "mem".into(), desc: ExportDesc::Memory(0) });
    let mut store = RuntimeStore::new();
    let inst = instantiate(&mut store, m, &Registry::new()).unwrap();
    let ext = store.export(inst, "mem").unwrap();
    let addr = match ext {
        ExternVal::Memory(a) => a,
        _ => panic!(),
    };
    assert_eq!(&store.memory(addr).unwrap().data()[8..12], &77i32.to_le_bytes());
}

#[test]
fn host_function_is_deterministic() {
    let mut m = Module::default();
    m.types.push(FuncType { params: vec![], results: vec![ValType::I64] });
    m.imports.push(Import {
        module: "env".into(),
        name: "rand".into(),
        desc: ImportDesc::Func(0),
    });
    m.types.push(FuncType { params: vec![], results: vec![ValType::I64] });
    m.funcs.push(Func {
        type_idx: 1,
        body: FuncBody { locals: vec![], body: vec![Call(0), End] },
    });
    export(&mut m, "r", 1);

    let make = || {
        let mut store = RuntimeStore::new();
        let mut reg = Registry::new();
        reg.define_host(
            "env",
            "rand",
            FuncType { params: vec![], results: vec![ValType::I64] },
            std::rc::Rc::new(|env: &mut vela_core::runtime::HostEnv, _: &[Value]| {
                Ok(vec![Value::I64(env.random_u64() as i64)])
            }),
        );
        reg.install_hosts(&mut store);
        let inst = instantiate(&mut store, m.clone(), &reg).unwrap();
        let out = invoke_export(&mut store, inst, "r", vec![], Limits::default()).unwrap();
        let mut h = vela_core::runtime::Hasher::new();
        for v in &out {
            h.write_value(v);
        }
        h.finish()
    };
    assert_eq!(make(), make());
}

#[test]
fn br_table_selects_targets() {
    let mut m = build(
        vec![ValType::I32],
        vec![ValType::I32],
        vec![],
        vec![
            Block(BlockType::Value(ValType::I32)),
            I32Const(10),
            LocalGet(0),
            BrTable(Box::new(BrTable { targets: vec![0, 0], default: 0 })),
            End,
        ],
    );
    export(&mut m, "f", 0);
    assert_eq!(run(m, "f", vec![Value::I32(1)]).unwrap(), vec![Value::I32(10)]);
}

#[test]
fn three_arg_call_with_locals() {
    let mut m = build(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
        vec![ValType::I64],
        vec![
            LocalGet(0),
            LocalGet(1),
            I32Mul,
            LocalGet(2),
            I64Eqz,
            I32Add,
        ],
    );
    export(&mut m, "f", 0);
    assert_eq!(
        run(m, "f", vec![Value::I32(6), Value::I32(7)]).unwrap(),
        vec![Value::I32(43)]
    );
}
