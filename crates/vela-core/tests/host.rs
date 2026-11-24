//! Host functions: results, errors and deterministic environment access.

use std::rc::Rc;

use vela_core::runtime::{
    instantiate, invoke_export, HostEnv, Limits, Registry, Store, Trap, Value,
};
use vela_core::text::parse_wat;
use vela_core::types::{FuncType, ValType};

const WAT: &str = r#"
(module
  (import "env" "f" (func $f (param i32) (result i32)))
  (func (export "call") (param i32) (result i32)
    local.get 0
    call $f))
"#;

fn session(
    result: impl Fn(&mut HostEnv, &[Value]) -> Result<Vec<Value>, Trap> + 'static,
) -> (Store, u32) {
    let module = parse_wat(WAT).unwrap();
    let mut store = Store::new();
    let mut registry = Registry::new();
    let ty = FuncType {
        params: vec![ValType::I32],
        results: vec![ValType::I32],
    };
    registry.define_host("env", "f", ty, Rc::new(result));
    registry.install_hosts(&mut store);
    let inst = instantiate(&mut store, module, &registry).unwrap();
    (store, inst)
}

fn call(store: &mut Store, inst: u32) -> Result<Vec<Value>, Trap> {
    invoke_export(store, inst, "call", vec![Value::I32(5)], Limits::default()).map_err(
        |e| match e {
            vela_core::runtime::InstantiateError::Trap(t) => t,
            other => Trap::Host(other.to_string()),
        },
    )
}

#[test]
fn host_results_flow_back() {
    let (mut store, inst) = session(|_, args| Ok(vec![Value::I32(args[0].as_i32().unwrap() * 3)]));
    assert_eq!(call(&mut store, inst).unwrap(), vec![Value::I32(15)]);
}

#[test]
fn host_errors_become_typed_traps() {
    let (mut store, inst) = session(|_, _| Err(Trap::Host("no luck".into())));
    assert_eq!(call(&mut store, inst), Err(Trap::Host("no luck".into())));
}

#[test]
fn host_result_count_is_checked() {
    let (mut store, inst) = session(|_, _| Ok(vec![]));
    assert_eq!(
        call(&mut store, inst),
        Err(Trap::TypeMismatch("host result count"))
    );
}

#[test]
fn host_result_type_is_checked() {
    let (mut store, inst) = session(|_, _| Ok(vec![Value::I64(1)]));
    assert_eq!(
        call(&mut store, inst),
        Err(Trap::TypeMismatch("host result type"))
    );
}

#[test]
fn host_argument_types_are_checked() {
    let (mut store, inst) = session(|_, _| Ok(vec![Value::I32(0)]));
    let err = invoke_export(
        &mut store,
        inst,
        "call",
        vec![Value::I64(5)],
        Limits::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("type mismatch"), "{err}");
}

#[test]
fn host_environment_is_shared_and_deterministic() {
    let (mut store, inst) = session(|env, _| Ok(vec![Value::I32(env.now_millis() as i32)]));
    let first = call(&mut store, inst).unwrap();
    let second = call(&mut store, inst).unwrap();
    assert_eq!(first, vec![Value::I32(1)]);
    assert_eq!(second, vec![Value::I32(2)]);
    assert_eq!(store.host.call_count(), 2);
}
