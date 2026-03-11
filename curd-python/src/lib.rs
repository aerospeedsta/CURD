use curd::API_VERSION;
use curd_core::{
    EngineContext, check_workspace_config, handle_contract, handle_debug_dispatcher,
    handle_diagram, handle_doctor, handle_edit, handle_graph, handle_lsp, handle_manage_file,
    handle_profile, handle_read, handle_search, handle_shell, handle_workspace,
};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pythonize::{depythonize, pythonize};
use serde_json::json;
use std::sync::Arc;

/// The main Python class we expose for codebase analysis
#[pyclass(name = "CurdEngine")]
pub struct PyCurdEngine {
    ctx: Arc<EngineContext>,
}

impl PyCurdEngine {
    fn to_py_object(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
        pythonize(py, value).map(Into::into).map_err(|e| {
            PyRuntimeError::new_err(format!(
                "Failed to convert Rust value to Python object: {}",
                e
            ))
        })
    }

    fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime")
            .block_on(future)
    }
}

#[pymethods]
impl PyCurdEngine {
    /// Initialize the native Rust engine for a specific workspace root
    #[new]
    #[pyo3(signature = (root="."))]
    fn new(root: &str) -> PyResult<Self> {
        let workspace_root = root.to_string();
        if let Err(findings) = check_workspace_config(std::path::Path::new(&workspace_root)) {
            return Err(PyRuntimeError::new_err(
                serde_json::json!({
                    "error": "Invalid CURD workspace configuration",
                    "findings": findings
                })
                .to_string(),
            ));
        }
        let ctx = EngineContext::new(&workspace_root);
        Ok(Self { ctx })
    }

    #[getter]
    fn api_version(&self) -> String {
        API_VERSION.to_string()
    }

