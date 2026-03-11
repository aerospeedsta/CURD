use crate::plan::{IdOrTag, PlanNode, ToolOperation};
use crate::{DslNode, Plan, ReplState};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptMetadata {
    pub profile: Option<String>,
    pub session: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptAnnotations {
    pub explain: Option<String>,
    #[serde(default)]
    pub why: Vec<String>,
    #[serde(default)]
    pub risk: Vec<String>,
    #[serde(default)]
    pub review: Vec<String>,
    #[serde(default)]
    pub tag: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptArgDecl {
    pub name: String,
    pub declared_type: Option<String>,
    pub default_value: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurdScript {
    pub metadata: ScriptMetadata,
    pub explainability: ScriptAnnotations,
    pub args: Vec<ScriptArgDecl>,
    pub body: Vec<ScriptStatement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScriptStatement {
    Let {
        name: String,
        value: Value,
    },
    ToolCall {
        tool: String,
        named_args: Map<String, Value>,
        positional_args: Vec<Value>,
    },
    Sequence {
        name: Option<String>,
        body: Vec<ScriptStatement>,
    },
    Parallel {
        name: Option<String>,
        body: Vec<ScriptStatement>,
    },
    Atomic {
        name: Option<String>,
        body: Vec<ScriptStatement>,
    },
    Abort {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledCurdScript {
    pub metadata: ScriptMetadata,
    pub explainability: ScriptAnnotations,
    pub nodes: Vec<DslNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledCurdPlanArtifact {
    pub format: String,
    pub source_kind: String,
    pub source_sha256: String,
    pub source_path: Option<String>,
    pub metadata: ScriptMetadata,
    pub arg_bindings: Map<String, Value>,
    pub explainability: ScriptAnnotations,
    pub safeguards: CompiledCurdSafeguards,
    pub runtime_ceiling: Option<String>,
    pub plan: Plan,
    pub compiled_dsl: Vec<DslNode>,
    pub node_artifacts: Vec<CompiledCurdPlanNodeArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledCurdSafeguards {
    pub session_required: bool,
    #[serde(default)]
    pub mutation_targets: Vec<String>,
    #[serde(default)]
    pub recommended_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledCurdPlanNodeArtifact {
    pub node_id: Uuid,
    pub tool: String,
    pub args: Value,
    pub explainability: ScriptAnnotations,
    pub block: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    String(String),
    MultilineString(String),
    Number(String),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Equals,
    Dollar,
    Newline,
    Eof,
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn remaining(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn next_token(&mut self) -> Result<Token> {
        loop {
            let Some(ch) = self.peek_char() else {
                return Ok(Token::Eof);
            };
            match ch {
                ' ' | '\t' | '\r' => {
                    self.bump_char();
                }
                '\n' => {
                    self.bump_char();
                    return Ok(Token::Newline);
                }
                '#' => {
                    while let Some(c) = self.peek_char() {
                        self.bump_char();
                        if c == '\n' {
                            return Ok(Token::Newline);
                        }
                    }
                    return Ok(Token::Eof);
                }
                '{' => {
                    self.bump_char();
                    return Ok(Token::LBrace);
                }
                '}' => {
                    self.bump_char();
                    return Ok(Token::RBrace);
                }
                '[' => {
                    self.bump_char();
                    return Ok(Token::LBracket);
                }
                ']' => {
                    self.bump_char();
                    return Ok(Token::RBracket);
                }
                ',' => {
                    self.bump_char();
                    return Ok(Token::Comma);
                }
                ':' => {
                    self.bump_char();
                    return Ok(Token::Colon);
                }
                '=' => {
                    self.bump_char();
                    return Ok(Token::Equals);
                }
                '$' => {
                    self.bump_char();
                    return Ok(Token::Dollar);
                }
                '"' => {
                    if self.remaining().starts_with("\"\"\"") {
                        self.pos += 3;
                        let start = self.pos;
                        let end = self.remaining().find("\"\"\"").ok_or_else(|| {
                            anyhow!("unterminated multiline string literal in .curd script")
                        })?;
                        let content = self.src[start..start + end].to_string();
                        self.pos = start + end + 3;
                        return Ok(Token::MultilineString(content));
                    }
                    self.bump_char();
                    let mut out = String::new();
                    while let Some(c) = self.bump_char() {
                        match c {
                            '"' => return Ok(Token::String(out)),
                            '\\' => {
                                let Some(next) = self.bump_char() else {
                                    bail!("unterminated escape sequence in string literal");
                                };
                                let resolved = match next {
                                    'n' => '\n',
                                    'r' => '\r',
                                    't' => '\t',
                                    '"' => '"',
                                    '\\' => '\\',
                                    other => other,
                                };
                                out.push(resolved);
                            }
                            other => out.push(other),
                        }
                    }
                    bail!("unterminated string literal in .curd script");
                }
                '-' | '0'..='9' => {
                    let start = self.pos;
                    self.bump_char();
                    while matches!(self.peek_char(), Some('0'..='9' | '.')) {
                        self.bump_char();
                    }
                    return Ok(Token::Number(self.src[start..self.pos].to_string()));
                }
                _ if is_ident_start(ch) => {
                    let start = self.pos;
                    self.bump_char();
                    while matches!(self.peek_char(), Some(c) if is_ident_continue(c)) {
                        self.bump_char();
                    }
                    return Ok(Token::Ident(self.src[start..self.pos].to_string()));
                }
                _ => bail!("unexpected character '{}' in .curd script", ch),
            }
        }
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-')
}

struct Parser {
    tokens: Vec<Token>,
    idx: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, idx: 0 }
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.idx).unwrap_or(&Token::Eof)
    }

    fn peek_next(&self) -> &Token {
        self.tokens.get(self.idx + 1).unwrap_or(&Token::Eof)
    }

    fn bump(&mut self) -> Token {
        let tok = self.current().clone();
        self.idx += 1;
        tok
    }

    fn consume_newlines(&mut self) {
        while matches!(self.current(), Token::Newline) {
            self.bump();
        }
    }

    fn expect_ident(&mut self) -> Result<String> {
        match self.bump() {
            Token::Ident(v) => Ok(v),
            other => bail!("expected identifier, found {:?}", other),
        }
    }

    fn expect_token(&mut self, expected: Token) -> Result<()> {
        let got = self.bump();
        if got == expected {
            Ok(())
        } else {
            bail!("expected {:?}, found {:?}", expected, got)
        }
    }

    fn parse_program(&mut self) -> Result<CurdScript> {
        let mut metadata = ScriptMetadata::default();
        let mut args = Vec::new();
        let mut body = Vec::new();
        self.consume_newlines();
        while !matches!(self.current(), Token::Eof) {
            match self.current() {
                Token::Ident(word) if word == "use" => self.parse_use(&mut metadata)?,
                Token::Ident(word) if word == "arg" => args.push(self.parse_arg_decl()?),
                _ => body.push(self.parse_statement()?),
            }
            self.consume_newlines();
        }
        Ok(CurdScript {
            metadata,
            explainability: ScriptAnnotations::default(),
            args,
            body,
        })
    }

    fn parse_use(&mut self, metadata: &mut ScriptMetadata) -> Result<()> {
        self.expect_ident()?;
        let directive = self.expect_ident()?;
        let value = self.expect_ident()?;
        match directive.as_str() {
            "profile" => metadata.profile = Some(value),
            "session" => metadata.session = Some(value),
            _ => bail!("unsupported use directive '{}'", directive),
        }
        Ok(())
    }

    fn parse_arg_decl(&mut self) -> Result<ScriptArgDecl> {
        self.expect_ident()?;
        let name = self.expect_ident()?;
        let declared_type = if matches!(self.current(), Token::Colon) {
            self.bump();
            Some(self.expect_ident()?)
        } else {
            None
        };
        let default_value = if matches!(self.current(), Token::Equals) {
            self.bump();
            Some(self.parse_value()?)
        } else {
            None
        };
        Ok(ScriptArgDecl {
            name,
            declared_type,
            default_value,
        })
    }

    fn parse_statement(&mut self) -> Result<ScriptStatement> {
        match self.current() {
            Token::Ident(word) if word == "let" => self.parse_let(),
            Token::Ident(word) if word == "abort" => self.parse_abort(),
            Token::Ident(word) if word == "sequence" => self.parse_block_statement("sequence"),
            Token::Ident(word) if word == "parallel" => self.parse_block_statement("parallel"),
            Token::Ident(word) if word == "atomic" => self.parse_block_statement("atomic"),
            Token::Ident(_) => self.parse_tool_call(),
            other => bail!("unexpected token {:?} in statement position", other),
        }
    }

    fn parse_let(&mut self) -> Result<ScriptStatement> {
        self.expect_ident()?;
        let name = self.expect_ident()?;
        self.expect_token(Token::Equals)?;
        let value = self.parse_value()?;
        Ok(ScriptStatement::Let { name, value })
    }

    fn parse_abort(&mut self) -> Result<ScriptStatement> {
        self.expect_ident()?;
        let reason = match self.parse_value()? {
            Value::String(s) => s,
            other => other.to_string(),
        };
        Ok(ScriptStatement::Abort { reason })
    }

    fn parse_block_statement(&mut self, kind: &str) -> Result<ScriptStatement> {
        self.expect_ident()?;
        let name = if !matches!(self.current(), Token::LBrace) {
            Some(self.expect_ident()?)
        } else {
            None
        };
        self.expect_token(Token::LBrace)?;
        self.consume_newlines();
        let mut body = Vec::new();
        while !matches!(self.current(), Token::RBrace | Token::Eof) {
            body.push(self.parse_statement()?);
            self.consume_newlines();
        }
        self.expect_token(Token::RBrace)?;
        Ok(match kind {
            "sequence" => ScriptStatement::Sequence { name, body },
            "parallel" => ScriptStatement::Parallel { name, body },
            "atomic" => ScriptStatement::Atomic { name, body },
            _ => bail!("unsupported block kind '{}'", kind),
        })
    }

    fn parse_tool_call(&mut self) -> Result<ScriptStatement> {
        let tool = self.expect_ident()?;
        let mut named_args = Map::new();
        let mut positional_args = Vec::new();
        while !matches!(self.current(), Token::Newline | Token::RBrace | Token::Eof) {
            if let Token::Ident(name) = self.current().clone() {
                if matches!(self.peek_next(), Token::Equals) {
                    self.bump();
                    self.bump();
                    let value = self.parse_value()?;
                    named_args.insert(name, value);
                    continue;
                }
            }
            positional_args.push(self.parse_value()?);
        }
        Ok(ScriptStatement::ToolCall {
            tool,
            named_args,
            positional_args,
        })
    }

    fn parse_value(&mut self) -> Result<Value> {
        match self.bump() {
            Token::String(v) | Token::MultilineString(v) => Ok(Value::String(v)),
            Token::Number(v) => {
                if v.contains('.') {
                    let parsed = v.parse::<f64>()?;
                    let num =
                        Number::from_f64(parsed).ok_or_else(|| anyhow!("invalid float literal"))?;
                    Ok(Value::Number(num))
                } else {
                    Ok(Value::Number(Number::from(v.parse::<i64>()?)))
                }
            }
            Token::Ident(v) if v == "true" => Ok(Value::Bool(true)),
            Token::Ident(v) if v == "false" => Ok(Value::Bool(false)),
            Token::Ident(v) if v == "null" => Ok(Value::Null),
            Token::Ident(v) => Ok(Value::String(v)),
            Token::Dollar => Ok(Value::String(format!("${}", self.expect_ident()?))),
            Token::LBracket => {
                let mut values = Vec::new();
                if !matches!(self.current(), Token::RBracket) {
                    loop {
                        values.push(self.parse_value()?);
                        if matches!(self.current(), Token::Comma) {
                            self.bump();
                            continue;
                        }
                        break;
                    }
                }
                self.expect_token(Token::RBracket)?;
                Ok(Value::Array(values))
            }
            Token::LBrace => {
                let mut map = Map::new();
                if !matches!(self.current(), Token::RBrace) {
                    loop {
                        let key = match self.bump() {
                            Token::Ident(k) | Token::String(k) => k,
                            other => bail!("expected object key, found {:?}", other),
                        };
                        self.expect_token(Token::Colon)?;
                        let value = self.parse_value()?;
                        map.insert(key, value);
                        if matches!(self.current(), Token::Comma) {
                            self.bump();
                            continue;
                        }
                        break;
                    }
                }
                self.expect_token(Token::RBrace)?;
                Ok(Value::Object(map))
            }
            other => bail!("unexpected token {:?} while parsing value", other),
        }
    }
}

pub fn parse_curd_script(source: &str) -> Result<CurdScript> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token()?;
        let done = token == Token::Eof;
        tokens.push(token);
        if done {
            break;
        }
    }
    let mut parsed = Parser::new(tokens).parse_program()?;
    parsed.explainability = extract_script_annotations(source);
    Ok(parsed)
}

pub fn compile_curd_script(
    script: &CurdScript,
    overrides: &Map<String, Value>,
) -> Result<CompiledCurdScript> {
    let mut nodes = Vec::new();
    for decl in &script.args {
        let value = if let Some(v) = overrides.get(&decl.name) {
            v.clone()
        } else if let Some(v) = &decl.default_value {
            v.clone()
        } else {
            bail!("missing required script argument '{}'", decl.name);
        };
        nodes.push(DslNode::Assign {
            var: decl.name.clone(),
            value,
        });
    }
    compile_statements(&script.body, &mut nodes)?;
    Ok(CompiledCurdScript {
        metadata: script.metadata.clone(),
        explainability: script.explainability.clone(),
        nodes,
    })
}

pub fn parse_and_compile_curd_script(
    source: &str,
    overrides: &Map<String, Value>,
) -> Result<CompiledCurdScript> {
    let script = parse_curd_script(source)?;
    compile_curd_script(&script, overrides)
}

pub fn compile_curd_script_to_plan(
    script: &CurdScript,
    overrides: &Map<String, Value>,
) -> Result<CompiledCurdPlanArtifact> {
    let compiled = compile_curd_script(script, overrides)?;
    let source_sha256 = sha256_hex(
        &serde_json::to_vec(script)
            .map_err(|e| anyhow!("failed to serialize script for hashing: {e}"))?,
    );
    let mut state = ReplState::new();
    let mut arg_bindings = Map::new();
    for decl in &script.args {
        let value = if let Some(v) = overrides.get(&decl.name) {
            v.clone()
        } else if let Some(v) = &decl.default_value {
            v.clone()
        } else {
            bail!("missing required script argument '{}'", decl.name);
        };
        state.variables.insert(decl.name.clone(), value);
        arg_bindings.insert(
            decl.name.clone(),
            state
                .variables
                .get(&decl.name)
                .cloned()
                .unwrap_or(Value::Null),
        );
    }
    let mut nodes = Vec::new();
    let mut artifacts = Vec::new();
    let mut prev: Option<Uuid> = None;
    compile_statements_to_plan(
        &script.body,
        &mut state,
        &mut nodes,
        &mut artifacts,
        &mut prev,
        None,
    )?;
    let mutation_targets = collect_plan_artifact_targets(&artifacts);
    let session_required = compiled_script_requires_shadow_session(&compiled.nodes);
    let safeguards = CompiledCurdSafeguards {
        session_required,
        mutation_targets: mutation_targets.clone(),
        recommended_actions: recommend_script_safeguards(session_required, &mutation_targets),
    };
    Ok(CompiledCurdPlanArtifact {
        format: "curd_plan_v1".to_string(),
        source_kind: "curd_script".to_string(),
        source_sha256,
        source_path: None,
        metadata: script.metadata.clone(),
        arg_bindings,
        explainability: script.explainability.clone(),
        safeguards,
        runtime_ceiling: None,
        plan: Plan {
            id: Uuid::new_v4(),
            nodes,
        },
        compiled_dsl: compiled.nodes,
        node_artifacts: artifacts,
    })
}

pub fn parse_and_compile_curd_script_to_plan(
    source: &str,
    overrides: &Map<String, Value>,
) -> Result<CompiledCurdPlanArtifact> {
    let script = parse_curd_script(source)?;
    compile_curd_script_to_plan(&script, overrides)
}

fn compile_statements(body: &[ScriptStatement], out: &mut Vec<DslNode>) -> Result<()> {
    for stmt in body {
        match stmt {
            ScriptStatement::Let { name, value } => out.push(DslNode::Assign {
                var: name.clone(),
                value: value.clone(),
            }),
            ScriptStatement::ToolCall {
                tool,
                named_args,
                positional_args,
            } => {
                let args = lower_tool_args(tool, named_args, positional_args)?;
                out.push(DslNode::Call {
                    tool: tool.clone(),
                    args,
                });
            }
            ScriptStatement::Sequence { body, .. } => compile_statements(body, out)?,
            ScriptStatement::Atomic { body, .. } => {
                let mut nested = Vec::new();
                compile_statements(body, &mut nested)?;
                out.push(DslNode::Atomic { nodes: nested });
            }
            ScriptStatement::Parallel { name, .. } => {
                if let Some(name) = name {
                    bail!(
                        "parallel block '{}' is not executable yet; current CURD runtime only lowers .curd scripts to sequential DslNode IR",
                        name
                    );
                }
                bail!(
                    "parallel blocks are not executable yet; current CURD runtime only lowers .curd scripts to sequential DslNode IR"
                );
            }
            ScriptStatement::Abort { reason } => out.push(DslNode::Abort {
                reason: reason.clone(),
            }),
        }
    }
    Ok(())
}

fn compile_statements_to_plan(
    body: &[ScriptStatement],
    state: &mut ReplState,
    out: &mut Vec<PlanNode>,
    artifacts: &mut Vec<CompiledCurdPlanNodeArtifact>,
    prev: &mut Option<Uuid>,
    block: Option<String>,
) -> Result<()> {
    for stmt in body {
        match stmt {
            ScriptStatement::Let { name, value } => {
                let resolved = state.resolve(value);
                state.variables.insert(name.clone(), resolved);
            }
            ScriptStatement::ToolCall {
                tool,
                named_args,
                positional_args,
            } => {
                let lowered = lower_tool_args(tool, named_args, positional_args)?;
                let resolved = state.resolve(&lowered);
                let node_id = Uuid::new_v4();
                let mut dependencies = Vec::new();
                if let Some(prev_id) = prev {
                    dependencies.push(IdOrTag::Id(*prev_id));
                }
                out.push(PlanNode {
                    id: node_id,
                    op: ToolOperation::McpCall {
                        tool: tool.clone(),
                        args: resolved.clone(),
                    },
                    dependencies,
                    output_limit: 64 * 1024,
                    retry_limit: 0,
                });
                artifacts.push(CompiledCurdPlanNodeArtifact {
                    node_id,
                    tool: tool.clone(),
                    args: resolved,
                    explainability: ScriptAnnotations::default(),
                    block: block.clone(),
                });
                *prev = Some(node_id);
            }
            ScriptStatement::Sequence { name, body } => {
                compile_statements_to_plan(body, state, out, artifacts, prev, name.clone())?;
            }
            ScriptStatement::Atomic { name, body } => {
                compile_statements_to_plan(body, state, out, artifacts, prev, name.clone())?;
            }
            ScriptStatement::Parallel { name, .. } => {
                if let Some(name) = name {
                    bail!(
                        "parallel block '{}' cannot be compiled into a concrete sequential plan yet",
                        name
                    );
                }
                bail!("parallel blocks cannot be compiled into a concrete sequential plan yet");
            }
            ScriptStatement::Abort { .. } => {
                bail!(
                    "abort statements cannot be compiled into executable plan.json artifacts yet"
                );
            }
        }
    }
    Ok(())
}

fn extract_script_annotations(source: &str) -> ScriptAnnotations {
    let mut annotations = ScriptAnnotations::default();
    for line in source.lines() {
        let trimmed = line.trim();
        let Some(stripped) = trimmed.strip_prefix('#') else {
            continue;
        };
        let directive = stripped.trim();
        if let Some((key, value)) = directive.split_once(':') {
            let value = value.trim().to_string();
            match key.trim() {
                "explain" => {
                    if annotations.explain.is_none() {
                        annotations.explain = Some(value);
                    }
                }
                "why" => annotations.why.push(value),
                "risk" => annotations.risk.push(value),
                "review" => annotations.review.push(value),
                "tag" => annotations.tag.push(value),
                _ => {}
            }
        }
    }
    annotations
}

pub fn compiled_script_requires_shadow_session(nodes: &[DslNode]) -> bool {
    fn visit(nodes: &[DslNode]) -> bool {
        for node in nodes {
            match node {
                DslNode::Call { tool, .. } => {
                    if matches!(
                        tool.as_str(),
                        "edit"
                            | "manage_file"
                            | "mutate"
                            | "proposal"
                            | "refactor"
                            | "shell"
                            | "build"
                            | "execute_plan"
                            | "execute_active_plan"
                            | "execute_dsl"
                    ) {
                        return true;
                    }
                }
                DslNode::Atomic { nodes } => {
                    if visit(nodes) {
                        return true;
                    }
                }
                DslNode::Assign { .. } | DslNode::Abort { .. } => {}
            }
        }
        false
    }
    visit(nodes)
}

pub fn collect_compiled_script_targets(nodes: &[DslNode]) -> Vec<String> {
    let mut targets = Vec::new();
    fn visit(nodes: &[DslNode], out: &mut Vec<String>) {
        for node in nodes {
            match node {
                DslNode::Call { tool, args } => match tool.as_str() {
                    "edit" | "refactor" => {
                        if let Some(uri) = args.get("uri").and_then(|v| v.as_str()) {
                            out.push(uri.to_string());
                        }
                    }
                    "read" | "graph" | "lsp" | "diagram" => {
                        if let Some(uris) = args.get("uris").and_then(|v| v.as_array()) {
                            out.extend(
                                uris.iter()
                                    .filter_map(|v| v.as_str().map(ToOwned::to_owned)),
                            );
                        }
                    }
                    _ => {}
                },
                DslNode::Atomic { nodes } => visit(nodes, out),
                DslNode::Assign { .. } | DslNode::Abort { .. } => {}
            }
        }
    }
    visit(nodes, &mut targets);
    targets.sort();
    targets.dedup();
    targets
}

pub fn recommend_script_safeguards(
    session_required: bool,
    mutation_targets: &[String],
) -> Vec<String> {
    let mut out = Vec::new();
    if session_required {
        out.push("open a workspace session before execution".to_string());
        out.push("compile to a plan artifact before mutating execution".to_string());
    }
    if mutation_targets.len() > 1 {
        out.push("review target overlap before executing multi-target changes".to_string());
        out.push(
            "consider splitting unrelated edits into separate atomic blocks or plan branches"
                .to_string(),
        );
    }
    if !mutation_targets.is_empty() {
        out.push("run verify_impact after each mutating block".to_string());
        out.push("use plan edit to tune retry_limit and output_limit before promotion".to_string());
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

fn collect_plan_artifact_targets(artifacts: &[CompiledCurdPlanNodeArtifact]) -> Vec<String> {
    let mut targets = Vec::new();
    for node in artifacts {
        match node.tool.as_str() {
            "edit" | "refactor" => {
                if let Some(uri) = node.args.get("uri").and_then(|v| v.as_str()) {
                    targets.push(uri.to_string());
                }
            }
            "read" | "graph" | "lsp" | "diagram" => {
                if let Some(uris) = node.args.get("uris").and_then(|v| v.as_array()) {
                    targets.extend(
                        uris.iter()
                            .filter_map(|v| v.as_str().map(ToOwned::to_owned)),
                    );
                }
            }
            _ => {}
        }
    }
    targets.sort();
    targets.dedup();
    targets
}

fn lower_tool_args(tool: &str, named: &Map<String, Value>, positional: &[Value]) -> Result<Value> {
    if positional.is_empty() {
        return Ok(Value::Object(named.clone()));
    }
    if !named.is_empty() {
        bail!(
            "tool '{}' mixes named and positional arguments; use named args in .curd scripts",
            tool
        );
    }
    if positional.len() != 1 {
        bail!(
            "tool '{}' uses {} positional arguments; only a single positional shorthand is supported",
            tool,
            positional.len()
        );
    }
    let single = positional[0].clone();
    Ok(match tool {
        "search" => json!({ "query": single }),
        "read" | "graph" | "lsp" | "diagram" => json!({ "uris": [single] }),
        "shell" => json!({ "command": single }),
        _ => bail!("tool '{}' requires named arguments in .curd scripts", tool),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_and_compiles_basic_script() {
        let compiled = parse_and_compile_curd_script(
            r#"
use profile supervised
arg target_uri: string
let patch = """
fn validate(token: &str) -> bool {
  !token.is_empty()
}
"""

sequence main {
  atomic apply_fix {
    edit uri=$target_uri action="upsert" code=$patch
    verify_impact strict=true
  }
}
"#,
            &Map::from_iter([("target_uri".to_string(), json!("src/auth.rs::validate"))]),
        )
        .expect("script should compile");

        assert_eq!(compiled.metadata.profile.as_deref(), Some("supervised"));
        assert_eq!(compiled.nodes.len(), 3);
        match &compiled.nodes[0] {
            DslNode::Assign { var, value } => {
                assert_eq!(var, "target_uri");
                assert_eq!(value, "src/auth.rs::validate");
            }
            other => panic!("unexpected first node: {:?}", other),
        }
        match &compiled.nodes[1] {
            DslNode::Assign { var, value } => {
                assert_eq!(var, "patch");
                assert!(value.as_str().unwrap_or("").contains("validate"));
            }
            other => panic!("unexpected second node: {:?}", other),
        }
        assert!(matches!(compiled.nodes[2], DslNode::Atomic { .. }));
    }

    #[test]
    fn applies_default_args() {
        let compiled = parse_and_compile_curd_script(
            r#"
arg strict: bool = true
verify_impact strict=$strict
"#,
            &Map::new(),
        )
        .expect("script should compile");
        assert!(matches!(
            compiled.nodes.first(),
            Some(DslNode::Assign { var, value }) if var == "strict" && value == &json!(true)
        ));
    }

    #[test]
    fn rejects_parallel_for_now() {
        let err = parse_and_compile_curd_script(
            r#"
parallel fanout {
  search query="Auth"
}
"#,
            &Map::new(),
        )
        .expect_err("parallel should not compile yet");
        assert!(err.to_string().contains("parallel block"));
    }

    #[test]
    fn extracts_structured_comment_annotations() {
        let parsed = parse_curd_script(
            r#"
# explain: tighten auth validation without changing public API
# why: downstream callers still depend on the same symbol shape
# risk: auth and session are tightly connected
verify_impact strict=true
"#,
        )
        .expect("script should parse");
        assert_eq!(
            parsed.explainability.explain.as_deref(),
            Some("tighten auth validation without changing public API")
        );
        assert_eq!(parsed.explainability.why.len(), 1);
        assert_eq!(parsed.explainability.risk.len(), 1);
    }

    #[test]
    fn compiles_concrete_plan_artifact() {
        let script = parse_curd_script(
            r#"
# explain: read before review
arg target: string = "src/lib.rs::alpha"
read $target
verify_impact strict=true
"#,
        )
        .expect("script should parse");
        let artifact = compile_curd_script_to_plan(&script, &Map::new()).expect("plan compile");
        assert_eq!(artifact.plan.nodes.len(), 2);
        assert_eq!(artifact.node_artifacts.len(), 2);
        assert_eq!(
            artifact.arg_bindings.get("target"),
            Some(&json!("src/lib.rs::alpha"))
        );
        assert_eq!(artifact.source_path, None);
        assert_eq!(artifact.runtime_ceiling, None);
        assert_eq!(artifact.safeguards.session_required, false);
        assert_eq!(
            artifact.safeguards.mutation_targets,
            vec!["src/lib.rs::alpha".to_string()]
        );
        assert!(
            artifact
                .safeguards
                .recommended_actions
                .iter()
                .any(|v| v.contains("verify_impact"))
        );
        assert_eq!(
            artifact.explainability.explain.as_deref(),
            Some("read before review")
        );
    }

    #[test]
    fn mutation_scripts_compile_with_session_safeguards() {
        let script = parse_curd_script(
            r#"
arg target: string = "src/lib.rs::alpha"
let patch = "pub fn alpha() {}"
atomic apply_fix {
  edit uri=$target code=$patch adaptation_justification="test"
}
"#,
        )
        .expect("script should parse");
        let artifact = compile_curd_script_to_plan(&script, &Map::new()).expect("plan compile");
        assert!(artifact.safeguards.session_required);
        assert_eq!(
            artifact.safeguards.mutation_targets,
            vec!["src/lib.rs::alpha".to_string()]
        );
        assert!(
            artifact
                .safeguards
                .recommended_actions
                .iter()
                .any(|v| v.contains("workspace session"))
        );
    }
}
