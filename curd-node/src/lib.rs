use curd_core::{
    API_VERSION, EngineContext, McpServer, check_workspace_config, handle_contract,
    handle_debug_dispatcher, handle_diagram, handle_doctor, handle_edit, handle_graph, handle_lsp,
    handle_manage_file, handle_profile, handle_read, handle_search, handle_shell, handle_workspace,
    scan_workspace,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::Arc;

#[napi]
pub struct CurdEngine {
    workspace_root: String,
    ctx: Arc<EngineContext>,
}

#[napi]
impl CurdEngine {
    #[napi(constructor)]
    pub fn new(root: String) -> Result<Self> {
        let workspace_root = root.clone();
        if let Err(findings) = check_workspace_config(std::path::Path::new(&workspace_root)) {
            let err_json = serde_json::json!({
                "error": "Invalid CURD workspace configuration",
                "findings": findings
            });
            return Err(Error::from_reason(err_json.to_string()));
        }
        let ctx = EngineContext::new(&workspace_root);
        Ok(Self {
            workspace_root,
            ctx,
        })
    }

    #[napi]
    pub fn api_version(&self) -> String {
        API_VERSION.to_string()
    }

    #[napi]
    pub fn run_mcp_server(&self) -> Result<()> {
        let root = self.workspace_root.clone();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::from_reason(format!("Failed to build tokio runtime: {}", e)))?;

        rt.block_on(async {
            let server = McpServer::new(&root);
            server.run().await
        })
        .map_err(|e| Error::from_reason(format!("MCP Server error: {}", e)))
    }

    #[napi]
    pub fn scan_workspace(&self) -> Result<Vec<String>> {
        match scan_workspace(&self.workspace_root) {
            Ok(paths) => Ok(paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()),
            Err(e) => Err(Error::from_reason(format!(
                "Failed to scan workspace: {}",
                e
            ))),
        }
    }

    #[napi]
    pub async fn search(
        &self,
        query: String,
        mode: Option<String>,
        kind: Option<String>,
        limit: Option<u32>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "query": query,
            "mode": mode.unwrap_or_else(|| "symbol".to_string()),
            "kind": kind,
            "limit": limit.unwrap_or(20)
        });
        let res = handle_search(&params, &self.ctx).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn contract(&self, uri: String) -> Result<serde_json::Value> {
        let params = serde_json::json!({ "uri": uri });
        let res = handle_contract(&params, &self.ctx).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn read(&self, uris: Vec<String>, verbosity: Option<u32>) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "uris": uris,
            "verbosity": verbosity.unwrap_or(1)
        });
        let shadow_root = self.ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
        let res = handle_read(&params, Arc::clone(&self.ctx.re), shadow_root).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn edit(
        &self,
        uri: String,
        code: String,
        action: Option<String>,
        justification: Option<String>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "uri": uri,
            "code": code,
            "action": action.unwrap_or_else(|| "upsert".to_string()),
            "adaptation_justification": justification.unwrap_or_default()
        });
        let shadow_root = self.ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
        let res = handle_edit(&params, Arc::clone(&self.ctx.ee), shadow_root).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn graph(
        &self,
        uris: Vec<String>,
        direction: Option<String>,
        depth: Option<u32>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "uris": uris,
            "direction": direction.unwrap_or_else(|| "both".to_string()),
            "depth": depth.unwrap_or(1)
        });
        let res = handle_graph(&params, Arc::clone(&self.ctx.ge)).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn workspace(
        &self,
        action: Option<String>,
        proposal_id: Option<String>,
        allow_unapproved: Option<bool>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "action": action.unwrap_or_else(|| "status".to_string()),
            "proposal_id": proposal_id.unwrap_or_default(),
            "allow_unapproved": allow_unapproved.unwrap_or(false)
        });
        let res = handle_workspace(&params, &self.ctx).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn find(&self, query: String) -> Result<serde_json::Value> {
        self.search(query, Some("text".to_string()), None, None).await
    }

    #[napi]
    pub async fn diagram(
        &self,
        uris: Vec<String>,
        format: Option<String>,
        up_depth: Option<u32>,
        down_depth: Option<u32>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "uris": uris,
            "format": format.unwrap_or_else(|| "mermaid".to_string()),
            "up_depth": up_depth.unwrap_or(1),
            "down_depth": down_depth.unwrap_or(1)
        });
        let res: serde_json::Value = handle_diagram(&params, Arc::clone(&self.ctx.de)).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn shell(&self, command: String) -> Result<serde_json::Value> {
        let params = serde_json::json!({ "command": command });
        let shadow_root = self.ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
        let res: serde_json::Value =
            handle_shell(&params, &self.ctx.she, shadow_root.as_deref()).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn manage_file(
        &self,
        path: String,
        action: Option<String>,
        destination: Option<String>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "path": path,
            "action": action.unwrap_or_else(|| "create".to_string()),
            "destination": destination
        });
        let shadow_root = self.ctx.we.shadow.lock().unwrap().get_shadow_root().cloned();
        let res: serde_json::Value =
            handle_manage_file(&params, Arc::clone(&self.ctx.fie), shadow_root).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn lsp(
        &self,
        uri: Option<String>,
        mode: Option<String>,
        scope: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "uri": uri,
            "mode": mode.unwrap_or_else(|| "syntax".to_string()),
            "scope": scope.unwrap_or_else(|| "file".to_string()),
            "limit": limit,
            "offset": offset
        });
        let res: serde_json::Value = handle_lsp(&params, &self.ctx.le).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn profile(
        &self,
        roots: Vec<String>,
        command: Option<String>,
        compare_command: Option<String>,
        format: Option<String>,
        up_depth: Option<u32>,
        down_depth: Option<u32>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "roots": roots,
            "command": command,
            "compare_command": compare_command,
            "format": format.unwrap_or_else(|| "ascii".to_string()),
            "up_depth": up_depth.unwrap_or(2),
            "down_depth": down_depth.unwrap_or(3)
        });
        let res: serde_json::Value = handle_profile(&params, &self.ctx.pe).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub fn debug_backends(&self) -> Result<serde_json::Value> {
        Ok(self.ctx.dbe.backends())
    }

    #[napi]
    pub async fn debug(
        &self,
        language: String,
        snippet: String,
        target: Option<String>,
        target_args: Option<Vec<String>>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "action": "execute",
            "language": language,
            "snippet": snippet,
            "target": target,
            "target_args": target_args.unwrap_or_default()
        });
        let res: serde_json::Value = handle_debug_dispatcher(&params, &self.ctx.dbe).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn debug_session_start(
        &self,
        language: String,
        target: Option<String>,
        target_args: Option<Vec<String>>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "action": "start_session",
            "language": language,
            "target": target,
            "target_args": target_args.unwrap_or_default()
        });
        let res: serde_json::Value = handle_debug_dispatcher(&params, &self.ctx.dbe).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn debug_session_send(
        &self,
        session_id: u32,
        snippet: String,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "action": "send_session",
            "session_id": session_id,
            "snippet": snippet
        });
        let res: serde_json::Value = handle_debug_dispatcher(&params, &self.ctx.dbe).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn debug_session_recv(&self, session_id: u32) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "action": "recv_session",
            "session_id": session_id
        });
        let res: serde_json::Value = handle_debug_dispatcher(&params, &self.ctx.dbe).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn debug_session_stop(&self, session_id: u32) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "action": "stop_session",
            "session_id": session_id
        });
        let res: serde_json::Value = handle_debug_dispatcher(&params, &self.ctx.dbe).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }

    #[napi]
    pub async fn doctor(
        &self,
        strict: Option<bool>,
        profile: Option<String>,
        thresholds: Option<serde_json::Value>,
        index_config: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "strict": strict.unwrap_or(false),
            "profile": profile,
            "thresholds": thresholds,
            "index_config": index_config
        });
        let res: serde_json::Value = handle_doctor(&params, &self.ctx.doctore).await;
        if res.get("error").is_some() {
            return Err(Error::from_reason(res["error"].to_string()));
        }
        Ok(res)
    }
}
