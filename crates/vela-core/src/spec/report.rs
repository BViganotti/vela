//! Assertion report model for `.wast` runs.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub line: u32,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct WastReport {
    pub file: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub failures: Vec<Failure>,
}

impl WastReport {
    pub fn ok(&self) -> bool {
        self.failed == 0
    }

    pub fn summary(&self) -> String {
        format!(
            "{}: {}/{} assertions passed",
            self.file, self.passed, self.total
        )
    }
}
