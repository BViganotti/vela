//! Studio application state: source, module session, actions and breakpoints.

use vela_core::module::{ExportDesc, Module};
use vela_core::runtime::{instantiate, Limits, Registry, Store, Value};
use vela_core::text::parse_wat;
use vela_replay::{Breakpoint, RunStop, Session, SessionError};

use crate::assets;

pub struct Studio {
    pub source: String,
    pub export: String,
    pub args: Vec<Value>,
    pub session: Option<Session>,
    pub error: Option<String>,
    pub message: String,
    pub last_stop: Option<RunStop>,
    pub limits: Limits,
}

impl Default for Studio {
    fn default() -> Self {
        let limits = Limits {
            max_steps: 300_000,
            max_call_depth: 512,
            max_stack: 200_000,
        };
        Studio {
            source: assets::DEFAULT_MODULE.to_string(),
            export: "main".to_string(),
            args: vec![Value::I32(10)],
            session: None,
            error: None,
            message: "ready".to_string(),
            last_stop: None,
            limits,
        }
    }
}

impl Studio {
    pub fn new() -> Self {
        let mut s = Studio::default();
        let _ = s.reload();
        s
    }

    pub fn reload(&mut self) -> Result<(), String> {
        self.load(self.source.clone(), self.export.clone(), self.args.clone())
    }

    pub fn load(
        &mut self,
        source: String,
        export: String,
        args: Vec<Value>,
    ) -> Result<(), String> {
        self.error = None;
        let module: Module = parse_wat(&source).map_err(|e| {
            let msg = format!("parse error: {e}");
            self.error = Some(msg.clone());
            msg
        })?;
        let export = if export.is_empty() {
            module
                .exports
                .iter()
                .find(|e| matches!(e.desc, ExportDesc::Func(_)))
                .map(|e| e.name.clone())
                .unwrap_or_default()
        } else {
            export
        };
        if export.is_empty() {
            let msg = "module has no function exports".to_string();
            self.error = Some(msg.clone());
            return Err(msg);
        }
        let mut store = Store::new();
        let instance = instantiate(&mut store, module, &Registry::new()).map_err(|e| {
            let msg = format!("{e}");
            self.error = Some(msg.clone());
            msg
        })?;
        let session = Session::from_instance(
            store,
            instance,
            &export,
            args.clone(),
            self.limits,
            32,
        )
        .map_err(|e| {
            let msg = session_error(&e);
            self.error = Some(msg.clone());
            msg
        })?;
        self.session = Some(session);
        self.source = source;
        self.export = export;
        self.args = args;
        self.message = "loaded".to_string();
        self.last_stop = None;
        Ok(())
    }

    pub fn action(&mut self, action: &str, target: Option<u64>) -> Result<(), String> {
        if action == "restart" {
            return self.reload();
        }
        let session = match self.session.as_mut() {
            Some(s) => s,
            None => return Err("no session; load a module first".to_string()),
        };
        let stop = match action {
            "run" => {
                if session.hit_breakpoint().is_some() {
                    session.resume()
                } else {
                    session.run()
                }
            }
            "resume" => session.resume(),
            "step" => match session.step_forward() {
                vela_core::runtime::StepResult::Running => RunStop::Breakpoint(Breakpoint::HostCall),
                vela_core::runtime::StepResult::Finished => RunStop::Finished,
                vela_core::runtime::StepResult::Trapped(t) => RunStop::Trapped(t),
            },
            "back" => {
                session.step_back().map_err(|e| e.to_string())?;
                RunStop::Finished
            }
            "rewind" => {
                let target = target.ok_or_else(|| "rewind needs a target".to_string())?;
                session.rewind_to(target).map_err(|e| e.to_string())?;
                RunStop::Finished
            }
            "finish" => session.run_to_completion(),
            other => return Err(format!("unknown action {other:?}")),
        };
        let message = match &stop {
            RunStop::Finished => {
                if action == "step" && !session.exec.finished {
                    "stepped".to_string()
                } else if action == "back" || action == "rewind" {
                    "rewound".to_string()
                } else {
                    "finished".to_string()
                }
            }
            RunStop::Breakpoint(bp) => format!("breakpoint: {bp:?}"),
            RunStop::Trapped(t) => format!("trapped: {t}"),
            RunStop::StepLimit => "step limit reached".to_string(),
        };
        self.message = message;
        self.last_stop = Some(stop);
        Ok(())
    }

    pub fn add_breakpoint(&mut self, kind: &str) -> Result<(), String> {
        let session = self.session.as_mut().ok_or_else(|| "no session".to_string())?;
        let (func, offset) = session
            .current_instruction()
            .map(|(f, o, _)| (f, o))
            .unwrap_or((0, 0));
        let bp = match kind {
            "instruction" => Breakpoint::Instruction { func_addr: func, offset },
            "function" => Breakpoint::Function { func_addr: func },
            "host" => Breakpoint::HostCall,
            other => return Err(format!("unknown breakpoint kind {other:?}")),
        };
        session.add_breakpoint(bp);
        Ok(())
    }

    pub fn add_memory_breakpoint(&mut self, address: u32) -> Result<(), String> {
        let session = self.session.as_mut().ok_or_else(|| "no session".to_string())?;
        session.add_breakpoint(Breakpoint::MemoryAddress(address));
        Ok(())
    }

    pub fn remove_host_breakpoint(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.breakpoints.retain(|b| *b != Breakpoint::HostCall);
        }
    }

    pub fn clear_breakpoints(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.clear_breakpoints();
        }
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Address of the current function, or the exported function when idle.
    pub fn current_func_addr(&self) -> Option<u32> {
        let s = self.session.as_ref()?;
        if let Some(frame) = s.exec.frames.last() {
            return Some(frame.func_addr);
        }
        for inst in &s.store.instances {
            if let Some(vela_core::runtime::ExternVal::Func(addr)) = inst.exports.get(&self.export) {
                return Some(*addr);
            }
        }
        None
    }
}

fn session_error(e: &SessionError) -> String {
    match e {
        SessionError::Instantiate(vela_core::runtime::InstantiateError::Validation(v)) => {
            format!("validation error: {v}")
        }
        other => format!("{other}"),
    }
}
