//! Structured property test: random well-typed i32 expressions must execute to
//! the same value as an independent Rust evaluator.

use vela_core::runtime::{instantiate, invoke, Limits, Registry, Store, Value};
use vela_core::text::parse_wat;
use vela_core::validate::validate_module;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    Shl,
    ShrS,
    ShrU,
    Rotl,
    Rotr,
    Eq,
    LtS,
    GeU,
}

const OPS: [Op; 14] = [
    Op::Add,
    Op::Sub,
    Op::Mul,
    Op::And,
    Op::Or,
    Op::Xor,
    Op::Shl,
    Op::ShrS,
    Op::ShrU,
    Op::Rotl,
    Op::Rotr,
    Op::Eq,
    Op::LtS,
    Op::GeU,
];

#[derive(Clone, Debug)]
enum Expr {
    Const(i32),
    Param,
    Bin(Op, Box<Expr>, Box<Expr>),
}

fn gen(rng: &mut Rng, depth: u32) -> Expr {
    if depth == 0 || rng.below(3) == 0 {
        if rng.below(2) == 0 {
            Expr::Param
        } else {
            Expr::Const(rng.next() as i32)
        }
    } else {
        let op = OPS[rng.below(OPS.len() as u64) as usize];
        Expr::Bin(
            op,
            Box::new(gen(rng, depth - 1)),
            Box::new(gen(rng, depth - 1)),
        )
    }
}

fn render(e: &Expr) -> String {
    match e {
        Expr::Const(v) => format!("(i32.const {v})"),
        Expr::Param => "(local.get 0)".to_string(),
        Expr::Bin(op, a, b) => {
            let name = match op {
                Op::Add => "i32.add",
                Op::Sub => "i32.sub",
                Op::Mul => "i32.mul",
                Op::And => "i32.and",
                Op::Or => "i32.or",
                Op::Xor => "i32.xor",
                Op::Shl => "i32.shl",
                Op::ShrS => "i32.shr_s",
                Op::ShrU => "i32.shr_u",
                Op::Rotl => "i32.rotl",
                Op::Rotr => "i32.rotr",
                Op::Eq => "i32.eq",
                Op::LtS => "i32.lt_s",
                Op::GeU => "i32.ge_u",
            };
            format!("({name} {} {})", render(a), render(b))
        }
    }
}

fn eval(e: &Expr, x: i32) -> i32 {
    match e {
        Expr::Const(v) => *v,
        Expr::Param => x,
        Expr::Bin(op, a, b) => {
            let a = eval(a, x);
            let b = eval(b, x);
            match op {
                Op::Add => a.wrapping_add(b),
                Op::Sub => a.wrapping_sub(b),
                Op::Mul => a.wrapping_mul(b),
                Op::And => a & b,
                Op::Or => a | b,
                Op::Xor => a ^ b,
                Op::Shl => a.wrapping_shl(b as u32 & 31),
                Op::ShrS => a >> (b as u32 & 31),
                Op::ShrU => ((a as u32) >> (b as u32 & 31)) as i32,
                Op::Rotl => a.rotate_left(b as u32 & 31),
                Op::Rotr => a.rotate_right(b as u32 & 31),
                Op::Eq => (a == b) as i32,
                Op::LtS => (a < b) as i32,
                Op::GeU => ((a as u32) >= (b as u32)) as i32,
            }
        }
    }
}

#[test]
fn random_typed_expressions_match_reference_evaluator() {
    let mut rng = Rng(0xdec0_de01_2345_6789);
    for case in 0..500 {
        let expr = gen(&mut rng, 4);
        let arg = rng.next() as i32;
        let expected = eval(&expr, arg);
        let wat = format!(
            "(module (func (export \"f\") (param i32) (result i32) {}))",
            render(&expr)
        );
        let module = parse_wat(&wat).unwrap_or_else(|e| panic!("case {case} parse: {e}\n{wat}"));
        validate_module(&module).unwrap_or_else(|e| panic!("case {case} validate: {e}\n{wat}"));
        let mut store = Store::new();
        let inst = instantiate(&mut store, module, &Registry::new()).unwrap();
        let addr = match store.instances[inst as usize].exports["f"] {
            vela_core::runtime::ExternVal::Func(a) => a,
            _ => unreachable!(),
        };
        let out = invoke(&mut store, addr, vec![Value::I32(arg)], Limits::default())
            .unwrap_or_else(|e| panic!("case {case} trap: {e}\n{wat}"));
        assert_eq!(
            out,
            vec![Value::I32(expected)],
            "case {case}: {wat} with {arg}"
        );
    }
}
