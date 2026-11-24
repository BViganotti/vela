//! Control-flow metadata used by the interpreter: matching end/else indices.

use vela_core::instr::Instr;
use vela_core::runtime::BodyInfo;
use vela_core::types::BlockType;

fn body(instrs: Vec<Instr>) -> Vec<Instr> {
    instrs
}

#[test]
fn matches_nested_blocks() {
    let b = body(vec![
        Instr::Block(BlockType::Empty),
        Instr::Block(BlockType::Empty),
        Instr::Nop,
        Instr::End,
        Instr::End,
        Instr::End,
    ]);
    let info = BodyInfo::analyze(&b);
    assert_eq!(info.end_of[0], 4);
    assert_eq!(info.end_of[1], 3);
    assert_eq!(info.end_of[5], 5);
    assert!(info.else_of.iter().all(|e| e.is_none()));
}

#[test]
fn matches_if_else() {
    let b = body(vec![
        Instr::I32Const(1),
        Instr::If(BlockType::Empty),
        Instr::Nop,
        Instr::Else,
        Instr::Nop,
        Instr::End,
        Instr::End,
    ]);
    let info = BodyInfo::analyze(&b);
    assert_eq!(info.else_of[1], Some(3));
    assert_eq!(info.end_of[1], 5);
}

#[test]
fn nested_if_else_pairs_correctly() {
    let b = body(vec![
        Instr::If(BlockType::Empty),
        Instr::If(BlockType::Empty),
        Instr::Else,
        Instr::End,
        Instr::Else,
        Instr::End,
        Instr::End,
    ]);
    let info = BodyInfo::analyze(&b);
    assert_eq!(info.else_of[0], Some(4));
    assert_eq!(info.else_of[1], Some(2));
    assert_eq!(info.end_of[0], 5);
    assert_eq!(info.end_of[1], 3);
}

#[test]
fn malformed_bodies_do_not_panic() {
    let _ = BodyInfo::analyze(&[Instr::End]);
    let _ = BodyInfo::analyze(&[Instr::Else, Instr::End]);
    let _ = BodyInfo::analyze(&[Instr::Block(BlockType::Empty)]);
}