    #[pyo3(signature = (query, mode=None, kind=None, limit=None))]
    fn search(
        &self,
        py: Python,
        query: &str,
        mode: Option<&str>,
        kind: Option<&str>,
        limit: Option<usize>,
    ) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "query": query,
            "mode": mode.unwrap_or("symbol"),
            "kind": kind,
            "limit": limit.unwrap_or(20)
        });
        let res = self.block_on(handle_search(&params, &self.ctx));
        if let Some(e) = res.get("error").and_then(|v| v.as_str()) {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (uri))]
    fn contract(&self, py: Python, uri: &str) -> PyResult<PyObject> {
        let params = serde_json::json!({ "uri": uri });
        let res = self.block_on(handle_contract(&params, &self.ctx));
        if let Some(e) = res.get("error").and_then(|v| v.as_str()) {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (uris, verbosity=None))]
    fn read(&self, py: Python, uris: Vec<String>, verbosity: Option<u8>) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "uris": uris,
            "verbosity": verbosity.unwrap_or(1)
        });
        let shadow_root = self
            .ctx
            .we
            .shadow
            .lock()
            .unwrap()
            .get_shadow_root()
            .cloned();
        let res = self.block_on(handle_read(&params, Arc::clone(&self.ctx.re), shadow_root));
        if let Some(e) = res.get("error").and_then(|v| v.as_str()) {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (uri, code, action=None, justification=None))]
    fn edit(
        &self,
        py: Python,
        uri: &str,
        code: &str,
        action: Option<&str>,
        justification: Option<&str>,
    ) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "uri": uri,
            "code": code,
            "action": action.unwrap_or("upsert"),
            "adaptation_justification": justification.unwrap_or("")
        });
        let shadow_root = self
            .ctx
            .we
            .shadow
            .lock()
            .unwrap()
            .get_shadow_root()
            .cloned();
        let res = self.block_on(handle_edit(&params, Arc::clone(&self.ctx.ee), shadow_root));
        if let Some(e) = res.get("error").and_then(|v| v.as_str()) {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (uris, direction=None, depth=None))]
    fn graph(
        &self,
        py: Python,
        uris: Vec<String>,
        direction: Option<&str>,
        depth: Option<u8>,
    ) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "uris": uris,
            "direction": direction.unwrap_or("both"),
            "depth": depth.unwrap_or(1)
        });
        let res = self.block_on(handle_graph(&params, Arc::clone(&self.ctx.ge)));
        if let Some(e) = res.get("error").and_then(|v| v.as_str()) {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (action=None, proposal_id=None, allow_unapproved=None))]
    fn workspace(
        &self,
        py: Python,
        action: Option<&str>,
        proposal_id: Option<&str>,
        allow_unapproved: Option<bool>,
    ) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "action": action.unwrap_or("status"),
            "proposal_id": proposal_id.unwrap_or(""),
            "allow_unapproved": allow_unapproved.unwrap_or(false)
        });
        let res = self.block_on(handle_workspace(&params, &self.ctx));
        if let Some(e) = res.get("error").and_then(|v| v.as_str()) {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (query, _is_regex=None))]
    fn find(&self, py: Python, query: &str, _is_regex: Option<bool>) -> PyResult<PyObject> {
        self.search(py, query, Some("text"), None, None)
    }

    #[pyo3(signature = (uris, format=None, up_depth=None, down_depth=None))]
    fn diagram(
        &self,
        py: Python,
        uris: Vec<String>,
        format: Option<&str>,
        up_depth: Option<u8>,
        down_depth: Option<u8>,
    ) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "uris": uris,
            "format": format.unwrap_or("mermaid"),
            "up_depth": up_depth.unwrap_or(1),
            "down_depth": down_depth.unwrap_or(1)
        });
        let res: serde_json::Value =
            self.block_on(handle_diagram(&params, Arc::clone(&self.ctx.de)));
        if let Some(e) = res
            .get("error")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (command))]
    fn shell(&self, py: Python, command: &str) -> PyResult<PyObject> {
        let params = serde_json::json!({ "command": command });
        let shadow_root = self
            .ctx
            .we
            .shadow
            .lock()
            .unwrap()
            .get_shadow_root()
            .cloned();
        let res: serde_json::Value =
            self.block_on(handle_shell(&params, &self.ctx.she, shadow_root.as_deref()));
        if let Some(e) = res
            .get("error")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (path, action=None, destination=None))]
    fn manage_file(
        &self,
        py: Python,
        path: &str,
        action: Option<&str>,
        destination: Option<&str>,
    ) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "path": path,
            "action": action.unwrap_or("create"),
            "destination": destination
        });
        let shadow_root = self
            .ctx
            .we
            .shadow
            .lock()
            .unwrap()
            .get_shadow_root()
            .cloned();
        let res: serde_json::Value = self.block_on(handle_manage_file(
            &params,
            Arc::clone(&self.ctx.fie),
            shadow_root,
        ));
        if let Some(e) = res
            .get("error")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (uri=None, mode=None, scope=None, limit=None, offset=None))]
    fn lsp(
        &self,
        py: Python,
        uri: Option<&str>,
        mode: Option<&str>,
        scope: Option<&str>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "uri": uri,
            "mode": mode.unwrap_or("syntax"),
            "scope": scope.unwrap_or("file"),
            "limit": limit,
            "offset": offset
        });
        let res: serde_json::Value = self.block_on(handle_lsp(&params, &self.ctx.le));
        if let Some(e) = res
            .get("error")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (roots, command=None, compare_command=None, format=None, up_depth=None, down_depth=None))]
    #[allow(clippy::too_many_arguments)]
    fn profile(
        &self,
        py: Python,
        roots: Vec<String>,
        command: Option<&str>,
        compare_command: Option<&str>,
        format: Option<&str>,
        up_depth: Option<u8>,
        down_depth: Option<u8>,
    ) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "roots": roots,
            "command": command,
            "compare_command": compare_command,
            "format": format.unwrap_or("ascii"),
            "up_depth": up_depth.unwrap_or(2),
            "down_depth": down_depth.unwrap_or(3)
        });
        let res: serde_json::Value = self.block_on(handle_profile(&params, &self.ctx.pe));
        if let Some(e) = res
            .get("error")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    fn debug_backends(&self, py: Python) -> PyResult<PyObject> {
        let val = self.ctx.dbe.backends();
        Self::to_py_object(py, &val)
    }

    #[pyo3(signature = (language, snippet, target=None, target_args=None))]
    fn debug(
        &self,
        py: Python,
        language: &str,
        snippet: &str,
        target: Option<&str>,
        target_args: Option<Vec<String>>,
    ) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "action": "execute",
            "language": language,
            "snippet": snippet,
            "target": target,
            "target_args": target_args.unwrap_or_default()
        });
        let res: serde_json::Value = self.block_on(handle_debug_dispatcher(&params, &self.ctx.dbe));
        if let Some(e) = res
            .get("error")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    #[pyo3(signature = (language, target=None, target_args=None))]
    fn debug_session_start(
        &self,
        py: Python,
        language: &str,
        target: Option<&str>,
        target_args: Option<Vec<String>>,
    ) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "action": "start_session",
            "language": language,
            "target": target,
            "target_args": target_args.unwrap_or_default()
        });
        let res: serde_json::Value = self.block_on(handle_debug_dispatcher(&params, &self.ctx.dbe));
        if let Some(e) = res
            .get("error")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    fn debug_session_send(&self, py: Python, session_id: u64, snippet: &str) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "action": "send_session",
            "session_id": session_id,
            "snippet": snippet
        });
        let res: serde_json::Value = self.block_on(handle_debug_dispatcher(&params, &self.ctx.dbe));
        if let Some(e) = res
            .get("error")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    fn debug_session_recv(&self, py: Python, session_id: u64) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "action": "recv_session",
            "session_id": session_id
        });
        let res: serde_json::Value = self.block_on(handle_debug_dispatcher(&params, &self.ctx.dbe));
        if let Some(e) = res
            .get("error")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }

    fn debug_session_stop(&self, py: Python, session_id: u64) -> PyResult<PyObject> {
        let params = serde_json::json!({
            "action": "stop_session",
            "session_id": session_id
        });
        let res: serde_json::Value = self.block_on(handle_debug_dispatcher(&params, &self.ctx.dbe));
        if let Some(e) = res
            .get("error")
            .and_then(|v: &serde_json::Value| v.as_str())
        {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        Self::to_py_object(py, &res)
    }
    #[pyo3(signature = (strict=None, profile=None, thresholds=None, index_config=None))]
    fn doctor(
        &self,
        py: Python,
        strict: Option<bool>,
        profile: Option<String>,
        thresholds: Option<Bound<'_, PyAny>>,
        index_config: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let thresholds_val: serde_json::Value = if let Some(t) = thresholds {
            depythonize(&t).map_err(|e| PyRuntimeError::new_err(e.to_string()))?
        } else {
            json!(null)
        };
        let index_cfg_val: serde_json::Value = if let Some(i) = index_config {
            depythonize(&i).map_err(|e| PyRuntimeError::new_err(e.to_string()))?
        } else {
            json!(null)
        };

        let params = json!({
            "strict": strict.unwrap_or(false),
            "profile": profile,
            "thresholds": thresholds_val,
            "index_config": index_cfg_val
        });
        let res: serde_json::Value = self.block_on(handle_doctor(&params, &self.ctx.doctore));
        if let Some(e) = res.get("error").and_then(|v| v.as_str()) {
            return Err(PyRuntimeError::new_err(e.to_string()));
        }
        pythonize(py, &res)
            .map(|b| b.unbind())
            .map_err(|e: pythonize::PythonizeError| PyRuntimeError::new_err(e.to_string()))
    }
}

/// The initializer for the Python module `curd_python`
#[pymodule]
fn curd_python(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyCurdEngine>()?;
    Ok(())
}
