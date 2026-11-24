//! Decoder tests: well-formed modules, round-trips and malformed inputs.

use vela_core::binary::{decode_module, encode_module};
use vela_core::instr::{Instr, LoadKind};
use vela_core::module::ExportDesc;
use vela_core::types::ValType;

fn add_module_bytes() -> Vec<u8> {
    vec![
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
        // type section
        0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f,
        // function section
        0x03, 0x02, 0x01, 0x00,
        // export section
        0x07, 0x07, 0x01, 0x03, b'a', b'd', b'd', 0x00, 0x00,
        // code section
        0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
    ]
}

#[test]
fn decodes_minimal_add() {
    let m = decode_module(&add_module_bytes()).expect("valid module");
    assert_eq!(m.types.len(), 1);
    assert_eq!(m.types[0].params, vec![ValType::I32, ValType::I32]);
    assert_eq!(m.types[0].results, vec![ValType::I32]);
    assert_eq!(m.funcs.len(), 1);
    assert_eq!(m.funcs[0].type_idx, 0);
    assert_eq!(m.funcs[0].body.locals, Vec::<ValType>::new());
    assert_eq!(
        m.funcs[0].body.body,
        vec![
            Instr::LocalGet(0),
            Instr::LocalGet(1),
            Instr::I32Add,
            Instr::End
        ]
    );
    assert_eq!(m.exports[0].name, "add");
    assert!(matches!(m.exports[0].desc, ExportDesc::Func(0)));
}

#[test]
fn round_trips_encoding() {
    let m = decode_module(&add_module_bytes()).unwrap();
    let bytes = encode_module(&m);
    let m2 = decode_module(&bytes).expect("re-decoded module");
    assert_eq!(m, m2);
}

#[test]
fn rejects_bad_header() {
    assert!(decode_module(b"").is_err());
    assert!(decode_module(b"\0asx").is_err());
    let mut bytes = add_module_bytes();
    bytes[4] = 2;
    assert!(decode_module(&bytes).is_err());
}

#[test]
fn rejects_truncated_sections() {
    let bytes = add_module_bytes();
    for n in 9..bytes.len() {
        if n == 17 {
            // A type-only module is decodable.
            continue;
        }
        assert!(
            decode_module(&bytes[..n]).is_err(),
            "prefix of length {n} should be rejected"
        );
    }
}

#[test]
fn rejects_unknown_opcode() {
    let mut bytes = add_module_bytes();
    // Replace i32.add (0x6a) with an illegal opcode.
    let pos = bytes.len() - 2;
    bytes[pos] = 0xFE;
    assert!(decode_module(&bytes).is_err());
}

#[test]
fn rejects_duplicate_section() {
    let mut bytes = add_module_bytes();
    bytes.extend_from_slice(&[0x03, 0x01, 0x00]);
    assert!(decode_module(&bytes).is_err());
}

#[test]
fn rejects_section_out_of_order() {
    // Function section before type section.
    let bytes = vec![
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x03, 0x01, 0x00, 0x01, 0x01, 0x00,
    ];
    assert!(decode_module(&bytes).is_err());
}

#[test]
fn rejects_oversized_local_count() {
    // Code section body with an absurd local group count.
    let bytes = vec![
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x04, 0x01, 0x60, 0x00, 0x00,
        0x03, 0x02, 0x01, 0x00,
        0x0a, 0x0a, 0x01, 0x08, 0x01, 0xff, 0xff, 0x7f, 0x7f, 0x0b,
    ];
    assert!(decode_module(&bytes).is_err());
}

#[test]
fn decodes_memory_ops() {
    // (module (memory 1) (func (result i32) i32.const 0 i32.load))
    let bytes = vec![
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f,
        0x03, 0x02, 0x01, 0x00,
        0x05, 0x03, 0x01, 0x00, 0x01,
        0x0a, 0x09, 0x01, 0x07, 0x00, 0x41, 0x00, 0x28, 0x02, 0x00, 0x0b,
    ];
    let m = decode_module(&bytes).unwrap();
    assert_eq!(m.memories.len(), 1);
    assert_eq!(
        m.funcs[0].body.body[1],
        Instr::Load {
            kind: LoadKind::I32,
            mem: 0,
            arg: vela_core::instr::MemArg { align: 2, offset: 0 }
        }
    );
}

#[test]
fn rejects_oversized_module() {
    let bytes = vec![0u8; vela_core::binary::MAX_MODULE_BYTES + 1];
    assert!(decode_module(&bytes).is_err());
}
