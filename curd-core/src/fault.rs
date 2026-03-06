use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticFault {
    pub id: Uuid,
    pub source: FaultSource,
    pub message: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub end_line: Option<usize>,
    pub end_column: Option<usize>,
    pub severity: FaultSeverity,
    pub related_trace: Vec<TraceFrame>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FaultSource {
    Lsp,
    Dap,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FaultSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceFrame {
    pub name: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl SemanticFault {
    pub fn new_lsp(
        message: String,
        file: String,
        line: usize,
        column: usize,
        severity: FaultSeverity,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            source: FaultSource::Lsp,
            message,
            file,
            line,
            column,
            end_line: None,
            end_column: None,
            severity,
            related_trace: Vec::new(),
        }
    }

    pub fn new_dap(
        message: String,
        file: String,
        line: usize,
        column: usize,
        trace: Vec<TraceFrame>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            source: FaultSource::Dap,
            message,
            file,
            line,
            column,
            end_line: None,
            end_column: None,
            severity: FaultSeverity::Error,
            related_trace: trace,
        }
    }

    /// Implement telescoping: keep Top 3 and Bottom 2 frames.
    pub fn telescope_trace(&mut self) {
        let len = self.related_trace.len();
        if len <= 5 {
            return;
        }
        let top = self.related_trace[..3].to_vec();
        let bottom = self.related_trace[len - 2..].to_vec();

        let mut new_trace = top;
        new_trace.push(TraceFrame {
            name: format!("... [{} frames hidden] ...", len - 5),
            file: None,
            line: None,
            column: None,
        });
        new_trace.extend(bottom);
        self.related_trace = new_trace;
    }
}
