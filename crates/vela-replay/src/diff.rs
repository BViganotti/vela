//! First-divergence diff between two execution traces.

use crate::record::{Trace, TraceEvent};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    pub step: u64,
    pub left: Option<TraceEvent>,
    pub right: Option<TraceEvent>,
    pub reason: String,
}

pub fn first_divergence(left: &Trace, right: &Trace) -> Option<Divergence> {
    let n = left.events.len().min(right.events.len());
    for i in 0..n {
        let a = &left.events[i];
        let b = &right.events[i];
        let reason = if a.func_addr != b.func_addr {
            "different function".to_string()
        } else if a.offset != b.offset {
            "different instruction offset".to_string()
        } else if a.op != b.op {
            format!("different opcode: {} vs {}", a.op, b.op)
        } else if a.stack_height != b.stack_height {
            format!(
                "different stack height: {} vs {}",
                a.stack_height, b.stack_height
            )
        } else if a.call_depth != b.call_depth {
            format!("different call depth: {} vs {}", a.call_depth, b.call_depth)
        } else if a.stack_hash != b.stack_hash {
            "divergent operand stack".to_string()
        } else {
            continue;
        };
        return Some(Divergence {
            step: a.step,
            left: Some(a.clone()),
            right: Some(b.clone()),
            reason,
        });
    }
    if left.events.len() != right.events.len() {
        return Some(Divergence {
            step: n as u64,
            left: left.events.get(n).cloned(),
            right: right.events.get(n).cloned(),
            reason: format!(
                "traces have different lengths ({} vs {})",
                left.events.len(),
                right.events.len()
            ),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::TraceEvent;

    fn ev(step: u64, op: &'static str) -> TraceEvent {
        TraceEvent {
            step,
            func_addr: 0,
            offset: step as usize,
            op,
            stack_height: 0,
            call_depth: 0,
            stack_hash: 0,
        }
    }

    #[test]
    fn identical_traces_do_not_diverge() {
        let mut a = Trace::new();
        a.push(ev(0, "i32.const"));
        assert!(first_divergence(&a, &a.clone()).is_none());
    }

    #[test]
    fn reports_first_difference() {
        let mut a = Trace::new();
        a.push(ev(0, "i32.const"));
        a.push(ev(1, "i32.add"));
        let mut b = a.clone();
        b.events[1].op = "i32.mul";
        let d = first_divergence(&a, &b).unwrap();
        assert_eq!(d.step, 1);
        assert!(d.reason.contains("opcode"));
    }

    #[test]
    fn reports_length_difference() {
        let mut a = Trace::new();
        a.push(ev(0, "a"));
        let mut b = a.clone();
        b.push(ev(1, "b"));
        assert!(first_divergence(&a, &b).is_some());
    }
}
