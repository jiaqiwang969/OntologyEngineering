// Generated semantic group by locked official ggen; no engineering dependencies.

/// Normalized source time fields; does not validate interval ordering or engineering applicability
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticBiTemporalFields {
    /// Explicit recorded time, valid_from fallback, or explicit request clock
    pub recorded_at: String,
    /// Canonical UTC transaction end or open null
    pub superseded_at: Option<String>,
    /// Canonical UTC start or unknown/unbounded null
    pub valid_from: Option<String>,
    /// Canonical UTC end or open null
    pub valid_until: Option<String>,
}
impl SemanticBiTemporalFields {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.recorded_at.trim().is_empty() { failures.push("SemanticBiTemporalFields.recorded_at:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一已注册书源的显式受控读源/场景执行目录请求；不接受新的文件路径或创建registry
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticBookCatalogRequest {
    /// 同一已注册书源的显式受控读源/场景执行目录请求；不接受新的文件路径或创建registry：arguments
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub arguments: serde_json::Value,
    /// 同一已注册书源的显式受控读源/场景执行目录请求；不接受新的文件路径或创建registry：operation
    pub operation: String,
    /// 同一已注册书源的显式受控读源/场景执行目录请求；不接受新的文件路径或创建registry：schema
    pub schema: String,
}
impl SemanticBookCatalogRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticBookCatalogRequest.operation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticBookCatalogRequest.schema:required".into()); }

        if !["get","list","run","verify","verify_sources",].contains(&self.operation.as_str()) { failures.push("SemanticBookCatalogRequest.operation:status".into()); }
        if !["semantica.book-catalog-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticBookCatalogRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 保留原书籍目录/manifest和原来源checker/场景DTO、真实读取源身份；新Rust回执另标profile，原partial/blocked状态不升级
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticBookCatalogResult {
    /// 保留原书籍目录/manifest和原来源checker/场景DTO、真实读取源身份；新Rust回执另标profile，原partial/blocked状态不升级：data
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub data: serde_json::Value,
    /// 保留原书籍目录/manifest和原来源checker/场景DTO、真实读取源身份；新Rust回执另标profile，原partial/blocked状态不升级：schema
    pub schema: String,
    /// 保留原书籍目录/manifest和原来源checker/场景DTO、真实读取源身份；新Rust回执另标profile，原partial/blocked状态不升级：source_sha256
    pub source_sha256: String,
}
impl SemanticBookCatalogResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticBookCatalogResult.schema:required".into()); }
        if self.source_sha256.len() != 64 || !self.source_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticBookCatalogResult.source_sha256:sha256".into()); }

        if !["semantica.book-catalog-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticBookCatalogResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Exact caller-bound logical source asset; identity is not provider authorization
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticContentAsset3 {
    /// Complete expected source byte count
    pub bytes: u64,
    /// Exact existing SourceProvider logical read ID; no new root or registry
    pub logical_id: String,
    /// Complete expected source bytes SHA256
    pub sha256: String,
}
impl SemanticContentAsset3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.logical_id.trim().is_empty() { failures.push("SemanticContentAsset3.logical_id:required".into()); }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticContentAsset3.sha256:sha256".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One operation with exactly its matching optional payload; invalid combinations reject the whole request before execution
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogAction {
    /// Unique ASCII alphanumeric/underscore/hyphen identifier,1..64 bytes; does not rename engineering objects
    pub action_id: String,
    /// Required only for add_fact: original string or dictionary; scalar kinds reject at admission
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub fact: Option<serde_json::Value>,
    /// Required only for load_graph; nodes/edges are immutable dictionary snapshots, visited edges first then nodes. Valid preceding records survive a later semantic error.
    pub graph: Option<SemanticDatalogGraph>,
    /// Original operation or non-mutating state observation
    pub operation: String,
    /// Required only for query; original automatic inference precedes query syntax validation; no second query evaluator
    pub query: Option<SemanticDatalogQuery>,
    /// Required only for add_rule; original bounded safe positive syntax is checked when the action runs
    pub rule: Option<String>,
}
impl SemanticDatalogAction {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.action_id.trim().is_empty() { failures.push("SemanticDatalogAction.action_id:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticDatalogAction.operation:required".into()); }

        if !["add_fact","add_rule","clear","derive","load_graph","query","snapshot",].contains(&self.operation.as_str()) { failures.push("SemanticDatalogAction.operation:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Lowerable work/output-count ceilings; all fields must be positive and at most the existing DatalogLimits defaults
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogBudget {
    /// Caller bound from1 through100000. Match attempts apply separately to one closure and at most one query; no aggregate or wall-clock claim.
    pub max_facts: u64,
    /// Caller bound from1 through1024. Match attempts apply separately to one closure and at most one query; no aggregate or wall-clock claim.
    pub max_iterations: u64,
    /// Caller bound from1 through10000000. Match attempts apply separately to one closure and at most one query; no aggregate or wall-clock claim.
    pub max_match_attempts: u64,
    /// Caller bound from1 through10000. Match attempts apply separately to one closure and at most one query; no aggregate or wall-clock claim.
    pub max_query_rows: u64,
}
impl SemanticDatalogBudget {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One ground predicate and ordered arguments from the existing fact-set owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogFact {
    /// Exact ordered constant terms; boundaries retained without reparsing a formatted fact
    pub args: Vec<String>,
    /// Actual predicate, including original empty dictionary predicate; no conversion to RDF or a physical engineering assertion
    pub predicate: String,
}
impl SemanticDatalogFact {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit snapshot of ContextGraph find_nodes/find_edges dictionary records, retaining input order and values
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogGraph {
    /// Dictionary edge records; load original edges before nodes
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub edges: Vec<serde_json::Value>,
    /// Dictionary node records, including unknown dictionaries; no Python object inspection
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub nodes: Vec<serde_json::Value>,
}
impl SemanticDatalogGraph {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Seed facts followed by original graph import then rule closure and optional query; old datalog/1 unchanged
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogGraphRequest {
    /// Original lowerable bounds retained; combined facts+edges+nodes at most4096, full request1MiB, complete output16MiB
    pub budget: SemanticDatalogBudget,
    /// Original string/dictionary seed facts loaded before graph
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub facts: Vec<serde_json::Value>,
    /// Full node/edge snapshot, never a path or mutable graph handle
    pub graph: SemanticDatalogGraph,
    /// Optional original projection; ground query remains empty projection
    pub query: Option<SemanticDatalogQuery>,
    /// At most256 original bounded safe positive rules
    pub rules: Vec<String>,
    /// Independent graph wire contract
    pub schema: String,
}
impl SemanticDatalogGraphRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticDatalogGraphRequest.schema:required".into()); }

        if !["semantica.datalog-graph-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticDatalogGraphRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Whole completed closure and graph insertion count; no retained state or partial success on failure
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogGraphResult {
    /// Entire unique typed fact closure; no string reparse or truncation
    pub closure: Vec<SemanticDatalogFact>,
    /// True only after fixed point, query and full output bound; does not approve an engineering relation
    pub complete: bool,
    /// New facts after inference, excluding both seed sources
    pub derived_fact_count: u64,
    /// Raw edge entry count including skipped or duplicate dictionaries
    pub graph_edge_count: u64,
    /// Distinct new facts added by original graph importer after seed facts; unknown and duplicate entries do not increase this count
    pub graph_facts_added: u64,
    /// Raw node entry count including skipped or duplicate dictionaries
    pub graph_node_count: u64,
    /// Actual inference work, excludes query and serialization
    pub inference_match_attempts: u64,
    /// Original seed facts insertion flags only, before graph load
    pub input_inserted: Vec<bool>,
    /// Actual Rust inference iterations, not original Python iteration count
    pub iterations: u64,
    /// Existing bounded graph import and Datalog execution
    pub profile: String,
    /// Original sorted unique binding projection; null means not requested
    pub query_rows: Option<Vec<std::collections::BTreeMap<String, String>>>,
    /// Whole canonical typed graph request before deduplication/normalization, including graph order and budgets
    pub request_sha256: String,
    /// Successful graph wire result
    pub schema: String,
    /// Distinct facts after both explicit seeds and graph, before inference
    pub seed_fact_count: u64,
}
impl SemanticDatalogGraphResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.profile.trim().is_empty() { failures.push("SemanticDatalogGraphResult.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticDatalogGraphResult.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticDatalogGraphResult.schema:required".into()); }

        if !["semantica.positive-function-free-datalog-graph/1",].contains(&self.profile.as_str()) { failures.push("SemanticDatalogGraphResult.profile:status".into()); }
        if !["semantica.datalog-graph-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticDatalogGraphResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One optional binding projection over the completed closure, using the original variable naming and ground-query behavior
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogQuery {
    /// Original prebindings; opaque keys/values and original case mapping are retained
    pub bindings: std::collections::BTreeMap<String, String>,
    /// Original query text; a fully ground query returns [] rather than an ASK boolean
    pub pattern: String,
}
impl SemanticDatalogQuery {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.pattern.trim().is_empty() { failures.push("SemanticDatalogQuery.pattern:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Stateless input to the existing DatalogReasoner, with explicit bounded inference and optional single query
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogRequest {
    /// Existing algorithm limits can only be lowered. Full typed request <=1MiB, result <=16MiB; unchanged per-term and working-set bounds also apply.
    pub budget: SemanticDatalogBudget,
    /// At most4096 original string/dictionary inputs; other JSON kinds reject. Unknown dictionaries retain original skip; complex dictionary terms remain explicitly unsupported.
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub facts: Vec<serde_json::Value>,
    /// Absent or null means no query; returned None is distinct from an executed query with zero rows
    pub query: Option<SemanticDatalogQuery>,
    /// At most256 complete safe positive rules; original Rust safety, trailing-token and head-variable checks remain
    pub rules: Vec<String>,
    /// Explicit new wire identity; never dispatch old textual rules as Datalog
    pub schema: String,
}
impl SemanticDatalogRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticDatalogRequest.schema:required".into()); }

        if !["semantica.datalog-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticDatalogRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete finite closure and optional query, bound to the exact typed request; errors never produce a partial success
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogResult {
    /// Entire sorted unique fact set including seeds; never truncated to match an output budget
    pub closure: Vec<SemanticDatalogFact>,
    /// True only after actual fixed point AND optional query complete and result fits. No truncated result or implicit engineering approval.
    pub complete: bool,
    /// Closure size minus seed fact count; this is logical derivation, not new physical evidence
    pub derived_fact_count: u64,
    /// Actual closure match count from the Rust algorithm; excludes optional query matching, serialization and wall time
    pub inference_match_attempts: u64,
    /// One original Rust add_fact insertion observation per input. False means duplicate or skipped unknown dictionary; it does not certify input validity or engineer truth.
    pub input_inserted: Vec<bool>,
    /// Actual completed closure rounds, including the final empty-delta check when it executes; not Python iteration-count equivalence
    pub iterations: u64,
    /// Actually executed finite Rust Datalog profile
    pub profile: String,
    /// Original variable projection with sorted unique bindings; None means not requested, [] means empty projection/result
    pub query_rows: Option<Vec<std::collections::BTreeMap<String, String>>>,
    /// Canonical complete typed request identity before normalization/deduplication; array order and budget remain bound
    pub request_sha256: String,
    /// Explicit successful result identity
    pub schema: String,
    /// Number of distinct inserted ground facts before inference
    pub seed_fact_count: u64,
}
impl SemanticDatalogResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.profile.trim().is_empty() { failures.push("SemanticDatalogResult.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticDatalogResult.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticDatalogResult.schema:required".into()); }

        if !["semantica.positive-function-free-datalog/1",].contains(&self.profile.as_str()) { failures.push("SemanticDatalogResult.profile:status".into()); }
        if !["semantica.datalog-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticDatalogResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// At most32 actions over one engine in this request; no stored session or cross-request effects
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogSessionRequest {
    /// Ordered actions; empty list is valid. Total fact plus graph records at most4096, rules at most256; clear does not reset input quotas.
    pub actions: Vec<SemanticDatalogAction>,
    /// Existing lowerable bounds apply to each inference and each query separately, with at most32 actions. No aggregate match or wall-clock claim; full input1MiB and complete output16MiB remain hard bounds.
    pub budget: SemanticDatalogBudget,
    /// Explicit execution policy after an action error; malformed envelope always rejects before any action
    pub on_error: String,
    /// Explicit new protocol
    pub schema: String,
}
impl SemanticDatalogSessionRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.on_error.trim().is_empty() { failures.push("SemanticDatalogSessionRequest.on_error:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticDatalogSessionRequest.schema:required".into()); }

        if !["continue","stop",].contains(&self.on_error.as_str()) { failures.push("SemanticDatalogSessionRequest.on_error:status".into()); }
        if !["semantica.datalog-session-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticDatalogSessionRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete bounded history envelope; successful recording is separate from successful actions, fixed point or engineering acceptance
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogSessionResult {
    /// True only when all requested actions completed; false for any failure/skips, including expected negative controls. Never engineering approval.
    pub all_actions_succeeded: bool,
    /// Same engine after last requested action; not written to Project or retained for a subsequent call
    pub final_state: SemanticDatalogState,
    /// Explicit source ordering and per-record import with existing finite Rust safeguards
    pub profile: String,
    /// SHA256 of complete canonical typed request including ids, order, policy, inputs and budget
    pub request_sha256: String,
    /// Explicit result identity
    pub schema: String,
    /// Exactly one result per supplied action; skip placeholders preserve the requested sequence
    pub steps: Vec<SemanticDatalogStep>,
}
impl SemanticDatalogSessionResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.profile.trim().is_empty() { failures.push("SemanticDatalogSessionResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticDatalogSessionResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticDatalogSessionResult.schema:required".into()); }

        if !["semantica.bounded-source-datalog-session/1",].contains(&self.profile.as_str()) { failures.push("SemanticDatalogSessionResult.profile:status".into()); }
        if !["semantica.datalog-session-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticDatalogSessionResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Exact non-mutating observation of the same engine after an action; false derived does not mean action failure
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogState {
    /// Actual fixed-point flag; adding a new fact or rule invalidates it, duplicates/unknown dictionaries do not; no-rules query need not derive
    pub derived: bool,
    /// Complete current fact set including retained derived facts; no truncation or inferred object identity
    pub facts: Vec<SemanticDatalogFact>,
    /// Actual rule count, including original duplicate rules; clear resets it to zero
    pub rule_count: u64,
}
impl SemanticDatalogState {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One ordered action outcome with actual before and after states, including failures and explicitly unexecuted later actions
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatalogStep {
    /// Original request action id
    pub action_id: String,
    /// Only failed action; never auto-retried. Query failure may follow completed inference; derive budget failure keeps existing transactional Rust state.
    pub failure: Option<SemanticRuntimeFailure>,
    /// Only successful graph load: unique facts added relative to its prior state. Failed load returns no count; before/after state still shows retained prefix.
    pub graph_facts_added: Option<u64>,
    /// Only successful add_fact: true for one new fact, false for duplicate or skipped unknown dictionary. This is a Rust observation; original add_fact returns None.
    pub inserted: Option<bool>,
    /// Original requested operation
    pub operation: String,
    /// Only successful query; sorted set of bindings. None is distinct from empty projection; ground query [] is not ASK.
    pub query_rows: Option<Vec<std::collections::BTreeMap<String, String>>>,
    /// Read after actual completion/error, or unchanged for skipped actions
    pub state_after: SemanticDatalogState,
    /// Read before the action, with no implicit derive
    pub state_before: SemanticDatalogState,
    /// completed means operation returned; failed retains its error and current state; skipped means not called after stop
    pub status: String,
}
impl SemanticDatalogStep {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.action_id.trim().is_empty() { failures.push("SemanticDatalogStep.action_id:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticDatalogStep.operation:required".into()); }
        if self.status.trim().is_empty() { failures.push("SemanticDatalogStep.status:required".into()); }

        if !["add_fact","add_rule","clear","derive","load_graph","query","snapshot",].contains(&self.operation.as_str()) { failures.push("SemanticDatalogStep.operation:status".into()); }
        if !["completed","failed","skipped",].contains(&self.status.as_str()) { failures.push("SemanticDatalogStep.status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 比较同一规范化身份下两个快照；不将跨算法摘要视为同一执行
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatasetDiff {
    /// 新快照独有的规范N-Quads行，空集合保留
    pub added: Vec<String>,
    /// 新快照规范内容身份
    pub after_sha256: String,
    /// 原快照规范内容身份
    pub before_sha256: String,
    /// 双方共同且明确的规范化算法身份
    pub canonicalization: String,
    /// 原快照独有的规范N-Quads行，blank node跨变更标签不承诺稳定
    pub removed: Vec<String>,
}
impl SemanticDatasetDiff {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.after_sha256.len() != 64 || !self.after_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticDatasetDiff.after_sha256:sha256".into()); }
        if self.before_sha256.len() != 64 || !self.before_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticDatasetDiff.before_sha256:sha256".into()); }
        if self.canonicalization.trim().is_empty() { failures.push("SemanticDatasetDiff.canonicalization:required".into()); }

        if !["semantica.rdfc-1.0-sha256/1",].contains(&self.canonicalization.as_str()) { failures.push("SemanticDatasetDiff.canonicalization:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 工程本体分别规定原兼容term身份、当前RDF1.1内容身份、源字节观察与采用责任；此合同仅诊断同一Session实际身份
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatasetIdentityDiagnostic {
    /// 仅观察当前仍保留的语言拼写；去重前拼写及全部其他不稳定性未覆盖
    pub diagnostic_scope: String,
    /// 完整原term键分组的保留拼写别名，空集合不是稳定证明
    pub language_case_aliases: Vec<SemanticRdfLibLanguageCaseAlias>,
    /// 同一Session现有原RDFLib算法输出，保留plain/typed区别和旧全部字段；hash是该算法观察
    pub legacy_snapshot: SemanticRdfLibDatasetSnapshot,
    /// 发现保留大小写别名时具体报告，否则稳定性仍未建立；任何结果都不授予旧算法稳定资格
    pub legacy_stability_assessment: String,
    /// 现有projection从实际load保留的原字节摘要，保持原序；受supported mutation影响，不是完整变化ledger或真实性证明
    pub loaded_source_byte_sha256: Vec<String>,
    /// 同一Session实际RDF1.1内容的原RDFC1.0算法输出，与旧term算法分别标识
    pub rdf11_snapshot: SemanticDatasetSnapshot,
    /// 现有Session唯一mutation revision；两快照必须属于此实际值
    pub revision: u64,
    /// 明确的新诊断profile，不重标旧快照或旧通过
    pub schema: String,
}
impl SemanticDatasetIdentityDiagnostic {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.diagnostic_scope.trim().is_empty() { failures.push("SemanticDatasetIdentityDiagnostic.diagnostic_scope:required".into()); }
        if self.legacy_stability_assessment.trim().is_empty() { failures.push("SemanticDatasetIdentityDiagnostic.legacy_stability_assessment:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticDatasetIdentityDiagnostic.schema:required".into()); }

        if !["retained_projection_language_case/1",].contains(&self.diagnostic_scope.as_str()) { failures.push("SemanticDatasetIdentityDiagnostic.diagnostic_scope:status".into()); }
        if !["known_language_case_alias","not_established",].contains(&self.legacy_stability_assessment.as_str()) { failures.push("SemanticDatasetIdentityDiagnostic.legacy_stability_assessment:status".into()); }
        if !["semantica.dataset-identity-diagnostic/1",].contains(&self.schema.as_str()) { failures.push("SemanticDatasetIdentityDiagnostic.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一DatasetSession的显式资源规格选择，不改变工程事实或RDF身份
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatasetResourceSelection {
    /// 有限资源规格标识；50k规格为50000节点/50000边、500000quad、128MiB数据集
    pub profile: String,
    /// 有限资源规格标识；50k规格为50000节点/50000边、500000quad、128MiB数据集
    pub schema: String,
}
impl SemanticDatasetResourceSelection {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.profile.trim().is_empty() { failures.push("SemanticDatasetResourceSelection.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticDatasetResourceSelection.schema:required".into()); }

        if !["semantica.context-50k/1","semantica.dataset-default/1",].contains(&self.profile.as_str()) { failures.push("SemanticDatasetResourceSelection.profile:allowed_values".into()); }
        if !["semantica.dataset-resource-selection/1",].contains(&self.schema.as_str()) { failures.push("SemanticDatasetResourceSelection.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 保存完整RDF数据集的版本化规范身份；不改签旧RDFLib快照
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticDatasetSnapshot {
    /// 标准规范N-Quads原文，包含named graph和blank node；空图合法
    pub canonical_nquads: String,
    /// 明确使用RDFC1.0 SHA256，与旧RDFLib算法独立
    pub canonicalization: String,
    /// 规范N-Quads UTF8字节摘要
    pub dataset_sha256: String,
    /// 去重后四元组数，空图为零
    pub quad_count: u64,
    /// 当前运行时mutation修订号，不属于语义内容摘要
    pub revision: u64,
    /// 快照协议版本
    pub schema: String,
}
impl SemanticDatasetSnapshot {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.canonicalization.trim().is_empty() { failures.push("SemanticDatasetSnapshot.canonicalization:required".into()); }
        if self.dataset_sha256.len() != 64 || !self.dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticDatasetSnapshot.dataset_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticDatasetSnapshot.schema:required".into()); }

        if !["semantica.rdfc-1.0-sha256/1",].contains(&self.canonicalization.as_str()) { failures.push("SemanticDatasetSnapshot.canonicalization:status".into()); }
        if !["semantica.dataset-snapshot/1",].contains(&self.schema.as_str()) { failures.push("SemanticDatasetSnapshot.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 逐条实际输入和观察；credential与budget权威由上层注入
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEmbeddingAttempt {
    /// 每次真实请求及安全执行观察：failure
    pub failure: Option<SemanticProviderFailure>,
    /// 每次真实请求及安全执行观察：index
    pub index: u64,
    /// 每次真实请求及安全执行观察：request
    pub request: SemanticEmbeddingRequest,
    /// 每次真实请求及安全执行观察：reserved
    pub reserved: bool,
    /// 每次真实请求及安全执行观察：result
    pub result: Option<SemanticProviderResult>,
}
impl SemanticEmbeddingAttempt {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Err(errors) = self.request.validate() { failures.extend(errors); }


        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原ProviderStore逐条batch逻辑输入；预算仍归Harness
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEmbeddingBatchRequest {
    /// 原ProviderStore逐条batch逻辑输入；预算仍归Harness；字段 requests
    pub requests: Vec<SemanticEmbeddingRequest>,
    /// 原ProviderStore逐条batch逻辑输入；预算仍归Harness；字段 schema
    pub schema: String,
}
impl SemanticEmbeddingBatchRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticEmbeddingBatchRequest.schema:required".into()); }

        if !["semantica.embedding-batch-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticEmbeddingBatchRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 逐条真实回执；首次失败无partial batch vector但保留先前实际观察
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEmbeddingBatchResult {
    /// 每条实际逻辑输入、reserved状态及结果或安全失败；不把reservation冒称网络成功
    pub attempts: Vec<SemanticEmbeddingAttempt>,
    /// 逐条真实回执；首次失败无partial batch vector但保留先前实际观察；字段 execution
    pub execution: String,
    /// 逐条真实回执；首次失败无partial batch vector但保留先前实际观察；字段 failed_index
    pub failed_index: Option<u64>,
    /// 逐条真实回执；首次失败无partial batch vector但保留先前实际观察；字段 failure
    pub failure: Option<SemanticProviderFailure>,
    /// 逐条真实回执；首次失败无partial batch vector但保留先前实际观察；字段 logical_request_sha256
    pub logical_request_sha256: String,
    /// 逐条真实回执；首次失败无partial batch vector但保留先前实际观察；字段 output
    pub output: Option<SemanticEmbeddingTensor>,
    /// 逐条真实回执；首次失败无partial batch vector但保留先前实际观察；字段 results
    pub results: Vec<SemanticProviderResult>,
    /// 逐条真实回执；首次失败无partial batch vector但保留先前实际观察；字段 schema
    pub schema: String,
}
impl SemanticEmbeddingBatchResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.execution.trim().is_empty() { failures.push("SemanticEmbeddingBatchResult.execution:required".into()); }
        if self.logical_request_sha256.len() != 64 || !self.logical_request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticEmbeddingBatchResult.logical_request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticEmbeddingBatchResult.schema:required".into()); }

        if !["completed","failed",].contains(&self.execution.as_str()) { failures.push("SemanticEmbeddingBatchResult.execution:status".into()); }
        if !["semantica.embedding-batch-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticEmbeddingBatchResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原五pool的有限2D provided_vectors算法输入；不声称已生成semantic embedding
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEmbeddingPoolRequest {
    /// 原五pool的有限2D provided_vectors算法输入；不声称已生成semantic embedding；字段 chunk_size
    pub chunk_size: Option<u64>,
    /// 原五pool的有限2D provided_vectors算法输入；不声称已生成semantic embedding；字段 embeddings
    pub embeddings: Vec<Vec<f64>>,
    /// 原五pool的有限2D provided_vectors算法输入；不声称已生成semantic embedding；字段 schema
    pub schema: String,
    /// 原五pool的有限2D provided_vectors算法输入；不声称已生成semantic embedding；字段 strategy
    pub strategy: String,
}
impl SemanticEmbeddingPoolRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticEmbeddingPoolRequest.schema:required".into()); }
        if self.strategy.trim().is_empty() { failures.push("SemanticEmbeddingPoolRequest.strategy:required".into()); }

        if !["semantica.embedding-pool-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticEmbeddingPoolRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原五pool有限2D算术结果；与HF/SentenceTransformer推理验收分开
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEmbeddingPoolResult {
    /// 原五pool有限2D算术结果；与HF/SentenceTransformer推理验收分开；字段 execution
    pub execution: String,
    /// 原五pool有限2D算术结果；与HF/SentenceTransformer推理验收分开；字段 output
    pub output: Vec<f64>,
    /// 原五pool有限2D算术结果；与HF/SentenceTransformer推理验收分开；字段 profile
    pub profile: String,
    /// 原五pool有限2D算术结果；与HF/SentenceTransformer推理验收分开；字段 request_sha256
    pub request_sha256: String,
    /// 原五pool有限2D算术结果；与HF/SentenceTransformer推理验收分开；字段 schema
    pub schema: String,
    /// 原五pool有限2D算术结果；与HF/SentenceTransformer推理验收分开；字段 strategy
    pub strategy: String,
}
impl SemanticEmbeddingPoolResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.execution.trim().is_empty() { failures.push("SemanticEmbeddingPoolResult.execution:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticEmbeddingPoolResult.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticEmbeddingPoolResult.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticEmbeddingPoolResult.schema:required".into()); }
        if self.strategy.trim().is_empty() { failures.push("SemanticEmbeddingPoolResult.strategy:required".into()); }

        if !["completed",].contains(&self.execution.as_str()) { failures.push("SemanticEmbeddingPoolResult.execution:status".into()); }
        if !["semantica.provided-vectors-finite-f64-2d/1",].contains(&self.profile.as_str()) { failures.push("SemanticEmbeddingPoolResult.profile:status".into()); }
        if !["semantica.embedding-pool-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticEmbeddingPoolResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原OpenAIStore单文本实际请求；凭据从上层同pool注入
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEmbeddingRequest {
    /// 原OpenAIStore单文本实际请求；凭据从上层同pool注入；字段 custom_endpoint
    pub custom_endpoint: bool,
    /// 原OpenAIStore单文本实际请求；凭据从上层同pool注入；字段 input
    pub input: String,
    /// 原OpenAIStore单文本实际请求；凭据从上层同pool注入；字段 interface_profile
    pub interface_profile: String,
    /// 原OpenAIStore单文本实际请求；凭据从上层同pool注入；字段 model
    pub model: Option<String>,
    /// 原OpenAIStore单文本实际请求；凭据从上层同pool注入；字段 options
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub options: serde_json::Value,
    /// 原OpenAIStore单文本实际请求；凭据从上层同pool注入；字段 provider
    pub provider: String,
    /// 原OpenAIStore单文本实际请求；凭据从上层同pool注入；字段 schema
    pub schema: String,
}
impl SemanticEmbeddingRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.interface_profile.trim().is_empty() { failures.push("SemanticEmbeddingRequest.interface_profile:required".into()); }
        if self.provider.trim().is_empty() { failures.push("SemanticEmbeddingRequest.provider:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticEmbeddingRequest.schema:required".into()); }

        if !["provider_store",].contains(&self.interface_profile.as_str()) { failures.push("SemanticEmbeddingRequest.interface_profile:status".into()); }
        if !["openai",].contains(&self.provider.as_str()) { failures.push("SemanticEmbeddingRequest.provider:status".into()); }
        if !["semantica.embedding-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticEmbeddingRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 无损JSON向量编码；保留float32舍入、负零与非有限值，不替代模型推理
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEmbeddingTensor {
    /// 无损JSON向量编码；保留float32舍入、负零与非有限值，不替代模型推理；字段 bits
    pub bits: Vec<u64>,
    /// 无损JSON向量编码；保留float32舍入、负零与非有限值，不替代模型推理；字段 dtype
    pub dtype: String,
    /// 无损JSON向量编码；保留float32舍入、负零与非有限值，不替代模型推理；字段 shape
    pub shape: Vec<u64>,
}
impl SemanticEmbeddingTensor {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.dtype.trim().is_empty() { failures.push("SemanticEmbeddingTensor.dtype:required".into()); }

        if !["float32","float64",].contains(&self.dtype.as_str()) { failures.push("SemanticEmbeddingTensor.dtype:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原完整JSON数据内容与本次Rust执行观察；内容完整性不授予来源批准、行业晋升或工程采用
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExecutionProvenanceBundle {
    /// 原执行provenance字段 bindings；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub bindings: serde_json::Value,
    /// 原执行provenance字段 bundle_id；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub bundle_id: String,
    /// 原执行provenance字段 bundle_sha256；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub bundle_sha256: String,
    /// 原执行provenance字段 generated_at；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub generated_at: serde_json::Value,
    /// 原执行provenance字段 records；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub records: Vec<SemanticExecutionProvenanceEntry>,
    /// 原执行provenance字段 schema_version；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub schema_version: String,
}
impl SemanticExecutionProvenanceBundle {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.bundle_id.trim().is_empty() { failures.push("SemanticExecutionProvenanceBundle.bundle_id:required".into()); }
        if self.bundle_sha256.len() != 64 || !self.bundle_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticExecutionProvenanceBundle.bundle_sha256:sha256".into()); }
        if self.schema_version.trim().is_empty() { failures.push("SemanticExecutionProvenanceBundle.schema_version:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原完整JSON数据内容与本次Rust执行观察；内容完整性不授予来源批准、行业晋升或工程采用
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExecutionProvenanceEntry {
    /// 原执行provenance字段 acted_on_behalf_of；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub acted_on_behalf_of: serde_json::Value,
    /// 原执行provenance字段 activity_ended_at_time；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub activity_ended_at_time: serde_json::Value,
    /// 原执行provenance字段 activity_id；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub activity_id: serde_json::Value,
    /// 原执行provenance字段 activity_started_at_time；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub activity_started_at_time: serde_json::Value,
    /// 原执行provenance字段 agent_id；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub agent_id: serde_json::Value,
    /// 原执行provenance字段 agent_type；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub agent_type: serde_json::Value,
    /// 原执行provenance字段 bundle_id；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub bundle_id: serde_json::Value,
    /// 原执行provenance字段 checksum；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub checksum: serde_json::Value,
    /// 原执行provenance字段 confidence；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub confidence: serde_json::Value,
    /// 原执行provenance字段 credibility；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub credibility: serde_json::Value,
    /// 原执行provenance字段 derived_from_id；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub derived_from_id: serde_json::Value,
    /// 原执行provenance字段 end_index；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub end_index: serde_json::Value,
    /// 原执行provenance字段 entity_id；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub entity_id: serde_json::Value,
    /// 原执行provenance字段 entity_type；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub entity_type: serde_json::Value,
    /// 原执行provenance字段 first_seen；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub first_seen: serde_json::Value,
    /// 原执行provenance字段 informed_by_activities；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub informed_by_activities: serde_json::Value,
    /// 原执行provenance字段 invalidated；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub invalidated: serde_json::Value,
    /// 原执行provenance字段 invalidated_at_time；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub invalidated_at_time: serde_json::Value,
    /// 原执行provenance字段 invalidated_by；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub invalidated_by: serde_json::Value,
    /// 原执行provenance字段 invalidation_reason；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub invalidation_reason: serde_json::Value,
    /// 原执行provenance字段 is_automated；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub is_automated: serde_json::Value,
    /// 原执行provenance字段 last_updated；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub last_updated: serde_json::Value,
    /// 原执行provenance字段 metadata；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub metadata: serde_json::Value,
    /// 原执行provenance字段 parent_entity_id；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub parent_entity_id: serde_json::Value,
    /// 原执行provenance字段 previous_checksum；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub previous_checksum: serde_json::Value,
    /// 原执行provenance字段 previous_version_id；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub previous_version_id: serde_json::Value,
    /// 原执行provenance字段 revision_type；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub revision_type: serde_json::Value,
    /// 原执行provenance字段 role；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub role: serde_json::Value,
    /// 原执行provenance字段 sequence_id；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub sequence_id: serde_json::Value,
    /// 原执行provenance字段 source_document；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub source_document: serde_json::Value,
    /// 原执行provenance字段 source_location；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub source_location: serde_json::Value,
    /// 原执行provenance字段 source_quote；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub source_quote: serde_json::Value,
    /// 原执行provenance字段 start_index；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub start_index: serde_json::Value,
    /// 原执行provenance字段 supersedes；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub supersedes: serde_json::Value,
    /// 原执行provenance字段 timestamp；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub timestamp: serde_json::Value,
    /// 原执行provenance字段 used_entities；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub used_entities: serde_json::Value,
    /// 原执行provenance字段 valid_from；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub valid_from: serde_json::Value,
    /// 原执行provenance字段 valid_until；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub valid_until: serde_json::Value,
    /// 原执行provenance字段 version；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub version: serde_json::Value,
}
impl SemanticExecutionProvenanceEntry {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原完整JSON数据内容与本次Rust执行观察；内容完整性不授予来源批准、行业晋升或工程采用
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExecutionProvenanceObservation {
    /// 原执行provenance字段 engineering_adoption；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub engineering_adoption: String,
    /// 原执行provenance字段 profile；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub profile: String,
    /// 原执行provenance字段 receipt；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub receipt: SemanticExecutionProvenanceReceipt,
    /// 原执行provenance字段 release_status；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub release_status: String,
    /// 原执行provenance字段 runtime_origin；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub runtime_origin: String,
    /// 原执行provenance字段 snapshot_profile；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub snapshot_profile: String,
    /// 原执行provenance字段 source_authenticity；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub source_authenticity: String,
}
impl SemanticExecutionProvenanceObservation {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.engineering_adoption.trim().is_empty() { failures.push("SemanticExecutionProvenanceObservation.engineering_adoption:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticExecutionProvenanceObservation.profile:required".into()); }
        if self.release_status.trim().is_empty() { failures.push("SemanticExecutionProvenanceObservation.release_status:required".into()); }
        if self.runtime_origin.trim().is_empty() { failures.push("SemanticExecutionProvenanceObservation.runtime_origin:required".into()); }
        if self.snapshot_profile.trim().is_empty() { failures.push("SemanticExecutionProvenanceObservation.snapshot_profile:required".into()); }
        if self.source_authenticity.trim().is_empty() { failures.push("SemanticExecutionProvenanceObservation.source_authenticity:required".into()); }

        if !["unknown",].contains(&self.engineering_adoption.as_str()) { failures.push("SemanticExecutionProvenanceObservation.engineering_adoption:status".into()); }
        if !["semantica.execution-provenance-rust-observation/1",].contains(&self.profile.as_str()) { failures.push("SemanticExecutionProvenanceObservation.profile:status".into()); }
        if !["blocked",].contains(&self.release_status.as_str()) { failures.push("SemanticExecutionProvenanceObservation.release_status:status".into()); }
        if !["unknown",].contains(&self.runtime_origin.as_str()) { failures.push("SemanticExecutionProvenanceObservation.runtime_origin:status".into()); }
        if !["unknown",].contains(&self.source_authenticity.as_str()) { failures.push("SemanticExecutionProvenanceObservation.source_authenticity:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原完整JSON数据内容与本次Rust执行观察；内容完整性不授予来源批准、行业晋升或工程采用
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExecutionProvenanceReceipt {
    /// 原执行provenance字段 asset_hashes；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub asset_hashes: serde_json::Value,
    /// 原执行provenance字段 capability_report；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub capability_report: SemanticExecutionProvenanceReport,
    /// 原执行provenance字段 chapter_contract_sha256；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub chapter_contract_sha256: String,
    /// 原执行provenance字段 cq_report；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub cq_report: SemanticExecutionProvenanceReport,
    /// 原执行provenance字段 created_at；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub created_at: serde_json::Value,
    /// 原执行provenance字段 dataset_quad_count；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub dataset_quad_count: serde_json::Value,
    /// 原执行provenance字段 dataset_revision；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub dataset_revision: serde_json::Value,
    /// 原执行provenance字段 dataset_sha256；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub dataset_sha256: String,
    /// 原执行provenance字段 oracle_report；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub oracle_report: SemanticExecutionProvenanceReport,
    /// 原执行provenance字段 output_hashes；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub output_hashes: serde_json::Value,
    /// 原执行provenance字段 package_digest；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub package_digest: String,
    /// 原执行provenance字段 package_id；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub package_id: String,
    /// 原执行provenance字段 package_version；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub package_version: String,
    /// 原执行provenance字段 provenance_bundle；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub provenance_bundle: SemanticExecutionProvenanceBundle,
    /// 原执行provenance字段 receipt_sha256；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub receipt_sha256: String,
    /// 原执行provenance字段 runtime_artifact_sha256；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub runtime_artifact_sha256: String,
    /// 原执行provenance字段 runtime_commit；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub runtime_commit: String,
    /// 原执行provenance字段 runtime_version；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub runtime_version: String,
    /// 原执行provenance字段 schema_version；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub schema_version: String,
    /// 原执行provenance字段 shacl_report；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub shacl_report: SemanticExecutionProvenanceReport,
}
impl SemanticExecutionProvenanceReceipt {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.chapter_contract_sha256.trim().is_empty() { failures.push("SemanticExecutionProvenanceReceipt.chapter_contract_sha256:required".into()); }
        if self.dataset_sha256.trim().is_empty() { failures.push("SemanticExecutionProvenanceReceipt.dataset_sha256:required".into()); }
        if self.package_digest.trim().is_empty() { failures.push("SemanticExecutionProvenanceReceipt.package_digest:required".into()); }
        if self.package_id.trim().is_empty() { failures.push("SemanticExecutionProvenanceReceipt.package_id:required".into()); }
        if self.package_version.trim().is_empty() { failures.push("SemanticExecutionProvenanceReceipt.package_version:required".into()); }
        if self.receipt_sha256.len() != 64 || !self.receipt_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticExecutionProvenanceReceipt.receipt_sha256:sha256".into()); }
        if self.runtime_artifact_sha256.trim().is_empty() { failures.push("SemanticExecutionProvenanceReceipt.runtime_artifact_sha256:required".into()); }
        if self.runtime_commit.trim().is_empty() { failures.push("SemanticExecutionProvenanceReceipt.runtime_commit:required".into()); }
        if self.runtime_version.trim().is_empty() { failures.push("SemanticExecutionProvenanceReceipt.runtime_version:required".into()); }
        if self.schema_version.trim().is_empty() { failures.push("SemanticExecutionProvenanceReceipt.schema_version:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原完整JSON数据内容与本次Rust执行观察；内容完整性不授予来源批准、行业晋升或工程采用
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExecutionProvenanceReport {
    /// 原执行provenance字段 kind；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub kind: String,
    /// 原执行provenance字段 payload；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub payload: serde_json::Value,
    /// 原执行provenance字段 sha256；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub sha256: String,
    /// 原执行provenance字段 status；Entry的JSON值保留原dataclass未强制注解的纯JSON域
    pub status: String,
}
impl SemanticExecutionProvenanceReport {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.kind.trim().is_empty() { failures.push("SemanticExecutionProvenanceReport.kind:required".into()); }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticExecutionProvenanceReport.sha256:sha256".into()); }
        if self.status.trim().is_empty() { failures.push("SemanticExecutionProvenanceReport.status:required".into()); }

        if !["blocked","failed","passed",].contains(&self.status.as_str()) { failures.push("SemanticExecutionProvenanceReport.status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 显式修正后的图上因果边距离计算；历史同名ID路由不改签
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerCausalDistanceQuery {
    /// 计算问题版本
    pub schema: String,
    /// 显式原起点值；Some空串为不可达，None拒绝为缺参数
    pub source_id: Option<String>,
    /// 显式原终点值；Some空串为不可达，None拒绝为缺参数
    pub target_id: Option<String>,
}
impl SemanticExplorerCausalDistanceQuery {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerCausalDistanceQuery.schema:required".into()); }

        if !["semantica.explorer-causal-distance-query/2",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerCausalDistanceQuery.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 显式修正后的图上因果边距离计算；历史同名ID路由不改签
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerCausalDistanceRequest {
    /// 原数据摘要
    pub expected_dataset_sha256: String,
    /// 原数据修订
    pub expected_revision: u64,
    /// 明确调用因果边计算的问题
    pub query: SemanticExplorerCausalDistanceQuery,
    /// 有源服务请求版本
    pub schema: String,
}
impl SemanticExplorerCausalDistanceRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.expected_dataset_sha256.trim().is_empty() { failures.push("SemanticExplorerCausalDistanceRequest.expected_dataset_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerCausalDistanceRequest.schema:required".into()); }

        if !["semantica.explorer-causal-distance-request/2",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerCausalDistanceRequest.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 显式修正后的图上因果边距离计算；历史同名ID路由不改签
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerCausalDistanceResult {
    /// 原计算报告值，不是实物因果证明
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub body: serde_json::Value,
    /// 计算结果版本
    pub schema: String,
    /// 当前结果状态
    pub status: u64,
}
impl SemanticExplorerCausalDistanceResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerCausalDistanceResult.schema:required".into()); }

        if !["semantica.explorer-causal-distance-result/2",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerCausalDistanceResult.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 有界备选简单节点路径数量及未知下界
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerCountMetric {
    /// 达到显示上限，实际总数仍未知
    pub capped: bool,
    /// 已知备选数量下界，不冒完整结果
    pub lower_bound: u64,
    /// 精确范围或不可用原因
    pub reason: String,
    /// 完成、未知或不适用
    pub status: String,
    /// 完整有界计数结果，已扣所选路径
    pub value: Option<u64>,
}
impl SemanticExplorerCountMetric {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.reason.trim().is_empty() { failures.push("SemanticExplorerCountMetric.reason:required".into()); }
        if self.status.trim().is_empty() { failures.push("SemanticExplorerCountMetric.status:required".into()); }

        if !["computed","not_applicable","unavailable",].contains(&self.status.as_str()) { failures.push("SemanticExplorerCountMetric.status:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原决策路由的有范围图谱观察；因果距离修正另版，不冒原不可达路由已执行
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerDecisionQuery {
    /// 原可选类别过滤
    pub category: Option<String>,
    /// 原决策或节点 ID
    pub decision_id: Option<String>,
    /// 列表或先例页大小
    pub limit: u64,
    /// 原五种决策读取操作
    pub operation: String,
    /// 决策读取合同版本
    pub schema: String,
    /// 非负分页位置，保留超过机器整数的原值
    pub skip: String,
}
impl SemanticExplorerDecisionQuery {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticExplorerDecisionQuery.operation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerDecisionQuery.schema:required".into()); }
        if self.skip.trim().is_empty() { failures.push("SemanticExplorerDecisionQuery.skip:required".into()); }

        if !["chain","compliance","get","list","precedents",].contains(&self.operation.as_str()) { failures.push("SemanticExplorerDecisionQuery.operation:allowed_values".into()); }
        if !["semantica.explorer-decision-query/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerDecisionQuery.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 决策读取前核对同一会话版本与摘要
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerDecisionRequest {
    /// 原数据集摘要
    pub expected_dataset_sha256: String,
    /// 原会话修订
    pub expected_revision: u64,
    /// 有类型决策读取问题
    pub query: SemanticExplorerDecisionQuery,
    /// 请求版本
    pub schema: String,
}
impl SemanticExplorerDecisionRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.expected_dataset_sha256.trim().is_empty() { failures.push("SemanticExplorerDecisionRequest.expected_dataset_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerDecisionRequest.schema:required".into()); }

        if !["semantica.explorer-decision-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerDecisionRequest.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原决策路由的有范围图谱观察；因果距离修正另版，不冒原不可达路由已执行
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerDecisionResult {
    /// 原响应正文，不授予工程采用
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub body: serde_json::Value,
    /// 决策读取结果版本
    pub schema: String,
    /// 观察结果的响应状态
    pub status: u64,
}
impl SemanticExplorerDecisionResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerDecisionResult.schema:required".into()); }

        if !["semantica.explorer-decision-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerDecisionResult.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原节点、边、单节点及邻居读取问题；资源配置不来自请求
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerGraphReadQuery {
    /// 原视口过滤文本
    pub bbox: Option<String>,
    /// 原 UTF8 URLsafe base64 游标
    pub cursor: Option<String>,
    /// 有向邻居深度 1 至 5
    pub depth: u64,
    /// 边类型精确过滤
    pub edge_type: Option<String>,
    /// 原最大 5000 的页大小
    pub limit: u64,
    /// 单节点或邻居起点
    pub node_id: Option<String>,
    /// 节点类型精确过滤
    pub node_type: Option<String>,
    /// 读取对象
    pub operation: String,
    /// 查询版本
    pub schema: String,
    /// 原节点搜索
    pub search: Option<String>,
    /// 非负整数规范十进制原值，支持超过 u64；响应恢复整数，游标覆盖仍回显
    pub skip: String,
    /// 边起点过滤
    pub source: Option<String>,
    /// 边终点过滤
    pub target: Option<String>,
}
impl SemanticExplorerGraphReadQuery {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticExplorerGraphReadQuery.operation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerGraphReadQuery.schema:required".into()); }
        if self.skip.trim().is_empty() { failures.push("SemanticExplorerGraphReadQuery.skip:required".into()); }

        if !["edges","neighbors","node","nodes",].contains(&self.operation.as_str()) { failures.push("SemanticExplorerGraphReadQuery.operation:allowed_values".into()); }
        if !["semantica.explorer-graph-read-query/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerGraphReadQuery.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 读取前核对同一会话版本与摘要
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerGraphReadRequest {
    /// 原数据集摘要
    pub expected_dataset_sha256: String,
    /// 原会话修订
    pub expected_revision: u64,
    /// 有类型读取问题
    pub query: SemanticExplorerGraphReadQuery,
    /// 请求版本
    pub schema: String,
}
impl SemanticExplorerGraphReadRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.expected_dataset_sha256.trim().is_empty() { failures.push("SemanticExplorerGraphReadRequest.expected_dataset_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerGraphReadRequest.schema:required".into()); }

        if !["semantica.explorer-graph-read-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerGraphReadRequest.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原读取响应及 HTTP 状态；不代表工程采用
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerGraphReadResult {
    /// 保留原公开响应形状；正常记录经既有原模型验证
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub body: serde_json::Value,
    /// 结果版本
    pub schema: String,
    /// 200、404 或原输入错误 422
    pub status: u64,
}
impl SemanticExplorerGraphReadResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerGraphReadResult.schema:required".into()); }

        if !["semantica.explorer-graph-read-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerGraphReadResult.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原GraphStatsResponse五个wire字段；明确当前规范ContextGraph记录统计，原历史type-index域未重建
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerGraphStats {
    /// 原GraphStatsResponse五个wire字段；明确当前规范ContextGraph记录统计，原历史type-index域未重建：density
    pub density: f64,
    /// 原GraphStatsResponse五个wire字段；明确当前规范ContextGraph记录统计，原历史type-index域未重建：edge_count
    pub edge_count: u64,
    /// 原GraphStatsResponse五个wire字段；明确当前规范ContextGraph记录统计，原历史type-index域未重建：edge_types
    pub edge_types: std::collections::BTreeMap<String, u64>,
    /// 原GraphStatsResponse五个wire字段；明确当前规范ContextGraph记录统计，原历史type-index域未重建：node_count
    pub node_count: u64,
    /// 原GraphStatsResponse五个wire字段；明确当前规范ContextGraph记录统计，原历史type-index域未重建：node_types
    pub node_types: std::collections::BTreeMap<String, u64>,
}
impl SemanticExplorerGraphStats {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if !self.density.is_finite() { failures.push("SemanticExplorerGraphStats.density:finite".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一已初始化ContextGraph记录的只读统计请求；请求中没有替代图或认证证明
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerGraphStatsRequest {
    /// 同一已初始化ContextGraph记录的只读统计请求；请求中没有替代图或认证证明：expected_dataset_sha256
    pub expected_dataset_sha256: String,
    /// 同一已初始化ContextGraph记录的只读统计请求；请求中没有替代图或认证证明：expected_revision
    pub expected_revision: u64,
    /// 同一已初始化ContextGraph记录的只读统计请求；请求中没有替代图或认证证明：schema
    pub schema: String,
}
impl SemanticExplorerGraphStatsRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.expected_dataset_sha256.len() != 64 || !self.expected_dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticExplorerGraphStatsRequest.expected_dataset_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerGraphStatsRequest.schema:required".into()); }

        if !["semantica.explorer-graph-stats-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerGraphStatsRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原图更新的执行、实际变化与宿主原子拒绝分离
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerMutationInput {
    /// 原事件回调模式，初始化后不得静默改变
    pub bridge: bool,
    /// 原五种方法的JSON参数
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub call: serde_json::Value,
    /// 更新协议版本
    pub schema: String,
}
impl SemanticExplorerMutationInput {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerMutationInput.schema:required".into()); }

        if !["semantica.explorer-mutation-input/1","semantica.explorer-mutation-input/2",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerMutationInput.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原图更新的执行、实际变化与宿主原子拒绝分离
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerMutationResult {
    /// 原节点边内容是否实际变化
    pub graph_changed: bool,
    /// 原GraphSession版本，不冒充DatasetSession发布版本
    pub graph_revision: u64,
    /// 原返回值或异常类别，可能伴随部分写入
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub result: serde_json::Value,
    /// 实际变化观测协议版本
    pub schema: String,
    /// 原回调吞下的字段异常诊断
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub warnings: serde_json::Value,
}
impl SemanticExplorerMutationResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerMutationResult.schema:required".into()); }

        if !["semantica.explorer-mutation-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerMutationResult.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原值Explorer状态，仍归原mutation namedgraph与DatasetSession
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerNativeState {
    /// 原图回调模式
    pub bridge: bool,
    /// 实际context对象状态/4载荷字节摘要
    pub context_sha256: String,
    /// 原GraphSession变更计数，区别Dataset发布修订
    pub graph_revision: u64,
    /// 同一GraphSearchIndex原值存储
    pub index: SemanticNativeValueEnvelope,
    /// 有类型原类型索引历史
    pub node_type_index: SemanticNativeValueEnvelope,
    /// 原值状态版本
    pub schema: String,
}
impl SemanticExplorerNativeState {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.context_sha256.trim().is_empty() { failures.push("SemanticExplorerNativeState.context_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerNativeState.schema:required".into()); }

        if !["semantica.explorer-mutation-state/2",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerNativeState.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 节点统计结果与明确可用状态
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerNodeMetric {
    /// 计算含义或未知原因
    pub reason: String,
    /// 计算完成、不可用或不适用
    pub status: String,
    /// 仅computed存在所选节点标识
    pub value: Option<String>,
}
impl SemanticExplorerNodeMetric {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.reason.trim().is_empty() { failures.push("SemanticExplorerNodeMetric.reason:required".into()); }
        if self.status.trim().is_empty() { failures.push("SemanticExplorerNodeMetric.status:required".into()); }

        if !["computed","not_applicable","unavailable",].contains(&self.status.as_str()) { failures.push("SemanticExplorerNodeMetric.status:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 路径诱导无向子图的结构瓶颈，非物理薄弱点
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathBottleneck {
    /// 中间节点中介中心性最大者，同值选路径中最先出现者
    pub bottleneck_node: SemanticExplorerNodeMetric,
    /// 原路径查询
    pub query: SemanticExplorerPathQuery,
    /// 同一会话实际路径
    pub result: SemanticExplorerPathResult,
    /// 节点中心性组合版本
    pub schema: String,
    /// 显式算法工作计数，不等同CPU时间或全进程费用
    pub work_used: u64,
}
impl SemanticExplorerPathBottleneck {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerPathBottleneck.schema:required".into()); }

        if !["semantica.explorer-path-bottleneck/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerPathBottleneck.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一图和原截止时间上的完整原生路径解释
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathDetails {
    /// 统计到hop_count加2条简单节点路径后扣1；不是最多hop_count加2跳
    pub alternative_path_count: SemanticExplorerCountMetric,
    /// 备选算法已用计数，不能续开预算
    pub alternative_work_used: u64,
    /// 路径节点诱导无向无权图精确中介中心性
    pub bottleneck_node: SemanticExplorerNodeMetric,
    /// 瓶颈算法已用计数
    pub bottleneck_work_used: u64,
    /// 历史字段名，含义仅选定边ID对应权重的顺序f64乘积；非工程可信度
    pub confidence_decay: SemanticExplorerScalarMetric,
    /// 按跳数0至1直接、2至3近、4至6中、7起远；不可达无分带
    pub distance_band: Option<String>,
    /// 确定性图解释，明确不构成因果或工程采用
    pub interpretation: String,
    /// 全部相邻余弦均值，缺段则未知
    pub path_coherence_score: SemanticExplorerScalarMetric,
    /// 原查询
    pub query: SemanticExplorerPathQuery,
    /// 实际路径、边ID、跳数与总权重
    pub result: SemanticExplorerPathResult,
    /// 组合解释协议
    pub schema: String,
    /// 端点向量余弦，模型空间适用性另证
    pub semantic_similarity: SemanticExplorerScalarMetric,
    /// 宿主选择的瓶颈与路径计数共享算法工作上限
    pub work_limit: u64,
    /// 两种有界图算法计数之和，非全部CPU费用
    pub work_used: u64,
}
impl SemanticExplorerPathDetails {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.interpretation.trim().is_empty() { failures.push("SemanticExplorerPathDetails.interpretation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerPathDetails.schema:required".into()); }

        if !["semantica.explorer-path-details/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerPathDetails.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 程序完成与路径问题成功分开
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathDetailsExecution {
    /// 实际只读图摘要
    pub dataset_sha256: String,
    /// 拒绝时有范围错误；不发布路径
    pub failure: Option<SemanticRuntimeFailure>,
    /// 完整任务原字节摘要
    pub input_sha256: String,
    /// 路径计算完成或明确拒绝
    pub outcome: String,
    /// 完成时路径结果；不可达也属于完成
    pub result: Option<SemanticExplorerPathDetails>,
    /// 执行版本
    pub schema: String,
}
impl SemanticExplorerPathDetailsExecution {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.dataset_sha256.len() != 64 || !self.dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticExplorerPathDetailsExecution.dataset_sha256:sha256".into()); }
        if self.input_sha256.len() != 64 || !self.input_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticExplorerPathDetailsExecution.input_sha256:sha256".into()); }
        if self.outcome.trim().is_empty() { failures.push("SemanticExplorerPathDetailsExecution.outcome:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerPathDetailsExecution.schema:required".into()); }

        if !["completed","rejected",].contains(&self.outcome.as_str()) { failures.push("SemanticExplorerPathDetailsExecution.outcome:allowed_values".into()); }
        if !["semantica.explorer-path-details-execution/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerPathDetailsExecution.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 宿主选定资源的源绑定路径子任务
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathDetailsJob {
    /// 同一快照的规范NQuads
    pub dataset_nquads: String,
    /// 规范图身份
    pub expected_dataset_sha256: String,
    /// 只读路径问题
    pub query: SemanticExplorerPathQuery,
    /// 宿主原会话资源合同
    pub resources: SemanticDatasetResourceSelection,
    /// 任务版本
    pub schema: String,
    /// 宿主选择的瓶颈与备选计数共享算法工作上限，必须随完整任务摘要绑定
    pub work_limit: u64,
}
impl SemanticExplorerPathDetailsJob {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.dataset_nquads.trim().is_empty() { failures.push("SemanticExplorerPathDetailsJob.dataset_nquads:required".into()); }
        if self.expected_dataset_sha256.len() != 64 || !self.expected_dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticExplorerPathDetailsJob.expected_dataset_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerPathDetailsJob.schema:required".into()); }

        if !["semantica.explorer-path-details-job/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerPathDetailsJob.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 程序完成与路径问题成功分开
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathExecution {
    /// 实际只读图摘要
    pub dataset_sha256: String,
    /// 拒绝时有范围错误；不发布路径
    pub failure: Option<SemanticRuntimeFailure>,
    /// 完整任务原字节摘要
    pub input_sha256: String,
    /// 路径计算完成或明确拒绝
    pub outcome: String,
    /// 完成时路径结果；不可达也属于完成
    pub result: Option<SemanticExplorerPathResult>,
    /// 执行版本
    pub schema: String,
}
impl SemanticExplorerPathExecution {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.dataset_sha256.len() != 64 || !self.dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticExplorerPathExecution.dataset_sha256:sha256".into()); }
        if self.input_sha256.len() != 64 || !self.input_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticExplorerPathExecution.input_sha256:sha256".into()); }
        if self.outcome.trim().is_empty() { failures.push("SemanticExplorerPathExecution.outcome:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerPathExecution.schema:required".into()); }

        if !["completed","rejected",].contains(&self.outcome.as_str()) { failures.push("SemanticExplorerPathExecution.outcome:allowed_values".into()); }
        if !["semantica.explorer-path-execution/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerPathExecution.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 路径HTTP指标状态与共享工作计数
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathHttpMetrics {
    /// 有界简单路径计数及下界
    pub alternative_path_count: SemanticExplorerCountMetric,
    /// 备选路径工作计数
    pub alternative_work_used: u64,
    /// 诱导路径图中介中心性节点
    pub bottleneck_node: SemanticExplorerNodeMetric,
    /// 瓶颈工作计数
    pub bottleneck_work_used: u64,
    /// 选定边权乘积，非工程概率
    pub confidence_decay: SemanticExplorerScalarMetric,
    /// 全部相邻余弦状态及值
    pub path_coherence_score: SemanticExplorerScalarMetric,
    /// 端点余弦状态及值
    pub semantic_similarity: SemanticExplorerScalarMetric,
    /// 宿主共享额度
    pub work_limit: u64,
    /// 已用两类图算法工作计数
    pub work_used: u64,
}
impl SemanticExplorerPathHttpMetrics {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 已源绑定且可达的路径HTTP响应
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathHttpResponse {
    /// bfs或dijkstra
    pub algorithm: String,
    /// 实际有界备选计数；未知不能写零
    pub alternative_path_count: Option<u64>,
    /// 瓶颈节点或未知/不适用
    pub bottleneck_node: Option<String>,
    /// 选定边权乘积或未知，非工程可信度
    pub confidence_decay: Option<f64>,
    /// 是否有向
    pub directed: bool,
    /// 按跳数分带
    pub distance_band: String,
    /// 实际选定边ID
    pub edge_ids: Vec<String>,
    /// 实际跳数
    pub hop_count: u64,
    /// 图解释及工程含义限制
    pub interpretation: String,
    /// 指标状态、原因、下界和工作量
    pub metrics: SemanticExplorerPathHttpMetrics,
    /// 实际节点路径
    pub path: Vec<String>,
    /// 全部相邻余弦均值或未知
    pub path_coherence_score: Option<f64>,
    /// 显式原生响应协议
    pub schema: String,
    /// 端点余弦或未知
    pub semantic_similarity: Option<f64>,
    /// 原查询起点
    pub source: String,
    /// 原查询终点
    pub target: String,
    /// 选定边权之和
    pub total_weight: f64,
}
impl SemanticExplorerPathHttpResponse {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.algorithm.trim().is_empty() { failures.push("SemanticExplorerPathHttpResponse.algorithm:required".into()); }
        if self.distance_band.trim().is_empty() { failures.push("SemanticExplorerPathHttpResponse.distance_band:required".into()); }
        if self.interpretation.trim().is_empty() { failures.push("SemanticExplorerPathHttpResponse.interpretation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerPathHttpResponse.schema:required".into()); }
        if self.source.trim().is_empty() { failures.push("SemanticExplorerPathHttpResponse.source:required".into()); }
        if self.target.trim().is_empty() { failures.push("SemanticExplorerPathHttpResponse.target:required".into()); }
        if !self.total_weight.is_finite() { failures.push("SemanticExplorerPathHttpResponse.total_weight:finite".into()); }

        if !["semantica.explorer-path-http-response/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerPathHttpResponse.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 宿主选定资源的源绑定路径子任务
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathJob {
    /// 同一快照的规范NQuads
    pub dataset_nquads: String,
    /// 规范图身份
    pub expected_dataset_sha256: String,
    /// 只读路径问题
    pub query: SemanticExplorerPathQuery,
    /// 宿主原会话资源合同
    pub resources: SemanticDatasetResourceSelection,
    /// 任务版本
    pub schema: String,
}
impl SemanticExplorerPathJob {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.dataset_nquads.trim().is_empty() { failures.push("SemanticExplorerPathJob.dataset_nquads:required".into()); }
        if self.expected_dataset_sha256.len() != 64 || !self.expected_dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticExplorerPathJob.expected_dataset_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerPathJob.schema:required".into()); }

        if !["semantica.explorer-path-job/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerPathJob.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一ContextGraph记录上的只读路径问题
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathQuery {
    /// bfs最少边数或dijkstra非负权重最短
    pub algorithm: String,
    /// 是否按原关系方向遍历
    pub directed: bool,
    /// 原生路径问题版本
    pub schema: String,
    /// 已存在的起点标识
    pub source: String,
    /// 已存在的终点标识
    pub target: String,
}
impl SemanticExplorerPathQuery {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.algorithm.trim().is_empty() { failures.push("SemanticExplorerPathQuery.algorithm:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerPathQuery.schema:required".into()); }
        if self.source.trim().is_empty() { failures.push("SemanticExplorerPathQuery.source:required".into()); }
        if self.target.trim().is_empty() { failures.push("SemanticExplorerPathQuery.target:required".into()); }

        if !["bfs","dijkstra",].contains(&self.algorithm.as_str()) { failures.push("SemanticExplorerPathQuery.algorithm:allowed_values".into()); }
        if !["semantica.explorer-path-query/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerPathQuery.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一记录图的路径结果，不是因果或工程采用判断
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathResult {
    /// 每一跳实际选中的原边标识
    pub edge_ids: Vec<String>,
    /// 可达时边数；不可达为null
    pub hop_count: Option<u64>,
    /// 含两端点的实际节点序列；不可达为空
    pub path: Vec<String>,
    /// 在本次指定方向和算法下可达
    pub reachable: bool,
    /// 原生路径结果版本
    pub schema: String,
    /// 原边有限权重之和；不可达为null，不解释为置信概率
    pub total_weight: Option<f64>,
}
impl SemanticExplorerPathResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerPathResult.schema:required".into()); }

        if !["semantica.explorer-path-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerPathResult.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 路径核心和完整相邻嵌入比较，当前会话只读计算
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerPathSimilarity {
    /// 所有相邻向量余弦均可计算时的均值，缺任一段则未知
    pub path_coherence_score: SemanticExplorerScalarMetric,
    /// 原路径查询
    pub query: SemanticExplorerPathQuery,
    /// 同一会话实际算出的路径
    pub result: SemanticExplorerPathResult,
    /// 路径相似度组合版本
    pub schema: String,
    /// 端点向量余弦几何，不证明嵌入模型空间一致或工程语义
    pub semantic_similarity: SemanticExplorerScalarMetric,
}
impl SemanticExplorerPathSimilarity {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerPathSimilarity.schema:required".into()); }

        if !["semantica.explorer-path-similarity/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerPathSimilarity.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 图计算标量与明确可用状态，不是工程采用
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerScalarMetric {
    /// 具体计算约定或不可用原因
    pub reason: String,
    /// 计算完成、不可用或不适用
    pub status: String,
    /// 仅computed时存在有限标量
    pub value: Option<f64>,
}
impl SemanticExplorerScalarMetric {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.reason.trim().is_empty() { failures.push("SemanticExplorerScalarMetric.reason:required".into()); }
        if self.status.trim().is_empty() { failures.push("SemanticExplorerScalarMetric.status:required".into()); }

        if !["computed","not_applicable","unavailable",].contains(&self.status.as_str()) { failures.push("SemanticExplorerScalarMetric.status:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 保留原 SearchRequest 输入，由既有模型准入后在唯一数据会话执行
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerSearchQuery {
    /// 原请求值，含任意精度跳数及历史 coercion，不以索引参数替代准入
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub body: serde_json::Value,
    /// 搜索问题版本
    pub schema: String,
}
impl SemanticExplorerSearchQuery {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerSearchQuery.schema:required".into()); }

        if !["semantica.explorer-search-query/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerSearchQuery.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 搜索前核对同一会话版本与摘要
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerSearchRequest {
    /// 原数据集摘要
    pub expected_dataset_sha256: String,
    /// 原会话修订
    pub expected_revision: u64,
    /// 有类型搜索问题
    pub query: SemanticExplorerSearchQuery,
    /// 请求版本
    pub schema: String,
}
impl SemanticExplorerSearchRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.expected_dataset_sha256.trim().is_empty() { failures.push("SemanticExplorerSearchRequest.expected_dataset_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerSearchRequest.schema:required".into()); }

        if !["semantica.explorer-search-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerSearchRequest.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 搜索响应与工程采用分离
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerSearchResult {
    /// 原响应内容及未使用语义相似度的 null 状态
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub body: serde_json::Value,
    /// 搜索结果版本
    pub schema: String,
    /// 响应状态
    pub status: u64,
}
impl SemanticExplorerSearchResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerSearchResult.schema:required".into()); }

        if !["semantica.explorer-search-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerSearchResult.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一工程图临时查询执行；输入SHA和数据集SHA分别绑定，不替代来源或工程决定
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerSparqlExecution {
    /// 有源Explorer查询子执行载荷：dataset_sha256
    pub dataset_sha256: String,
    /// 有源Explorer查询子执行载荷：input_sha256
    pub input_sha256: String,
    /// 有源Explorer查询子执行载荷：result
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub result: serde_json::Value,
    /// 有源Explorer查询子执行载荷：schema
    pub schema: String,
}
impl SemanticExplorerSparqlExecution {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.dataset_sha256.trim().is_empty() { failures.push("SemanticExplorerSparqlExecution.dataset_sha256:required".into()); }
        if self.input_sha256.trim().is_empty() { failures.push("SemanticExplorerSparqlExecution.input_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerSparqlExecution.schema:required".into()); }

        if !["semantica.explorer-sparql-execution/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerSparqlExecution.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 有限HTTP查询输入；不携带数据集、预算或授权
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerSparqlHttpRequest {
    /// 客户端原查询文本，空串交准入处理
    pub query: String,
}
impl SemanticExplorerSparqlHttpRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.query.trim().is_empty() { failures.push("SemanticExplorerSparqlHttpRequest.query:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一工程图临时查询执行；输入SHA和数据集SHA分别绑定，不替代来源或工程决定
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerSparqlJob {
    /// 有源Explorer查询子执行载荷：dataset_nquads
    pub dataset_nquads: String,
    /// 有源Explorer查询子执行载荷：expected_dataset_sha256
    pub expected_dataset_sha256: String,
    /// 有源Explorer查询子执行载荷：query
    pub query: String,
    /// 有源Explorer查询子执行载荷：row_limit
    pub row_limit: u64,
    /// 有源Explorer查询子执行载荷：schema
    pub schema: String,
}
impl SemanticExplorerSparqlJob {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.dataset_nquads.trim().is_empty() { failures.push("SemanticExplorerSparqlJob.dataset_nquads:required".into()); }
        if self.expected_dataset_sha256.trim().is_empty() { failures.push("SemanticExplorerSparqlJob.expected_dataset_sha256:required".into()); }
        if self.query.trim().is_empty() { failures.push("SemanticExplorerSparqlJob.query:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerSparqlJob.schema:required".into()); }

        if !["semantica.explorer-sparql-job/1",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerSparqlJob.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 显式资源规格随同一数据集查询传递，旧job/1原封保留为内层合同
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExplorerSparqlJobV2 {
    /// 版本化资源选择与有源查询载荷
    pub job: SemanticExplorerSparqlJob,
    /// 版本化资源选择与有源查询载荷
    pub resources: SemanticDatasetResourceSelection,
    /// 版本化资源选择与有源查询载荷
    pub schema: String,
}
impl SemanticExplorerSparqlJobV2 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticExplorerSparqlJobV2.schema:required".into()); }

        if !["semantica.explorer-sparql-job/2",].contains(&self.schema.as_str()) { failures.push("SemanticExplorerSparqlJobV2.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 明确选择的原后备算法或同Harness提供的模型端口；抽取结果不是工程事实、采用或模型能力证书
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExtractionRequest {
    /// 抽取请求/结果字段 operation
    pub operation: String,
    /// 抽取请求/结果字段 schema
    pub schema: String,
    /// 抽取请求/结果字段 text
    pub text: String,
}
impl SemanticExtractionRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticExtractionRequest.operation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExtractionRequest.schema:required".into()); }
        if self.text.trim().is_empty() { failures.push("SemanticExtractionRequest.text:required".into()); }

        if !["entities","relations",].contains(&self.operation.as_str()) { failures.push("SemanticExtractionRequest.operation:status".into()); }
        if !["semantica.extraction-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticExtractionRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 明确选择的原后备算法或同Harness提供的模型端口；抽取结果不是工程事实、采用或模型能力证书
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticExtractionResult {
    /// 抽取请求/结果字段 data
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub data: serde_json::Value,
    /// 抽取请求/结果字段 model_execution
    pub model_execution: String,
    /// 抽取请求/结果字段 profile
    pub profile: String,
    /// 抽取请求/结果字段 schema
    pub schema: String,
    /// 抽取请求/结果字段 uncovered
    pub uncovered: Vec<String>,
}
impl SemanticExtractionResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.model_execution.trim().is_empty() { failures.push("SemanticExtractionResult.model_execution:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticExtractionResult.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticExtractionResult.schema:required".into()); }

        if !["completed","failed","not_run","unavailable",].contains(&self.model_execution.as_str()) { failures.push("SemanticExtractionResult.model_execution:status".into()); }
        if !["semantica.extraction-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticExtractionResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One normalized original textual forward rule with its source identity and metadata; not a SWRL rule or an engineering truth claim
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticForwardRule {
    /// Rule author metadata preserved on the first derivation of a conclusion; not measured engineering reliability or a combined probability
    pub confidence: f64,
    /// Original signed integer scheduling priority; higher values execute first and equal priorities retain caller order
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub priority: serde_json::Value,
    /// Original nonempty rule identifier without outer Python whitespace; explicit IDs are preserved, and only string items consume rule_N defaults
    pub rule_id: String,
    /// Original IF conditions THEN conclusion text after outer Python whitespace removal; source wording is retained in each inference
    pub text: String,
}
impl SemanticForwardRule {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if !self.confidence.is_finite() { failures.push("SemanticForwardRule.confidence:finite".into()); }
        if self.rule_id.trim().is_empty() { failures.push("SemanticForwardRule.rule_id:required".into()); }
        if self.text.trim().is_empty() { failures.push("SemanticForwardRule.text:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原来源关系边；无工程因果验证
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticLineageEdge {
    /// 原subject IRI
    pub from: String,
    /// 明确来源边类型
    pub kind: String,
    /// 原来源或修订对象IRI
    pub to: String,
}
impl SemanticLineageEdge {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.from.trim().is_empty() { failures.push("SemanticLineageEdge.from:required".into()); }
        if self.kind.trim().is_empty() { failures.push("SemanticLineageEdge.kind:required".into()); }
        if self.to.trim().is_empty() { failures.push("SemanticLineageEdge.to:required".into()); }

        if !["derived_from","revision_of",].contains(&self.kind.as_str()) { failures.push("SemanticLineageEdge.kind:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 来源关系遍历，不推断物理因果
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticLineageRequest {
    /// 明确方向
    pub direction: String,
    /// 仅derived_from/revision_of；未知类型拒绝
    pub edge_kinds: Vec<String>,
    /// 唯一图范围
    pub graph_id: String,
    /// 1..100000
    pub max_edges: u64,
    /// 1只包含root；1..96
    pub max_levels: u64,
    /// 1..10000
    pub max_nodes: u64,
    /// 根entity IRI
    pub root: String,
    /// 明确协议版本
    pub schema: String,
}
impl SemanticLineageRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.direction.trim().is_empty() { failures.push("SemanticLineageRequest.direction:required".into()); }
        if self.graph_id.trim().is_empty() { failures.push("SemanticLineageRequest.graph_id:required".into()); }
        if self.root.trim().is_empty() { failures.push("SemanticLineageRequest.root:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticLineageRequest.schema:required".into()); }

        if !["ancestors","descendants",].contains(&self.direction.as_str()) { failures.push("SemanticLineageRequest.direction:status".into()); }
        if !["semantica.lineage-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticLineageRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 有界遍历保留缺失、循环及未完成frontier
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticLineageResult {
    /// 来源关系不证明物理因果
    pub causal_validation: String,
    /// 有缺失或frontier只能partial
    pub coverage: String,
    /// 选定关系存在循环
    pub cycle: bool,
    /// 明确来源边
    pub edges: Vec<SemanticLineageEdge>,
    /// 完成遍历与部分分开
    pub execution: String,
    /// 深度/预算阻止遍历的对象
    pub frontier: Vec<String>,
    /// 不存在对象，不记完整
    pub missing_references: Vec<String>,
    /// 每IRI最多访问一次
    pub nodes: Vec<String>,
    /// 明确来源遍历范围
    pub profile: String,
    /// 明确协议版本
    pub schema: String,
}
impl SemanticLineageResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.causal_validation.trim().is_empty() { failures.push("SemanticLineageResult.causal_validation:required".into()); }
        if self.coverage.trim().is_empty() { failures.push("SemanticLineageResult.coverage:required".into()); }
        if self.execution.trim().is_empty() { failures.push("SemanticLineageResult.execution:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticLineageResult.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticLineageResult.schema:required".into()); }

        if !["not_validated",].contains(&self.causal_validation.as_str()) { failures.push("SemanticLineageResult.causal_validation:status".into()); }
        if !["complete","partial",].contains(&self.coverage.as_str()) { failures.push("SemanticLineageResult.coverage:status".into()); }
        if !["completed","not_found","partial",].contains(&self.execution.as_str()) { failures.push("SemanticLineageResult.execution:status".into()); }
        if !["semantica.prov-source-span/1",].contains(&self.profile.as_str()) { failures.push("SemanticLineageResult.profile:status".into()); }
        if !["semantica.lineage-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticLineageResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticLiteLlmFailure
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticLiteLlmFailure {
    /// Caller-owned provider behavior: code
    pub code: String,
    /// Caller-owned provider behavior: message
    pub message: String,
    /// Caller-owned provider behavior: phase
    pub phase: String,
}
impl SemanticLiteLlmFailure {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.code.trim().is_empty() { failures.push("SemanticLiteLlmFailure.code:required".into()); }
        if self.phase.trim().is_empty() { failures.push("SemanticLiteLlmFailure.phase:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticLiteLlmObjectChoice
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticLiteLlmObjectChoice {
    /// Caller-owned provider behavior: message
    pub message: SemanticLiteLlmObjectMessage,
}
impl SemanticLiteLlmObjectChoice {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Err(errors) = self.message.validate() { failures.extend(errors); }


        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticLiteLlmObjectMessage
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticLiteLlmObjectMessage {
    /// Caller-owned provider behavior: content
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub content: serde_json::Value,
}
impl SemanticLiteLlmObjectMessage {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticLiteLlmObjectResponse
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticLiteLlmObjectResponse {
    /// Caller-owned provider behavior: choices
    pub choices: Vec<SemanticLiteLlmObjectChoice>,
}
impl SemanticLiteLlmObjectResponse {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticLiteLlmRequest
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticLiteLlmRequest {
    /// Caller-owned provider behavior: config
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub config: serde_json::Value,
    /// Caller-owned provider behavior: generation
    pub generation: SemanticProviderRequest,
}
impl SemanticLiteLlmRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Err(errors) = self.generation.validate() { failures.extend(errors); }


        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一Dataset修订上的中性MCP原wire请求
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticMcpRequest {
    /// 同一Dataset修订上的中性MCP原wire请求：expected_revision
    pub expected_revision: u64,
    /// 同一Dataset修订上的中性MCP原wire请求：request
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub request: serde_json::Value,
    /// 同一Dataset修订上的中性MCP原wire请求：schema
    pub schema: String,
}
impl SemanticMcpRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticMcpRequest.schema:required".into()); }

        if !["semantica.mcp-service-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticMcpRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 明确执行状态、wire响应或通知null与当前Dataset身份
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticMcpResult {
    /// 明确执行状态、wire响应或通知null与当前Dataset身份：dataset_sha256
    pub dataset_sha256: String,
    /// 明确执行状态、wire响应或通知null与当前Dataset身份：execution
    pub execution: String,
    /// 明确执行状态、wire响应或通知null与当前Dataset身份：profile
    pub profile: String,
    /// 明确执行状态、wire响应或通知null与当前Dataset身份：response
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub response: serde_json::Value,
    /// 明确执行状态、wire响应或通知null与当前Dataset身份：revision
    pub revision: u64,
    /// 明确执行状态、wire响应或通知null与当前Dataset身份：schema
    pub schema: String,
}
impl SemanticMcpResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.dataset_sha256.len() != 64 || !self.dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticMcpResult.dataset_sha256:sha256".into()); }
        if self.execution.trim().is_empty() { failures.push("SemanticMcpResult.execution:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticMcpResult.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticMcpResult.schema:required".into()); }

        if !["completed","failed","not_run",].contains(&self.execution.as_str()) { failures.push("SemanticMcpResult.execution:status".into()); }
        if !["semantica.mcp-service-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticMcpResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原17工具完整名字/schema与实际可执行状态；unavailable必须可观察
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticMcpTool {
    /// 原17工具完整名字/schema与实际可执行状态；unavailable必须可观察：availability
    pub availability: String,
    /// 原17工具完整名字/schema与实际可执行状态；unavailable必须可观察：description
    pub description: String,
    /// 原17工具完整名字/schema与实际可执行状态；unavailable必须可观察：input_schema
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub input_schema: serde_json::Value,
    /// 原17工具完整名字/schema与实际可执行状态；unavailable必须可观察：name
    pub name: String,
    /// 原17工具完整名字/schema与实际可执行状态；unavailable必须可观察：reason
    pub reason: String,
}
impl SemanticMcpTool {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.availability.trim().is_empty() { failures.push("SemanticMcpTool.availability:required".into()); }
        if self.description.trim().is_empty() { failures.push("SemanticMcpTool.description:required".into()); }
        if self.name.trim().is_empty() { failures.push("SemanticMcpTool.name:required".into()); }
        if self.reason.trim().is_empty() { failures.push("SemanticMcpTool.reason:required".into()); }

        if !["available","unavailable",].contains(&self.availability.as_str()) { failures.push("SemanticMcpTool.availability:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Generated complete model declarations owned solely by annotated specifications
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelDefinition {
    /// Model semantic declaration aliases
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub aliases: serde_json::Value,
    /// Model semantic declaration class_sha256
    pub class_sha256: String,
    /// Model semantic declaration config
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub config: serde_json::Value,
    /// Model semantic declaration fields
    pub fields: Vec<SemanticModelField>,
    /// Model semantic declaration id
    pub id: String,
    /// Model semantic declaration json_schema
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub json_schema: serde_json::Value,
    /// Model semantic declaration meaning
    pub meaning: String,
    /// Model semantic declaration name
    pub name: String,
    /// Model semantic declaration profile
    pub profile: String,
    /// Model semantic declaration properties
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub properties: serde_json::Value,
    /// Model semantic declaration source
    pub source: String,
    /// Model semantic declaration source_sha256
    pub source_sha256: String,
    /// Model semantic declaration validators
    pub validators: Vec<String>,
}
impl SemanticModelDefinition {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.class_sha256.len() != 64 || !self.class_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticModelDefinition.class_sha256:sha256".into()); }
        if self.id.trim().is_empty() { failures.push("SemanticModelDefinition.id:required".into()); }
        if self.meaning.trim().is_empty() { failures.push("SemanticModelDefinition.meaning:required".into()); }
        if self.name.trim().is_empty() { failures.push("SemanticModelDefinition.name:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticModelDefinition.profile:required".into()); }
        if self.source.trim().is_empty() { failures.push("SemanticModelDefinition.source:required".into()); }
        if self.source_sha256.len() != 64 || !self.source_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticModelDefinition.source_sha256:sha256".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Generated complete model declarations owned solely by annotated specifications
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelField {
    /// Model semantic declaration alias
    pub alias: Option<String>,
    /// Model semantic declaration annotation
    pub annotation: String,
    /// Model semantic declaration default
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub default: serde_json::Value,
    /// Model semantic declaration meaning
    pub meaning: String,
    /// Model semantic declaration name
    pub name: String,
    /// Model semantic declaration required
    pub required: bool,
    /// Model semantic declaration source_line
    pub source_line: u64,
    /// Model semantic declaration validators
    pub validators: Vec<String>,
}
impl SemanticModelField {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.annotation.trim().is_empty() { failures.push("SemanticModelField.annotation:required".into()); }
        if self.meaning.trim().is_empty() { failures.push("SemanticModelField.meaning:required".into()); }
        if self.name.trim().is_empty() { failures.push("SemanticModelField.name:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 仅记录数据集操作与修订变化，不能成为语义符合或工程通过
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticMutationResult {
    /// 本次增加的去重四元组数
    pub added: u64,
    /// 实际完成的数据集操作
    pub operation: String,
    /// 操作完成后四元组数
    pub quad_count: u64,
    /// 本次删除的去重四元组数
    pub removed: u64,
    /// 成功mutation后的修订号
    pub revision: u64,
}
impl SemanticMutationResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticMutationResult.operation:required".into()); }

        if !["clear","load","update",].contains(&self.operation.as_str()) { failures.push("SemanticMutationResult.operation:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original full dataset and Turtle shapes with independent source identities and an explicit named-IRI selector; default-graph entry points remain unchanged
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNamedGraphValidationRequest {
    /// Complete original TriG or N-Quads UTF-8 dataset, including default and unselected graphs; source bytes are never trimmed or flattened
    pub data: String,
    /// SHA256 of complete original data bytes, not a hash of only selected triples or a caller-supplied graph snapshot
    pub data_sha256: String,
    /// Explicit syntax retaining graph names; never inferred from a file extension or source content
    pub dataset_format: String,
    /// Required absolute IRI selecting one existing or empty named graph; it is not the default graph, a blank graph name or a union selector
    pub graph_name: String,
    /// Explicit named-graph validation transport version
    pub schema: String,
    /// Complete original Turtle shapes bytes; empty or invalid shapes follow existing parser/preflight semantics; no dataset shapes syntax or remote fetch
    pub shapes: String,
    /// SHA256 of original complete shape bytes before parsing, independent of data identity
    pub shapes_sha256: String,
}
impl SemanticNamedGraphValidationRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.data_sha256.len() != 64 || !self.data_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticNamedGraphValidationRequest.data_sha256:sha256".into()); }
        if self.dataset_format.trim().is_empty() { failures.push("SemanticNamedGraphValidationRequest.dataset_format:required".into()); }
        if self.graph_name.trim().is_empty() { failures.push("SemanticNamedGraphValidationRequest.graph_name:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticNamedGraphValidationRequest.schema:required".into()); }
        if self.shapes_sha256.len() != 64 || !self.shapes_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticNamedGraphValidationRequest.shapes_sha256:sha256".into()); }

        if !["nquads","trig",].contains(&self.dataset_format.as_str()) { failures.push("SemanticNamedGraphValidationRequest.dataset_format:status".into()); }
        if !["semantica.named-graph-validation-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticNamedGraphValidationRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Actual bounded SHACL report for one explicit named graph, with full original sources and unchanged validation revision; conformance is not engineering adoption
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNamedGraphValidationResult {
    /// Rechecked complete original dataset source identity
    pub data_sha256: String,
    /// Exact validated named IRI; no inference of source presence from a legal selector
    pub graph_name: String,
    /// Unmodified bounded SHACL report from validate_dataset on only the selected triples, retaining full findings, focus coverage, scope and supported-profile metadata
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub report: serde_json::Value,
    /// SHA256 of canonical complete typed request; original data/shapes, selector and format all remain bound
    pub request_sha256: String,
    /// Actual same-session revision after validation; equals revision_before for every successful report
    pub revision_after: u64,
    /// Actual DatasetSession revision after source load and immediately before immutable validation
    pub revision_before: u64,
    /// Explicit named-graph validation result transport version
    pub schema: String,
    /// Count of triples actually projected from the selected named graph; zero remains visible
    pub selected_quad_count: u64,
    /// Rechecked complete original Turtle shape source identity
    pub shapes_sha256: String,
}
impl SemanticNamedGraphValidationResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.data_sha256.len() != 64 || !self.data_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticNamedGraphValidationResult.data_sha256:sha256".into()); }
        if self.graph_name.trim().is_empty() { failures.push("SemanticNamedGraphValidationResult.graph_name:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticNamedGraphValidationResult.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticNamedGraphValidationResult.schema:required".into()); }
        if self.shapes_sha256.len() != 64 || !self.shapes_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticNamedGraphValidationResult.shapes_sha256:sha256".into()); }

        if !["semantica.named-graph-validation-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticNamedGraphValidationResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原GraphSession查询的有类型结果与索引诊断
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNativeExplorerSearchResult {
    /// 实际派生索引缓存命中
    pub cache_hit: bool,
    /// 索引候选数量
    pub candidates: u64,
    /// 原查询路径
    pub path: String,
    /// 原List节点与分数结果，保留特殊原值
    pub results: SemanticNativeValueEnvelope,
    /// 原值查询结果版本
    pub schema: String,
}
impl SemanticNativeExplorerSearchResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.path.trim().is_empty() { failures.push("SemanticNativeExplorerSearchResult.path:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticNativeExplorerSearchResult.schema:required".into()); }

        if !["semantica.explorer-native-search-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticNativeExplorerSearchResult.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// exact model asset caller identity; no second source authority
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNativeModelAsset {
    /// exact model asset caller identity; no second source authority; byte_count
    pub byte_count: u64,
    /// exact model asset caller identity; no second source authority; role
    pub role: String,
    /// exact model asset caller identity; no second source authority; sha256
    pub sha256: String,
    /// exact model asset caller identity; no second source authority; source_id
    pub source_id: String,
}
impl SemanticNativeModelAsset {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.role.trim().is_empty() { failures.push("SemanticNativeModelAsset.role:required".into()); }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticNativeModelAsset.sha256:sha256".into()); }
        if self.source_id.trim().is_empty() { failures.push("SemanticNativeModelAsset.source_id:required".into()); }

        if !["config","generation_config","modules","pooling","sentence_config","tokenizer","tokenizer_config","weights",].contains(&self.role.as_str()) { failures.push("SemanticNativeModelAsset.role:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// source selected model bytes and CPU architecture, separate from engineering adoption
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNativeModelSpec {
    /// source selected model bytes and CPU architecture, separate from engineering adoption; architecture
    pub architecture: String,
    /// source selected model bytes and CPU architecture, separate from engineering adoption; assets
    pub assets: Vec<SemanticNativeModelAsset>,
    /// source selected model bytes and CPU architecture, separate from engineering adoption; device
    pub device: String,
    /// source selected model bytes and CPU architecture, separate from engineering adoption; model_id
    pub model_id: String,
    /// source selected model bytes and CPU architecture, separate from engineering adoption; revision
    pub revision: String,
    /// source selected model bytes and CPU architecture, separate from engineering adoption; schema
    pub schema: String,
}
impl SemanticNativeModelSpec {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.architecture.trim().is_empty() { failures.push("SemanticNativeModelSpec.architecture:required".into()); }
        if self.device.trim().is_empty() { failures.push("SemanticNativeModelSpec.device:required".into()); }
        if self.model_id.trim().is_empty() { failures.push("SemanticNativeModelSpec.model_id:required".into()); }
        if self.revision.trim().is_empty() { failures.push("SemanticNativeModelSpec.revision:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticNativeModelSpec.schema:required".into()); }

        if !["bert","llama",].contains(&self.architecture.as_str()) { failures.push("SemanticNativeModelSpec.architecture:allowed_values".into()); }
        if !["cpu",].contains(&self.device.as_str()) { failures.push("SemanticNativeModelSpec.device:allowed_values".into()); }
        if !["semantica.native-model/1",].contains(&self.schema.as_str()) { failures.push("SemanticNativeModelSpec.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// actual native invocation with explicit inputs and measured outputs
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNativeRequest {
    /// actual native invocation with explicit inputs and measured outputs; batch
    pub batch: bool,
    /// actual native invocation with explicit inputs and measured outputs; interface_profile
    pub interface_profile: String,
    /// actual native invocation with explicit inputs and measured outputs; model
    pub model: SemanticNativeModelSpec,
    /// actual native invocation with explicit inputs and measured outputs; normalize
    pub normalize: bool,
    /// actual native invocation with explicit inputs and measured outputs; operation
    pub operation: String,
    /// actual native invocation with explicit inputs and measured outputs; options
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub options: serde_json::Value,
    /// actual native invocation with explicit inputs and measured outputs; schema
    pub schema: String,
    /// actual native invocation with explicit inputs and measured outputs; seed
    pub seed: u64,
    /// actual native invocation with explicit inputs and measured outputs; texts
    pub texts: Vec<String>,
}
impl SemanticNativeRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.interface_profile.trim().is_empty() { failures.push("SemanticNativeRequest.interface_profile:required".into()); }
        if let Err(errors) = self.model.validate() { failures.extend(errors); }

        if self.operation.trim().is_empty() { failures.push("SemanticNativeRequest.operation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticNativeRequest.schema:required".into()); }

        if !["bge_store","huggingface_llm","native_validation","text_embedder",].contains(&self.interface_profile.as_str()) { failures.push("SemanticNativeRequest.interface_profile:allowed_values".into()); }
        if !["argmax_validation","causal_logits","embed","generate","generate_structured",].contains(&self.operation.as_str()) { failures.push("SemanticNativeRequest.operation:allowed_values".into()); }
        if !["semantica.native-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticNativeRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNativeResult {
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; execution
    pub execution: String,
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; execution_identity
    pub execution_identity: String,
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; model_sha256
    pub model_sha256: String,
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; output
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub output: serde_json::Value,
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; parameters
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub parameters: serde_json::Value,
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; profile
    pub profile: String,
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; request_sha256
    pub request_sha256: String,
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; schema
    pub schema: String,
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; token_inputs
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub token_inputs: serde_json::Value,
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; usage
    pub usage: SemanticProviderUsage,
    /// actual weighted CPU result and input/model/token identities; no inference claim for supplied vectors; usage_status
    pub usage_status: String,
}
impl SemanticNativeResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.execution.trim().is_empty() { failures.push("SemanticNativeResult.execution:required".into()); }
        if self.execution_identity.trim().is_empty() { failures.push("SemanticNativeResult.execution_identity:required".into()); }
        if self.model_sha256.len() != 64 || !self.model_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticNativeResult.model_sha256:sha256".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticNativeResult.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticNativeResult.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticNativeResult.schema:required".into()); }
        if let Err(errors) = self.usage.validate() { failures.extend(errors); }

        if self.usage_status.trim().is_empty() { failures.push("SemanticNativeResult.usage_status:required".into()); }

        if !["completed",].contains(&self.execution.as_str()) { failures.push("SemanticNativeResult.execution:status".into()); }
        if !["semantica.native-candle-f32-cpu/1",].contains(&self.profile.as_str()) { failures.push("SemanticNativeResult.profile:allowed_values".into()); }
        if !["semantica.native-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticNativeResult.schema:status".into()); }
        if !["known","partial","unknown",].contains(&self.usage_status.as_str()) { failures.push("SemanticNativeResult.usage_status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 保留非普通JSON值的有类型数据表示，不执行Python或授予工程采用
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNativeValueEnvelope {
    /// 有序类型节点；整数十进制、float64位、Unicode码点及有类型的容器/字典键
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub nodes: Vec<serde_json::Value>,
    /// 表示的唯一根节点序号
    pub root: u64,
    /// 有类型值协议版本
    pub schema: String,
}
impl SemanticNativeValueEnvelope {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticNativeValueEnvelope.schema:required".into()); }

        if !["semantica.native-value/1",].contains(&self.schema.as_str()) { failures.push("SemanticNativeValueEnvelope.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete fixed TSV content fixture only; never actual OCR execution or accepted page transcription
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticOcrTsvContent3 {
    /// Exact original header and LF/CRLF terminator
    pub header_raw: String,
    /// Retained actual decoded image evidence; declared page binding remains unknown
    pub image: SemanticPageImageContent3,
    /// This operation does not execute OCR
    pub ocr_execution: String,
    /// Explicit fixture content only
    pub qualification: String,
    /// Exact fixed fixture bytes through same SourceProvider
    pub raw_asset: SemanticContentAsset3,
    /// Actual complete fixture byte count
    pub raw_bytes: u64,
    /// Actual complete fixture raw byte SHA
    pub raw_sha256: String,
    /// Observed row order only
    pub reading_order: String,
    /// Every ordered original row, no confidence filtering
    pub rows: Vec<SemanticOcrTsvRow3>,
    /// Explicit new fixed TSV content schema
    pub schema: String,
    /// Mandatory actual transcription/tool/language/layout/deadline/numeric/engineering/full-OCR unknowns imposed by producer and replay
    pub unknown: Vec<String>,
    /// Every level5 text in raw order, including empty/low-confidence text
    pub word_text_in_order: Vec<String>,
}
impl SemanticOcrTsvContent3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.ocr_execution.trim().is_empty() { failures.push("SemanticOcrTsvContent3.ocr_execution:required".into()); }
        if self.qualification.trim().is_empty() { failures.push("SemanticOcrTsvContent3.qualification:required".into()); }
        if self.raw_sha256.len() != 64 || !self.raw_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticOcrTsvContent3.raw_sha256:sha256".into()); }
        if self.reading_order.trim().is_empty() { failures.push("SemanticOcrTsvContent3.reading_order:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticOcrTsvContent3.schema:required".into()); }

        if !["not_run",].contains(&self.ocr_execution.as_str()) { failures.push("SemanticOcrTsvContent3.ocr_execution:status".into()); }
        if !["fixed_fixture_content_only",].contains(&self.qualification.as_str()) { failures.push("SemanticOcrTsvContent3.qualification:status".into()); }
        if !["tsv_raw_order_only",].contains(&self.reading_order.as_str()) { failures.push("SemanticOcrTsvContent3.reading_order:status".into()); }
        if !["semantica.ocr-tsv-fixture-content/3",].contains(&self.schema.as_str()) { failures.push("SemanticOcrTsvContent3.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Every complete original TSV row and terminator; confidences are OCR quality indications only
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticOcrTsvRow3 {
    /// Original block number
    pub block_num: u64,
    /// Original UTF8 raw byte coordinate including header and terminators
    pub byte_offset: u64,
    /// Finite0..100 quality value; -1 is None, never truth
    pub confidence: Option<f64>,
    /// Exact raw confidence lexical value; -1 sentinel retained
    pub confidence_raw: String,
    /// Original bounded region height
    pub height: u64,
    /// Original whole-image pixel x from left
    pub left: u64,
    /// Original TSV level1..5
    pub level: u64,
    /// Original line number
    pub line_num: u64,
    /// Original zero-based data row ordinal
    pub ordinal: u64,
    /// Original OCR single-image page1; not physical document page
    pub page_num: u64,
    /// Original paragraph number
    pub par_num: u64,
    /// Exact row including original LF/CRLF or no final newline
    pub raw_line: String,
    /// Exact last TSV field including empty or low-confidence text
    pub text: String,
    /// Original whole-image pixel y from top
    pub top: u64,
    /// Original bounded region width
    pub width: u64,
    /// Original word number
    pub word_num: u64,
}
impl SemanticOcrTsvRow3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete raw TSV source content with no neutral execution authority
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticOcrTsvSourceContent5 {
    /// execution_authority in the separately bound source/content/process domain
    pub execution_authority: String,
    /// header_raw in the separately bound source/content/process domain
    pub header_raw: String,
    /// image in the separately bound source/content/process domain
    pub image: SemanticPageImageContent3,
    /// Original row ordinals whose reported box does not nest within the referenced reported parent box; hierarchy reference is distinct from geometry containment and engineering truth
    pub noncontaining_parent_rows: Vec<u64>,
    /// qualification in the separately bound source/content/process domain
    pub qualification: String,
    /// raw_asset in the separately bound source/content/process domain
    pub raw_asset: SemanticContentAsset3,
    /// raw_bytes in the separately bound source/content/process domain
    pub raw_bytes: u64,
    /// raw_sha256 in the separately bound source/content/process domain
    pub raw_sha256: String,
    /// reading_order in the separately bound source/content/process domain
    pub reading_order: String,
    /// rows in the separately bound source/content/process domain
    pub rows: Vec<SemanticOcrTsvRow3>,
    /// schema in the separately bound source/content/process domain
    pub schema: String,
    /// unknown in the separately bound source/content/process domain
    pub unknown: Vec<String>,
    /// word_text_in_order in the separately bound source/content/process domain
    pub word_text_in_order: Vec<String>,
}
impl SemanticOcrTsvSourceContent5 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.execution_authority.trim().is_empty() { failures.push("SemanticOcrTsvSourceContent5.execution_authority:required".into()); }
        if self.header_raw.trim().is_empty() { failures.push("SemanticOcrTsvSourceContent5.header_raw:required".into()); }
        if self.qualification.trim().is_empty() { failures.push("SemanticOcrTsvSourceContent5.qualification:required".into()); }
        if self.raw_sha256.len() != 64 || !self.raw_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticOcrTsvSourceContent5.raw_sha256:sha256".into()); }
        if self.reading_order.trim().is_empty() { failures.push("SemanticOcrTsvSourceContent5.reading_order:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticOcrTsvSourceContent5.schema:required".into()); }

        if !["not_owned_by_content_library",].contains(&self.execution_authority.as_str()) { failures.push("SemanticOcrTsvSourceContent5.execution_authority:status".into()); }
        if !["source_content_only",].contains(&self.qualification.as_str()) { failures.push("SemanticOcrTsvSourceContent5.qualification:status".into()); }
        if !["tsv_raw_order_only",].contains(&self.reading_order.as_str()) { failures.push("SemanticOcrTsvSourceContent5.reading_order:status".into()); }
        if !["semantica.ocr-tsv-source-content/5",].contains(&self.schema.as_str()) { failures.push("SemanticOcrTsvSourceContent5.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 首次登记的完整原字节、规范化metadata和声明哈希缺项；不是工程采用证据
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPackageAsset {
    /// 唯一修剪ID
    pub asset_id: String,
    /// 完整实际原字节，允许空、NUL和非UTF8
    pub content: Vec<u8>,
    /// 实际输入方式
    pub data_origin: String,
    /// 调用者声明或缺项，不能代替实际字节SHA
    pub expected_sha256: Option<String>,
    /// 原格式或按旧别名/实际路径推断的RDF格式；缺失保留null
    pub format: Option<String>,
    /// 原命名图或null，实际加载才核IRI
    pub graph_name: Option<String>,
    /// 角色默认或显式Python truthiness，包括null false
    pub loaded_into_dataset: bool,
    /// 保留大小写的角色
    pub role: String,
    /// 实际完整字节摘要
    pub sha256: String,
    /// 缺批准来源链仍unknown；声明匹配不能自授authenticity
    pub source_authenticity: String,
}
impl SemanticPackageAsset {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.asset_id.trim().is_empty() { failures.push("SemanticPackageAsset.asset_id:required".into()); }
        if self.data_origin.trim().is_empty() { failures.push("SemanticPackageAsset.data_origin:required".into()); }
        if self.role.trim().is_empty() { failures.push("SemanticPackageAsset.role:required".into()); }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticPackageAsset.sha256:sha256".into()); }
        if self.source_authenticity.trim().is_empty() { failures.push("SemanticPackageAsset.source_authenticity:required".into()); }

        if !["inline_bytes","inline_text","path",].contains(&self.data_origin.as_str()) { failures.push("SemanticPackageAsset.data_origin:status".into()); }
        if !["unknown",].contains(&self.source_authenticity.as_str()) { failures.push("SemanticPackageAsset.source_authenticity:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 已读实际完整字节及旧规范化metadata形成的包身份；声明来源真实性另取证
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPackageIdentity {
    /// 已读实际资产SHA与排序metadata的旧ASCII canonical SHA256
    pub digest: String,
    /// 原namespace或null，加载时作base IRI
    pub namespace: Option<String>,
    /// 修剪空白后的精确包ID
    pub package_id: String,
    /// 原stage1 manifest版本
    pub schema_version: String,
    /// 精确版本
    pub version: String,
}
impl SemanticPackageIdentity {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.digest.len() != 64 || !self.digest.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticPackageIdentity.digest:sha256".into()); }
        if self.package_id.trim().is_empty() { failures.push("SemanticPackageIdentity.package_id:required".into()); }
        if self.schema_version.trim().is_empty() { failures.push("SemanticPackageIdentity.schema_version:required".into()); }
        if self.version.trim().is_empty() { failures.push("SemanticPackageIdentity.version:required".into()); }

        if !["1.0",].contains(&self.schema_version.as_str()) { failures.push("SemanticPackageIdentity.schema_version:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 通用包输入；mapping或文件择一，native bytes显式运输，来源根注入
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPackageInput {
    /// mapping路径根；manifest文件输入忽略此值
    pub base_path: Option<String>,
    /// 原native bytes按ID注入，对应manifest空文本data marker；不是JSON数组隐式改型
    pub inline_bytes: std::collections::BTreeMap<String, Vec<u8>>,
    /// 原mapping输入或缺失
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub manifest: Option<serde_json::Value>,
    /// provider内原JSON/YAML文件或缺失
    pub manifest_path: Option<String>,
    /// Rust输入运输版本
    pub schema: String,
}
impl SemanticPackageInput {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticPackageInput.schema:required".into()); }

        if !["semantica.package-input/1",].contains(&self.schema.as_str()) { failures.push("SemanticPackageInput.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一DatasetSession的首次身份/原件保留和本次RDF增量/修订；重复不重新灌图
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPackageLoadResult {
    /// 按asset_id排序的首次原件及metadata
    pub assets: Vec<SemanticPackageAsset>,
    /// 精确包身份
    pub identity: SemanticPackageIdentity,
    /// 本次新增四元组数量，零不证明未执行
    pub rdf_added: u64,
    /// 本次live session修订
    pub revision: u64,
}
impl SemanticPackageLoadResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Err(errors) = self.identity.validate() { failures.extend(errors); }


        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Declared original document/physical page and selected image asset; renderer relationship remains unverified here
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPageImageBinding3 {
    /// Declared nonzero whole PDF byte count
    pub document_bytes: u64,
    /// Declared whole PDF SHA; neutral PNG parsing does not verify PDF or renderer
    pub document_sha256: String,
    /// Declared rendering DPI72..300; PNG metadata is not renderer proof
    pub dpi: u64,
    /// Exact image bytes obtained through the existing SourceProvider
    pub image_asset: SemanticContentAsset3,
    /// Declared original document physical page count
    pub page_count: u64,
    /// Declared 1-based physical file page; never OCR single-image page number
    pub physical_page: u64,
    /// Explicit new content binding schema
    pub schema: String,
}
impl SemanticPageImageBinding3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.document_sha256.len() != 64 || !self.document_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticPageImageBinding3.document_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticPageImageBinding3.schema:required".into()); }

        if !["semantica.page-image-binding/3",].contains(&self.schema.as_str()) { failures.push("SemanticPageImageBinding3.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Actual bounded PNG decoded content only; caller-declared page/renderer/provider relationship and engineering remain unknown
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPageImageContent3 {
    /// Original unchanged caller binding separately from actual pixel evidence
    pub binding: SemanticPageImageBinding3,
    /// Actual supported sample bit depth8
    pub bit_depth: u64,
    /// Actual untransformed sample channels1,3,2,4
    pub channels: u64,
    /// Every original chunk including pHYs; metadata not dropped
    pub chunks: Vec<SemanticPngChunk3>,
    /// Actual supported PNG colour type0,2,4,6
    pub colour_type: u64,
    /// Complete untransformed source sample count
    pub decoded_bytes: u64,
    /// Complete untransformed row-major sample SHA; no colour conversion
    pub decoded_sha256: String,
    /// Actual complete encoded PNG bytes
    pub encoded_bytes: u64,
    /// Actual complete encoded PNG SHA
    pub encoded_sha256: String,
    /// All decoded scanline filter counts in0..4 order
    pub filter_counts: Vec<u64>,
    /// Actual decoded source pixel height
    pub height: u64,
    /// Finite sample-coordinate profile
    pub pixel_profile: String,
    /// Content behavior only
    pub qualification: String,
    /// Actual row-major source sample bytes per row
    pub row_stride: u64,
    /// Explicit new PNG content schema
    pub schema: String,
    /// Mandatory source/renderer/provider/read deadline/all-format/engineering unknowns imposed by producer and replay
    pub unknown: Vec<String>,
    /// Actual decoded source pixel width
    pub width: u64,
}
impl SemanticPageImageContent3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.decoded_sha256.len() != 64 || !self.decoded_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticPageImageContent3.decoded_sha256:sha256".into()); }
        if self.encoded_sha256.len() != 64 || !self.encoded_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticPageImageContent3.encoded_sha256:sha256".into()); }
        if self.pixel_profile.trim().is_empty() { failures.push("SemanticPageImageContent3.pixel_profile:required".into()); }
        if self.qualification.trim().is_empty() { failures.push("SemanticPageImageContent3.qualification:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticPageImageContent3.schema:required".into()); }

        if !["row_major_untransformed_u8",].contains(&self.pixel_profile.as_str()) { failures.push("SemanticPageImageContent3.pixel_profile:status".into()); }
        if !["source_content_only",].contains(&self.qualification.as_str()) { failures.push("SemanticPageImageContent3.qualification:status".into()); }
        if !["semantica.page-image-content/3",].contains(&self.schema.as_str()) { failures.push("SemanticPageImageContent3.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Every original PNG chunk retained in raw order; no extra frame or unknown ancillary accepted
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPngChunk3 {
    /// Original encoded PNG byte offset of length field
    pub byte_offset: u64,
    /// Exact four-byte chunk kind
    pub kind: String,
    /// Original zero-based chunk ordinal
    pub ordinal: u64,
    /// Original bounded payload byte count
    pub payload_bytes: u64,
    /// Original complete payload SHA
    pub payload_sha256: String,
    /// Complete length/kind/payload/CRC bytes SHA
    pub raw_chunk_sha256: String,
}
impl SemanticPngChunk3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.kind.trim().is_empty() { failures.push("SemanticPngChunk3.kind:required".into()); }
        if self.payload_sha256.len() != 64 || !self.payload_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticPngChunk3.payload_sha256:sha256".into()); }
        if self.raw_chunk_sha256.len() != 64 || !self.raw_chunk_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticPngChunk3.raw_chunk_sha256:sha256".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Existing current-promoted task/evidence review with actual Rust observations; no engineering adoption
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPromotedReviewRequest1 {
    /// Timezone-aware observation time
    pub created_at: String,
    /// Exact bounded task evidence bytes
    pub evidence_bytes: Vec<u8>,
    /// Timezone-aware source capture time
    pub evidence_captured_at: String,
    /// Explicit existing parser format
    pub evidence_format: String,
    /// Task source ID
    pub evidence_id: String,
    /// Original input digest
    pub evidence_sha256: String,
    /// Task source URI
    pub evidence_uri: String,
    /// Binding baseline must equal current version
    pub expected_current_version: String,
    /// Binding logical package digest; never archive SHA
    pub expected_registry_package_sha256: String,
    /// Evidence-origin focus IRI
    pub focus_iri: String,
    /// Evidence-origin focus class IRI
    pub focus_type_iri: String,
    /// Exact promoted package
    pub package_id: String,
    /// Selected unloaded SPARQL asset
    pub query_asset_id: String,
    /// Existing host-owned registry identity
    pub registry_id: String,
    /// Existing SELECT finding plus SHACL decision meaning
    pub review_contract: String,
    /// Versioned observation request
    pub schema: String,
    /// Task-defined review scope, not typed Project validation
    pub scope_id: String,
    /// Selected unloaded SHACL asset
    pub shape_asset_id: String,
}
impl SemanticPromotedReviewRequest1 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.created_at.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.created_at:required".into()); }
        if self.evidence_captured_at.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.evidence_captured_at:required".into()); }
        if self.evidence_format.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.evidence_format:required".into()); }
        if self.evidence_id.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.evidence_id:required".into()); }
        if self.evidence_sha256.len() != 64 || !self.evidence_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticPromotedReviewRequest1.evidence_sha256:sha256".into()); }
        if self.evidence_uri.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.evidence_uri:required".into()); }
        if self.expected_current_version.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.expected_current_version:required".into()); }
        if self.expected_registry_package_sha256.len() != 64 || !self.expected_registry_package_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticPromotedReviewRequest1.expected_registry_package_sha256:sha256".into()); }
        if self.focus_iri.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.focus_iri:required".into()); }
        if self.focus_type_iri.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.focus_type_iri:required".into()); }
        if self.package_id.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.package_id:required".into()); }
        if self.query_asset_id.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.query_asset_id:required".into()); }
        if self.registry_id.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.registry_id:required".into()); }
        if self.review_contract.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.review_contract:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.schema:required".into()); }
        if self.scope_id.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.scope_id:required".into()); }
        if self.shape_asset_id.trim().is_empty() { failures.push("SemanticPromotedReviewRequest1.shape_asset_id:required".into()); }

        if !["json-ld","nquads","nt","trig","turtle","xml",].contains(&self.evidence_format.as_str()) { failures.push("SemanticPromotedReviewRequest1.evidence_format:allowed_values".into()); }
        if !["semantica.ontology.decision-review/v1",].contains(&self.review_contract.as_str()) { failures.push("SemanticPromotedReviewRequest1.review_contract:allowed_values".into()); }
        if !["semantica.promoted-current-review-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticPromotedReviewRequest1.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Existing current-promoted task/evidence review with actual Rust observations; no engineering adoption
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPromotedReviewResult1 {
    /// Only scoped declared-record consistency
    pub conclusion: String,
    /// Exact operated scope/query/shape/focus only
    pub coverage: Vec<String>,
    /// Same session evaluated canonical snapshot and explicit graph profile
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub dataset_observation: serde_json::Value,
    /// No adoption action or authority
    pub engineering_adoption: String,
    /// Exact evidence identity and input snapshot
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub evidence: serde_json::Value,
    /// Actual operation completion distinct from semantic conclusion
    pub execution: String,
    /// Always false; review never writes Project facts
    pub fact_admitted: bool,
    /// All actual phase errors and unknown causes retained
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub failures: Vec<serde_json::Value>,
    /// Original evidence focus presence/type/instance count
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub focus_observation: serde_json::Value,
    /// Existing v1 SELECT finding semantics or explicit unknown
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub interpretation: serde_json::Value,
    /// Verified selected asset bindings
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub package_assets: serde_json::Value,
    /// Actual query kind/columns/RDF rows or failure; no scenario substitution
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub query_observation: serde_json::Value,
    /// Canonical response content excluding this field
    pub report_sha256: String,
    /// Exact transport projection under existing v1 meaning, not historical engine
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub review_v1: serde_json::Value,
    /// Versioned actual observation
    pub schema: String,
    /// Actual bounded shared SHACL findings or unsupported/error
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub shape_observation: serde_json::Value,
    /// Verified registry/current descriptor with separate manifest/promotion/projection identities
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub subject: serde_json::Value,
    /// No complete typed Project supplied
    pub typed_project_checks: String,
}
impl SemanticPromotedReviewResult1 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.conclusion.trim().is_empty() { failures.push("SemanticPromotedReviewResult1.conclusion:required".into()); }
        if self.engineering_adoption.trim().is_empty() { failures.push("SemanticPromotedReviewResult1.engineering_adoption:required".into()); }
        if self.execution.trim().is_empty() { failures.push("SemanticPromotedReviewResult1.execution:required".into()); }
        if self.report_sha256.len() != 64 || !self.report_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticPromotedReviewResult1.report_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticPromotedReviewResult1.schema:required".into()); }
        if self.typed_project_checks.trim().is_empty() { failures.push("SemanticPromotedReviewResult1.typed_project_checks:required".into()); }

        if !["pass","unknown","violated",].contains(&self.conclusion.as_str()) { failures.push("SemanticPromotedReviewResult1.conclusion:allowed_values".into()); }
        if !["unknown",].contains(&self.engineering_adoption.as_str()) { failures.push("SemanticPromotedReviewResult1.engineering_adoption:allowed_values".into()); }
        if !["completed","failed","partial",].contains(&self.execution.as_str()) { failures.push("SemanticPromotedReviewResult1.execution:allowed_values".into()); }
        if !["semantica.promoted-current-review-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticPromotedReviewResult1.schema:allowed_values".into()); }
        if !["not_run",].contains(&self.typed_project_checks.as_str()) { failures.push("SemanticPromotedReviewResult1.typed_project_checks:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 实际声明的来源活动；缺时间不自动补历史
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceActivity {
    /// 活动IRI
    pub activity_id: String,
    /// 活动主体及角色
    pub associations: Vec<SemanticProvenanceAssociation>,
    /// 可选带时区xsd:dateTime结束时间
    pub ended_at: Option<String>,
    /// 可选带时区xsd:dateTime起始时间
    pub started_at: Option<String>,
    /// 明确使用的entity IRIs
    pub used: Vec<String>,
}
impl SemanticProvenanceActivity {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.activity_id.trim().is_empty() { failures.push("SemanticProvenanceActivity.activity_id:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 来源声明的主体，不是治理批准权限
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceAgent {
    /// 主体IRI
    pub agent_id: String,
    /// 明确PROV主体类型
    pub kind: String,
}
impl SemanticProvenanceAgent {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.agent_id.trim().is_empty() { failures.push("SemanticProvenanceAgent.agent_id:required".into()); }
        if self.kind.trim().is_empty() { failures.push("SemanticProvenanceAgent.kind:required".into()); }

        if !["organization","person","software_agent",].contains(&self.kind.as_str()) { failures.push("SemanticProvenanceAgent.kind:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 角色挂在activity的qualifiedAssociation
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceAssociation {
    /// 关联主体IRI
    pub agent_id: String,
    /// 关联IRI
    pub association_id: String,
    /// 可选角色IRI；不成为批准权限
    pub role_id: Option<String>,
}
impl SemanticProvenanceAssociation {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.agent_id.trim().is_empty() { failures.push("SemanticProvenanceAssociation.agent_id:required".into()); }
        if self.association_id.trim().is_empty() { failures.push("SemanticProvenanceAssociation.association_id:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一DatasetSession原子发布，sources含实际字节
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceBatchRequest {
    /// 来源活动
    pub activities: Vec<SemanticProvenanceActivity>,
    /// 来源主体
    pub agents: Vec<SemanticProvenanceAgent>,
    /// 明确具名图IRI；不遍历其他图
    pub graph_id: String,
    /// 输出及选定现图检查预算1..100000
    pub max_quads: u64,
    /// 记录预算1..4096
    pub max_records: u64,
    /// 不可变新记录
    pub records: Vec<SemanticProvenanceRecord>,
    /// 明确协议版本
    pub schema: String,
    /// 实际源与片段
    pub sources: Vec<SemanticProvenanceSource>,
}
impl SemanticProvenanceBatchRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.graph_id.trim().is_empty() { failures.push("SemanticProvenanceBatchRequest.graph_id:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticProvenanceBatchRequest.schema:required".into()); }

        if !["semantica.provenance-batch/1",].contains(&self.schema.as_str()) { failures.push("SemanticProvenanceBatchRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 新增撤回活动；不删除或覆盖原来源记录
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceInvalidationRequest {
    /// 新的独立撤回活动IRI
    pub activity_id: String,
    /// 明确撤回actor IRI，非自动批准
    pub agent_id: String,
    /// 原具名图
    pub graph_id: String,
    /// 有界检查1..100000
    pub max_quads: u64,
    /// 明确原撤回理由
    pub reason: String,
    /// 原record IRI
    pub record_id: String,
    /// 明确协议版本
    pub schema: String,
    /// 原subject IRI
    pub subject_id: String,
    /// 可选带时区xsd:dateTime原时间，缺失保持未知
    pub timestamp: Option<String>,
}
impl SemanticProvenanceInvalidationRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.activity_id.trim().is_empty() { failures.push("SemanticProvenanceInvalidationRequest.activity_id:required".into()); }
        if self.agent_id.trim().is_empty() { failures.push("SemanticProvenanceInvalidationRequest.agent_id:required".into()); }
        if self.graph_id.trim().is_empty() { failures.push("SemanticProvenanceInvalidationRequest.graph_id:required".into()); }
        if self.reason.trim().is_empty() { failures.push("SemanticProvenanceInvalidationRequest.reason:required".into()); }
        if self.record_id.trim().is_empty() { failures.push("SemanticProvenanceInvalidationRequest.record_id:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticProvenanceInvalidationRequest.schema:required".into()); }
        if self.subject_id.trim().is_empty() { failures.push("SemanticProvenanceInvalidationRequest.subject_id:required".into()); }

        if !["semantica.provenance-invalidation/1",].contains(&self.schema.as_str()) { failures.push("SemanticProvenanceInvalidationRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 有界单一图查询，不将not_found解释为通过
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceLookup {
    /// 选定具名图
    pub graph_id: String,
    /// 扫描选定图预算1..100000
    pub max_quads: u64,
    /// 明确协议版本
    pub schema: String,
    /// 选定subject或record IRI
    pub subject_id: String,
}
impl SemanticProvenanceLookup {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.graph_id.trim().is_empty() { failures.push("SemanticProvenanceLookup.graph_id:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticProvenanceLookup.schema:required".into()); }
        if self.subject_id.trim().is_empty() { failures.push("SemanticProvenanceLookup.subject_id:required".into()); }

        if !["semantica.provenance-lookup/1",].contains(&self.schema.as_str()) { failures.push("SemanticProvenanceLookup.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 来源派生图，不能自报采用；由唯一session加载和规范化
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceProjection {
    /// 只覆盖明确source-span切片
    pub coverage: String,
    /// 工程审查仍由原Project权威
    pub engineering_status: String,
    /// 明确图IRI
    pub graph_id: String,
    /// 完整typed请求内容摘要
    pub input_sha256: String,
    /// OxRDF构造的可解析NQuads
    pub nquads: String,
    /// PROV生成范围
    pub profile: String,
    /// 新记录原身份与内容摘要
    pub record_identities: Vec<SemanticProvenanceRecordIdentity>,
    /// 明确协议版本
    pub schema: String,
    /// 实际核验的全部来源SHA
    pub source_sha256: Vec<String>,
}
impl SemanticProvenanceProjection {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.coverage.trim().is_empty() { failures.push("SemanticProvenanceProjection.coverage:required".into()); }
        if self.engineering_status.trim().is_empty() { failures.push("SemanticProvenanceProjection.engineering_status:required".into()); }
        if self.graph_id.trim().is_empty() { failures.push("SemanticProvenanceProjection.graph_id:required".into()); }
        if self.input_sha256.len() != 64 || !self.input_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticProvenanceProjection.input_sha256:sha256".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticProvenanceProjection.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticProvenanceProjection.schema:required".into()); }

        if !["explicit_slice",].contains(&self.coverage.as_str()) { failures.push("SemanticProvenanceProjection.coverage:status".into()); }
        if !["unknown",].contains(&self.engineering_status.as_str()) { failures.push("SemanticProvenanceProjection.engineering_status:status".into()); }
        if !["semantica.prov-source-span/1",].contains(&self.profile.as_str()) { failures.push("SemanticProvenanceProjection.profile:status".into()); }
        if !["semantica.provenance-projection/1",].contains(&self.schema.as_str()) { failures.push("SemanticProvenanceProjection.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 不可变事件与来源、派生、修订分列；不改变Project
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceRecord {
    /// 来源归属主体IRIs
    pub attributed_to: Vec<String>,
    /// 可选有限0..1来源声明；不转工程通过
    pub confidence: Option<f64>,
    /// 明确派生entity IRIs
    pub derived_from: Vec<String>,
    /// 实体来源类型
    pub entity_kind: String,
    /// 可选生成活动IRI
    pub generated_by: Option<String>,
    /// 原元数据；原值保留，不转工程状态
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub metadata: Option<serde_json::Value>,
    /// 不可变记录IRI；已存在拒绝
    pub record_id: String,
    /// 明确修订entity IRIs；不覆盖派生
    pub revision_of: Vec<String>,
    /// verified片段IRI引用
    pub source_spans: Vec<String>,
    /// 不可变实体IRI；修订使用新IRI
    pub subject_id: String,
}
impl SemanticProvenanceRecord {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.entity_kind.trim().is_empty() { failures.push("SemanticProvenanceRecord.entity_kind:required".into()); }
        if self.record_id.trim().is_empty() { failures.push("SemanticProvenanceRecord.record_id:required".into()); }
        if self.subject_id.trim().is_empty() { failures.push("SemanticProvenanceRecord.subject_id:required".into()); }

        if !["attribute","chunk","entity","relation",].contains(&self.entity_kind.as_str()) { failures.push("SemanticProvenanceRecord.entity_kind:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 新profile对全部输入和source证明的内容绑定
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceRecordIdentity {
    /// 新内容绑定profile摘要，非旧checksum
    pub content_sha256: String,
    /// 原record IRI
    pub record_id: String,
    /// 原subject IRI
    pub subject_id: String,
}
impl SemanticProvenanceRecordIdentity {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.content_sha256.len() != 64 || !self.content_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticProvenanceRecordIdentity.content_sha256:sha256".into()); }
        if self.record_id.trim().is_empty() { failures.push("SemanticProvenanceRecordIdentity.record_id:required".into()); }
        if self.subject_id.trim().is_empty() { failures.push("SemanticProvenanceRecordIdentity.subject_id:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// prepare/load每次核验实际原UTF8字节，不消费传入passed声明
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceSource {
    /// 源身份
    pub binding: SemanticSourceBinding,
    /// 实际全部UTF8字节，空源合法
    pub data: String,
    /// 实际源中的明确片段列表
    pub spans: Vec<SemanticSourceSpan>,
}
impl SemanticProvenanceSource {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Err(errors) = self.binding.validate() { failures.extend(errors); }


        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原RDF来源视图和撤回活动原值，非工程状态
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProvenanceView {
    /// 此对象有界视图
    pub coverage: String,
    /// 存在与缺失分开
    pub execution: String,
    /// 明确撤回活动；保留原件
    pub invalidation_activities: Vec<String>,
    /// 明确原对象/片段相关RDF；空缺合法
    pub nquads: String,
    /// 明确视图范围
    pub profile: String,
    /// 可追溯的来源实体
    pub related_entities: Vec<String>,
    /// 明确协议版本
    pub schema: String,
    /// 原查询对象
    pub subject_id: String,
}
impl SemanticProvenanceView {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.coverage.trim().is_empty() { failures.push("SemanticProvenanceView.coverage:required".into()); }
        if self.execution.trim().is_empty() { failures.push("SemanticProvenanceView.execution:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticProvenanceView.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticProvenanceView.schema:required".into()); }
        if self.subject_id.trim().is_empty() { failures.push("SemanticProvenanceView.subject_id:required".into()); }

        if !["explicit_slice",].contains(&self.coverage.as_str()) { failures.push("SemanticProvenanceView.coverage:status".into()); }
        if !["completed","not_found",].contains(&self.execution.as_str()) { failures.push("SemanticProvenanceView.execution:status".into()); }
        if !["semantica.prov-source-span/1",].contains(&self.profile.as_str()) { failures.push("SemanticProvenanceView.profile:status".into()); }
        if !["semantica.provenance-view/1",].contains(&self.schema.as_str()) { failures.push("SemanticProvenanceView.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticProviderBinding
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderBinding {
    /// Caller-owned provider behavior: constructor_id
    pub constructor_id: String,
    /// Caller-owned provider behavior: name
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub name: serde_json::Value,
}
impl SemanticProviderBinding {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.constructor_id.trim().is_empty() { failures.push("SemanticProviderBinding.constructor_id:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticProviderCacheEntry
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderCacheEntry {
    /// Caller-owned provider behavior: instance
    pub instance: SemanticProviderInstance,
    /// Caller-owned provider behavior: key_sha256
    pub key_sha256: String,
}
impl SemanticProviderCacheEntry {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Err(errors) = self.instance.validate() { failures.extend(errors); }

        if self.key_sha256.len() != 64 || !self.key_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticProviderCacheEntry.key_sha256:sha256".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticProviderConstructionRequest
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderConstructionRequest {
    /// Caller-owned provider behavior: backend
    pub backend: String,
    /// Caller-owned provider behavior: configuration_sha256
    pub configuration_sha256: String,
    /// Caller-owned provider behavior: constructor_id
    pub constructor_id: Option<String>,
    /// Caller-owned provider behavior: instance_id
    pub instance_id: String,
    /// Caller-owned provider behavior: kwargs
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub kwargs: serde_json::Value,
    /// Caller-owned provider behavior: kwargs_profile
    pub kwargs_profile: String,
    /// Caller-owned provider behavior: name
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub name: serde_json::Value,
}
impl SemanticProviderConstructionRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.backend.trim().is_empty() { failures.push("SemanticProviderConstructionRequest.backend:required".into()); }
        if self.configuration_sha256.len() != 64 || !self.configuration_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticProviderConstructionRequest.configuration_sha256:sha256".into()); }
        if self.instance_id.trim().is_empty() { failures.push("SemanticProviderConstructionRequest.instance_id:required".into()); }
        if self.kwargs_profile.trim().is_empty() { failures.push("SemanticProviderConstructionRequest.kwargs_profile:required".into()); }

        if !["json/1","python-values/1",].contains(&self.kwargs_profile.as_str()) { failures.push("SemanticProviderConstructionRequest.kwargs_profile:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticProviderFactoryFailure
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderFactoryFailure {
    /// Caller-owned provider behavior: code
    pub code: String,
    /// Caller-owned provider behavior: phase
    pub phase: String,
    /// Caller-owned provider behavior: python_exception
    pub python_exception: String,
}
impl SemanticProviderFactoryFailure {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.code.trim().is_empty() { failures.push("SemanticProviderFactoryFailure.code:required".into()); }
        if self.phase.trim().is_empty() { failures.push("SemanticProviderFactoryFailure.phase:required".into()); }
        if self.python_exception.trim().is_empty() { failures.push("SemanticProviderFactoryFailure.python_exception:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticProviderFactoryPlan
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderFactoryPlan {
    /// Caller-owned provider behavior: commands
    pub commands: Vec<SemanticProviderFactoryRequest>,
    /// Caller-owned provider behavior: schema
    pub schema: String,
}
impl SemanticProviderFactoryPlan {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticProviderFactoryPlan.schema:required".into()); }

        if !["semantica.provider-factory-plan/1",].contains(&self.schema.as_str()) { failures.push("SemanticProviderFactoryPlan.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticProviderFactoryRequest
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderFactoryRequest {
    /// Caller-owned provider behavior: constructor_id
    pub constructor_id: Option<String>,
    /// Caller-owned provider behavior: kwargs
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub kwargs: serde_json::Value,
    /// Caller-owned provider behavior: kwargs_profile
    pub kwargs_profile: String,
    /// Caller-owned provider behavior: name
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub name: serde_json::Value,
    /// Caller-owned provider behavior: operation
    pub operation: String,
    /// Caller-owned provider behavior: schema
    pub schema: String,
    /// Caller-owned provider behavior: use_pool
    pub use_pool: bool,
}
impl SemanticProviderFactoryRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.kwargs_profile.trim().is_empty() { failures.push("SemanticProviderFactoryRequest.kwargs_profile:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticProviderFactoryRequest.operation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticProviderFactoryRequest.schema:required".into()); }

        if !["json/1","python-values/1",].contains(&self.kwargs_profile.as_str()) { failures.push("SemanticProviderFactoryRequest.kwargs_profile:status".into()); }
        if !["clear","create_provider","get","list_all","register","unregister",].contains(&self.operation.as_str()) { failures.push("SemanticProviderFactoryRequest.operation:status".into()); }
        if !["semantica.provider-factory-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticProviderFactoryRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticProviderFactoryResult
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderFactoryResult {
    /// Caller-owned provider behavior: binding
    pub binding: Option<SemanticProviderBinding>,
    /// Caller-owned provider behavior: cache_hit
    pub cache_hit: bool,
    /// Caller-owned provider behavior: instance
    pub instance: Option<SemanticProviderInstance>,
    /// Caller-owned provider behavior: names
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub names: Vec<serde_json::Value>,
    /// Caller-owned provider behavior: operation
    pub operation: String,
    /// Caller-owned provider behavior: schema
    pub schema: String,
}
impl SemanticProviderFactoryResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticProviderFactoryResult.operation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticProviderFactoryResult.schema:required".into()); }

        if !["clear","create_provider","get","list_all","register","unregister",].contains(&self.operation.as_str()) { failures.push("SemanticProviderFactoryResult.operation:status".into()); }
        if !["semantica.provider-factory-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticProviderFactoryResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticProviderFactoryState
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderFactoryState {
    /// Caller-owned provider behavior: bindings
    pub bindings: Vec<SemanticProviderBinding>,
    /// Caller-owned provider behavior: cache
    pub cache: Vec<SemanticProviderCacheEntry>,
    /// Caller-owned provider behavior: next_instance
    pub next_instance: u64,
    /// Caller-owned provider behavior: schema
    pub schema: String,
}
impl SemanticProviderFactoryState {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticProviderFactoryState.schema:required".into()); }

        if !["semantica.provider-factory-state/1",].contains(&self.schema.as_str()) { failures.push("SemanticProviderFactoryState.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 稳定安全原因码，不含正文、prompt、路径、授权头或凭据
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderFailure {
    /// 稳定安全原因码，不含正文、prompt、路径、授权头或凭据；字段 code
    pub code: String,
    /// 稳定安全原因码，不含正文、prompt、路径、授权头或凭据；字段 http_status
    pub http_status: Option<u64>,
    /// 稳定安全原因码，不含正文、prompt、路径、授权头或凭据；字段 phase
    pub phase: String,
    /// 稳定安全原因码，不含正文、prompt、路径、授权头或凭据；字段 retry_after
    pub retry_after: Option<f64>,
    /// 稳定安全原因码，不含正文、prompt、路径、授权头或凭据；字段 retryable
    pub retryable: bool,
}
impl SemanticProviderFailure {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.code.trim().is_empty() { failures.push("SemanticProviderFailure.code:required".into()); }
        if self.phase.trim().is_empty() { failures.push("SemanticProviderFailure.phase:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original provider wrapper behavior with explicit state/dispatch boundary; SemanticProviderInstance
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderInstance {
    /// Caller-owned provider behavior: available
    pub available: Option<bool>,
    /// Caller-owned provider behavior: backend
    pub backend: String,
    /// Caller-owned provider behavior: configuration_sha256
    pub configuration_sha256: String,
    /// Caller-owned provider behavior: constructor_id
    pub constructor_id: Option<String>,
    /// Caller-owned provider behavior: default_model
    pub default_model: Option<String>,
    /// Caller-owned provider behavior: instance_id
    pub instance_id: String,
    /// Caller-owned provider behavior: provider_name
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub provider_name: serde_json::Value,
}
impl SemanticProviderInstance {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.backend.trim().is_empty() { failures.push("SemanticProviderInstance.backend:required".into()); }
        if self.configuration_sha256.len() != 64 || !self.configuration_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticProviderInstance.configuration_sha256:sha256".into()); }
        if self.instance_id.trim().is_empty() { failures.push("SemanticProviderInstance.instance_id:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 通用模型请求；凭据由上层注入，模型输出不建立工程事实
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderRequest {
    /// 通用模型请求；凭据由上层注入，模型输出不建立工程事实；字段 custom_endpoint
    pub custom_endpoint: bool,
    /// 通用模型请求；凭据由上层注入，模型输出不建立工程事实；字段 interface_profile
    pub interface_profile: String,
    /// 通用模型请求；凭据由上层注入，模型输出不建立工程事实；字段 model
    pub model: Option<String>,
    /// 通用模型请求；凭据由上层注入，模型输出不建立工程事实；字段 operation
    pub operation: String,
    /// 通用模型请求；凭据由上层注入，模型输出不建立工程事实；字段 options
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub options: serde_json::Value,
    /// 通用模型请求；凭据由上层注入，模型输出不建立工程事实；字段 prompt
    pub prompt: String,
    /// 通用模型请求；凭据由上层注入，模型输出不建立工程事实；字段 provider
    pub provider: String,
    /// 通用模型请求；凭据由上层注入，模型输出不建立工程事实；字段 schema
    pub schema: String,
}
impl SemanticProviderRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.interface_profile.trim().is_empty() { failures.push("SemanticProviderRequest.interface_profile:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticProviderRequest.operation:required".into()); }
        if self.provider.trim().is_empty() { failures.push("SemanticProviderRequest.provider:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticProviderRequest.schema:required".into()); }

        if !["llms","semantic_extract",].contains(&self.interface_profile.as_str()) { failures.push("SemanticProviderRequest.interface_profile:status".into()); }
        if !["generate","generate_structured",].contains(&self.operation.as_str()) { failures.push("SemanticProviderRequest.operation:status".into()); }
        if !["groq","litellm","openai",].contains(&self.provider.as_str()) { failures.push("SemanticProviderRequest.provider:status".into()); }
        if !["semantica.provider-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticProviderRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderResult {
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 execution
    pub execution: String,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 operation
    pub operation: String,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 output
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub output: serde_json::Value,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 provider
    pub provider: String,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 provider_response_id
    pub provider_response_id: Option<String>,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 request_sha256
    pub request_sha256: String,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 requested_model
    pub requested_model: Option<String>,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 resolved_model
    pub resolved_model: Option<String>,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 response_sha256
    pub response_sha256: String,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 schema
    pub schema: String,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 transport_identity
    pub transport_identity: String,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 transport_kind
    pub transport_kind: String,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 usage
    pub usage: SemanticProviderUsage,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 usage_status
    pub usage_status: String,
    /// 模型答复与实际执行身份；首choice原含义，structured可为scalar/null；字段 wire_request_sha256
    pub wire_request_sha256: String,
}
impl SemanticProviderResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.execution.trim().is_empty() { failures.push("SemanticProviderResult.execution:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticProviderResult.operation:required".into()); }
        if self.provider.trim().is_empty() { failures.push("SemanticProviderResult.provider:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticProviderResult.request_sha256:sha256".into()); }
        if self.response_sha256.len() != 64 || !self.response_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticProviderResult.response_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticProviderResult.schema:required".into()); }
        if self.transport_identity.trim().is_empty() { failures.push("SemanticProviderResult.transport_identity:required".into()); }
        if self.transport_kind.trim().is_empty() { failures.push("SemanticProviderResult.transport_kind:required".into()); }
        if self.usage_status.trim().is_empty() { failures.push("SemanticProviderResult.usage_status:required".into()); }
        if self.wire_request_sha256.len() != 64 || !self.wire_request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticProviderResult.wire_request_sha256:sha256".into()); }

        if !["completed",].contains(&self.execution.as_str()) { failures.push("SemanticProviderResult.execution:status".into()); }
        if !["embed","generate","generate_structured",].contains(&self.operation.as_str()) { failures.push("SemanticProviderResult.operation:status".into()); }
        if !["groq","litellm","openai",].contains(&self.provider.as_str()) { failures.push("SemanticProviderResult.provider:status".into()); }
        if !["semantica.provider-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticProviderResult.schema:status".into()); }
        if !["invalid","partial","reported","unknown",].contains(&self.usage_status.as_str()) { failures.push("SemanticProviderResult.usage_status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一上层CredentialPool检验解析后结果；不另存凭据
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderResultGuard {
    /// 同一上层CredentialPool检验解析后结果；不另存凭据；字段 credential_material_absent
    pub credential_material_absent: bool,
    /// 同一上层CredentialPool检验解析后结果；不另存凭据；字段 schema
    pub schema: String,
}
impl SemanticProviderResultGuard {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticProviderResultGuard.schema:required".into()); }

        if !["semantica.provider-result-guard/1",].contains(&self.schema.as_str()) { failures.push("SemanticProviderResultGuard.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原响应报告的任意精度无符号整数；缺项为未知且不估零
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderUsage {
    /// 原响应报告的任意精度无符号整数；缺项为未知且不估零；字段 input_tokens
    pub input_tokens: Option<String>,
    /// 原响应报告的任意精度无符号整数；缺项为未知且不估零；字段 output_tokens
    pub output_tokens: Option<String>,
    /// 原响应报告的任意精度无符号整数；缺项为未知且不估零；字段 total_tokens
    pub total_tokens: Option<String>,
}
impl SemanticProviderUsage {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 唯一通用算法构造实际HTTP body；不含授权头
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderWireRequest {
    /// 唯一通用算法构造实际HTTP body；不含授权头；字段 body
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub body: serde_json::Value,
    /// 唯一通用算法构造实际HTTP body；不含授权头；字段 body_sha256
    pub body_sha256: String,
    /// 唯一通用算法构造实际HTTP body；不含授权头；字段 custom_endpoint
    pub custom_endpoint: bool,
    /// 唯一通用算法构造实际HTTP body；不含授权头；字段 operation
    pub operation: String,
    /// 唯一通用算法构造实际HTTP body；不含授权头；字段 provider
    pub provider: String,
}
impl SemanticProviderWireRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.body_sha256.len() != 64 || !self.body_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticProviderWireRequest.body_sha256:sha256".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticProviderWireRequest.operation:required".into()); }
        if self.provider.trim().is_empty() { failures.push("SemanticProviderWireRequest.provider:required".into()); }

        if !["embed","generate","generate_structured",].contains(&self.operation.as_str()) { failures.push("SemanticProviderWireRequest.operation:status".into()); }
        if !["groq","litellm","openai",].contains(&self.provider.as_str()) { failures.push("SemanticProviderWireRequest.provider:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 真实单次传输返回，正文只在安全检查后解析；禁止日志回显
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProviderWireResponse {
    /// 真实单次传输返回，正文只在安全检查后解析；禁止日志回显；字段 body
    pub body: String,
    /// 真实单次传输返回，正文只在安全检查后解析；禁止日志回显；字段 body_sha256
    pub body_sha256: String,
    /// 真实单次传输返回，正文只在安全检查后解析；禁止日志回显；字段 http_status
    pub http_status: u64,
    /// 真实单次传输返回，正文只在安全检查后解析；禁止日志回显；字段 transport_identity
    pub transport_identity: String,
    /// 真实单次传输返回，正文只在安全检查后解析；禁止日志回显；字段 transport_kind
    pub transport_kind: String,
}
impl SemanticProviderWireResponse {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.body_sha256.len() != 64 || !self.body_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticProviderWireResponse.body_sha256:sha256".into()); }
        if self.transport_identity.trim().is_empty() { failures.push("SemanticProviderWireResponse.transport_identity:required".into()); }
        if self.transport_kind.trim().is_empty() { failures.push("SemanticProviderWireResponse.transport_kind:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit versioned query options within the same Rust semantic owner and Dataset; no neutral default switch
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticQueryOptionsRequestV2 {
    /// Exact application names to NTriples RDF term strings; no credential or guessed values
    pub bindings: std::collections::BTreeMap<String, String>,
    /// Complete original UTF8 RDF source; empty graph is valid
    pub data: String,
    /// Exact SHA256 of supplied data UTF8 bytes
    pub data_sha256: String,
    /// Exact caller prefix to IRI map; empty is valid
    pub namespaces: std::collections::BTreeMap<String, String>,
    /// Original RDFLib Describe modifier keys with genuinely selected targets separate
    pub profile: String,
    /// Complete local read-only query; SERVICE FROM and mutation remain refused
    pub query: String,
    /// Explicit supported local source format
    pub rdf_format: String,
    /// Explicit versioned options request
    pub schema: String,
}
impl SemanticQueryOptionsRequestV2 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.data_sha256.len() != 64 || !self.data_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticQueryOptionsRequestV2.data_sha256:sha256".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV2.profile:required".into()); }
        if self.query.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV2.query:required".into()); }
        if self.rdf_format.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV2.rdf_format:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV2.schema:required".into()); }

        if !["rdflib760-describe-context/2",].contains(&self.profile.as_str()) { failures.push("SemanticQueryOptionsRequestV2.profile:status".into()); }
        if !["ntriples","rdfxml","turtle",].contains(&self.rdf_format.as_str()) { failures.push("SemanticQueryOptionsRequestV2.rdf_format:status".into()); }
        if !["semantica.query-options-request/2",].contains(&self.schema.as_str()) { failures.push("SemanticQueryOptionsRequestV2.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete local RDF/query/terms/source plus explicit bounded initial-shadow3 seam in same owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticQueryOptionsRequestV3 {
    /// Complete exact application names to NTriples RDF term strings
    pub bindings: std::collections::BTreeMap<String, String>,
    /// Complete explicitly bounded owned limits
    pub budget: SemanticQueryShadowBudgetV3,
    /// Complete UTF8 original RDF source, empty graph valid
    pub data: String,
    /// Complete data original byte SHA256
    pub data_sha256: String,
    /// Complete fixed owned scope policy declaration
    pub guard: SemanticQueryShadowGuardV3,
    /// Complete exact prefix to IRI map
    pub namespaces: std::collections::BTreeMap<String, String>,
    /// Explicit current RDFLib initial assignment shadow policy
    pub profile: String,
    /// Complete genuine local readonly query
    pub query: String,
    /// Declared source format
    pub rdf_format: String,
    /// Exact full options request version
    pub schema: String,
}
impl SemanticQueryOptionsRequestV3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.data_sha256.len() != 64 || !self.data_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticQueryOptionsRequestV3.data_sha256:sha256".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV3.profile:required".into()); }
        if self.query.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV3.query:required".into()); }
        if self.rdf_format.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV3.rdf_format:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV3.schema:required".into()); }

        if !["rdflib760-initial-shadow/3",].contains(&self.profile.as_str()) { failures.push("SemanticQueryOptionsRequestV3.profile:status".into()); }
        if !["ntriples","rdfxml","turtle",].contains(&self.rdf_format.as_str()) { failures.push("SemanticQueryOptionsRequestV3.rdf_format:status".into()); }
        if !["semantica.query-options-request/3",].contains(&self.schema.as_str()) { failures.push("SemanticQueryOptionsRequestV3.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit top-level grouping-key null projection; old neutral and compatibility contracts retain their original meaning
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticQueryOptionsRequestV4 {
    /// Exact application names to one NTriples RDF term each; immutable context and existing computed-shadow policy remain
    pub bindings: std::collections::BTreeMap<String, String>,
    /// Reuse owned V3 capacities unchanged; callers may only tighten them and stack remains exactly8MiB
    pub budget: SemanticQueryShadowBudgetV3,
    /// Complete original local RDF UTF-8 source, retaining existing16MiB decoded bound
    pub data: String,
    /// SHA256 of complete original data bytes, checked before parse
    pub data_sha256: String,
    /// Reuse existing original-query declaration, projection, aggregate-target, same-Dataset and remote guards verbatim
    pub guard: SemanticQueryShadowGuardV3,
    /// Exact prefix-to-IRI declarations, within existing parameter count and byte bounds
    pub namespaces: std::collections::BTreeMap<String, String>,
    /// Caller explicitly selects top-level GROUP key null projection in the existing RDFLib initial-shadow evaluator
    pub profile: String,
    /// Complete genuine query parsed by the same parser; at most existing1MiB with all local-only guards
    pub query: String,
    /// Declared existing RDF syntax with no implicit format inference
    pub rdf_format: String,
    /// Explicit request4 identity, never inferred from the shape of an old request
    pub schema: String,
}
impl SemanticQueryOptionsRequestV4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.data_sha256.len() != 64 || !self.data_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticQueryOptionsRequestV4.data_sha256:sha256".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV4.profile:required".into()); }
        if self.query.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV4.query:required".into()); }
        if self.rdf_format.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV4.rdf_format:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticQueryOptionsRequestV4.schema:required".into()); }

        if !["rdflib760-group-null-projection/4",].contains(&self.profile.as_str()) { failures.push("SemanticQueryOptionsRequestV4.profile:status".into()); }
        if !["ntriples","rdfxml","turtle",].contains(&self.rdf_format.as_str()) { failures.push("SemanticQueryOptionsRequestV4.rdf_format:status".into()); }
        if !["semantica.query-options-request/4",].contains(&self.schema.as_str()) { failures.push("SemanticQueryOptionsRequestV4.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Versioned query options result wraps the complete unchanged neutral query result
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticQueryOptionsResultV2 {
    /// Actually executed Describe target/context compatibility profile
    pub profile: String,
    /// Complete original SemanticQueryResult with query operation data identity and output
    pub query_result: SemanticQueryResult,
    /// Exact canonical complete typed request identity
    pub request_sha256: String,
    /// Explicit result envelope version
    pub schema: String,
}
impl SemanticQueryOptionsResultV2 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.profile.trim().is_empty() { failures.push("SemanticQueryOptionsResultV2.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticQueryOptionsResultV2.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticQueryOptionsResultV2.schema:required".into()); }

        if !["rdflib760-describe-context/2",].contains(&self.profile.as_str()) { failures.push("SemanticQueryOptionsResultV2.profile:status".into()); }
        if !["semantica.query-options-result/2",].contains(&self.schema.as_str()) { failures.push("SemanticQueryOptionsResultV2.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete original semantic result wrapped with actual request/policy/budget identity
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticQueryOptionsResultV3 {
    /// Actually executed initial-shadow3 policy
    pub profile: String,
    /// Complete actual old result fields including query kind/data SHA and all rows/triples
    pub query_result: SemanticQueryResult,
    /// Canonical complete current typed request SHA
    pub request_sha256: String,
    /// Exact result envelope
    pub schema: String,
}
impl SemanticQueryOptionsResultV3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.profile.trim().is_empty() { failures.push("SemanticQueryOptionsResultV3.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticQueryOptionsResultV3.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticQueryOptionsResultV3.schema:required".into()); }

        if !["rdflib760-initial-shadow/3",].contains(&self.profile.as_str()) { failures.push("SemanticQueryOptionsResultV3.profile:status".into()); }
        if !["semantica.query-options-result/3",].contains(&self.schema.as_str()) { failures.push("SemanticQueryOptionsResultV3.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit top-level grouping-key null projection; old neutral and compatibility contracts retain their original meaning
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticQueryOptionsResultV4 {
    /// Profile actually selected by this bounded runtime
    pub profile: String,
    /// Actual same-owner result; only a projected grouping key missing from an already emitted, AST-proved top-level group row is materialized as JSON null; null is never an RDF term
    pub query_result: SemanticQueryResult,
    /// SHA256 of canonical complete typed V4 request, including original sources, budgets and policies
    pub request_sha256: String,
    /// Explicit result4 envelope
    pub schema: String,
}
impl SemanticQueryOptionsResultV4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.profile.trim().is_empty() { failures.push("SemanticQueryOptionsResultV4.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticQueryOptionsResultV4.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticQueryOptionsResultV4.schema:required".into()); }

        if !["rdflib760-group-null-projection/4",].contains(&self.profile.as_str()) { failures.push("SemanticQueryOptionsResultV4.profile:status".into()); }
        if !["semantica.query-options-result/4",].contains(&self.schema.as_str()) { failures.push("SemanticQueryOptionsResultV4.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 通用原始RDF与只读查询合同；无工程Project、设备或凭据依赖
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticQueryRequest {
    /// 原始UTF-8 RDF全部字节，允许空图；required只要求字段存在
    pub data: String,
    /// 对原始data UTF-8字节的精确SHA256，禁止摘要不同内容
    pub data_sha256: String,
    /// 非空本地只读SPARQL；不开放SERVICE/FROM/UPDATE
    pub query: String,
    /// 明确支持的解析格式，未声明能力拒绝
    pub rdf_format: String,
    /// 通用协议版本
    pub schema: String,
}
impl SemanticQueryRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.data_sha256.len() != 64 || !self.data_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticQueryRequest.data_sha256:sha256".into()); }
        if self.query.trim().is_empty() { failures.push("SemanticQueryRequest.query:required".into()); }
        if self.rdf_format.trim().is_empty() { failures.push("SemanticQueryRequest.rdf_format:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticQueryRequest.schema:required".into()); }

        if !["ntriples","rdfxml","turtle",].contains(&self.rdf_format.as_str()) { failures.push("SemanticQueryRequest.rdf_format:status".into()); }
        if !["semantica.query-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticQueryRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 通用查询完成状态与原RDF身份；查询完成不代表工程通过
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticQueryResult {
    /// 保持请求原RDF身份不改签
    pub data_sha256: String,
    /// 查询执行状态，结论/工程采用在调用者独立记录
    pub execution: String,
    /// 已解析只读查询的结果种类
    pub query_kind: String,
    /// RDF术语原词法/datatype/language或明确查询DTO，空结果保持空
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub result: serde_json::Value,
}
impl SemanticQueryResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.data_sha256.len() != 64 || !self.data_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticQueryResult.data_sha256:sha256".into()); }
        if self.execution.trim().is_empty() { failures.push("SemanticQueryResult.execution:required".into()); }
        if self.query_kind.trim().is_empty() { failures.push("SemanticQueryResult.query_kind:required".into()); }

        if !["completed","failed","not_run","partial",].contains(&self.execution.as_str()) { failures.push("SemanticQueryResult.execution:status".into()); }
        if !["ask","construct","describe","select",].contains(&self.query_kind.as_str()) { failures.push("SemanticQueryResult.query_kind:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Owned limits for explicit current shadow query, caller may only tighten existing caps
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticQueryShadowBudgetV3 {
    /// Existing bounded genuine AST depth maximum96
    pub ast_depth: u64,
    /// Existing bounded genuine AST nodes maximum100000
    pub ast_nodes: u64,
    /// Complete RDF source byte cap at most16MiB
    pub data_bytes: u64,
    /// Total original prefix/name/N3 term byte cap at most16MiB
    pub parameter_bytes: u64,
    /// Prefix and binding map count each at most128
    pub parameter_count: u64,
    /// One Dataset maximum100000 quads
    pub quads: u64,
    /// Complete query byte cap at most1MiB
    pub query_bytes: u64,
    /// Complete SELECT maximum10000 rows
    pub rows: u64,
    /// Existing dedicated query stack exactly8388608 bytes
    pub stack_bytes: u64,
    /// Complete graph result maximum100000 triples
    pub triples: u64,
}
impl SemanticQueryShadowBudgetV3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Fixed owned shadow policy declaration, not caller authority to remove guards or raise scope
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticQueryShadowGuardV3 {
    /// Only external initial value collisions at genuine legal Extend targets
    pub assignment_scope: String,
    /// Successful computed values have source-origin marks outside RDF identity
    pub computed_scope: String,
    /// One current Dataset and evaluator, no parallel registry or Dataset
    pub dataset_scope: String,
    /// Direct aggregate destination collision remains refused
    pub group_guard: String,
    /// Original application values remain exact immutable context
    pub initial_context: String,
    /// Project publishes selected values and marks only
    pub project_scope: String,
    /// Query-scope redeclarations/duplicates retain parser refusal
    pub query_scope: String,
    /// SERVICE FROM and mutating operations remain refused
    pub remote_guard: String,
    /// Exact guard policy schema
    pub schema: String,
}
impl SemanticQueryShadowGuardV3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.assignment_scope.trim().is_empty() { failures.push("SemanticQueryShadowGuardV3.assignment_scope:required".into()); }
        if self.computed_scope.trim().is_empty() { failures.push("SemanticQueryShadowGuardV3.computed_scope:required".into()); }
        if self.dataset_scope.trim().is_empty() { failures.push("SemanticQueryShadowGuardV3.dataset_scope:required".into()); }
        if self.group_guard.trim().is_empty() { failures.push("SemanticQueryShadowGuardV3.group_guard:required".into()); }
        if self.initial_context.trim().is_empty() { failures.push("SemanticQueryShadowGuardV3.initial_context:required".into()); }
        if self.project_scope.trim().is_empty() { failures.push("SemanticQueryShadowGuardV3.project_scope:required".into()); }
        if self.query_scope.trim().is_empty() { failures.push("SemanticQueryShadowGuardV3.query_scope:required".into()); }
        if self.remote_guard.trim().is_empty() { failures.push("SemanticQueryShadowGuardV3.remote_guard:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticQueryShadowGuardV3.schema:required".into()); }

        if !["external_initial_targets_only",].contains(&self.assignment_scope.as_str()) { failures.push("SemanticQueryShadowGuardV3.assignment_scope:status".into()); }
        if !["marked_tuple_shadow",].contains(&self.computed_scope.as_str()) { failures.push("SemanticQueryShadowGuardV3.computed_scope:status".into()); }
        if !["same_owner_dataset",].contains(&self.dataset_scope.as_str()) { failures.push("SemanticQueryShadowGuardV3.dataset_scope:status".into()); }
        if !["aggregate_target_refused",].contains(&self.group_guard.as_str()) { failures.push("SemanticQueryShadowGuardV3.group_guard:status".into()); }
        if !["immutable_exact_terms",].contains(&self.initial_context.as_str()) { failures.push("SemanticQueryShadowGuardV3.initial_context:status".into()); }
        if !["selected_tuple_domain",].contains(&self.project_scope.as_str()) { failures.push("SemanticQueryShadowGuardV3.project_scope:status".into()); }
        if !["genuine_parser_declarations",].contains(&self.query_scope.as_str()) { failures.push("SemanticQueryShadowGuardV3.query_scope:status".into()); }
        if !["service_from_refused",].contains(&self.remote_guard.as_str()) { failures.push("SemanticQueryShadowGuardV3.remote_guard:status".into()); }
        if !["semantica.query-shadow-guard/3",].contains(&self.schema.as_str()) { failures.push("SemanticQueryShadowGuardV3.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 工程本体规定内容、来源与采用不同责任；原RDFLib快照的Rust算法合同
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRdfLibDatasetSnapshot {
    /// 原RDFLib算法身份，不能借RDFC改签
    pub algorithm: String,
    /// 原n3排序以换行连接、无末尾换行，多行literal仍保持三引号
    pub canonical_nquads: String,
    /// 观察时间，可按原接口重绑定，不参与内容摘要
    pub created_at: String,
    /// 原n3 UTF8字节摘要
    pub dataset_sha256: String,
    /// 原term域的去重四元组数，保留plain与显式xsd:string区别
    pub quad_count: u64,
    /// 同一DatasetSession当前mutation revision，不参与内容摘要
    pub revision: u64,
    /// 原快照数据schema
    pub schema_version: String,
}
impl SemanticRdfLibDatasetSnapshot {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.algorithm.trim().is_empty() { failures.push("SemanticRdfLibDatasetSnapshot.algorithm:required".into()); }
        if self.dataset_sha256.len() != 64 || !self.dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticRdfLibDatasetSnapshot.dataset_sha256:sha256".into()); }
        if self.schema_version.trim().is_empty() { failures.push("SemanticRdfLibDatasetSnapshot.schema_version:required".into()); }

        if !["semantica-rdf-dataset-c14n-v2",].contains(&self.algorithm.as_str()) { failures.push("SemanticRdfLibDatasetSnapshot.algorithm:status".into()); }
        if !["1.0",].contains(&self.schema_version.as_str()) { failures.push("SemanticRdfLibDatasetSnapshot.schema_version:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一实际保留term投影中的语言大小写别名观察；不代表已覆盖所有歧义或原来源认证
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRdfLibLanguageCaseAlias {
    /// 分组中原datatype，None保持原term含义
    pub datatype: Option<String>,
    /// 原Literal相等键的ASCII小写language；原拼写仍单独保留
    pub language_lowercase: String,
    /// 该完整原term键下实际仍保留的不同language拼写，按原字符串排序
    pub retained_spellings: Vec<String>,
    /// 保留literal text原UTF8字节摘要；不将不同text因摘要相同合并
    pub text_sha256: String,
}
impl SemanticRdfLibLanguageCaseAlias {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.text_sha256.len() != 64 || !self.text_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticRdfLibLanguageCaseAlias.text_sha256:sha256".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 工程本体规定内容、来源与采用不同责任；原RDFLib快照的Rust算法合同
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRdfLibSnapshotRequest {
    /// 调用者观察时间；空值用本轮host UTC，不授予任何权限
    pub created_at: String,
}
impl SemanticRdfLibSnapshotRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit neutral request to the same stateless textual reasoner used by BookRunner; no Project, Dataset or registry is constructed
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticReasonRequest {
    /// Ground facts as UTF-8 text or an ordered JSON array of strings; every array item yields exactly one noncomment fact, then facts are sorted and deduplicated
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub facts: serde_json::Value,
    /// Explicit first closure pass bound; a second closure probe must yield no additional facts before complete can be reported
    pub max_iterations: u64,
    /// UTF-8 rules text or an ordered JSON array mixing one-rule strings and objects with rule_id/text plus optional confidence/priority; mapping is the explicit wire projection of ForwardRuleDTO
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub rules: serde_json::Value,
    /// Explicit structured reasoning transport version, separate from old MCP infer_facts and kernel rules
    pub schema: String,
}
impl SemanticReasonRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticReasonRequest.schema:required".into()); }

        if !["semantica.reason-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticReasonRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Source-bound normalized facts, ordered rule metadata and actual inference details from the shared Rust owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticReasonResult {
    /// Original ForwardChainResultDTO projection: input_facts, priority-ordered rules, inferences with rule_id/rule_text/premises/confidence/explanation, all_facts, max_iterations, semantics and complete
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub data: serde_json::Value,
    /// SHA256 of canonical JSON of the entire original typed request, before rule defaults, normalization or sorting
    pub request_sha256: String,
    /// Current structured reasoning result transport version
    pub schema: String,
}
impl SemanticReasonResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticReasonResult.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticReasonResult.schema:required".into()); }

        if !["semantica.reason-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticReasonResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original AssetDecisionDTO as_dict public projection at refinery.py:1518-1584; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryAssetDecision {
    /// Original AssetDecisionDTO as_dict public projection at refinery.py:1518-1584; source requirement, not completion: asset_id
    pub asset_id: String,
    /// Original AssetDecisionDTO as_dict public projection at refinery.py:1518-1584; source requirement, not completion: category
    pub category: String,
    /// Original AssetDecisionDTO as_dict public projection at refinery.py:1518-1584; source requirement, not completion: operation
    pub operation: String,
    /// Original AssetDecisionDTO as_dict public projection at refinery.py:1518-1584; source requirement, not completion: reason
    pub reason: String,
    /// Original AssetDecisionDTO as_dict public projection at refinery.py:1518-1584; source requirement, not completion: replaces_sha256
    pub replaces_sha256: String,
    /// Original AssetDecisionDTO as_dict public projection at refinery.py:1518-1584; source requirement, not completion: verdict
    pub verdict: String,
}
impl SemanticRefineryAssetDecision {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryCandidateVerification {
    /// Original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: execution_suite_sha256
    pub execution_suite_sha256: String,
    /// Original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: regression_evidence
    pub regression_evidence: SemanticRefineryRefineryGateEvidence,
    /// Original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: release_evidence
    pub release_evidence: SemanticRefineryRefineryGateEvidence,
    /// Original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: schema_version
    pub schema_version: String,
    /// Original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: state
    pub state: SemanticRefineryRefineryState,
    /// Original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: verification_sha256
    pub verification_sha256: String,
}
impl SemanticRefineryCandidateVerification {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original case_assets tuple projects to explicit object
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryCaseAssetBinding {
    /// Original case_assets tuple projects to explicit object: asset_id
    pub asset_id: String,
    /// Original case_assets tuple projects to explicit object: case_kind
    pub case_kind: String,
    /// Original case_assets tuple projects to explicit object: sha256
    pub sha256: String,
}
impl SemanticRefineryCaseAssetBinding {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original transition_contexts tuple projects to explicit object
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryContextBinding {
    /// Original transition_contexts tuple projects to explicit object: action
    pub action: String,
    /// Original transition_contexts tuple projects to explicit object: context_object_sha256
    pub context_object_sha256: String,
    /// Original transition_contexts tuple projects to explicit object: context_sha256
    pub context_sha256: String,
}
impl SemanticRefineryContextBinding {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original EngagementPhaseDTO as_dict public projection at refinery.py:686-800; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryEngagementPhase {
    /// Original EngagementPhaseDTO as_dict public projection at refinery.py:686-800; source requirement, not completion: details
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub details: serde_json::Value,
    /// Original EngagementPhaseDTO as_dict public projection at refinery.py:686-800; source requirement, not completion: evidence_sha256
    pub evidence_sha256: Option<String>,
    /// Original EngagementPhaseDTO as_dict public projection at refinery.py:686-800; source requirement, not completion: missing_capabilities
    pub missing_capabilities: Vec<String>,
    /// Original EngagementPhaseDTO as_dict public projection at refinery.py:686-800; source requirement, not completion: name
    pub name: String,
    /// Original EngagementPhaseDTO as_dict public projection at refinery.py:686-800; source requirement, not completion: observed_capabilities
    pub observed_capabilities: Vec<String>,
    /// Original EngagementPhaseDTO as_dict public projection at refinery.py:686-800; source requirement, not completion: required_capabilities
    pub required_capabilities: Vec<String>,
    /// Original EngagementPhaseDTO as_dict public projection at refinery.py:686-800; source requirement, not completion: status
    pub status: String,
}
impl SemanticRefineryEngagementPhase {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original ExecutionReceiptReferenceDTO as_dict public projection at refinery.py:804-844; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryExecutionReceiptReference {
    /// Original ExecutionReceiptReferenceDTO as_dict public projection at refinery.py:804-844; source requirement, not completion: package_digest
    pub package_digest: String,
    /// Original ExecutionReceiptReferenceDTO as_dict public projection at refinery.py:804-844; source requirement, not completion: package_id
    pub package_id: String,
    /// Original ExecutionReceiptReferenceDTO as_dict public projection at refinery.py:804-844; source requirement, not completion: package_version
    pub package_version: String,
    /// Original ExecutionReceiptReferenceDTO as_dict public projection at refinery.py:804-844; source requirement, not completion: receipt_sha256
    pub receipt_sha256: String,
}
impl SemanticRefineryExecutionReceiptReference {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Typed lifecycle failure preserving malformed/state/authority/unknown distinctions
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryFailure {
    /// Typed lifecycle failure preserving malformed/state/authority/unknown distinctions: code
    pub code: String,
    /// Typed lifecycle failure preserving malformed/state/authority/unknown distinctions: message
    pub message: String,
    /// Typed lifecycle failure preserving malformed/state/authority/unknown distinctions: phase
    pub phase: String,
    /// Typed lifecycle failure preserving malformed/state/authority/unknown distinctions: retryable
    pub retryable: bool,
}
impl SemanticRefineryFailure {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit reference execution and target-subject plan; reference actual observations bootstrap engagement and never waive target twelve gates
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryFullObservationPlan {
    /// Exact current full observation input: reference_manifest_path
    pub reference_manifest_path: String,
    /// Exact current full observation input: reference_manifest_sha256
    pub reference_manifest_sha256: String,
    /// Exact current full observation input: reference_scenario_ids
    pub reference_scenario_ids: Vec<String>,
    /// Exact current full observation input: required_regression_check_ids
    pub required_regression_check_ids: Vec<String>,
    /// Exact current full observation input: required_release_check_ids
    pub required_release_check_ids: Vec<String>,
    /// Exact expected current Rust source observation; host reads current source and executable before/after actual work and compares this input
    pub runtime_source: SemanticRefineryRustSource,
    /// Exact current full observation input: schema
    pub schema: String,
    /// Exact current full observation input: target_manifest_path
    pub target_manifest_path: String,
    /// Exact current full observation input: target_manifest_sha256
    pub target_manifest_sha256: String,
}
impl SemanticRefineryFullObservationPlan {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.reference_manifest_path.trim().is_empty() { failures.push("SemanticRefineryFullObservationPlan.reference_manifest_path:required".into()); }
        if self.reference_manifest_sha256.trim().is_empty() { failures.push("SemanticRefineryFullObservationPlan.reference_manifest_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticRefineryFullObservationPlan.schema:required".into()); }
        if self.target_manifest_path.trim().is_empty() { failures.push("SemanticRefineryFullObservationPlan.target_manifest_path:required".into()); }
        if self.target_manifest_sha256.trim().is_empty() { failures.push("SemanticRefineryFullObservationPlan.target_manifest_sha256:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original GateCheckDTO as_dict public projection at refinery.py:1707-1744; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryGateCheck {
    /// Original GateCheckDTO as_dict public projection at refinery.py:1707-1744; source requirement, not completion: check_id
    pub check_id: String,
    /// Original GateCheckDTO as_dict public projection at refinery.py:1707-1744; source requirement, not completion: message
    pub message: String,
    /// Original GateCheckDTO as_dict public projection at refinery.py:1707-1744; source requirement, not completion: output_sha256
    pub output_sha256: String,
    /// Original GateCheckDTO as_dict public projection at refinery.py:1707-1744; source requirement, not completion: passed
    pub passed: bool,
}
impl SemanticRefineryGateCheck {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryIndustryPackageDescriptor {
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: authorization_object_sha256
    pub authorization_object_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: authorization_sha256
    pub authorization_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: book_impact
    pub book_impact: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: delta_sha256
    pub delta_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: engagement_receipt_sha256
    pub engagement_receipt_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: manifest_sha256
    pub manifest_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: package_id
    pub package_id: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: package_sha256
    pub package_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: promoted_at
    pub promoted_at: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: promotion_record_sha256
    pub promotion_record_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: provenance_closure_object_sha256
    pub provenance_closure_object_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: provenance_closure_sha256
    pub provenance_closure_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: regression_evidence_sha256
    pub regression_evidence_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: release_evidence_sha256
    pub release_evidence_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: subject_execution_suite_sha256
    pub subject_execution_suite_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: transition_context_object_sha256
    pub transition_context_object_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: transition_context_sha256
    pub transition_context_sha256: String,
    /// Original IndustryPackageDescriptorDTO as_dict public projection at refinery.py:3026-3101; source requirement, not completion: version
    pub version: String,
}
impl SemanticRefineryIndustryPackageDescriptor {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original LearningResultDTO as_dict public projection at refinery.py:848-886; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryLearningResult {
    /// Original LearningResultDTO as_dict public projection at refinery.py:848-886; source requirement, not completion: delta_sha256
    pub delta_sha256: Option<String>,
    /// Original LearningResultDTO as_dict public projection at refinery.py:848-886; source requirement, not completion: rationale
    pub rationale: String,
    /// Original LearningResultDTO as_dict public projection at refinery.py:848-886; source requirement, not completion: status
    pub status: String,
}
impl SemanticRefineryLearningResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Current Rust subject-observation plan; requested checks do not certify candidate eligibility
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryObservationRequest {
    /// Private manifest path read and checked byte-for-byte before and after actual execution
    pub manifest_path: String,
    /// Ordered bounded one-to-sixty-four exact scenario IDs executed by the existing BookRunner/DatasetSession
    pub scenario_ids: Vec<String>,
    /// Explicit request to observe actual Rust package/scenario execution; no caller phase claims
    pub schema: String,
}
impl SemanticRefineryObservationRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if !["semantica.refinery-observation-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticRefineryObservationRequest.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original PackageAssetDeltaDTO as_dict public projection at refinery.py:1160-1297; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryPackageAssetDelta {
    /// Original PackageAssetDeltaDTO as_dict public projection at refinery.py:1160-1297; source requirement, not completion: asset_id
    pub asset_id: String,
    /// Original PackageAssetDeltaDTO as_dict public projection at refinery.py:1160-1297; source requirement, not completion: case_kind
    pub case_kind: Option<String>,
    /// Original PackageAssetDeltaDTO as_dict public projection at refinery.py:1160-1297; source requirement, not completion: category
    pub category: String,
    /// Original PackageAssetDeltaDTO as_dict public projection at refinery.py:1160-1297; source requirement, not completion: content_base64
    pub content_base64: Option<String>,
    /// Original PackageAssetDeltaDTO as_dict public projection at refinery.py:1160-1297; source requirement, not completion: media_type
    pub media_type: String,
    /// Original PackageAssetDeltaDTO as_dict public projection at refinery.py:1160-1297; source requirement, not completion: operation
    pub operation: String,
    /// Original PackageAssetDeltaDTO as_dict public projection at refinery.py:1160-1297; source requirement, not completion: replaces_sha256
    pub replaces_sha256: Option<String>,
    /// Original PackageAssetDeltaDTO as_dict public projection at refinery.py:1160-1297; source requirement, not completion: role
    pub role: Option<String>,
    /// Original PackageAssetDeltaDTO as_dict public projection at refinery.py:1160-1297; source requirement, not completion: sha256
    pub sha256: String,
}
impl SemanticRefineryPackageAssetDelta {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryPackageDelta {
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: base_package_sha256
    pub base_package_sha256: String,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: base_version
    pub base_version: String,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: book_impact
    pub book_impact: String,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: cases
    pub cases: Vec<SemanticRefineryPackageAssetDelta>,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: competency_questions
    pub competency_questions: Vec<SemanticRefineryPackageAssetDelta>,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: contract
    pub contract: Vec<SemanticRefineryPackageAssetDelta>,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: created_at
    pub created_at: String,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: created_by
    pub created_by: String,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: delta_sha256
    pub delta_sha256: String,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: ontology
    pub ontology: Vec<SemanticRefineryPackageAssetDelta>,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: package_id
    pub package_id: String,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: provenance
    pub provenance: Vec<SemanticRefineryPackageAssetDelta>,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: queries
    pub queries: Vec<SemanticRefineryPackageAssetDelta>,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: rationale
    pub rationale: String,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: required_capabilities
    pub required_capabilities: Vec<String>,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: rules
    pub rules: Vec<SemanticRefineryPackageAssetDelta>,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: schema_version
    pub schema_version: String,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: shapes
    pub shapes: Vec<SemanticRefineryPackageAssetDelta>,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: source_evidence
    pub source_evidence: Vec<SemanticRefinerySourceEvidence>,
    /// Original PackageDelta as_dict public projection at refinery.py:1301-1514; source requirement, not completion: target_version
    pub target_version: String,
}
impl SemanticRefineryPackageDelta {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryProjectOntologyBinding {
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: allowed_actions
    pub allowed_actions: Vec<String>,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: baseline_package_sha256
    pub baseline_package_sha256: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: baseline_version
    pub baseline_version: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: binding_id
    pub binding_id: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: binding_sha256
    pub binding_sha256: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: created_at
    pub created_at: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: decision_authorities
    pub decision_authorities: Vec<String>,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: domain
    pub domain: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: evidence_root
    pub evidence_root: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: fact_authorities
    pub fact_authorities: Vec<String>,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: package_id
    pub package_id: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: project_id
    pub project_id: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: promotion_target
    pub promotion_target: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: schema_version
    pub schema_version: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: semantic_api_contract
    pub semantic_api_contract: String,
    /// Original ProjectOntologyBinding as_dict public projection at refinery.py:416-569; source requirement, not completion: workspace_id
    pub workspace_id: String,
}
impl SemanticRefineryProjectOntologyBinding {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryProvenanceClosure {
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: candidate_binding_sha256
    pub candidate_binding_sha256: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: candidate_envelope_sha256
    pub candidate_envelope_sha256: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: closure_sha256
    pub closure_sha256: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: delta_sha256
    pub delta_sha256: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: engagement_receipt_sha256
    pub engagement_receipt_sha256: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: execution_projection_sha256
    pub execution_projection_sha256: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: manifest_sha256
    pub manifest_sha256: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: package_id
    pub package_id: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: package_sha256
    pub package_sha256: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: package_version
    pub package_version: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: phase
    pub phase: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: prior_closure_sha256
    pub prior_closure_sha256: Option<String>,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: provenance_evidence_assets
    pub provenance_evidence_assets: std::collections::BTreeMap<String, String>,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: runtime_source
    pub runtime_source: SemanticRefineryRuntimeSourceIdentity,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: scenarios
    pub scenarios: Vec<SemanticRefineryProvenanceScenarioBinding>,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: schema_version
    pub schema_version: String,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: source_evidence
    pub source_evidence: Vec<SemanticRefinerySourceEvidence>,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: transition_contexts
    pub transition_contexts: Vec<SemanticRefineryContextBinding>,
    /// Original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: workspace_id
    pub workspace_id: String,
}
impl SemanticRefineryProvenanceClosure {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original ProvenanceScenarioBindingDTO as_dict public projection at refinery.py:2115-2218; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryProvenanceScenarioBinding {
    /// Original ProvenanceScenarioBindingDTO as_dict public projection at refinery.py:2115-2218; source requirement, not completion: case_assets
    pub case_assets: Vec<SemanticRefineryCaseAssetBinding>,
    /// Original ProvenanceScenarioBindingDTO as_dict public projection at refinery.py:2115-2218; source requirement, not completion: input_asset_hashes
    pub input_asset_hashes: std::collections::BTreeMap<String, String>,
    /// Original ProvenanceScenarioBindingDTO as_dict public projection at refinery.py:2115-2218; source requirement, not completion: native_provenance_bundle_sha256
    pub native_provenance_bundle_sha256: String,
    /// Original ProvenanceScenarioBindingDTO as_dict public projection at refinery.py:2115-2218; source requirement, not completion: output_hashes
    pub output_hashes: std::collections::BTreeMap<String, String>,
    /// Original ProvenanceScenarioBindingDTO as_dict public projection at refinery.py:2115-2218; source requirement, not completion: receipt_object_sha256
    pub receipt_object_sha256: String,
    /// Original ProvenanceScenarioBindingDTO as_dict public projection at refinery.py:2115-2218; source requirement, not completion: receipt_sha256
    pub receipt_sha256: String,
    /// Original ProvenanceScenarioBindingDTO as_dict public projection at refinery.py:2115-2218; source requirement, not completion: run_result_object_sha256
    pub run_result_object_sha256: String,
    /// Original ProvenanceScenarioBindingDTO as_dict public projection at refinery.py:2115-2218; source requirement, not completion: scenario_id
    pub scenario_id: String,
}
impl SemanticRefineryProvenanceScenarioBinding {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRefineryAuthorization {
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: action
    pub action: String,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: actor_id
    pub actor_id: String,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: authority
    pub authority: String,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: authorization_id
    pub authorization_id: String,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: authorization_sha256
    pub authorization_sha256: String,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: decisions
    pub decisions: Vec<SemanticRefineryAssetDecision>,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: delta_sha256
    pub delta_sha256: String,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: issued_at
    pub issued_at: String,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: package_id
    pub package_id: String,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: promotion_target
    pub promotion_target: String,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: reason
    pub reason: String,
    /// Original RefineryAuthorizationDTO as_dict public projection at refinery.py:1588-1703; source requirement, not completion: source
    pub source: SemanticRefinerySourceEvidence,
}
impl SemanticRefineryRefineryAuthorization {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRefineryGateEvidence {
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: checks
    pub checks: Vec<SemanticRefineryGateCheck>,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: evidence_sha256
    pub evidence_sha256: String,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: execution_suite_object_sha256
    pub execution_suite_object_sha256: String,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: execution_suite_sha256
    pub execution_suite_sha256: String,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: gate
    pub gate: String,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: missing_capabilities
    pub missing_capabilities: Vec<String>,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: observed_capabilities
    pub observed_capabilities: Vec<String>,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: package_sha256
    pub package_sha256: String,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: provenance_closure_object_sha256
    pub provenance_closure_object_sha256: String,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: provenance_closure_sha256
    pub provenance_closure_sha256: String,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: recorded_at
    pub recorded_at: String,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: required_capabilities
    pub required_capabilities: Vec<String>,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: runtime_source
    pub runtime_source: SemanticRefineryRuntimeSourceIdentity,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: status
    pub status: String,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: transition_context_object_sha256
    pub transition_context_object_sha256: String,
    /// Original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: transition_context_sha256
    pub transition_context_sha256: String,
}
impl SemanticRefineryRefineryGateEvidence {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original RefineryStateDTO as_dict public projection at refinery.py:2817-2898; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRefineryState {
    /// Original RefineryStateDTO as_dict public projection at refinery.py:2817-2898; source requirement, not completion: delta_sha256
    pub delta_sha256: String,
    /// Original RefineryStateDTO as_dict public projection at refinery.py:2817-2898; source requirement, not completion: event_sha256
    pub event_sha256: String,
    /// Original RefineryStateDTO as_dict public projection at refinery.py:2817-2898; source requirement, not completion: package_id
    pub package_id: String,
    /// Original RefineryStateDTO as_dict public projection at refinery.py:2817-2898; source requirement, not completion: package_sha256
    pub package_sha256: Option<String>,
    /// Original RefineryStateDTO as_dict public projection at refinery.py:2817-2898; source requirement, not completion: previous_event_sha256
    pub previous_event_sha256: Option<String>,
    /// Original RefineryStateDTO as_dict public projection at refinery.py:2817-2898; source requirement, not completion: recorded_at
    pub recorded_at: String,
    /// Original RefineryStateDTO as_dict public projection at refinery.py:2817-2898; source requirement, not completion: sequence
    pub sequence: u64,
    /// Original RefineryStateDTO as_dict public projection at refinery.py:2817-2898; source requirement, not completion: state
    pub state: String,
}
impl SemanticRefineryRefineryState {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit neutral lifecycle request; host policy/source checks retain authority
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRequest {
    /// Explicit neutral lifecycle request; host policy/source checks retain authority: authorization
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub authorization: Option<serde_json::Value>,
    /// Explicit neutral lifecycle request; host policy/source checks retain authority: binding
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub binding: serde_json::Value,
    /// Explicit neutral lifecycle request; host policy/source checks retain authority: candidate
    pub candidate: Option<String>,
    /// Explicit neutral lifecycle request; host policy/source checks retain authority: delta
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub delta: Option<serde_json::Value>,
    /// Explicit neutral lifecycle request; host policy/source checks retain authority: engagement
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub engagement: Option<serde_json::Value>,
    /// Explicit neutral lifecycle request; host policy/source checks retain authority: operation
    pub operation: String,
    /// Explicit neutral lifecycle request; host policy/source checks retain authority: recorded_at
    pub recorded_at: String,
    /// Explicit neutral lifecycle request; host policy/source checks retain authority: registry_id
    pub registry_id: String,
    /// Explicit neutral lifecycle request; host policy/source checks retain authority: schema
    pub schema: String,
    /// Explicit neutral lifecycle request; host policy/source checks retain authority: task
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub task: Option<serde_json::Value>,
}
impl SemanticRefineryRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if !["commit","history","open","promote","propose","status","verify",].contains(&self.operation.as_str()) { failures.push("SemanticRefineryRequest.operation:allowed_values".into()); }
        if !["semantica.refinery-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticRefineryRequest.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Current neutral lifecycle observation; incomplete gates and full original scope remain uncovered
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryResult {
    /// Current neutral lifecycle observation; incomplete gates and full original scope remain uncovered: operation
    pub operation: String,
    /// Current neutral lifecycle observation; incomplete gates and full original scope remain uncovered: profile
    pub profile: String,
    /// Current neutral lifecycle observation; incomplete gates and full original scope remain uncovered: result
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub result: serde_json::Value,
    /// Current neutral lifecycle observation; incomplete gates and full original scope remain uncovered: schema
    pub schema: String,
    /// Current neutral lifecycle observation; incomplete gates and full original scope remain uncovered: status
    pub status: String,
    /// Current neutral lifecycle observation; incomplete gates and full original scope remain uncovered: uncovered
    pub uncovered: Vec<String>,
}
impl SemanticRefineryResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if !["semantica.refinery-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticRefineryResult.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original RuntimeSourceIdentityDTO as_dict public projection at refinery.py:245-288; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRuntimeSourceIdentity {
    /// Original RuntimeSourceIdentityDTO as_dict public projection at refinery.py:245-288; source requirement, not completion: runtime_artifact_sha256
    pub runtime_artifact_sha256: String,
    /// Original RuntimeSourceIdentityDTO as_dict public projection at refinery.py:245-288; source requirement, not completion: runtime_commit
    pub runtime_commit: String,
    /// Original RuntimeSourceIdentityDTO as_dict public projection at refinery.py:245-288; source requirement, not completion: runtime_version
    pub runtime_version: String,
}
impl SemanticRefineryRuntimeSourceIdentity {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit current Rust counterpart; original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRustCandidateVerification {
    /// Explicit current Rust counterpart; original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: execution_suite_sha256
    pub execution_suite_sha256: String,
    /// Explicit current Rust counterpart; original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: regression_evidence
    pub regression_evidence: SemanticRefineryRustGateEvidence,
    /// Explicit current Rust counterpart; original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: release_evidence
    pub release_evidence: SemanticRefineryRustGateEvidence,
    /// Explicit current Rust counterpart; original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: schema_version
    pub schema_version: String,
    /// Explicit current Rust counterpart; original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: state
    pub state: SemanticRefineryRefineryState,
    /// Explicit current Rust counterpart; original CandidateVerificationDTO as_dict public projection at refinery.py:2902-3022; source requirement, not completion: verification_sha256
    pub verification_sha256: String,
}
impl SemanticRefineryRustCandidateVerification {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRustEngagement {
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: binding_sha256
    pub binding_sha256: String,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: blocked_reasons
    pub blocked_reasons: Vec<String>,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: contract
    pub contract: String,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: created_at
    pub created_at: String,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: engagement_id
    pub engagement_id: String,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: envelope_sha256
    pub envelope_sha256: String,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: execution
    pub execution: SemanticRefineryEngagementPhase,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: execution_receipts
    pub execution_receipts: Vec<SemanticRefineryExecutionReceiptReference>,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: learning
    pub learning: SemanticRefineryLearningResult,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: profile
    pub profile: String,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: receipt
    pub receipt: SemanticRefineryEngagementPhase,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: receipt_sha256
    pub receipt_sha256: String,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: regression
    pub regression: SemanticRefineryEngagementPhase,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: release
    pub release: SemanticRefineryEngagementPhase,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: required_capabilities
    pub required_capabilities: Vec<String>,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: runtime_source
    pub runtime_source: SemanticRefineryRustSource,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: schema_version
    pub schema_version: String,
    /// Current Rust engagement observation uses a distinct source/profile; original Python receipt remains historical: status
    pub status: String,
}
impl SemanticRefineryRustEngagement {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if !["semantica.refinery-rust-observation/1",].contains(&self.profile.as_str()) { failures.push("SemanticRefineryRustEngagement.profile:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRustGateEvidence {
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: checks
    pub checks: Vec<SemanticRefineryGateCheck>,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: evidence_sha256
    pub evidence_sha256: String,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: execution_suite_object_sha256
    pub execution_suite_object_sha256: String,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: execution_suite_sha256
    pub execution_suite_sha256: String,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: gate
    pub gate: String,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: missing_capabilities
    pub missing_capabilities: Vec<String>,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: observed_capabilities
    pub observed_capabilities: Vec<String>,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: package_sha256
    pub package_sha256: String,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: provenance_closure_object_sha256
    pub provenance_closure_object_sha256: String,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: provenance_closure_sha256
    pub provenance_closure_sha256: String,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: recorded_at
    pub recorded_at: String,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: required_capabilities
    pub required_capabilities: Vec<String>,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: runtime_source
    pub runtime_source: SemanticRefineryRustSource,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: status
    pub status: String,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: transition_context_object_sha256
    pub transition_context_object_sha256: String,
    /// Explicit current Rust counterpart; original RefineryGateEvidenceDTO as_dict public projection at refinery.py:1748-1997; source requirement, not completion: transition_context_sha256
    pub transition_context_sha256: String,
}
impl SemanticRefineryRustGateEvidence {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRustProvenanceClosure {
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: candidate_binding_sha256
    pub candidate_binding_sha256: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: candidate_envelope_sha256
    pub candidate_envelope_sha256: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: closure_sha256
    pub closure_sha256: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: delta_sha256
    pub delta_sha256: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: engagement_receipt_sha256
    pub engagement_receipt_sha256: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: execution_projection_sha256
    pub execution_projection_sha256: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: manifest_sha256
    pub manifest_sha256: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: package_id
    pub package_id: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: package_sha256
    pub package_sha256: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: package_version
    pub package_version: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: phase
    pub phase: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: prior_closure_sha256
    pub prior_closure_sha256: Option<String>,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: provenance_evidence_assets
    pub provenance_evidence_assets: std::collections::BTreeMap<String, String>,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: runtime_source
    pub runtime_source: SemanticRefineryRustSource,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: scenarios
    pub scenarios: Vec<SemanticRefineryProvenanceScenarioBinding>,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: schema_version
    pub schema_version: String,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: source_evidence
    pub source_evidence: Vec<SemanticRefinerySourceEvidence>,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: transition_contexts
    pub transition_contexts: Vec<SemanticRefineryContextBinding>,
    /// Explicit current Rust counterpart; original ProvenanceClosureDTO as_dict public projection at refinery.py:2222-2496; source requirement, not completion: workspace_id
    pub workspace_id: String,
}
impl SemanticRefineryRustProvenanceClosure {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit current Rust observed identity; serialization is not attestation or authority
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRustSource {
    /// Explicit current Rust observed identity; serialization is not attestation or authority: compilation_origin
    pub compilation_origin: String,
    /// Explicit current Rust observed identity; serialization is not attestation or authority: engineering_adoption
    pub engineering_adoption: String,
    /// Explicit current Rust observed identity; serialization is not attestation or authority: executable_sha256
    pub executable_sha256: String,
    /// Explicit current Rust observed identity; serialization is not attestation or authority: executable_size_bytes
    pub executable_size_bytes: u64,
    /// Explicit current Rust observed identity; serialization is not attestation or authority: schema
    pub schema: String,
    /// Explicit current Rust observed identity; serialization is not attestation or authority: source_sha256
    pub source_sha256: String,
    /// Explicit current Rust observed identity; serialization is not attestation or authority: status
    pub status: String,
}
impl SemanticRefineryRustSource {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.executable_sha256.len() != 64 || !self.executable_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticRefineryRustSource.executable_sha256:sha256".into()); }
        if self.source_sha256.len() != 64 || !self.source_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticRefineryRustSource.source_sha256:sha256".into()); }

        if !["semantica.refinery-rust-source/1",].contains(&self.schema.as_str()) { failures.push("SemanticRefineryRustSource.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRustSubjectExecutionSuite {
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: cq_registry_bindings
    pub cq_registry_bindings: std::collections::BTreeMap<String, String>,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: created_at
    pub created_at: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: delta_sha256
    pub delta_sha256: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: execution_asset_hashes
    pub execution_asset_hashes: std::collections::BTreeMap<String, String>,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: execution_projection_sha256
    pub execution_projection_sha256: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: observed_capabilities
    pub observed_capabilities: Vec<String>,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: provenance_closure_object_sha256
    pub provenance_closure_object_sha256: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: provenance_closure_sha256
    pub provenance_closure_sha256: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: required_case_kinds
    pub required_case_kinds: Vec<String>,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: required_cq_ids
    pub required_cq_ids: Vec<String>,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: required_prior_case_bindings
    pub required_prior_case_bindings: std::collections::BTreeMap<String, String>,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: required_prior_cq_bindings
    pub required_prior_cq_bindings: std::collections::BTreeMap<String, String>,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: required_scenario_ids
    pub required_scenario_ids: Vec<String>,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: runs
    pub runs: Vec<SemanticRefinerySubjectScenarioRun>,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: runtime_source
    pub runtime_source: SemanticRefineryRustSource,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: schema_version
    pub schema_version: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: status
    pub status: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: subject_manifest_sha256
    pub subject_manifest_sha256: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: subject_package_id
    pub subject_package_id: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: subject_package_sha256
    pub subject_package_sha256: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: subject_package_version
    pub subject_package_version: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: suite_sha256
    pub suite_sha256: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: transition_context_object_sha256
    pub transition_context_object_sha256: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: transition_context_sha256
    pub transition_context_sha256: String,
    /// Explicit current Rust counterpart; original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: workspace_id
    pub workspace_id: String,
}
impl SemanticRefineryRustSubjectExecutionSuite {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Current actual runner result preserved alongside the full retained release-content observation; source fields never impersonate the legacy Git profile
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryRustSubjectSnapshot {
    /// Current retained Rust subject execution binding: actual_run
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub actual_run: serde_json::Value,
    /// Current retained Rust subject execution binding: profile
    pub profile: String,
    /// Current retained Rust subject execution binding: release_content
    pub release_content: SemanticReleaseContentObservation,
}
impl SemanticRefineryRustSubjectSnapshot {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.profile.trim().is_empty() { failures.push("SemanticRefineryRustSubjectSnapshot.profile:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefinerySemanticEngagementReceipt {
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: binding_sha256
    pub binding_sha256: String,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: blocked_reasons
    pub blocked_reasons: Vec<String>,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: contract
    pub contract: String,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: created_at
    pub created_at: String,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: engagement_id
    pub engagement_id: String,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: envelope_sha256
    pub envelope_sha256: String,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: execution
    pub execution: SemanticRefineryEngagementPhase,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: execution_receipts
    pub execution_receipts: Vec<SemanticRefineryExecutionReceiptReference>,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: learning
    pub learning: SemanticRefineryLearningResult,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: receipt
    pub receipt: SemanticRefineryEngagementPhase,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: receipt_sha256
    pub receipt_sha256: String,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: regression
    pub regression: SemanticRefineryEngagementPhase,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: release
    pub release: SemanticRefineryEngagementPhase,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: required_capabilities
    pub required_capabilities: Vec<String>,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: runtime_source
    pub runtime_source: SemanticRefineryRuntimeSourceIdentity,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: schema_version
    pub schema_version: String,
    /// Original SemanticEngagementReceipt as_dict public projection at refinery.py:890-1156; source requirement, not completion: status
    pub status: String,
}
impl SemanticRefinerySemanticEngagementReceipt {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefinerySemanticTaskEnvelope {
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: actor_id
    pub actor_id: String,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: created_at
    pub created_at: String,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: domain
    pub domain: String,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: envelope_sha256
    pub envelope_sha256: String,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: evidence
    pub evidence: Vec<SemanticRefinerySourceEvidence>,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: intent
    pub intent: String,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: project_id
    pub project_id: String,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: requested_actions
    pub requested_actions: Vec<String>,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: requested_decision
    pub requested_decision: String,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: required_capabilities
    pub required_capabilities: Vec<String>,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: schema_version
    pub schema_version: String,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: task_id
    pub task_id: String,
    /// Original SemanticTaskEnvelope as_dict public projection at refinery.py:292-412; source requirement, not completion: task_kind
    pub task_kind: String,
}
impl SemanticRefinerySemanticTaskEnvelope {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original SourceEvidenceDTO as_dict public projection at refinery.py:199-241; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefinerySourceEvidence {
    /// Original SourceEvidenceDTO as_dict public projection at refinery.py:199-241; source requirement, not completion: captured_at
    pub captured_at: String,
    /// Original SourceEvidenceDTO as_dict public projection at refinery.py:199-241; source requirement, not completion: media_type
    pub media_type: String,
    /// Original SourceEvidenceDTO as_dict public projection at refinery.py:199-241; source requirement, not completion: sha256
    pub sha256: String,
    /// Original SourceEvidenceDTO as_dict public projection at refinery.py:199-241; source requirement, not completion: source_id
    pub source_id: String,
    /// Original SourceEvidenceDTO as_dict public projection at refinery.py:199-241; source requirement, not completion: uri
    pub uri: String,
}
impl SemanticRefinerySourceEvidence {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefinerySubjectExecutionSuite {
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: cq_registry_bindings
    pub cq_registry_bindings: std::collections::BTreeMap<String, String>,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: created_at
    pub created_at: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: delta_sha256
    pub delta_sha256: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: execution_asset_hashes
    pub execution_asset_hashes: std::collections::BTreeMap<String, String>,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: execution_projection_sha256
    pub execution_projection_sha256: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: observed_capabilities
    pub observed_capabilities: Vec<String>,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: provenance_closure_object_sha256
    pub provenance_closure_object_sha256: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: provenance_closure_sha256
    pub provenance_closure_sha256: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: required_case_kinds
    pub required_case_kinds: Vec<String>,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: required_cq_ids
    pub required_cq_ids: Vec<String>,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: required_prior_case_bindings
    pub required_prior_case_bindings: std::collections::BTreeMap<String, String>,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: required_prior_cq_bindings
    pub required_prior_cq_bindings: std::collections::BTreeMap<String, String>,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: required_scenario_ids
    pub required_scenario_ids: Vec<String>,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: runs
    pub runs: Vec<SemanticRefinerySubjectScenarioRun>,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: runtime_source
    pub runtime_source: SemanticRefineryRuntimeSourceIdentity,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: schema_version
    pub schema_version: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: status
    pub status: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: subject_manifest_sha256
    pub subject_manifest_sha256: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: subject_package_id
    pub subject_package_id: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: subject_package_sha256
    pub subject_package_sha256: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: subject_package_version
    pub subject_package_version: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: suite_sha256
    pub suite_sha256: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: transition_context_object_sha256
    pub transition_context_object_sha256: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: transition_context_sha256
    pub transition_context_sha256: String,
    /// Original SubjectExecutionSuiteDTO as_dict public projection at refinery.py:2500-2813; source requirement, not completion: workspace_id
    pub workspace_id: String,
}
impl SemanticRefinerySubjectExecutionSuite {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefinerySubjectScenarioRun {
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: case_assets
    pub case_assets: Vec<SemanticRefineryCaseAssetBinding>,
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: cq_ids
    pub cq_ids: Vec<String>,
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: executor_package_digest
    pub executor_package_digest: String,
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: executor_package_id
    pub executor_package_id: String,
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: executor_package_version
    pub executor_package_version: String,
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: receipt_object_sha256
    pub receipt_object_sha256: String,
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: receipt_sha256
    pub receipt_sha256: String,
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: release_status
    pub release_status: String,
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: run_result_object_sha256
    pub run_result_object_sha256: String,
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: scenario_id
    pub scenario_id: String,
    /// Original SubjectScenarioRunDTO as_dict public projection at refinery.py:2001-2111; source requirement, not completion: status
    pub status: String,
}
impl SemanticRefinerySubjectScenarioRun {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original TransitionContextDTO as_dict public projection at refinery.py:573-682; source requirement, not completion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRefineryTransitionContext {
    /// Original TransitionContextDTO as_dict public projection at refinery.py:573-682; source requirement, not completion: action
    pub action: String,
    /// Original TransitionContextDTO as_dict public projection at refinery.py:573-682; source requirement, not completion: binding
    pub binding: SemanticRefineryProjectOntologyBinding,
    /// Original TransitionContextDTO as_dict public projection at refinery.py:573-682; source requirement, not completion: context_sha256
    pub context_sha256: String,
    /// Original TransitionContextDTO as_dict public projection at refinery.py:573-682; source requirement, not completion: delta_sha256
    pub delta_sha256: String,
    /// Original TransitionContextDTO as_dict public projection at refinery.py:573-682; source requirement, not completion: envelope
    pub envelope: SemanticRefinerySemanticTaskEnvelope,
    /// Original TransitionContextDTO as_dict public projection at refinery.py:573-682; source requirement, not completion: schema_version
    pub schema_version: String,
}
impl SemanticRefineryTransitionContext {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原发布内容检查的代码合同，执行真实性/编译来源/工程采用另行取证
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticReleaseContentCheck {
    /// 原发布检查的稳定名称
    pub check_id: String,
    /// 该检查要求的原始说明
    pub message: String,
    /// 按原内容判据求得的结果；不是工程采用
    pub passed: bool,
}
impl SemanticReleaseContentCheck {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.check_id.trim().is_empty() { failures.push("SemanticReleaseContentCheck.check_id:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 工程本体论规定真实执行与内容、来源、采用的不同责任；由同一保留runner执行当前内容算法
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticReleaseContentObservation {
    /// 原15内容检查及私有保留声明/实际场景2检查的判定
    pub content_verdict: SemanticReleaseContentVerdict,
    /// 工程采用须基于适用工况和验证证据
    pub engineering_adoption: String,
    /// 此内容入口不发布库或工程结论
    pub formal_release: String,
    /// 有界真实执行内容检查的独立合同
    pub profile: String,
    /// 从真实原执行生成的完整PROV回执，未改写历史回执
    pub receipt: SemanticExecutionProvenanceReceipt,
    /// 仅私有保留状态可建立执行来源
    pub runner_execution: String,
    /// 编译来源须另外取证
    pub runtime_origin: String,
    /// 本轮来源观察数据，不作为编译来源认证
    pub runtime_source: SemanticRefineryRustSource,
    /// 同一Dataset的实际RDFC算法身份
    pub snapshot_profile: String,
    /// 内容摘要不证明来源真实性
    pub source_authenticity: String,
    /// 同一BookRunnerState原保留回执的实际摘要
    pub source_run_receipt_sha256: String,
}
impl SemanticReleaseContentObservation {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.engineering_adoption.trim().is_empty() { failures.push("SemanticReleaseContentObservation.engineering_adoption:required".into()); }
        if self.formal_release.trim().is_empty() { failures.push("SemanticReleaseContentObservation.formal_release:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticReleaseContentObservation.profile:required".into()); }
        if self.runner_execution.trim().is_empty() { failures.push("SemanticReleaseContentObservation.runner_execution:required".into()); }
        if self.runtime_origin.trim().is_empty() { failures.push("SemanticReleaseContentObservation.runtime_origin:required".into()); }
        if self.source_authenticity.trim().is_empty() { failures.push("SemanticReleaseContentObservation.source_authenticity:required".into()); }
        if self.source_run_receipt_sha256.len() != 64 || !self.source_run_receipt_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticReleaseContentObservation.source_run_receipt_sha256:sha256".into()); }

        if !["unknown",].contains(&self.engineering_adoption.as_str()) { failures.push("SemanticReleaseContentObservation.engineering_adoption:status".into()); }
        if !["not_published",].contains(&self.formal_release.as_str()) { failures.push("SemanticReleaseContentObservation.formal_release:status".into()); }
        if !["semantica.release-content-retained-rust/1",].contains(&self.profile.as_str()) { failures.push("SemanticReleaseContentObservation.profile:status".into()); }
        if !["retained",].contains(&self.runner_execution.as_str()) { failures.push("SemanticReleaseContentObservation.runner_execution:status".into()); }
        if !["unknown",].contains(&self.runtime_origin.as_str()) { failures.push("SemanticReleaseContentObservation.runtime_origin:status".into()); }
        if !["unknown",].contains(&self.source_authenticity.as_str()) { failures.push("SemanticReleaseContentObservation.source_authenticity:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一注册包和Dataset上的内容判定，不创建执行token或发布资格
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticReleaseContentVerdict {
    /// 边界注入的检查时间
    pub checked_at: String,
    /// 原runtime十五项保持构造序；runner加入两项后按check_id词典排序
    pub checks: Vec<SemanticReleaseContentCheck>,
    /// 原Git内容合同与新Rust来源合同分开
    pub profile: String,
    /// 失败项名称按相同顺序记录
    pub reasons: Vec<String>,
    /// 被检查的实际回执摘要
    pub receipt_sha256: String,
    /// 内容判定版本
    pub schema_version: String,
    /// 所有本次内容检查通过才complete，否则blocked
    pub status: String,
}
impl SemanticReleaseContentVerdict {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.checked_at.trim().is_empty() { failures.push("SemanticReleaseContentVerdict.checked_at:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticReleaseContentVerdict.profile:required".into()); }
        if self.receipt_sha256.len() != 64 || !self.receipt_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticReleaseContentVerdict.receipt_sha256:sha256".into()); }
        if self.schema_version.trim().is_empty() { failures.push("SemanticReleaseContentVerdict.schema_version:required".into()); }
        if self.status.trim().is_empty() { failures.push("SemanticReleaseContentVerdict.status:required".into()); }

        if !["semantica.release-content-legacy-data/1","semantica.release-content-rust/1",].contains(&self.profile.as_str()) { failures.push("SemanticReleaseContentVerdict.profile:status".into()); }
        if !["1.0",].contains(&self.schema_version.as_str()) { failures.push("SemanticReleaseContentVerdict.schema_version:status".into()); }
        if !["blocked","complete",].contains(&self.status.as_str()) { failures.push("SemanticReleaseContentVerdict.status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 通用结构化失败，无工程ErrorCode依赖；错误不成为成功或自动重试
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRuntimeFailure {
    /// 稳定原因码，包括schema/输入摘要/格式/解析/预算/查询失败
    pub code: String,
    /// 错误原因保留，不能只返回空结果
    pub message: String,
    /// 真实失败阶段，validate/parse/query等由实现明确记录
    pub phase: String,
    /// 是否允许调用者基于明确合同重试，未知写入不自动重试
    pub retryable: bool,
}
impl SemanticRuntimeFailure {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.code.trim().is_empty() { failures.push("SemanticRuntimeFailure.code:required".into()); }
        if self.message.trim().is_empty() { failures.push("SemanticRuntimeFailure.message:required".into()); }
        if self.phase.trim().is_empty() { failures.push("SemanticRuntimeFailure.phase:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit original schema and typed generation behavior, source-bound candidates and independent validation effects
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSchemaFailure {
    /// Original schema/typed execution field code
    pub code: String,
    /// Original schema/typed execution field message
    pub message: String,
    /// Original schema/typed execution field phase
    pub phase: String,
    /// Original schema/typed execution field retryable
    pub retryable: bool,
}
impl SemanticSchemaFailure {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.code.trim().is_empty() { failures.push("SemanticSchemaFailure.code:required".into()); }
        if self.phase.trim().is_empty() { failures.push("SemanticSchemaFailure.phase:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit original schema and typed generation behavior, source-bound candidates and independent validation effects
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSchemaIssue {
    /// Original schema/typed execution field kind
    pub kind: String,
    /// Original schema/typed execution field location
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub location: Vec<serde_json::Value>,
    /// Original schema/typed execution field message
    pub message: String,
}
impl SemanticSchemaIssue {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.kind.trim().is_empty() { failures.push("SemanticSchemaIssue.kind:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit original schema and typed generation behavior, source-bound candidates and independent validation effects
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSchemaValidationRequest {
    /// Original schema/typed execution field input_json
    pub input_json: String,
    /// Original schema/typed execution field input_sha256
    pub input_sha256: String,
    /// Original schema/typed execution field model_id
    pub model_id: String,
    /// Original schema/typed execution field operation
    pub operation: String,
    /// Original schema/typed execution field profile
    pub profile: String,
    /// Original schema/typed execution field schema
    pub schema: String,
}
impl SemanticSchemaValidationRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.input_sha256.len() != 64 || !self.input_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSchemaValidationRequest.input_sha256:sha256".into()); }
        if self.model_id.trim().is_empty() { failures.push("SemanticSchemaValidationRequest.model_id:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticSchemaValidationRequest.operation:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticSchemaValidationRequest.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSchemaValidationRequest.schema:required".into()); }

        if !["model_construct","model_validate",].contains(&self.operation.as_str()) { failures.push("SemanticSchemaValidationRequest.operation:status".into()); }
        if !["semantica.pydantic-json/1",].contains(&self.profile.as_str()) { failures.push("SemanticSchemaValidationRequest.profile:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit original schema and typed generation behavior, source-bound candidates and independent validation effects
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSchemaValidationResult {
    /// Original schema/typed execution field execution
    pub execution: String,
    /// Original schema/typed execution field input_after
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub input_after: serde_json::Value,
    /// Original schema/typed execution field input_mutated
    pub input_mutated: bool,
    /// Original schema/typed execution field input_sha256
    pub input_sha256: String,
    /// Original schema/typed execution field issues
    pub issues: Vec<SemanticSchemaIssue>,
    /// Original schema/typed execution field model_id
    pub model_id: String,
    /// Original schema/typed execution field output
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub output: serde_json::Value,
    /// Original schema/typed execution field output_by_alias
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub output_by_alias: serde_json::Value,
    /// Original schema/typed execution field profile
    pub profile: String,
    /// Original schema/typed execution field properties
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub properties: serde_json::Value,
    /// Original schema/typed execution field schema
    pub schema: String,
    /// Original schema/typed execution field valid
    pub valid: bool,
    /// Original schema/typed execution field validation_executed
    pub validation_executed: bool,
}
impl SemanticSchemaValidationResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.execution.trim().is_empty() { failures.push("SemanticSchemaValidationResult.execution:required".into()); }
        if self.input_sha256.len() != 64 || !self.input_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSchemaValidationResult.input_sha256:sha256".into()); }
        if self.model_id.trim().is_empty() { failures.push("SemanticSchemaValidationResult.model_id:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticSchemaValidationResult.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSchemaValidationResult.schema:required".into()); }

        if !["completed","failed","unknown",].contains(&self.execution.as_str()) { failures.push("SemanticSchemaValidationResult.execution:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 服务host注入的单预算请求，call/input/output分别累计；不承担模型费用或学习治理
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticServiceBudgetCharge {
    /// 服务host注入的单预算请求，call/input/output分别累计；不承担模型费用或学习治理：bytes
    pub bytes: u64,
    /// 服务host注入的单预算请求，call/input/output分别累计；不承担模型费用或学习治理：kind
    pub kind: String,
}
impl SemanticServiceBudgetCharge {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.kind.trim().is_empty() { failures.push("SemanticServiceBudgetCharge.kind:required".into()); }

        if !["framing","query","request","response",].contains(&self.kind.as_str()) { failures.push("SemanticServiceBudgetCharge.kind:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// native服务累计计数；预算消耗不是工程结果
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticServiceBudgetReceipt {
    /// native服务累计计数；预算消耗不是工程结果：calls
    pub calls: u64,
    /// native服务累计计数；预算消耗不是工程结果：input_bytes
    pub input_bytes: u64,
    /// native服务累计计数；预算消耗不是工程结果：output_bytes
    pub output_bytes: u64,
}
impl SemanticServiceBudgetReceipt {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 现有DatasetSession只读查询请求；不接受替代图或自报passed
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticServiceQueryRequest {
    /// 现有DatasetSession只读查询请求；不接受替代图或自报passed：expected_dataset_sha256
    pub expected_dataset_sha256: String,
    /// 现有DatasetSession只读查询请求；不接受替代图或自报passed：expected_revision
    pub expected_revision: u64,
    /// 现有DatasetSession只读查询请求；不接受替代图或自报passed：query
    pub query: String,
    /// 现有DatasetSession只读查询请求；不接受替代图或自报passed：schema
    pub schema: String,
    /// 现有DatasetSession只读查询请求；不接受替代图或自报passed：use_cache
    pub use_cache: bool,
}
impl SemanticServiceQueryRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.expected_dataset_sha256.len() != 64 || !self.expected_dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticServiceQueryRequest.expected_dataset_sha256:sha256".into()); }
        if self.query.trim().is_empty() { failures.push("SemanticServiceQueryRequest.query:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticServiceQueryRequest.schema:required".into()); }

        if !["semantica.service-query-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticServiceQueryRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 同一Dataset执行结果；typed RDF身份与cache/revision来自唯一runtime
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticServiceQueryResult {
    /// 同一Dataset执行结果；typed RDF身份与cache/revision来自唯一runtime：dataset_sha256
    pub dataset_sha256: String,
    /// 同一Dataset执行结果；typed RDF身份与cache/revision来自唯一runtime：execution
    pub execution: String,
    /// 同一Dataset执行结果；typed RDF身份与cache/revision来自唯一runtime：profile
    pub profile: String,
    /// 同一Dataset执行结果；typed RDF身份与cache/revision来自唯一runtime：result
    pub result: SemanticSessionQueryResult,
    /// 同一Dataset执行结果；typed RDF身份与cache/revision来自唯一runtime：schema
    pub schema: String,
}
impl SemanticServiceQueryResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.dataset_sha256.len() != 64 || !self.dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticServiceQueryResult.dataset_sha256:sha256".into()); }
        if self.execution.trim().is_empty() { failures.push("SemanticServiceQueryResult.execution:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticServiceQueryResult.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticServiceQueryResult.schema:required".into()); }

        if !["completed",].contains(&self.execution.as_str()) { failures.push("SemanticServiceQueryResult.execution:status".into()); }
        if !["semantica.service-query-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticServiceQueryResult.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 数据集查询的原结果、修订号与cache身份分开
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSessionQueryResult {
    /// 仅同一数据集修订与查询命中缓存时为真
    pub cache_hit: bool,
    /// 显式RDF术语和值的原查询结果
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub result: serde_json::Value,
    /// 执行或cache所属当前数据集修订号
    pub revision: u64,
}
impl SemanticSessionQueryResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original snapshot integrity operation, preserving native value reconstruction and source failure
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSnapshotChecksumRequest {
    /// compute_checksum or verify_checksum
    pub operation: String,
    /// Caller context retained in request identity; does not relax execution or source semantics
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub options: serde_json::Value,
    /// plain_json or native_value; explicit mode prevents ordinary dictionaries from being mistaken for carriers
    pub representation: String,
    /// Exact semantica.snapshot-checksum-request/1
    pub schema: String,
    /// Plain finite JSON with original integer ingress, or existing semantica.native-value/1 carrier; reconstruction node identity is explicit
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub value: serde_json::Value,
}
impl SemanticSnapshotChecksumRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticSnapshotChecksumRequest.operation:required".into()); }
        if self.representation.trim().is_empty() { failures.push("SemanticSnapshotChecksumRequest.representation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSnapshotChecksumRequest.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original snapshot integrity operation, preserving native value reconstruction and source failure
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSnapshotChecksumResult {
    /// Original source class/message; contract/resource refusal remains whole-request failure
    pub failure: Option<SemanticRuntimeFailure>,
    /// Actual unchanged requested operation
    pub operation: String,
    /// Exact semantica.source-snapshot-checksum-cpython313/1
    pub profile: String,
    /// Canonical hash of complete typed request, distinct from source checksum
    pub request_sha256: String,
    /// Original digest string or verification bool; absent on original exception
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub result: Option<serde_json::Value>,
    /// Exact semantica.snapshot-checksum-result/1
    pub schema: String,
    /// Exact original compact ASCII text for compute only; verify does not serialize on falsey checksum
    pub serialized_ascii: Option<String>,
}
impl SemanticSnapshotChecksumResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticSnapshotChecksumResult.operation:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticSnapshotChecksumResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticSnapshotChecksumResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSnapshotChecksumResult.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 原来源全部字节身份；只证明内容一致，不授予读取或采用
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceBinding {
    /// 原字节长度
    pub byte_count: u64,
    /// 明确协议版本
    pub schema: String,
    /// 来源实体IRI
    pub source_id: String,
    /// 原字节SHA256
    pub source_sha256: String,
}
impl SemanticSourceBinding {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticSourceBinding.schema:required".into()); }
        if self.source_id.trim().is_empty() { failures.push("SemanticSourceBinding.source_id:required".into()); }
        if self.source_sha256.len() != 64 || !self.source_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceBinding.source_sha256:sha256".into()); }

        if !["semantica.source-binding/1",].contains(&self.schema.as_str()) { failures.push("SemanticSourceBinding.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original source snapshot set difference, distinct from native RDFC result
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceDatasetDiff {
    /// Unique sorted source line additions after successful original integrity and algorithm checks
    pub added_quads: Vec<String>,
    /// Verified exact source after canonical byte identity
    pub after_sha256: String,
    /// Verified exact source before canonical byte identity
    pub before_sha256: String,
    /// True exactly when addition or removal sets are nonempty
    pub changed: bool,
    /// Observation time supplied for this diff, separate from snapshots
    pub created_at: String,
    /// Unique sorted source line removals after successful original integrity and algorithm checks
    pub removed_quads: Vec<String>,
    /// Original source diff DTO version
    pub schema_version: String,
    /// Set-intersection count in source line identity domain
    pub unchanged_count: u64,
}
impl SemanticSourceDatasetDiff {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.after_sha256.len() != 64 || !self.after_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceDatasetDiff.after_sha256:sha256".into()); }
        if self.before_sha256.len() != 64 || !self.before_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceDatasetDiff.before_sha256:sha256".into()); }
        if self.schema_version.trim().is_empty() { failures.push("SemanticSourceDatasetDiff.schema_version:required".into()); }

        if !["1.0",].contains(&self.schema_version.as_str()) { failures.push("SemanticSourceDatasetDiff.schema_version:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One shared original prefix binding in store insertion order
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceNamespaceBinding4 {
    /// Exact namespace
    pub namespace: String,
    /// Exact prefix
    pub prefix: String,
}
impl SemanticSourceNamespaceBinding4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.namespace.trim().is_empty() { failures.push("SemanticSourceNamespaceBinding4.namespace:required".into()); }
        if self.prefix.trim().is_empty() { failures.push("SemanticSourceNamespaceBinding4.prefix:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Bounded read-only namespace state at an explicit phase
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceNamespaceCheckpoint4 {
    /// Actual local capture failure
    pub failure: Option<SemanticSourceSessionFailure>,
    /// Actual existing namespace owner charge, not serialized checkpoint bytes
    pub retained_bytes: Option<u64>,
    /// Actual existing owner entry charge
    pub retained_entries: Option<u64>,
    /// Actual complete namespace state only when captured
    pub state: Option<SemanticSourceNamespaceState4>,
    /// captured, failed or not_attempted are distinct
    pub status: String,
}
impl SemanticSourceNamespaceCheckpoint4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.status.trim().is_empty() { failures.push("SemanticSourceNamespaceCheckpoint4.status:required".into()); }

        if !["captured","failed","not_attempted",].contains(&self.status.as_str()) { failures.push("SemanticSourceNamespaceCheckpoint4.status:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original ordered trie child reference
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceNamespaceEdge4 {
    /// Child namespace label
    pub namespace: String,
    /// Canonical subtree node index
    pub target: u64,
}
impl SemanticSourceNamespaceEdge4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.namespace.trim().is_empty() { failures.push("SemanticSourceNamespaceEdge4.namespace:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Actual one-manager trie and QName state
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceNamespaceManager4 {
    /// Canonical first-visit graph, including referenced subtrees
    pub nodes: Vec<SemanticSourceNamespaceNode4>,
    /// Key-sorted original QName values
    pub qname_cache: Vec<SemanticSourceNamespaceQName4>,
    /// Key-sorted original split references
    pub split_cache: Vec<SemanticSourceNamespaceSplit4>,
    /// Explicit strict QName cache; empty in currently received Turtle paths
    pub strict_cache: Vec<SemanticSourceNamespaceQName4>,
}
impl SemanticSourceNamespaceManager4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original subtree identity, shared with split-cache references
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceNamespaceNode4 {
    /// Ordered child edges; node zero is manager root
    pub children: Vec<SemanticSourceNamespaceEdge4>,
}
impl SemanticSourceNamespaceNode4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Exact original QName cache entry
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceNamespaceQName4 {
    /// Cached local spelling
    pub local: String,
    /// Cached namespace
    pub namespace: String,
    /// Cached prefix
    pub prefix: String,
    /// Lookup key
    pub uri: String,
}
impl SemanticSourceNamespaceQName4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.local.trim().is_empty() { failures.push("SemanticSourceNamespaceQName4.local:required".into()); }
        if self.namespace.trim().is_empty() { failures.push("SemanticSourceNamespaceQName4.namespace:required".into()); }
        if self.prefix.trim().is_empty() { failures.push("SemanticSourceNamespaceQName4.prefix:required".into()); }
        if self.uri.trim().is_empty() { failures.push("SemanticSourceNamespaceQName4.uri:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original split cache referencing the same trie subtree
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceNamespaceSplit4 {
    /// Lookup key, lexically sorted for canonical capture only
    pub namespace: String,
    /// Actual retained subtree reference, not reconstructed from bindings
    pub target: u64,
}
impl SemanticSourceNamespaceSplit4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.namespace.trim().is_empty() { failures.push("SemanticSourceNamespaceSplit4.namespace:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete namespace state of existing Dataset and default Graph owners
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceNamespaceState4 {
    /// Shared store bindings in original order
    pub bindings: Vec<SemanticSourceNamespaceBinding4>,
    /// Absent means Dataset manager not initialized; distinct from temporary update-view manager
    pub dataset_manager: Option<SemanticSourceNamespaceManager4>,
    /// Absent means default Graph manager not initialized
    pub default_manager: Option<SemanticSourceNamespaceManager4>,
    /// Explicit state schema
    pub schema: String,
}
impl SemanticSourceNamespaceState4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticSourceNamespaceState4.schema:required".into()); }

        if !["semantica.source-namespace-state/4",].contains(&self.schema.as_str()) { failures.push("SemanticSourceNamespaceState4.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Writer output plus explicit warning observations, no delivery or engineering adoption
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSerialization {
    /// Actual ordered NT/NQ writer UTF8 including original blank line, whitespace and escaped literal newlines
    pub text: String,
    /// Source writer base warnings retained separately from raw text
    pub warnings: Vec<String>,
}
impl SemanticSourceSerialization {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete ordered row stream captured by the same original writer invocation
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSerializationWitness3 {
    /// All selected writer rows in actual traversal order, including an empty stream
    pub rows: Vec<SemanticSourceWriterRow3>,
}
impl SemanticSourceSerializationWitness3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit private NT/NQ source-writer arguments, independent of native serializer
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSerializeRequest {
    /// Retain source base warning; NT/NQ never resolve resources from a base
    pub base_uri: Option<String>,
    /// Declared original NT/NQ syntax or source alias; unrelated formats explicitly remain unreceived
    pub format: String,
    /// Explicit graph selection including empty registered graph read effect; no implicit default from union
    pub graph_name: Option<String>,
}
impl SemanticSourceSerializeRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit local source-session Action on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionAction {
    /// Caller selected parser base for load only; does not authorize resource retrieval
    pub base_uri: Option<String>,
    /// Explicit typed original initial bindings; duplicate names refused
    pub bindings: Vec<SemanticSourceSessionBinding>,
    /// Explicit load graph selection; old dataset syntax restrictions remain
    pub graph_name: Option<String>,
    /// Unique action identity within the complete ordered request
    pub id: String,
    /// Original prefix bindings; at most128 entries and existing encoded options budget
    pub namespaces: std::collections::BTreeMap<String, String>,
    /// Explicit existing DatasetSession operation; no generic scripting or implied adoption
    pub operation: String,
    /// Unmodified query or UPDATE text; original parser and guards execute it
    pub query: Option<String>,
    /// Exact source IRI selecting one resource for load; no implicit path resolution
    pub resource_id: Option<String>,
    /// Read/write query cache selection; other actions do not infer a cache operation
    pub use_cache: bool,
}
impl SemanticSourceSessionAction {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.id.trim().is_empty() { failures.push("SemanticSourceSessionAction.id:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticSourceSessionAction.operation:required".into()); }

        if !["clear","load","query","snapshot","update",].contains(&self.operation.as_str()) { failures.push("SemanticSourceSessionAction.operation:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit ordered source-session2 Action on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionAction2 {
    /// Optional diff endpoint: strictly earlier successfully retained complete snapshot; absent invokes existing current-state diff owner
    pub after_snapshot_action_id: Option<String>,
    /// Caller selected parser base for load only; does not authorize resource retrieval
    pub base_uri: Option<String>,
    /// Required for diff: strictly earlier successfully retained complete snapshot action in this request
    pub before_snapshot_action_id: Option<String>,
    /// Explicit typed original initial bindings; duplicate names refused
    pub bindings: Vec<SemanticSourceSessionBinding>,
    /// Explicit load graph selection; old dataset syntax restrictions remain
    pub graph_name: Option<String>,
    /// Unique action identity within the complete ordered request
    pub id: String,
    /// Original prefix bindings; at most128 entries and existing encoded options budget
    pub namespaces: std::collections::BTreeMap<String, String>,
    /// Explicit existing DatasetSession operation; no generic scripting or implied adoption
    pub operation: String,
    /// Unmodified query or UPDATE text; original parser and guards execute it
    pub query: Option<String>,
    /// Exact source IRI selecting one resource for load; no implicit path resolution
    pub resource_id: Option<String>,
    /// Existing NT/NQ source writer request only for serialize; typed warnings and exact text preserved
    pub serialize_request: Option<SemanticSourceSerializeRequest>,
    /// Read/write query cache selection; other actions do not infer a cache operation
    pub use_cache: bool,
}
impl SemanticSourceSessionAction2 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.id.trim().is_empty() { failures.push("SemanticSourceSessionAction2.id:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticSourceSessionAction2.operation:required".into()); }
        if let Some(value) = &self.serialize_request { if let Err(errors) = value.validate() { failures.extend(errors); } }


        if !["clear","diff","load","query","serialize","snapshot","update",].contains(&self.operation.as_str()) { failures.push("SemanticSourceSessionAction2.operation:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit local source-session Binding on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionBinding {
    /// Original literal datatype IRI; mutually exclusive with language
    pub datatype: Option<String>,
    /// Original language spelling; no implicit case or datatype normalization
    pub language: Option<String>,
    /// Original SPARQL variable name; duplicate bindings rejected before execution
    pub name: String,
    /// Explicit source RDF term kind; no result sentinel or identity rewriting
    pub term_type: String,
    /// Exact caller RDF lexical value; blank identifiers and literal spelling preserved
    pub value: String,
}
impl SemanticSourceSessionBinding {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.name.trim().is_empty() { failures.push("SemanticSourceSessionBinding.name:required".into()); }
        if self.term_type.trim().is_empty() { failures.push("SemanticSourceSessionBinding.term_type:required".into()); }

        if !["blank_node","iri","literal",].contains(&self.term_type.as_str()) { failures.push("SemanticSourceSessionBinding.term_type:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit local source-session Failure on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionFailure {
    /// Stable adapter failure code; actual error details retained separately
    pub code: String,
    /// Actual failure detail; not diagnostic-text-based original exception classification
    pub message: String,
    /// Original exception class only when existing typed SourceFailure proves it
    pub original_class: Option<String>,
    /// Original exception module only when existing typed SourceFailure proves it
    pub original_module: Option<String>,
    /// Actual admission/action/observation/capture phase
    pub phase: String,
}
impl SemanticSourceSessionFailure {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.code.trim().is_empty() { failures.push("SemanticSourceSessionFailure.code:required".into()); }
        if self.phase.trim().is_empty() { failures.push("SemanticSourceSessionFailure.phase:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit local source-session Request on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionRequest {
    /// Complete ordered actions; one32 action envelope, never truncate or zip against expectations
    pub actions: Vec<SemanticSourceSessionAction>,
    /// Enable the existing bounded source query cache in this sole session
    pub cache_queries: bool,
    /// Caller observation timestamp passed to original snapshot; no content identity authority
    pub created_at: String,
    /// Explicit local source session admission; not public query profile5
    pub profile: String,
    /// Complete declared resources in input order; no implicit network or filesystem sources
    pub resources: Vec<SemanticSourceSessionResource>,
    /// Explicit versioned envelope identity
    pub schema: String,
}
impl SemanticSourceSessionRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.created_at.trim().is_empty() { failures.push("SemanticSourceSessionRequest.created_at:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticSourceSessionRequest.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSourceSessionRequest.schema:required".into()); }

        if !["semantica.rdflib760-source-session/1",].contains(&self.profile.as_str()) { failures.push("SemanticSourceSessionRequest.profile:status".into()); }
        if !["semantica.source-session-request/1",].contains(&self.schema.as_str()) { failures.push("SemanticSourceSessionRequest.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit ordered source-session2 Request on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionRequest2 {
    /// Complete ordered actions; one32 action envelope, never truncate or zip against expectations
    pub actions: Vec<SemanticSourceSessionAction2>,
    /// Enable the existing bounded source query cache in this sole session
    pub cache_queries: bool,
    /// Caller observation timestamp passed to original snapshot; no content identity authority
    pub created_at: String,
    /// Explicit local source session admission; not public query profile5
    pub profile: String,
    /// Complete declared resources in input order; no implicit network or filesystem sources
    pub resources: Vec<SemanticSourceSessionResource>,
    /// Explicit versioned envelope identity
    pub schema: String,
}
impl SemanticSourceSessionRequest2 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.created_at.trim().is_empty() { failures.push("SemanticSourceSessionRequest2.created_at:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticSourceSessionRequest2.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSourceSessionRequest2.schema:required".into()); }

        if !["semantica.rdflib760-source-session/2",].contains(&self.profile.as_str()) { failures.push("SemanticSourceSessionRequest2.profile:status".into()); }
        if !["semantica.source-session-request/2",].contains(&self.schema.as_str()) { failures.push("SemanticSourceSessionRequest2.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit ordered source-session3 Request on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionRequest3 {
    /// Complete ordered actions; one32 action envelope, never truncate or zip against expectations
    pub actions: Vec<SemanticSourceSessionAction2>,
    /// Enable the existing bounded source query cache in this sole session
    pub cache_queries: bool,
    /// Caller observation timestamp passed to original snapshot; no content identity authority
    pub created_at: String,
    /// Explicit local source session admission; not public query profile5
    pub profile: String,
    /// Complete declared resources in input order; no implicit network or filesystem sources
    pub resources: Vec<SemanticSourceSessionResource>,
    /// Explicit versioned envelope identity
    pub schema: String,
}
impl SemanticSourceSessionRequest3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.created_at.trim().is_empty() { failures.push("SemanticSourceSessionRequest3.created_at:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticSourceSessionRequest3.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSourceSessionRequest3.schema:required".into()); }

        if !["semantica.rdflib760-source-session/3",].contains(&self.profile.as_str()) { failures.push("SemanticSourceSessionRequest3.profile:status".into()); }
        if !["semantica.source-session-request/3",].contains(&self.schema.as_str()) { failures.push("SemanticSourceSessionRequest3.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit ordered source-session4 Request on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionRequest4 {
    /// Complete ordered actions; one32 action envelope, never truncate or zip against expectations
    pub actions: Vec<SemanticSourceSessionAction2>,
    /// Enable the existing bounded source query cache in this sole session
    pub cache_queries: bool,
    /// Caller observation timestamp passed to original snapshot; no content identity authority
    pub created_at: String,
    /// Explicit local source session admission; not public query profile5
    pub profile: String,
    /// Complete declared resources in input order; no implicit network or filesystem sources
    pub resources: Vec<SemanticSourceSessionResource>,
    /// Explicit versioned envelope identity
    pub schema: String,
}
impl SemanticSourceSessionRequest4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.created_at.trim().is_empty() { failures.push("SemanticSourceSessionRequest4.created_at:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticSourceSessionRequest4.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSourceSessionRequest4.schema:required".into()); }

        if !["semantica.rdflib760-source-session/4",].contains(&self.profile.as_str()) { failures.push("SemanticSourceSessionRequest4.profile:status".into()); }
        if !["semantica.source-session-request/4",].contains(&self.schema.as_str()) { failures.push("SemanticSourceSessionRequest4.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit local source-session Resource on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionResource {
    /// Original byte count SHA and source IRI; identity is not filesystem or network authority
    pub binding: SemanticSourceBinding,
    /// Complete caller supplied original UTF8 RDF bytes
    pub data: String,
    /// Explicit existing source loader syntax; LOAD and USING admit only declared Turtle resources
    pub rdf_format: String,
}
impl SemanticSourceSessionResource {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Err(errors) = self.binding.validate() { failures.extend(errors); }

        if self.rdf_format.trim().is_empty() { failures.push("SemanticSourceSessionResource.rdf_format:required".into()); }

        if !["jsonld","nquads","ntriples","rdfxml","trig","turtle",].contains(&self.rdf_format.as_str()) { failures.push("SemanticSourceSessionResource.rdf_format:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit local source-session Result on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionResult {
    /// This local session never establishes engineering adoption
    pub engineering_adoption: String,
    /// Actual action or envelope failure; never replaced by empty success
    pub failure: Option<SemanticSourceSessionFailure>,
    /// Final same-owner source identity observed without warming live caches
    pub final_state: Option<SemanticRdfLibDatasetSnapshot>,
    /// Actual final observation error; successful actions alone do not imply complete readback
    pub final_state_failure: Option<SemanticSourceSessionFailure>,
    /// Actual admitted constructor profile and cache setting; original new action representation
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub initial_result: serde_json::Value,
    /// Actual immediately constructed session observation, corresponding to original new action
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub initial_state: serde_json::Value,
    /// Initial observation error; no fabricated new state
    pub initial_state_failure: Option<SemanticSourceSessionFailure>,
    /// Explicit local source session admission; not public query profile5
    pub profile: String,
    /// SHA256 of complete ordered canonical request JSON, distinct from original RDF byte digests
    pub request_sha256: String,
    /// Explicit versioned envelope identity
    pub schema: String,
    /// SHA256 of complete ordered source-binding array; complete original bindings remain in the immutable request, avoiding unbounded repeated identity text in responses
    pub source_bindings_sha256: String,
    /// Actual execution status, separate from engineering acceptance and whole-scope coverage
    pub status: String,
    /// Exactly one result for every declared action in its original order including skipped actions
    pub steps: Vec<SemanticSourceSessionStep>,
}
impl SemanticSourceSessionResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.engineering_adoption.trim().is_empty() { failures.push("SemanticSourceSessionResult.engineering_adoption:required".into()); }
        if let Some(value) = &self.failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.final_state { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.final_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.initial_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.profile.trim().is_empty() { failures.push("SemanticSourceSessionResult.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSessionResult.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSourceSessionResult.schema:required".into()); }
        if self.source_bindings_sha256.len() != 64 || !self.source_bindings_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSessionResult.source_bindings_sha256:sha256".into()); }
        if self.status.trim().is_empty() { failures.push("SemanticSourceSessionResult.status:required".into()); }

        if !["not_assessed",].contains(&self.engineering_adoption.as_str()) { failures.push("SemanticSourceSessionResult.engineering_adoption:status".into()); }
        if !["semantica.rdflib760-source-session/1",].contains(&self.profile.as_str()) { failures.push("SemanticSourceSessionResult.profile:status".into()); }
        if !["semantica.source-session-result/1",].contains(&self.schema.as_str()) { failures.push("SemanticSourceSessionResult.schema:status".into()); }
        if !["completed","failed","partial",].contains(&self.status.as_str()) { failures.push("SemanticSourceSessionResult.status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit ordered source-session2 Result on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionResult2 {
    /// This local session never establishes engineering adoption
    pub engineering_adoption: String,
    /// Actual action or envelope failure; never replaced by empty success
    pub failure: Option<SemanticSourceSessionFailure>,
    /// Final same-owner source identity observed without warming live caches
    pub final_state: Option<SemanticRdfLibDatasetSnapshot>,
    /// Actual final observation error; successful actions alone do not imply complete readback
    pub final_state_failure: Option<SemanticSourceSessionFailure>,
    /// Actual admitted constructor profile and cache setting; original new action representation
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub initial_result: serde_json::Value,
    /// Actual immediately constructed session observation, corresponding to original new action
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub initial_state: serde_json::Value,
    /// Initial observation error; no fabricated new state
    pub initial_state_failure: Option<SemanticSourceSessionFailure>,
    /// Explicit local source session admission; not public query profile5
    pub profile: String,
    /// SHA256 of complete ordered canonical request JSON, distinct from original RDF byte digests
    pub request_sha256: String,
    /// Explicit versioned envelope identity
    pub schema: String,
    /// SHA256 of complete ordered source-binding array; complete original bindings remain in the immutable request, avoiding unbounded repeated identity text in responses
    pub source_bindings_sha256: String,
    /// Actual execution status, separate from engineering acceptance and whole-scope coverage
    pub status: String,
    /// Exactly one result for every declared action in its original order including skipped actions
    pub steps: Vec<SemanticSourceSessionStep2>,
}
impl SemanticSourceSessionResult2 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.engineering_adoption.trim().is_empty() { failures.push("SemanticSourceSessionResult2.engineering_adoption:required".into()); }
        if let Some(value) = &self.failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.final_state { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.final_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.initial_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.profile.trim().is_empty() { failures.push("SemanticSourceSessionResult2.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSessionResult2.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSourceSessionResult2.schema:required".into()); }
        if self.source_bindings_sha256.len() != 64 || !self.source_bindings_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSessionResult2.source_bindings_sha256:sha256".into()); }
        if self.status.trim().is_empty() { failures.push("SemanticSourceSessionResult2.status:required".into()); }

        if !["not_assessed",].contains(&self.engineering_adoption.as_str()) { failures.push("SemanticSourceSessionResult2.engineering_adoption:status".into()); }
        if !["semantica.rdflib760-source-session/2",].contains(&self.profile.as_str()) { failures.push("SemanticSourceSessionResult2.profile:status".into()); }
        if !["semantica.source-session-result/2",].contains(&self.schema.as_str()) { failures.push("SemanticSourceSessionResult2.schema:status".into()); }
        if !["completed","failed","partial",].contains(&self.status.as_str()) { failures.push("SemanticSourceSessionResult2.status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit ordered source-session3 Result on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionResult3 {
    /// This local session never establishes engineering adoption
    pub engineering_adoption: String,
    /// Actual action or envelope failure; never replaced by empty success
    pub failure: Option<SemanticSourceSessionFailure>,
    /// Final same-owner source identity observed without warming live caches
    pub final_state: Option<SemanticRdfLibDatasetSnapshot>,
    /// Actual final observation error; successful actions alone do not imply complete readback
    pub final_state_failure: Option<SemanticSourceSessionFailure>,
    /// Actual admitted constructor profile and cache setting; original new action representation
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub initial_result: serde_json::Value,
    /// Actual immediately constructed session observation, corresponding to original new action
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub initial_state: serde_json::Value,
    /// Initial observation error; no fabricated new state
    pub initial_state_failure: Option<SemanticSourceSessionFailure>,
    /// Explicit local source session admission; not public query profile5
    pub profile: String,
    /// SHA256 of complete ordered canonical request JSON, distinct from original RDF byte digests
    pub request_sha256: String,
    /// Explicit versioned envelope identity
    pub schema: String,
    /// SHA256 of complete ordered source-binding array; complete original bindings remain in the immutable request, avoiding unbounded repeated identity text in responses
    pub source_bindings_sha256: String,
    /// Actual execution status, separate from engineering acceptance and whole-scope coverage
    pub status: String,
    /// Exactly one result for every declared action in its original order including skipped actions
    pub steps: Vec<SemanticSourceSessionStep2>,
    /// One witness per completed storage action in exact action order; failed, skipped and other actions have none
    pub storage_witnesses: Vec<SemanticSourceSessionStorageWitness3>,
}
impl SemanticSourceSessionResult3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.engineering_adoption.trim().is_empty() { failures.push("SemanticSourceSessionResult3.engineering_adoption:required".into()); }
        if let Some(value) = &self.failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.final_state { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.final_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.initial_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.profile.trim().is_empty() { failures.push("SemanticSourceSessionResult3.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSessionResult3.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSourceSessionResult3.schema:required".into()); }
        if self.source_bindings_sha256.len() != 64 || !self.source_bindings_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSessionResult3.source_bindings_sha256:sha256".into()); }
        if self.status.trim().is_empty() { failures.push("SemanticSourceSessionResult3.status:required".into()); }

        if !["not_assessed",].contains(&self.engineering_adoption.as_str()) { failures.push("SemanticSourceSessionResult3.engineering_adoption:status".into()); }
        if !["semantica.rdflib760-source-session/3",].contains(&self.profile.as_str()) { failures.push("SemanticSourceSessionResult3.profile:status".into()); }
        if !["semantica.source-session-result/3",].contains(&self.schema.as_str()) { failures.push("SemanticSourceSessionResult3.schema:status".into()); }
        if !["completed","failed","partial",].contains(&self.status.as_str()) { failures.push("SemanticSourceSessionResult3.status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit ordered source-session4 Result on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionResult4 {
    /// This local session never establishes engineering adoption
    pub engineering_adoption: String,
    /// Actual action or envelope failure; never replaced by empty success
    pub failure: Option<SemanticSourceSessionFailure>,
    /// Final same-owner source identity observed without warming live caches
    pub final_state: Option<SemanticRdfLibDatasetSnapshot>,
    /// Actual final observation error; successful actions alone do not imply complete readback
    pub final_state_failure: Option<SemanticSourceSessionFailure>,
    /// Actual admitted constructor profile and cache setting; original new action representation
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub initial_result: serde_json::Value,
    /// Actual immediately constructed session observation, corresponding to original new action
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub initial_state: serde_json::Value,
    /// Initial observation error; no fabricated new state
    pub initial_state_failure: Option<SemanticSourceSessionFailure>,
    /// Explicit local source session admission; not public query profile5
    pub profile: String,
    /// SHA256 of complete ordered canonical request JSON, distinct from original RDF byte digests
    pub request_sha256: String,
    /// Explicit versioned envelope identity
    pub schema: String,
    /// SHA256 of complete ordered source-binding array; complete original bindings remain in the immutable request, avoiding unbounded repeated identity text in responses
    pub source_bindings_sha256: String,
    /// Actual execution status, separate from engineering acceptance and whole-scope coverage
    pub status: String,
    /// Exactly one result for every declared action in its original order including skipped actions
    pub steps: Vec<SemanticSourceSessionStep2>,
    /// One witness per completed storage action in exact action order; failed, skipped and other actions have none
    pub storage_witnesses: Vec<SemanticSourceSessionStorageWitness3>,
    /// Initial and every action namespace/source capture with actual failure and skipped state; same source owner
    pub trace: SemanticSourceSessionTrace4,
}
impl SemanticSourceSessionResult4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.engineering_adoption.trim().is_empty() { failures.push("SemanticSourceSessionResult4.engineering_adoption:required".into()); }
        if let Some(value) = &self.failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.final_state { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.final_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.initial_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.profile.trim().is_empty() { failures.push("SemanticSourceSessionResult4.profile:required".into()); }
        if self.request_sha256.len() != 64 || !self.request_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSessionResult4.request_sha256:sha256".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSourceSessionResult4.schema:required".into()); }
        if self.source_bindings_sha256.len() != 64 || !self.source_bindings_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSessionResult4.source_bindings_sha256:sha256".into()); }
        if self.status.trim().is_empty() { failures.push("SemanticSourceSessionResult4.status:required".into()); }

        if !["not_assessed",].contains(&self.engineering_adoption.as_str()) { failures.push("SemanticSourceSessionResult4.engineering_adoption:status".into()); }
        if !["semantica.rdflib760-source-session/4",].contains(&self.profile.as_str()) { failures.push("SemanticSourceSessionResult4.profile:status".into()); }
        if !["semantica.source-session-result/4",].contains(&self.schema.as_str()) { failures.push("SemanticSourceSessionResult4.schema:status".into()); }
        if !["completed","failed","partial",].contains(&self.status.as_str()) { failures.push("SemanticSourceSessionResult4.status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit local source-session Step on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionStep {
    /// Failure-state observation through disposable readback clone; live snapshot cache unchanged
    pub after_state: Option<SemanticRdfLibDatasetSnapshot>,
    /// Failed state observation remains unknown; no empty graph substitute
    pub after_state_failure: Option<SemanticSourceSessionFailure>,
    /// UPDATE typed before_backend boundary; not inferred from unchanged revision
    pub backend_phase: String,
    /// Actual action or envelope failure; never replaced by empty success
    pub failure: Option<SemanticSourceSessionFailure>,
    /// Unique action identity within the complete ordered request
    pub id: String,
    /// Explicit existing DatasetSession operation; no generic scripting or implied adoption
    pub operation: String,
    /// Actual loader IRI calls in execution order; no network access
    pub resource_requests: Vec<String>,
    /// False if bounded observation cannot retain all loader IRIs; never silently counted as complete
    pub resource_requests_complete: bool,
    /// Unchanged existing mutation query or source snapshot DTO as JSON; ordered rows/null/absent preserved
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub result: serde_json::Value,
    /// Same live session revision after action; failure can change graph without increasing it
    pub revision_after: u64,
    /// Same live session revision immediately before this action
    pub revision_before: u64,
    /// Original complete contexts/quads/cache/revision observation from same live source owner; only observation copies sorted
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub source_state: serde_json::Value,
    /// Actual state observation failure remains unknown
    pub source_state_failure: Option<SemanticSourceSessionFailure>,
    /// Actual execution status, separate from engineering acceptance and whole-scope coverage
    pub status: String,
}
impl SemanticSourceSessionStep {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Some(value) = &self.after_state { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.after_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.backend_phase.trim().is_empty() { failures.push("SemanticSourceSessionStep.backend_phase:required".into()); }
        if let Some(value) = &self.failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.id.trim().is_empty() { failures.push("SemanticSourceSessionStep.id:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticSourceSessionStep.operation:required".into()); }
        if let Some(value) = &self.source_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.status.trim().is_empty() { failures.push("SemanticSourceSessionStep.status:required".into()); }

        if !["not_applicable","not_entered","reached","unknown",].contains(&self.backend_phase.as_str()) { failures.push("SemanticSourceSessionStep.backend_phase:status".into()); }
        if !["clear","load","query","snapshot","update",].contains(&self.operation.as_str()) { failures.push("SemanticSourceSessionStep.operation:status".into()); }
        if !["completed","failed","skipped",].contains(&self.status.as_str()) { failures.push("SemanticSourceSessionStep.status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit ordered source-session2 Step on the existing source owner
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionStep2 {
    /// Failure-state observation through disposable readback clone; live snapshot cache unchanged
    pub after_state: Option<SemanticRdfLibDatasetSnapshot>,
    /// Failed state observation remains unknown; no empty graph substitute
    pub after_state_failure: Option<SemanticSourceSessionFailure>,
    /// UPDATE typed before_backend boundary; not inferred from unchanged revision
    pub backend_phase: String,
    /// Actual action or envelope failure; never replaced by empty success
    pub failure: Option<SemanticSourceSessionFailure>,
    /// Unique action identity within the complete ordered request
    pub id: String,
    /// Explicit existing DatasetSession operation; no generic scripting or implied adoption
    pub operation: String,
    /// Actual loader IRI calls in execution order; no network access
    pub resource_requests: Vec<String>,
    /// False if bounded observation cannot retain all loader IRIs; never silently counted as complete
    pub resource_requests_complete: bool,
    /// Unchanged existing mutation query or source snapshot DTO as JSON; ordered rows/null/absent preserved
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub result: serde_json::Value,
    /// Same live session revision after action; failure can change graph without increasing it
    pub revision_after: u64,
    /// Same live session revision immediately before this action
    pub revision_before: u64,
    /// Original complete contexts/quads/cache/revision observation from same live source owner; only observation copies sorted
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub source_state: serde_json::Value,
    /// Actual state observation failure remains unknown
    pub source_state_failure: Option<SemanticSourceSessionFailure>,
    /// Actual execution status, separate from engineering acceptance and whole-scope coverage
    pub status: String,
}
impl SemanticSourceSessionStep2 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Some(value) = &self.after_state { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.after_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.backend_phase.trim().is_empty() { failures.push("SemanticSourceSessionStep2.backend_phase:required".into()); }
        if let Some(value) = &self.failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.id.trim().is_empty() { failures.push("SemanticSourceSessionStep2.id:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticSourceSessionStep2.operation:required".into()); }
        if let Some(value) = &self.source_state_failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.status.trim().is_empty() { failures.push("SemanticSourceSessionStep2.status:required".into()); }

        if !["not_applicable","not_entered","reached","unknown",].contains(&self.backend_phase.as_str()) { failures.push("SemanticSourceSessionStep2.backend_phase:status".into()); }
        if !["clear","diff","load","query","serialize","snapshot","update",].contains(&self.operation.as_str()) { failures.push("SemanticSourceSessionStep2.operation:status".into()); }
        if !["completed","failed","skipped",].contains(&self.status.as_str()) { failures.push("SemanticSourceSessionStep2.status:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Actual same-owner action and observation sequence, including failed and skipped actions
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionStepWitness4 {
    /// Exactly the declared action identity
    pub action_id: String,
    /// Whether actual operation was entered; capture refusal before execution is false
    pub action_started: bool,
    /// After actual action before any live state observer
    pub after_action: Option<SemanticSourceNamespaceCheckpoint4>,
    /// After the original live source-state observation
    pub after_observation: Option<SemanticSourceNamespaceCheckpoint4>,
    /// Before action, without source-state observer side effects
    pub before: Option<SemanticSourceNamespaceCheckpoint4>,
    /// All required capture records present; action itself may still have failed
    pub complete: bool,
    /// Lossless complete typed rows and contexts from the very same state observer
    pub source: Option<SemanticSourceStateWitness3>,
    /// Complete successful Turtle read/text witness; unavailable after original or local failure
    pub turtle: Option<SemanticSourceTurtleWitness4>,
}
impl SemanticSourceSessionStepWitness4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.action_id.trim().is_empty() { failures.push("SemanticSourceSessionStepWitness4.action_id:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Operation-local full witness; not a caller snapshot or independently trusted source statement
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionStorageWitness3 {
    /// Exact completed storage action identity in this request
    pub action_id: String,
    /// Required only for completed implicit-current diff; existing owner actual once-computed snapshot
    pub current_snapshot: Option<SemanticRdfLibDatasetSnapshot>,
    /// Complete same-observation typed state for implicit diff when source observation succeeds; absent only with actual observation failure
    pub current_source: Option<SemanticSourceStateWitness3>,
    /// Required only for completed serialize; exact row content and order
    pub serialization: Option<SemanticSourceSerializationWitness3>,
}
impl SemanticSourceSessionStorageWitness3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.action_id.trim().is_empty() { failures.push("SemanticSourceSessionStorageWitness3.action_id:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Full batch namespace and observation trace
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSessionTrace4 {
    /// Final actual namespace, separate from cloned final snapshot
    pub final_namespace: SemanticSourceNamespaceCheckpoint4,
    /// After initial observer
    pub initial_after: SemanticSourceNamespaceCheckpoint4,
    /// Cold session before original initial observer
    pub initial_before: SemanticSourceNamespaceCheckpoint4,
    /// Typed original initial source-state rows
    pub initial_source: Option<SemanticSourceStateWitness3>,
    /// Exactly one trace per declared action including skipped actions
    pub steps: Vec<SemanticSourceSessionStepWitness4>,
}
impl SemanticSourceSessionTrace4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Caller source snapshot argument accepts both known v1/v2 labels without changing v2-only output DTO
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSnapshotArgument {
    /// Known source label input checked before runtime algorithm compatibility
    pub algorithm: String,
    /// Original source canonical text; not parsed or re-canonicalized as native RDFC
    pub canonical_nquads: String,
    /// Original caller observation metadata; never dataset identity or source authority
    pub created_at: String,
    /// Claimed canonical-text checksum; original integrity is checked by source diff
    pub dataset_sha256: String,
    /// Source count, separately compared with original Python physical splitlines
    pub quad_count: u64,
    /// Snapshot provenance metadata; does not change dataset identity
    pub revision: u64,
    /// Original snapshot DTO input version
    pub schema_version: String,
}
impl SemanticSourceSnapshotArgument {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.algorithm.trim().is_empty() { failures.push("SemanticSourceSnapshotArgument.algorithm:required".into()); }
        if self.dataset_sha256.len() != 64 || !self.dataset_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSnapshotArgument.dataset_sha256:sha256".into()); }
        if self.schema_version.trim().is_empty() { failures.push("SemanticSourceSnapshotArgument.schema_version:required".into()); }

        if !["semantica-rdf-dataset-c14n-v1","semantica-rdf-dataset-c14n-v2",].contains(&self.algorithm.as_str()) { failures.push("SemanticSourceSnapshotArgument.algorithm:status".into()); }
        if !["1.0",].contains(&self.schema_version.as_str()) { failures.push("SemanticSourceSnapshotArgument.schema_version:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 明确UTF8 byte半开区间，不能冒充原JSON char范围
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSpan {
    /// 不含结束字节
    pub end_byte: u64,
    /// 可选精确原文；提供时须与片段一致
    pub quote: Option<String>,
    /// 明确协议版本
    pub schema: String,
    /// 对应来源实体IRI
    pub source_id: String,
    /// 对应原字节身份
    pub source_sha256: String,
    /// 片段IRI
    pub span_id: String,
    /// 精确片段字节身份
    pub span_sha256: String,
    /// 含起始字节
    pub start_byte: u64,
}
impl SemanticSourceSpan {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticSourceSpan.schema:required".into()); }
        if self.source_id.trim().is_empty() { failures.push("SemanticSourceSpan.source_id:required".into()); }
        if self.source_sha256.len() != 64 || !self.source_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSpan.source_sha256:sha256".into()); }
        if self.span_id.trim().is_empty() { failures.push("SemanticSourceSpan.span_id:required".into()); }
        if self.span_sha256.len() != 64 || !self.span_sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) { failures.push("SemanticSourceSpan.span_sha256:sha256".into()); }

        if !["semantica.source-span/1",].contains(&self.schema.as_str()) { failures.push("SemanticSourceSpan.schema:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// 核验回执DTO；公开DTO不能作为可信输入，verified wrapper仅私有构造
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceSpanProof {
    /// 原字节绑定
    pub binding: SemanticSourceBinding,
    /// 来源一致不能授予工程采用
    pub engineering_status: String,
    /// 实际核验范围
    pub profile: String,
    /// 精确片段绑定
    pub span: SemanticSourceSpan,
}
impl SemanticSourceSpanProof {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Err(errors) = self.binding.validate() { failures.extend(errors); }

        if self.engineering_status.trim().is_empty() { failures.push("SemanticSourceSpanProof.engineering_status:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticSourceSpanProof.profile:required".into()); }
        if let Err(errors) = self.span.validate() { failures.extend(errors); }


        if !["unknown",].contains(&self.engineering_status.as_str()) { failures.push("SemanticSourceSpanProof.engineering_status:status".into()); }
        if !["semantica.source-utf8-span-sha256/1",].contains(&self.profile.as_str()) { failures.push("SemanticSourceSpanProof.profile:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete typed state from the same original source observation, including empty registered contexts
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceStateWitness3 {
    /// All original registered contexts, including empty/default contexts
    pub contexts: Vec<SemanticSourceStorageTerm3>,
    /// All actual source quads with exact generalized terms and contexts
    pub quads: Vec<SemanticSourceWriterRow3>,
}
impl SemanticSourceStateWitness3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Exact generalized original term, including Variable and relative IRI; not a new RDF binding domain
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceStorageTerm3 {
    /// Original optional datatype; absent differs from explicit xsd:string
    pub datatype: Option<String>,
    /// Original language spelling and optionality, without case folding
    pub language: Option<String>,
    /// Original source term kind; Variable observation error remains explicit
    pub term_type: String,
    /// Exact source lexical value; empty original values remain permitted
    pub value: String,
}
impl SemanticSourceStorageTerm3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.term_type.trim().is_empty() { failures.push("SemanticSourceStorageTerm3.term_type:required".into()); }

        if !["blank_node","iri","literal","variable",].contains(&self.term_type.as_str()) { failures.push("SemanticSourceStorageTerm3.term_type:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One actual ordered read by the existing Turtle writer
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceTurtleRead4 {
    /// Actual scalar comparison/truth/cache answer; absent for scan
    pub answer: Option<bool>,
    /// Original compared/tested terms; empty for scan
    pub arguments: Vec<SemanticSourceStorageTerm3>,
    /// Exact scan object or wildcard
    pub object: Option<SemanticSourceStorageTerm3>,
    /// Exact read kind
    pub operation: String,
    /// Exact scan predicate or wildcard
    pub predicate: Option<SemanticSourceStorageTerm3>,
    /// Complete selected-context scan in source index order, graph field absent
    pub rows: Vec<SemanticSourceWriterRow3>,
    /// Exact scan subject or wildcard
    pub subject: Option<SemanticSourceStorageTerm3>,
}
impl SemanticSourceTurtleRead4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticSourceTurtleRead4.operation:required".into()); }

        if !["cached","less","scan","truth",].contains(&self.operation.as_str()) { failures.push("SemanticSourceTurtleRead4.operation:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete successful internal Turtle render and namespace transition, based on actual source reads
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceTurtleWitness4 {
    /// Actual original default-context registration before preparation
    pub default_registered_before: bool,
    /// Namespace state after actual render, before later state observation
    pub namespace_after: SemanticSourceNamespaceState4,
    /// Namespace state before any preparation
    pub namespace_before: SemanticSourceNamespaceState4,
    /// Actual original Rust retained namespace byte charge after render
    pub namespace_bytes_after: u64,
    /// Actual original Rust retained namespace byte charge before preparation
    pub namespace_bytes_before: u64,
    /// Actual original Rust retained namespace entry charge after render
    pub namespace_entries_after: u64,
    /// Actual original Rust retained namespace entry charge before preparation
    pub namespace_entries_before: u64,
    /// Namespace state after actual preparation
    pub namespace_prepared: SemanticSourceNamespaceState4,
    /// Complete original preparation visit; selected triples with graph absent
    pub preparation_rows: Vec<SemanticSourceWriterRow3>,
    /// Complete successful read transcript; no complete witness on failed or incomplete capture
    pub reads: Vec<SemanticSourceTurtleRead4>,
    /// Full exact serialize request digest
    pub request_sha256: String,
    /// Explicit internal witness schema
    pub schema: String,
    /// Digest of complete exact emitted text
    pub text_sha256: String,
}
impl SemanticSourceTurtleWitness4 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.request_sha256.trim().is_empty() { failures.push("SemanticSourceTurtleWitness4.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticSourceTurtleWitness4.schema:required".into()); }
        if self.text_sha256.trim().is_empty() { failures.push("SemanticSourceTurtleWitness4.text_sha256:required".into()); }

        if !["semantica.source-turtle-witness/4",].contains(&self.schema.as_str()) { failures.push("SemanticSourceTurtleWitness4.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One exact source writer visit; order belongs to original visitor not sorted state
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSourceWriterRow3 {
    /// Actual context; none is original default graph, never UNION
    pub graph: Option<SemanticSourceStorageTerm3>,
    /// Actual source object
    pub object: SemanticSourceStorageTerm3,
    /// Actual generalized predicate; no false IRI admission
    pub predicate: SemanticSourceStorageTerm3,
    /// Actual source subject
    pub subject: SemanticSourceStorageTerm3,
}
impl SemanticSourceWriterRow3 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original report entry; fact identifier retains its actual JSON value
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalConsistencyIssue {
    /// Original truthy relationship ID; not string coerced
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub fact_id: serde_json::Value,
    /// Original issue kind
    pub issue_type: String,
    /// Exact original diagnostic text
    pub message: String,
}
impl SemanticTemporalConsistencyIssue {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.issue_type.trim().is_empty() { failures.push("SemanticTemporalConsistencyIssue.issue_type:required".into()); }
        if self.message.trim().is_empty() { failures.push("SemanticTemporalConsistencyIssue.message:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Exact two ordered issue sequences from source method
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalConsistencyReport {
    /// First input pass followed by group-order parse errors; duplicates retained
    pub errors: Vec<SemanticTemporalConsistencyIssue>,
    /// Insertion-order groups, stable start ordering, adjacent-pair observations
    pub warnings: Vec<SemanticTemporalConsistencyIssue>,
}
impl SemanticTemporalConsistencyReport {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Immutable explicit-clock consistency check
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalConsistencyRequest {
    /// Required explicit missing-recorded clock; no implied engineering effective time
    pub clock_at: String,
    /// Original comparison granularity, validated only when an actual comparison is reached
    pub granularity: String,
    /// Graph object; bounded entities and relationships, opaque remaining metadata
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub graph: serde_json::Value,
    /// Exact semantica.temporal-consistency-request/1
    pub schema: String,
}
impl SemanticTemporalConsistencyRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_at.trim().is_empty() { failures.push("SemanticTemporalConsistencyRequest.clock_at:required".into()); }
        if self.granularity.trim().is_empty() { failures.push("SemanticTemporalConsistencyRequest.granularity:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalConsistencyRequest.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Bounded completed source report with original positions and invocation witnesses
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalConsistencyResult {
    /// Canonical explicit UTC clock
    pub clock_at: String,
    /// Actual default clock substitutions in both passes
    pub clock_fallback_count: u64,
    /// Input relationship positions aligned with report.errors
    pub error_relationship_indices: Vec<u64>,
    /// Actual grouping comparisons, bounded separately from input record count
    pub group_comparison_count: u64,
    /// Exact semantica.source-temporal-consistency/1
    pub profile: String,
    /// Original errors and warnings; no implied all-engineering-valid boolean
    pub report: SemanticTemporalConsistencyReport,
    /// Canonical complete typed request identity
    pub request_sha256: String,
    /// Exact semantica.temporal-consistency-result/1
    pub schema: String,
    /// Input relationship positions aligned with report.warnings
    pub warning_relationship_indices: Vec<u64>,
}
impl SemanticTemporalConsistencyResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_at.trim().is_empty() { failures.push("SemanticTemporalConsistencyResult.clock_at:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTemporalConsistencyResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticTemporalConsistencyResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalConsistencyResult.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Source event with exact unmodified input record and list position
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalFactEvent {
    /// Original added/modified/removed event name; duplicates preserved
    pub change_type: String,
    /// Complete original JSON dictionary including unknown fields
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub fact: serde_json::Value,
    /// Zero-based input list index, independent of nonunique business id
    pub origin_index: u64,
    /// entities or relationships in the supplied graph
    pub origin_kind: String,
    /// Canonical UTC microsecond timestamp, not file time
    pub timestamp: String,
}
impl SemanticTemporalFactEvent {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.change_type.trim().is_empty() { failures.push("SemanticTemporalFactEvent.change_type:required".into()); }
        if self.origin_kind.trim().is_empty() { failures.push("SemanticTemporalFactEvent.origin_kind:required".into()); }
        if self.timestamp.trim().is_empty() { failures.push("SemanticTemporalFactEvent.timestamp:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Ordered input reference within a revision classification
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalFactReference {
    /// Complete unmodified original dictionary
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub fact: serde_json::Value,
    /// Zero-based originals list index; duplicate dictionaries retained separately
    pub index: u64,
}
impl SemanticTemporalFactReference {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One immutable projection, no graph retrieval, implicit time or Project mutation
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalFactsRequest {
    /// Timeline scalar JSON identifier using Python string semantics, including null; null for revision-impact
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub entity_id: serde_json::Value,
    /// Ordered originals for revision-impact; empty for timeline
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub facts: Vec<serde_json::Value>,
    /// Timeline graph dictionary with optional entities/relationships lists; null for revision-impact
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub graph: serde_json::Value,
    /// timeline or revision-impact
    pub operation: String,
    /// Revision-impact dictionary; null for timeline
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub revision: serde_json::Value,
    /// Exact semantica.temporal-facts-request/1
    pub schema: String,
}
impl SemanticTemporalFactsRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticTemporalFactsRequest.operation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalFactsRequest.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete source projection; classification does not approve facts or decisions
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalFactsResult {
    /// Original interval contains revision interval, inclusive endpoints
    pub affected: Vec<SemanticTemporalFactReference>,
    /// Timeline only, stable timestamp/change_type order
    pub events: Vec<SemanticTemporalFactEvent>,
    /// Exact admitted operation
    pub operation: String,
    /// Not affected, but original overlaps revision; a revision containing the original remains partial
    pub partial: Vec<SemanticTemporalFactReference>,
    /// Exact semantica.source-temporal-facts/1
    pub profile: String,
    /// Whole canonical typed request identity before normalization, including opaque input fields and input order
    pub request_sha256: String,
    /// Exact semantica.temporal-facts-result/1
    pub schema: String,
    /// Neither source containment nor overlap; unrelated engineering applicability remains unknown
    pub unaffected: Vec<SemanticTemporalFactReference>,
}
impl SemanticTemporalFactsResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticTemporalFactsResult.operation:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTemporalFactsResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticTemporalFactsResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalFactsResult.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One source interval; null or OPEN end; label retained only where original algorithm retains it
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalInterval {
    /// Null or exact OPEN is open-ended; finite end string normalized to UTC; output open end is null
    pub end: Option<String>,
    /// Opaque optional label; stable merge retains first label, normalization drops it and gaps use gap
    pub label: Option<String>,
    /// String time normalized through the existing fixed CPython ISO parser; ASCII date padding and naive-as-UTC follow the KG parser
    pub start: String,
}
impl SemanticTemporalInterval {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.start.trim().is_empty() { failures.push("SemanticTemporalInterval.start:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One original source path, with untouched node and edge values
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalPath {
    /// Full original relationship records in traversal order
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub edges: Vec<serde_json::Value>,
    /// Edge count, distinct from the original max_path_length node-count test
    pub length: u64,
    /// Original node sequence and values
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub path: Vec<serde_json::Value>,
}
impl SemanticTemporalPath {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Exact original traversal result
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalPathsReport {
    /// Number of returned paths, not a claim of graph completeness
    pub num_paths: u64,
    /// BFS result order; global visited can discard alternative paths
    pub paths: Vec<SemanticTemporalPath>,
    /// Original source parameter
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub source: serde_json::Value,
    /// Original target parameter
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub target: serde_json::Value,
}
impl SemanticTemporalPathsReport {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit-clock bounded source path query
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalPathsRequest {
    /// Required fallback clock; no implied engineering effective time
    pub clock_at: String,
    /// Optional inclusive window end; empty string false, OPEN parsed unknown
    pub end_time: Option<String>,
    /// Enables original timestamp ordering only
    pub enforce_causal_ordering: bool,
    /// Original comparison granularity, checked only when comparison occurs
    pub granularity: String,
    /// Graph object; at most4096 relationship records; entities and other metadata opaque
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub graph: serde_json::Value,
    /// Original node-count cap; absent/zero unlimited, negative prunes all
    pub max_path_length: Option<i64>,
    /// strict/overlap/loose; unknown strategy still normalizes two edges then accepts
    pub ordering_strategy: String,
    /// Exact semantica.temporal-paths-request/1
    pub schema: String,
    /// Original raw identity including Python bool/number equality
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub source: serde_json::Value,
    /// Optional inclusive window start; empty string false, OPEN parsed unknown
    pub start_time: Option<String>,
    /// Original raw target identity, checked before visited hashing
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub target: serde_json::Value,
    /// Retained but unused by original algorithm; cannot enforce engineering requirements
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub temporal_constraints: Option<serde_json::Value>,
}
impl SemanticTemporalPathsRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_at.trim().is_empty() { failures.push("SemanticTemporalPathsRequest.clock_at:required".into()); }
        if self.granularity.trim().is_empty() { failures.push("SemanticTemporalPathsRequest.granularity:required".into()); }
        if self.ordering_strategy.trim().is_empty() { failures.push("SemanticTemporalPathsRequest.ordering_strategy:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalPathsRequest.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete source report and execution witnesses; no engineering validity conclusion
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalPathsResult {
    /// Canonical explicit UTC clock
    pub clock_at: String,
    /// Actual normalization clock substitutions including repeated edges
    pub clock_fallback_count: u64,
    /// Actual adjacency edge visits, including rejected cycles and order
    pub edge_expansion_count: u64,
    /// Bounded raw Python-equality calls across indexing, visits and cycle checks
    pub identity_comparison_count: u64,
    /// Original relationship positions, aligned with report paths and edges
    pub path_relationship_indices: Vec<Vec<u64>>,
    /// Exact semantica.source-temporal-paths/1
    pub profile: String,
    /// Total queued entries including initial source
    pub queued_path_count: u64,
    /// Complete original output within declared operational limits
    pub report: SemanticTemporalPathsReport,
    /// Canonical complete typed request identity including unused metadata
    pub request_sha256: String,
    /// Exact semantica.temporal-paths-result/1
    pub schema: String,
}
impl SemanticTemporalPathsResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_at.trim().is_empty() { failures.push("SemanticTemporalPathsResult.clock_at:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTemporalPathsResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticTemporalPathsResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalPathsResult.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// One ordered source entry invocation
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalPatternAction {
    /// Original keyword dictionary with graph; query requires pattern. Absence and explicit null remain distinct. Duplicate Python bindings excluded; extra options opaque except gap_tolerance
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub arguments: serde_json::Value,
    /// detector or query, sharing the same explicit configuration
    pub entry: String,
}
impl SemanticTemporalPatternAction {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.entry.trim().is_empty() { failures.push("SemanticTemporalPatternAction.entry:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete action observation after success or failure
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalPatternStep {
    /// Actual normalization fallback calls, also retained on failed action
    pub clock_fallback_count: u64,
    /// Actual configuration after attempted action including mutation before a later source failure
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub config_after: serde_json::Value,
    /// Actual initial-edge and candidate visits within operational budget
    pub edge_visit_count: u64,
    /// Actual source entry
    pub entry: String,
    /// Source exception or explicit domain/resource refusal; no partial patterns
    pub failure: Option<SemanticRuntimeFailure>,
    /// Actual bounded Python equality calls
    pub identity_comparison_count: u64,
    /// Zero-based action position
    pub index: u64,
    /// Pattern, occurrence, then original edge positions, aligned with complete result
    pub occurrence_relationship_indices: Vec<Vec<Vec<u64>>>,
    /// Exact pattern list or query wrapper; absent on any action failure
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub result: Option<serde_json::Value>,
}
impl SemanticTemporalPatternStep {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.entry.trim().is_empty() { failures.push("SemanticTemporalPatternStep.entry:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit-clock bounded source pattern session
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalPatternsRequest {
    /// At most128 ordered actions; source exceptions do not discard configuration or preceding steps
    pub actions: Vec<SemanticTemporalPatternAction>,
    /// Required fallback clock, not engineering effective time
    pub clock_at: String,
    /// Complete detector configuration object, preserved across actions
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub initial_config: serde_json::Value,
    /// Exact semantica.temporal-patterns-request/1
    pub schema: String,
}
impl SemanticTemporalPatternsRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_at.trim().is_empty() { failures.push("SemanticTemporalPatternsRequest.clock_at:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalPatternsRequest.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete session observation; no engineering validity inference
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalPatternsResult {
    /// Canonical explicit UTC clock
    pub clock_at: String,
    /// Final explicit configuration for a subsequent request; no hidden global state
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub final_config: serde_json::Value,
    /// Exact semantica.source-temporal-patterns/1
    pub profile: String,
    /// Canonical entire typed request including ignored metadata
    pub request_sha256: String,
    /// Exact semantica.temporal-patterns-result/1
    pub schema: String,
    /// Every attempted action in order; source and operational failures separately coded
    pub steps: Vec<SemanticTemporalPatternStep>,
}
impl SemanticTemporalPatternsResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_at.trim().is_empty() { failures.push("SemanticTemporalPatternsResult.clock_at:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTemporalPatternsResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticTemporalPatternsResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalPatternsResult.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original query_at_time result; query text is not parsed
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalQueryView {
    /// Canonical UTC query point; null only when source branch permits it
    pub at_time: Option<String>,
    /// Raw filtered entity records, duplicate ID replacement preserves first insertion position
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub entities: Vec<serde_json::Value>,
    /// Returned entity count
    pub num_entities: u64,
    /// Returned relationship count
    pub num_relationships: u64,
    /// Original unused query string
    pub query: String,
    /// Raw selected edges; include_history substitutes ALL original edges after reconstruction
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub relationships: Vec<serde_json::Value>,
}
impl SemanticTemporalQueryView {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.query.trim().is_empty() { failures.push("SemanticTemporalQueryView.query:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original query_time_range view; exact raw selected relationships and calendar groups
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalRangeQueryView {
    /// Original string; unsupported names retain source union behavior
    pub aggregation: String,
    /// UTC query end expanded to last microsecond of period, or open null
    pub end_time: Option<String>,
    /// Selected count before calendar grouping
    pub num_relationships: u64,
    /// Unused original query text
    pub query: String,
    /// null outside evolution; object of nonempty calendar groups, each containing original records
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub relationship_buckets: serde_json::Value,
    /// Original relationship values in input order including duplicates
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub relationships: Vec<serde_json::Value>,
    /// UTC query start floored to source granularity
    pub start_time: String,
}
impl SemanticTemporalRangeQueryView {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.aggregation.trim().is_empty() { failures.push("SemanticTemporalRangeQueryView.aggregation:required".into()); }
        if self.query.trim().is_empty() { failures.push("SemanticTemporalRangeQueryView.query:required".into()); }
        if self.start_time.trim().is_empty() { failures.push("SemanticTemporalRangeQueryView.start_time:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Immutable explicit-clock range query
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalRangeRequest {
    /// union, intersection, evolution or original unknown-name union fallback
    pub aggregation: String,
    /// Required explicit default-recorded clock; never source validity
    pub clock_at: String,
    /// End string; null/OPEN represent open bound
    pub end_time: Option<String>,
    /// second/minute/hour/day/week/month/year; checked even on empty graph
    pub granularity: String,
    /// Graph object; only relationships consumed; entities/metadata stay opaque
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub graph: serde_json::Value,
    /// Original reserved unused parameter; not a clipping control
    pub include_intervals: bool,
    /// Returned without query-language interpretation
    pub query: String,
    /// Exact semantica.temporal-range-request/1
    pub schema: String,
    /// Start string; null/OPEN are source errors
    pub start_time: Option<String>,
    /// valid, transaction or both; unknown axis fails only if a record is evaluated
    pub time_axis: String,
}
impl SemanticTemporalRangeRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.aggregation.trim().is_empty() { failures.push("SemanticTemporalRangeRequest.aggregation:required".into()); }
        if self.clock_at.trim().is_empty() { failures.push("SemanticTemporalRangeRequest.clock_at:required".into()); }
        if self.granularity.trim().is_empty() { failures.push("SemanticTemporalRangeRequest.granularity:required".into()); }
        if self.query.trim().is_empty() { failures.push("SemanticTemporalRangeRequest.query:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalRangeRequest.schema:required".into()); }
        if self.time_axis.trim().is_empty() { failures.push("SemanticTemporalRangeRequest.time_axis:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete bounded source result and input position witnesses
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalRangeResult {
    /// null or calendar-group input position lists
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub bucket_indices: serde_json::Value,
    /// Canonical UTC explicit request clock
    pub clock_at: String,
    /// Actual source default clock invocations including short circuit and regrouping
    pub clock_fallback_count: u64,
    /// Exact semantica.source-temporal-range/1
    pub profile: String,
    /// Complete original range view
    pub query_result: SemanticTemporalRangeQueryView,
    /// Exact input positions of selected relationships
    pub relationship_indices: Vec<u64>,
    /// Canonical complete typed request identity including unused/raw fields
    pub request_sha256: String,
    /// Exact semantica.temporal-range-result/1
    pub schema: String,
}
impl SemanticTemporalRangeResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_at.trim().is_empty() { failures.push("SemanticTemporalRangeResult.clock_at:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTemporalRangeResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticTemporalRangeResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalRangeResult.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Immutable one-operation request; no graph, clock, API or engineering state change
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalRequest {
    /// Required finite-ended interval only for gaps/coverage
    pub domain: Option<SemanticTemporalInterval>,
    /// Optional for active, required for normalization, forbidden otherwise; second/minute/hour/day/week/month/year
    pub granularity: Option<String>,
    /// Ordered maximum4096 intervals; compare requires2, active/normalize-interval1, normalize-point0
    pub intervals: Vec<SemanticTemporalInterval>,
    /// compare, active, merge, gaps, coverage, normalize-point or normalize-interval
    pub operation: String,
    /// Required only for active or normalize-point; no implicit current time
    pub point: Option<String>,
    /// Exact semantica.temporal-request/1
    pub schema: String,
}
impl SemanticTemporalRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticTemporalRequest.operation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalRequest.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit source algorithm result; optional fields distinguish absent result from false or zero
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalResult {
    /// Active only: start-inclusive finite-end-exclusive; optional granularity floors all three times
    pub active: Option<bool>,
    /// Compare only: inclusive boundary containment, distinct from Allen relation contains
    pub contains: Option<bool>,
    /// Coverage only: union duration divided by finite domain duration, range0..1; zero domain returns0, not percent0..100
    pub coverage_fraction: Option<f64>,
    /// Complete merge/gaps/normalized list; canonical UTC timestamps; empty for scalar operations
    pub intervals: Vec<SemanticTemporalInterval>,
    /// The exact admitted operation
    pub operation: String,
    /// Compare only: excludes before/after/meets/met_by
    pub overlaps: Option<bool>,
    /// Normalized UTC point only for normalize-point
    pub point: Option<String>,
    /// Exact semantica.source-temporal-intervals/1
    pub profile: String,
    /// Compare only: original13 Allen labels with original branch order including zero-length meets
    pub relation: Option<String>,
    /// Canonical digest of complete typed request before normalization, including labels and raw strings
    pub request_sha256: String,
    /// Exact semantica.temporal-result/1
    pub schema: String,
}
impl SemanticTemporalResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.operation.trim().is_empty() { failures.push("SemanticTemporalResult.operation:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTemporalResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticTemporalResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalResult.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Immutable explicit-clock source temporal projection; no adoption or implicit wall clock
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalSnapshotRequest {
    /// Point to query, null/OPEN follow original lazy branch behavior
    pub at_time: Option<String>,
    /// Required explicit clock, always parsed; fallback provenance is not a source fact
    pub clock_at: String,
    /// Normalization dictionary, otherwise null
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub fact: serde_json::Value,
    /// Source constructor granularity, normally day; validated at active check
    pub granularity: String,
    /// Reconstruction/query dictionary, otherwise null
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub graph: serde_json::Value,
    /// query-at only: original all-edge substitution after reconstruction
    pub include_history: bool,
    /// normalize-fact, reconstruct or query-at
    pub operation: String,
    /// Returned unused source query text; empty outside query-at
    pub query: String,
    /// Exact semantica.temporal-snapshot-request/1
    pub schema: String,
    /// query-at only: preserved but unused source option; actual granularity is separate
    pub temporal_precision: Option<String>,
    /// valid, transaction or both; unsupported axes fail when source evaluates a record
    pub time_axis: String,
}
impl SemanticTemporalSnapshotRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_at.trim().is_empty() { failures.push("SemanticTemporalSnapshotRequest.clock_at:required".into()); }
        if self.granularity.trim().is_empty() { failures.push("SemanticTemporalSnapshotRequest.granularity:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticTemporalSnapshotRequest.operation:required".into()); }
        if self.query.trim().is_empty() { failures.push("SemanticTemporalSnapshotRequest.query:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalSnapshotRequest.schema:required".into()); }
        if self.time_axis.trim().is_empty() { failures.push("SemanticTemporalSnapshotRequest.time_axis:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Complete output with raw records and original index/clock witness
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalSnapshotResult {
    /// Canonical UTC caller clock; never sampled implicitly
    pub clock_at: String,
    /// Count of original default-recorded clock calls, including both-axis short circuit
    pub clock_fallback_count: u64,
    /// Exact original entity list positions in output order
    pub entity_indices: Vec<u64>,
    /// Raw normalization input or null
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub fact: serde_json::Value,
    /// Four source-normalized fields or null
    pub normalized: Option<SemanticBiTemporalFields>,
    /// Admitted operation
    pub operation: String,
    /// Exact semantica.source-temporal-snapshot/1
    pub profile: String,
    /// Original point-query view or null
    pub query_result: Option<SemanticTemporalQueryView>,
    /// explicit, valid_from, request_clock; normalization only
    pub recorded_at_basis: Option<String>,
    /// Exact original relationship positions including history substitution
    pub relationship_indices: Vec<u64>,
    /// Canonical complete typed request including clock and opaque fields
    pub request_sha256: String,
    /// Exact semantica.temporal-snapshot-result/1
    pub schema: String,
    /// Reconstructed full raw graph with metadata, or null
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub snapshot: serde_json::Value,
}
impl SemanticTemporalSnapshotResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_at.trim().is_empty() { failures.push("SemanticTemporalSnapshotResult.clock_at:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticTemporalSnapshotResult.operation:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTemporalSnapshotResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticTemporalSnapshotResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalSnapshotResult.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Source version comparison with explicit Python seed and immutable input identity
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalVersionDiffRequest {
    /// Original unused argument retained and bound
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub comparison_metrics: serde_json::Value,
    /// Explicit PYTHONHASHSEED in0..4294967295 for pinned CPython profile; never host entropy
    pub hash_seed: u64,
    /// Original unused options retained and bound
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub options: serde_json::Value,
    /// Exact semantica.temporal-version-diff-request/1
    pub schema: String,
    /// Explicit immutable in-memory storage dictionary; truthy lookup copy preserves source behavior
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub stored_versions: serde_json::Value,
    /// First source label or immediate snapshot value
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub version1: serde_json::Value,
    /// Second source label or immediate snapshot value
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub version2: serde_json::Value,
}
impl SemanticTemporalVersionDiffRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalVersionDiffRequest.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Source version comparison with explicit Python seed and immutable input identity
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalVersionDiffResult {
    /// Original exception with original type/message; no partial comparison
    pub failure: Option<SemanticRuntimeFailure>,
    /// Actual unchanged seed used for hash/set semantics
    pub hash_seed: u64,
    /// Exact semantica.source-temporal-version-diff-cpython313/1
    pub profile: String,
    /// Canonical complete typed request including ignored options and seed
    pub request_sha256: String,
    /// Original ordered comparison; absent on source exception
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub result: Option<serde_json::Value>,
    /// Exact semantica.temporal-version-diff-result/1
    pub schema: String,
    /// Actual label lookup order including failed reads; no fabricated persistence
    pub storage_reads: Vec<String>,
}
impl SemanticTemporalVersionDiffResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.profile.trim().is_empty() { failures.push("SemanticTemporalVersionDiffResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticTemporalVersionDiffResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalVersionDiffResult.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Bounded source version record operation
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalVersionRecordRequest {
    /// Original keyword dictionary: graph required for create_version; exactly snapshot for validation/migration. Preserve absent versus null, all opaque fields and Python truthiness.
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub arguments: serde_json::Value,
    /// Explicit canonical naive local YYYY-MM-DDTHH:MM:SS or nonzero six-digit microseconds; original fallback datetime.now().isoformat() semantics without implicit host clock, timezone or validity inference
    pub clock_local: String,
    /// create_version, validate_snapshot or migrate_snapshot
    pub operation: String,
    /// Exact semantica.temporal-version-records-request/1
    pub schema: String,
}
impl SemanticTemporalVersionRecordRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_local.trim().is_empty() { failures.push("SemanticTemporalVersionRecordRequest.clock_local:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticTemporalVersionRecordRequest.operation:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalVersionRecordRequest.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Immediate exact source record or source exception observation
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTemporalVersionRecordResult {
    /// Actual source fallback evaluation before a later graph/copy failure
    pub clock_fallback_count: u64,
    /// Unchanged explicit local wall clock; not UTC or evidence-effective date
    pub clock_local: String,
    /// Original exception type/message retained; transport or aggregate resource refusal rejects the entire request
    pub failure: Option<SemanticRuntimeFailure>,
    /// Actual source operation
    pub operation: String,
    /// Exact semantica.source-temporal-version-records/1
    pub profile: String,
    /// Canonical complete typed input, including unused fields
    pub request_sha256: String,
    /// Immediate by-value JSON record or validation bool, absent on source failure. False validation is a completed operation, not a runtime error.
    #[serde(default, deserialize_with = "crate::json::deserialize_preserved")]
    pub result: Option<serde_json::Value>,
    /// Exact semantica.temporal-version-records-result/1
    pub schema: String,
}
impl SemanticTemporalVersionRecordResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.clock_local.trim().is_empty() { failures.push("SemanticTemporalVersionRecordResult.clock_local:required".into()); }
        if self.operation.trim().is_empty() { failures.push("SemanticTemporalVersionRecordResult.operation:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTemporalVersionRecordResult.profile:required".into()); }
        if self.request_sha256.trim().is_empty() { failures.push("SemanticTemporalVersionRecordResult.request_sha256:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTemporalVersionRecordResult.schema:required".into()); }

        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit original schema and typed generation behavior, source-bound candidates and independent validation effects
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTypedAttempt {
    /// Original schema/typed execution field failure
    pub failure: Option<SemanticProviderFailure>,
    /// Original schema/typed execution field iteration
    pub iteration: u64,
    /// Original schema/typed execution field request
    pub request: SemanticProviderRequest,
    /// Original schema/typed execution field reserved
    pub reserved: bool,
    /// Original schema/typed execution field result
    pub result: Option<SemanticProviderResult>,
    /// Original schema/typed execution field stage
    pub stage: String,
    /// Original schema/typed execution field validation
    pub validation: Option<SemanticSchemaValidationResult>,
}
impl SemanticTypedAttempt {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Some(value) = &self.failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Err(errors) = self.request.validate() { failures.extend(errors); }

        if let Some(value) = &self.result { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.stage.trim().is_empty() { failures.push("SemanticTypedAttempt.stage:required".into()); }
        if let Some(value) = &self.validation { if let Err(errors) = value.validate() { failures.extend(errors); } }


        if !["plain","structured","validation",].contains(&self.stage.as_str()) { failures.push("SemanticTypedAttempt.stage:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original typed manual dispatch through the sole existing shared Harness context
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTypedDispatchRequest {
    /// Original typed dispatch generation observation; model-call authority remains injected
    pub generation: SemanticProviderRequest,
    /// Original typed dispatch mode observation; model-call authority remains injected
    pub mode: String,
    /// Original typed dispatch schema observation; model-call authority remains injected
    pub schema: String,
}
impl SemanticTypedDispatchRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Err(errors) = self.generation.validate() { failures.extend(errors); }

        if self.mode.trim().is_empty() { failures.push("SemanticTypedDispatchRequest.mode:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTypedDispatchRequest.schema:required".into()); }

        if !["plain","structured",].contains(&self.mode.as_str()) { failures.push("SemanticTypedDispatchRequest.mode:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Original typed manual dispatch through the sole existing shared Harness context
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTypedDispatchResult {
    /// Original typed dispatch execution observation; model-call authority remains injected
    pub execution: String,
    /// Original typed dispatch failure observation; model-call authority remains injected
    pub failure: Option<SemanticProviderFailure>,
    /// Original typed dispatch output observation; model-call authority remains injected
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub output: serde_json::Value,
    /// Original typed dispatch reserved observation; model-call authority remains injected
    pub reserved: bool,
    /// Original typed dispatch response observation; model-call authority remains injected
    pub response: Option<SemanticProviderResult>,
    /// Original typed dispatch schema observation; model-call authority remains injected
    pub schema: String,
}
impl SemanticTypedDispatchResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.execution.trim().is_empty() { failures.push("SemanticTypedDispatchResult.execution:required".into()); }
        if let Some(value) = &self.failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if let Some(value) = &self.response { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.schema.trim().is_empty() { failures.push("SemanticTypedDispatchResult.schema:required".into()); }

        if !["completed","failed",].contains(&self.execution.as_str()) { failures.push("SemanticTypedDispatchResult.execution:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit original schema and typed generation behavior, source-bound candidates and independent validation effects
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTypedGenerationRequest {
    /// Original schema/typed execution field generation
    pub generation: SemanticProviderRequest,
    /// Original schema/typed execution field max_retries
    pub max_retries: i64,
    /// Original schema/typed execution field model_id
    pub model_id: String,
    /// Original schema/typed execution field profile
    pub profile: String,
    /// Original schema/typed execution field schema
    pub schema: String,
}
impl SemanticTypedGenerationRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if let Err(errors) = self.generation.validate() { failures.extend(errors); }

        if self.model_id.trim().is_empty() { failures.push("SemanticTypedGenerationRequest.model_id:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTypedGenerationRequest.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTypedGenerationRequest.schema:required".into()); }

        if !["semantica.pydantic-json/1",].contains(&self.profile.as_str()) { failures.push("SemanticTypedGenerationRequest.profile:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit original schema and typed generation behavior, source-bound candidates and independent validation effects
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTypedGenerationResult {
    /// Original schema/typed execution field attempts
    pub attempts: Vec<SemanticTypedAttempt>,
    /// Original schema/typed execution field execution
    pub execution: String,
    /// Original schema/typed execution field failure
    pub failure: Option<SemanticProviderFailure>,
    /// Original schema/typed execution field model_id
    pub model_id: String,
    /// Original schema/typed execution field output
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub output: serde_json::Value,
    /// Original schema/typed execution field profile
    pub profile: String,
    /// Original schema/typed execution field schema
    pub schema: String,
}
impl SemanticTypedGenerationResult {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.execution.trim().is_empty() { failures.push("SemanticTypedGenerationResult.execution:required".into()); }
        if let Some(value) = &self.failure { if let Err(errors) = value.validate() { failures.extend(errors); } }

        if self.model_id.trim().is_empty() { failures.push("SemanticTypedGenerationResult.model_id:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTypedGenerationResult.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTypedGenerationResult.schema:required".into()); }

        if !["completed","failed","unknown",].contains(&self.execution.as_str()) { failures.push("SemanticTypedGenerationResult.execution:status".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit8 original typed relation consumer or source-bound host observation; JSON never grants authority
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTypedRelationRequest8 {
    /// Explicit8 entities; original finite typed relation semantics and source provenance remain separate from engineering truth
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub entities: serde_json::Value,
    /// Explicit8 generation; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub generation: SemanticProviderRequest,
    /// Explicit8 max_retries; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub max_retries: i64,
    /// Explicit8 relation_types; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub relation_types: Vec<String>,
    /// Explicit8 schema; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub schema: String,
    /// Explicit8 temporal; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub temporal: bool,
    /// Explicit8 text; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub text: String,
}
impl SemanticTypedRelationRequest8 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.schema.trim().is_empty() { failures.push("SemanticTypedRelationRequest8.schema:required".into()); }
        if self.text.trim().is_empty() { failures.push("SemanticTypedRelationRequest8.text:required".into()); }

        if !["semantica.typed-relation-request/8",].contains(&self.schema.as_str()) { failures.push("SemanticTypedRelationRequest8.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}

/// Explicit8 original typed relation consumer or source-bound host observation; JSON never grants authority
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTypedRelationResult8 {
    /// Explicit8 execution; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub execution: String,
    /// Explicit8 failure; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub failure: Option<SemanticProviderFailure>,
    /// Explicit8 fallback; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub fallback: Option<SemanticTypedDispatchResult>,
    /// Explicit8 profile; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub profile: String,
    /// Explicit8 relations; original finite typed relation semantics and source provenance remain separate from engineering truth
    #[serde(deserialize_with = "crate::json::deserialize_preserved")]
    pub relations: serde_json::Value,
    /// Explicit8 schema; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub schema: String,
    /// Explicit8 source_utf8_sha256; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub source_utf8_sha256: String,
    /// Explicit8 typed; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub typed: Option<SemanticTypedGenerationResult>,
    /// Explicit8 unknown; original finite typed relation semantics and source provenance remain separate from engineering truth
    pub unknown: Vec<String>,
}
impl SemanticTypedRelationResult8 {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        #[allow(unused_mut)]
        let mut failures = Vec::new();
        if self.execution.trim().is_empty() { failures.push("SemanticTypedRelationResult8.execution:required".into()); }
        if self.profile.trim().is_empty() { failures.push("SemanticTypedRelationResult8.profile:required".into()); }
        if self.schema.trim().is_empty() { failures.push("SemanticTypedRelationResult8.schema:required".into()); }
        if self.source_utf8_sha256.trim().is_empty() { failures.push("SemanticTypedRelationResult8.source_utf8_sha256:required".into()); }

        if !["completed","failed",].contains(&self.execution.as_str()) { failures.push("SemanticTypedRelationResult8.execution:allowed_values".into()); }
        if !["semantica.typed-relation-result/8",].contains(&self.schema.as_str()) { failures.push("SemanticTypedRelationResult8.schema:allowed_values".into()); }
        if failures.is_empty() { Ok(()) } else { Err(failures) }
    }
}


/// All model/field/default/alias semantics originate in editable specifications.
pub fn semantic_model_catalog_json() -> &'static str {
    "[{\"id\":\"integrations.crewai.decision_tool::SemanticaDecisionToolInput\",\"name\":\"SemanticaDecisionToolInput\",\"meaning\":\"Original SemanticaDecisionToolInput model from integrations/crewai/decision_tool.py; fields and methods retain explicit Pydantic profile\",\"source\":\"integrations/crewai/decision_tool.py\",\"source_sha256\":\"62ebeb4769503670627a3243fdfeb537588c4249349bf4045a3f08c040c5d8c1\",\"class_sha256\":\"e08a7b36fd732eed52116ba4d4600051ed6376e9bc2a7116f3e7458ad25f733d\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"description\":\"Input schema for ``SemanticaDecisionTool``.\\n\\nExactly one action is dispatched per call; the remaining fields are only\\nused by the actions that need them.\",\"properties\":{\"action\":{\"description\":\"Which decision-intelligence operation to run. One of: 'record_decision', 'find_precedents', 'trace_causal_chain', 'analyze_impact', 'check_policy'.\",\"enum\":[\"record_decision\",\"find_precedents\",\"trace_causal_chain\",\"analyze_impact\",\"check_policy\"],\"title\":\"Action\",\"type\":\"string\"},\"category\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"Domain category, e.g. 'loan_approval'. Used by 'record_decision'.\",\"title\":\"Category\"},\"scenario\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"Short description of the situation. Used by 'record_decision' and 'find_precedents'.\",\"title\":\"Scenario\"},\"reasoning\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"Why this outcome was chosen. Used by 'record_decision'.\",\"title\":\"Reasoning\"},\"outcome\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"The decision result. Used by 'record_decision'.\",\"title\":\"Outcome\"},\"confidence\":{\"default\":0.8,\"description\":\"Confidence score in [0, 1]. Used by 'record_decision'.\",\"maximum\":1.0,\"minimum\":0.0,\"title\":\"Confidence\",\"type\":\"number\"},\"entities\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"Comma-separated entity names. Used by 'record_decision'.\",\"title\":\"Entities\"},\"decision_id\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"Identifier of a decision. Used by 'trace_causal_chain' and 'analyze_impact'.\",\"title\":\"Decision Id\"},\"depth\":{\"default\":3,\"description\":\"Maximum chain depth. Used by 'trace_causal_chain'.\",\"maximum\":20,\"minimum\":1,\"title\":\"Depth\",\"type\":\"integer\"},\"decision_data\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"JSON object describing a proposed decision. Used by 'check_policy'.\",\"title\":\"Decision Data\"},\"policy_rules\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"JSON list of rule strings like 'confidence >= 0.7'. Used by 'check_policy'.\",\"title\":\"Policy Rules\"}},\"required\":[\"action\"],\"title\":\"SemanticaDecisionToolInput\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"action\",\"meaning\":\"Which decision-intelligence operation to run. One of: 'record_decision', 'find_precedents', 'trace_causal_chain', 'analyze_impact', 'check_policy'.\",\"annotation\":\"Literal[\\n        \\\"record_decision\\\",\\n        \\\"find_precedents\\\",\\n        \\\"trace_causal_chain\\\",\\n        \\\"analyze_impact\\\",\\n        \\\"check_policy\\\",\\n    ]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":66,\"validators\":[]},{\"name\":\"category\",\"meaning\":\"Domain category, e.g. 'loan_approval'. Used by 'record_decision'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":80,\"validators\":[]},{\"name\":\"scenario\",\"meaning\":\"Short description of the situation. Used by 'record_decision' and 'find_precedents'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":84,\"validators\":[]},{\"name\":\"reasoning\",\"meaning\":\"Why this outcome was chosen. Used by 'record_decision'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":91,\"validators\":[]},{\"name\":\"outcome\",\"meaning\":\"The decision result. Used by 'record_decision'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":94,\"validators\":[]},{\"name\":\"confidence\",\"meaning\":\"Confidence score in [0, 1]. Used by 'record_decision'.\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.8},\"source_line\":97,\"validators\":[]},{\"name\":\"entities\",\"meaning\":\"Comma-separated entity names. Used by 'record_decision'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":103,\"validators\":[]},{\"name\":\"decision_id\",\"meaning\":\"Identifier of a decision. Used by 'trace_causal_chain' and 'analyze_impact'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":107,\"validators\":[]},{\"name\":\"depth\",\"meaning\":\"Maximum chain depth. Used by 'trace_causal_chain'.\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":3},\"source_line\":114,\"validators\":[]},{\"name\":\"decision_data\",\"meaning\":\"JSON object describing a proposed decision. Used by 'check_policy'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":120,\"validators\":[]},{\"name\":\"policy_rules\",\"meaning\":\"JSON list of rule strings like 'confidence >= 0.7'. Used by 'check_policy'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":126,\"validators\":[]}]},{\"id\":\"integrations.crewai.kg_tool::SemanticaKGToolInput\",\"name\":\"SemanticaKGToolInput\",\"meaning\":\"Original SemanticaKGToolInput model from integrations/crewai/kg_tool.py; fields and methods retain explicit Pydantic profile\",\"source\":\"integrations/crewai/kg_tool.py\",\"source_sha256\":\"3ddca1c02e9243e81d843495502624e825376b35d1b40b6ea98a0a13b5347da3\",\"class_sha256\":\"a00eb2979f33464523f55a4c11efd7f97747bc231e33e18a5cf9bb3c9cdab22d\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"description\":\"Input schema for ``SemanticaKGTool``.\\n\\nExactly one action is dispatched per call; the remaining fields are only\\nused by the actions that need them.\",\"properties\":{\"action\":{\"description\":\"Which graph operation to run. One of: 'extract_entities', 'extract_relations', 'add_to_graph', 'query_graph', 'find_related'.\",\"enum\":[\"extract_entities\",\"extract_relations\",\"add_to_graph\",\"query_graph\",\"find_related\"],\"title\":\"Action\",\"type\":\"string\"},\"text\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"Input text. Used by 'extract_entities', 'extract_relations' and 'add_to_graph'.\",\"title\":\"Text\"},\"query\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"Search query. Used by 'query_graph'.\",\"title\":\"Query\"},\"entity\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"Root entity name. Used by 'find_related'.\",\"title\":\"Entity\"},\"hops\":{\"default\":1,\"description\":\"Maximum relationship hops. Used by 'find_related'.\",\"maximum\":10,\"minimum\":1,\"title\":\"Hops\",\"type\":\"integer\"}},\"required\":[\"action\"],\"title\":\"SemanticaKGToolInput\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"action\",\"meaning\":\"Which graph operation to run. One of: 'extract_entities', 'extract_relations', 'add_to_graph', 'query_graph', 'find_related'.\",\"annotation\":\"Literal[\\n        \\\"extract_entities\\\",\\n        \\\"extract_relations\\\",\\n        \\\"add_to_graph\\\",\\n        \\\"query_graph\\\",\\n        \\\"find_related\\\",\\n    ]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":78,\"validators\":[]},{\"name\":\"text\",\"meaning\":\"Input text. Used by 'extract_entities', 'extract_relations' and 'add_to_graph'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":91,\"validators\":[]},{\"name\":\"query\",\"meaning\":\"Search query. Used by 'query_graph'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":98,\"validators\":[]},{\"name\":\"entity\",\"meaning\":\"Root entity name. Used by 'find_related'.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":101,\"validators\":[]},{\"name\":\"hops\",\"meaning\":\"Maximum relationship hops. Used by 'find_related'.\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":1},\"source_line\":105,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::OntologyEntry\",\"name\":\"OntologyEntry\",\"meaning\":\"Original OntologyEntry model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"aca87ba6fc316ac641f0c921842407c6d8dd958262541b42bf12f49556cc4637\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"name\":{\"title\":\"Name\",\"type\":\"string\"},\"description\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Description\"},\"format\":{\"default\":\"unknown\",\"title\":\"Format\",\"type\":\"string\"},\"status\":{\"default\":\"external\",\"enum\":[\"published\",\"draft\",\"external\"],\"title\":\"Status\",\"type\":\"string\"},\"source_url\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Url\"},\"version\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Version\"},\"class_count\":{\"default\":0,\"title\":\"Class Count\",\"type\":\"integer\"},\"concept_count\":{\"default\":0,\"title\":\"Concept Count\",\"type\":\"integer\"},\"property_count\":{\"default\":0,\"title\":\"Property Count\",\"type\":\"integer\"},\"loaded_at\":{\"default\":\"\",\"title\":\"Loaded At\",\"type\":\"string\"},\"enabled\":{\"default\":true,\"title\":\"Enabled\",\"type\":\"boolean\"},\"tags\":{\"items\":{\"type\":\"string\"},\"title\":\"Tags\",\"type\":\"array\"}},\"required\":[\"uri\",\"name\"],\"title\":\"OntologyEntry\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in OntologyEntry\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":158,\"validators\":[]},{\"name\":\"name\",\"meaning\":\"Original name in OntologyEntry\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":159,\"validators\":[]},{\"name\":\"description\",\"meaning\":\"Original description in OntologyEntry\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":160,\"validators\":[]},{\"name\":\"format\",\"meaning\":\"Original format in OntologyEntry\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"unknown\"},\"source_line\":161,\"validators\":[]},{\"name\":\"status\",\"meaning\":\"Original status in OntologyEntry\",\"annotation\":\"Literal[\\\"published\\\", \\\"draft\\\", \\\"external\\\"]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"external\"},\"source_line\":162,\"validators\":[]},{\"name\":\"source_url\",\"meaning\":\"Original source_url in OntologyEntry\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":163,\"validators\":[]},{\"name\":\"version\",\"meaning\":\"Original version in OntologyEntry\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":164,\"validators\":[]},{\"name\":\"class_count\",\"meaning\":\"Original class_count in OntologyEntry\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":165,\"validators\":[]},{\"name\":\"concept_count\",\"meaning\":\"Original concept_count in OntologyEntry\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":166,\"validators\":[]},{\"name\":\"property_count\",\"meaning\":\"Original property_count in OntologyEntry\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":167,\"validators\":[]},{\"name\":\"loaded_at\",\"meaning\":\"Original loaded_at in OntologyEntry\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":168,\"validators\":[]},{\"name\":\"enabled\",\"meaning\":\"Original enabled in OntologyEntry\",\"annotation\":\"bool\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":true},\"source_line\":169,\"validators\":[]},{\"name\":\"tags\",\"meaning\":\"Original tags in OntologyEntry\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":170,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::OntologyPreview\",\"name\":\"OntologyPreview\",\"meaning\":\"Original OntologyPreview model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"376052f3e8773b3a97773f4fb81345e72e222d2ebf9f9f922bec21b6b6567e70\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"name\":{\"title\":\"Name\",\"type\":\"string\"},\"description\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Description\"},\"namespace\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Namespace\"},\"version\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Version\"},\"license\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"License\"},\"format\":{\"title\":\"Format\",\"type\":\"string\"},\"estimated_triples\":{\"default\":0,\"title\":\"Estimated Triples\",\"type\":\"integer\"},\"source_url\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Url\"}},\"required\":[\"uri\",\"name\",\"format\"],\"title\":\"OntologyPreview\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in OntologyPreview\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":174,\"validators\":[]},{\"name\":\"name\",\"meaning\":\"Original name in OntologyPreview\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":175,\"validators\":[]},{\"name\":\"description\",\"meaning\":\"Original description in OntologyPreview\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":176,\"validators\":[]},{\"name\":\"namespace\",\"meaning\":\"Original namespace in OntologyPreview\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":177,\"validators\":[]},{\"name\":\"version\",\"meaning\":\"Original version in OntologyPreview\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":178,\"validators\":[]},{\"name\":\"license\",\"meaning\":\"Original license in OntologyPreview\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":179,\"validators\":[]},{\"name\":\"format\",\"meaning\":\"Original format in OntologyPreview\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":180,\"validators\":[]},{\"name\":\"estimated_triples\",\"meaning\":\"Original estimated_triples in OntologyPreview\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":181,\"validators\":[]},{\"name\":\"source_url\",\"meaning\":\"Original source_url in OntologyPreview\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":182,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::LoadOntologyRequest\",\"name\":\"LoadOntologyRequest\",\"meaning\":\"Original LoadOntologyRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"d831cb63bba224401d4e64c4c95362702ab1c571c0cdd572ddf435abeb0b0009\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"url\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Url\"},\"content\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Content\"},\"format\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Format\"},\"name\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Name\"},\"description\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Description\"},\"tags\":{\"items\":{\"type\":\"string\"},\"title\":\"Tags\",\"type\":\"array\"}},\"title\":\"LoadOntologyRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"url\",\"meaning\":\"Original url in LoadOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":186,\"validators\":[]},{\"name\":\"content\",\"meaning\":\"Original content in LoadOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":187,\"validators\":[]},{\"name\":\"format\",\"meaning\":\"Original format in LoadOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":188,\"validators\":[]},{\"name\":\"name\",\"meaning\":\"Original name in LoadOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":189,\"validators\":[]},{\"name\":\"description\",\"meaning\":\"Original description in LoadOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":190,\"validators\":[]},{\"name\":\"tags\",\"meaning\":\"Original tags in LoadOntologyRequest\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":191,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::PreviewOntologyRequest\",\"name\":\"PreviewOntologyRequest\",\"meaning\":\"Original PreviewOntologyRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"21bfc57f7cccd5bccbfd07fc349f3c305c42585b877adf46dcad3c2273242453\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"url\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Url\"},\"content\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Content\"},\"format\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Format\"}},\"title\":\"PreviewOntologyRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"url\",\"meaning\":\"Original url in PreviewOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":195,\"validators\":[]},{\"name\":\"content\",\"meaning\":\"Original content in PreviewOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":196,\"validators\":[]},{\"name\":\"format\",\"meaning\":\"Original format in PreviewOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":197,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::CreateOntologyRequest\",\"name\":\"CreateOntologyRequest\",\"meaning\":\"Original CreateOntologyRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"8f6afe37e672edf26772a48567ef282a2862cd5cfa82b0454602d56d5f836cb5\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"mode\":{\"default\":\"scratch\",\"enum\":[\"scratch\",\"data\",\"text\"],\"title\":\"Mode\",\"type\":\"string\"},\"namespace\":{\"title\":\"Namespace\",\"type\":\"string\"},\"name\":{\"title\":\"Name\",\"type\":\"string\"},\"description\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Description\"},\"tags\":{\"items\":{\"type\":\"string\"},\"title\":\"Tags\",\"type\":\"array\"},\"sample_data\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Sample Data\"},\"schema_text\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Schema Text\"},\"provider\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Provider\"},\"model\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Model\"}},\"required\":[\"namespace\",\"name\"],\"title\":\"CreateOntologyRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"mode\",\"meaning\":\"Original mode in CreateOntologyRequest\",\"annotation\":\"Literal[\\\"scratch\\\", \\\"data\\\", \\\"text\\\"]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"scratch\"},\"source_line\":201,\"validators\":[]},{\"name\":\"namespace\",\"meaning\":\"Original namespace in CreateOntologyRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":202,\"validators\":[]},{\"name\":\"name\",\"meaning\":\"Original name in CreateOntologyRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":203,\"validators\":[]},{\"name\":\"description\",\"meaning\":\"Original description in CreateOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":204,\"validators\":[]},{\"name\":\"tags\",\"meaning\":\"Original tags in CreateOntologyRequest\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":205,\"validators\":[]},{\"name\":\"sample_data\",\"meaning\":\"Original sample_data in CreateOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":206,\"validators\":[]},{\"name\":\"schema_text\",\"meaning\":\"Original schema_text in CreateOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":207,\"validators\":[]},{\"name\":\"provider\",\"meaning\":\"Original provider in CreateOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":208,\"validators\":[]},{\"name\":\"model\",\"meaning\":\"Original model in CreateOntologyRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":209,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::OntologySearchResult\",\"name\":\"OntologySearchResult\",\"meaning\":\"Original OntologySearchResult model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"c10e5f2281fb5f6c65bccb838435fc4e441de48bccfe9904431d2e06026f3e20\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"label\":{\"title\":\"Label\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"entity_type\":{\"title\":\"Entity Type\",\"type\":\"string\"},\"definition\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Definition\"},\"source_ontology\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Ontology\"},\"namespace_prefix\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Namespace Prefix\"}},\"required\":[\"uri\",\"label\",\"type\",\"entity_type\"],\"title\":\"OntologySearchResult\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in OntologySearchResult\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":213,\"validators\":[]},{\"name\":\"label\",\"meaning\":\"Original label in OntologySearchResult\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":214,\"validators\":[]},{\"name\":\"type\",\"meaning\":\"Original type in OntologySearchResult\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":215,\"validators\":[]},{\"name\":\"entity_type\",\"meaning\":\"Original entity_type in OntologySearchResult\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":216,\"validators\":[]},{\"name\":\"definition\",\"meaning\":\"Original definition in OntologySearchResult\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":217,\"validators\":[]},{\"name\":\"source_ontology\",\"meaning\":\"Original source_ontology in OntologySearchResult\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":218,\"validators\":[]},{\"name\":\"namespace_prefix\",\"meaning\":\"Original namespace_prefix in OntologySearchResult\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":219,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::EntityDetailResponse\",\"name\":\"EntityDetailResponse\",\"meaning\":\"Original EntityDetailResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"6aeeccbe0791654364195c02207f614062fa396bc6ae0267a10e76be6609d233\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"label\":{\"title\":\"Label\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"entity_type\":{\"title\":\"Entity Type\",\"type\":\"string\"},\"definition\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Definition\"},\"source_ontology\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Ontology\"},\"superclasses\":{\"items\":{\"type\":\"string\"},\"title\":\"Superclasses\",\"type\":\"array\"},\"subclasses\":{\"items\":{\"type\":\"string\"},\"title\":\"Subclasses\",\"type\":\"array\"},\"domain\":{\"items\":{\"type\":\"string\"},\"title\":\"Domain\",\"type\":\"array\"},\"range\":{\"items\":{\"type\":\"string\"},\"title\":\"Range\",\"type\":\"array\"},\"instance_count\":{\"default\":0,\"title\":\"Instance Count\",\"type\":\"integer\"},\"properties\":{\"type\":\"object\",\"additionalProperties\":true,\"title\":\"Properties\"}},\"required\":[\"uri\",\"label\",\"type\",\"entity_type\"],\"title\":\"EntityDetailResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in EntityDetailResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":223,\"validators\":[]},{\"name\":\"label\",\"meaning\":\"Original label in EntityDetailResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":224,\"validators\":[]},{\"name\":\"type\",\"meaning\":\"Original type in EntityDetailResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":225,\"validators\":[]},{\"name\":\"entity_type\",\"meaning\":\"Original entity_type in EntityDetailResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":226,\"validators\":[]},{\"name\":\"definition\",\"meaning\":\"Original definition in EntityDetailResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":227,\"validators\":[]},{\"name\":\"source_ontology\",\"meaning\":\"Original source_ontology in EntityDetailResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":228,\"validators\":[]},{\"name\":\"superclasses\",\"meaning\":\"Original superclasses in EntityDetailResponse\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":229,\"validators\":[]},{\"name\":\"subclasses\",\"meaning\":\"Original subclasses in EntityDetailResponse\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":230,\"validators\":[]},{\"name\":\"domain\",\"meaning\":\"Original domain in EntityDetailResponse\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":231,\"validators\":[]},{\"name\":\"range\",\"meaning\":\"Original range in EntityDetailResponse\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":232,\"validators\":[]},{\"name\":\"instance_count\",\"meaning\":\"Original instance_count in EntityDetailResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":233,\"validators\":[]},{\"name\":\"properties\",\"meaning\":\"Original properties in EntityDetailResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":234,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::SKOSScheme\",\"name\":\"SKOSScheme\",\"meaning\":\"Original SKOSScheme model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"fbb9a87c7e57f12db755e98fa1f03ba8c52875b04b042ec774932d4463577fae\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"title\":{\"title\":\"Title\",\"type\":\"string\"},\"description\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Description\"},\"concept_count\":{\"default\":0,\"title\":\"Concept Count\",\"type\":\"integer\"}},\"required\":[\"uri\",\"title\"],\"title\":\"SKOSScheme\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in SKOSScheme\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":238,\"validators\":[]},{\"name\":\"title\",\"meaning\":\"Original title in SKOSScheme\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":239,\"validators\":[]},{\"name\":\"description\",\"meaning\":\"Original description in SKOSScheme\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":240,\"validators\":[]},{\"name\":\"concept_count\",\"meaning\":\"Original concept_count in SKOSScheme\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":241,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::SKOSConceptDetail\",\"name\":\"SKOSConceptDetail\",\"meaning\":\"Original SKOSConceptDetail model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"491eb90a7c33740526e2f8f4597c33c3226324e97b04580be08582721fea90fd\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"pref_label\":{\"title\":\"Pref Label\",\"type\":\"string\"},\"alt_labels\":{\"items\":{\"type\":\"string\"},\"title\":\"Alt Labels\",\"type\":\"array\"},\"hidden_labels\":{\"items\":{\"type\":\"string\"},\"title\":\"Hidden Labels\",\"type\":\"array\"},\"definition\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Definition\"},\"scope_note\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Scope Note\"},\"editorial_note\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Editorial Note\"},\"broader\":{\"items\":{\"type\":\"string\"},\"title\":\"Broader\",\"type\":\"array\"},\"narrower\":{\"items\":{\"type\":\"string\"},\"title\":\"Narrower\",\"type\":\"array\"},\"related\":{\"items\":{\"type\":\"string\"},\"title\":\"Related\",\"type\":\"array\"},\"exact_match\":{\"items\":{\"type\":\"string\"},\"title\":\"Exact Match\",\"type\":\"array\"},\"close_match\":{\"items\":{\"type\":\"string\"},\"title\":\"Close Match\",\"type\":\"array\"},\"broad_match\":{\"items\":{\"type\":\"string\"},\"title\":\"Broad Match\",\"type\":\"array\"},\"narrow_match\":{\"items\":{\"type\":\"string\"},\"title\":\"Narrow Match\",\"type\":\"array\"},\"scheme_uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Scheme Uri\"}},\"required\":[\"uri\",\"pref_label\"],\"title\":\"SKOSConceptDetail\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in SKOSConceptDetail\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":245,\"validators\":[]},{\"name\":\"pref_label\",\"meaning\":\"Original pref_label in SKOSConceptDetail\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":246,\"validators\":[]},{\"name\":\"alt_labels\",\"meaning\":\"Original alt_labels in SKOSConceptDetail\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":247,\"validators\":[]},{\"name\":\"hidden_labels\",\"meaning\":\"Original hidden_labels in SKOSConceptDetail\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":248,\"validators\":[]},{\"name\":\"definition\",\"meaning\":\"Original definition in SKOSConceptDetail\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":249,\"validators\":[]},{\"name\":\"scope_note\",\"meaning\":\"Original scope_note in SKOSConceptDetail\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":250,\"validators\":[]},{\"name\":\"editorial_note\",\"meaning\":\"Original editorial_note in SKOSConceptDetail\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":251,\"validators\":[]},{\"name\":\"broader\",\"meaning\":\"Original broader in SKOSConceptDetail\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":252,\"validators\":[]},{\"name\":\"narrower\",\"meaning\":\"Original narrower in SKOSConceptDetail\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":253,\"validators\":[]},{\"name\":\"related\",\"meaning\":\"Original related in SKOSConceptDetail\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":254,\"validators\":[]},{\"name\":\"exact_match\",\"meaning\":\"Original exact_match in SKOSConceptDetail\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":255,\"validators\":[]},{\"name\":\"close_match\",\"meaning\":\"Original close_match in SKOSConceptDetail\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":256,\"validators\":[]},{\"name\":\"broad_match\",\"meaning\":\"Original broad_match in SKOSConceptDetail\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":257,\"validators\":[]},{\"name\":\"narrow_match\",\"meaning\":\"Original narrow_match in SKOSConceptDetail\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":258,\"validators\":[]},{\"name\":\"scheme_uri\",\"meaning\":\"Original scheme_uri in SKOSConceptDetail\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":259,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::SKOSConceptSearchRequest\",\"name\":\"SKOSConceptSearchRequest\",\"meaning\":\"Original SKOSConceptSearchRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"7d85a427ae1ba7b9e70c8cf4632405bed4f2bcb79ce278ffde5bf68a07e99e5a\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"query\":{\"title\":\"Query\",\"type\":\"string\"},\"scheme_uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Scheme Uri\"}},\"required\":[\"query\"],\"title\":\"SKOSConceptSearchRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"query\",\"meaning\":\"Original query in SKOSConceptSearchRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":263,\"validators\":[]},{\"name\":\"scheme_uri\",\"meaning\":\"Original scheme_uri in SKOSConceptSearchRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":264,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::LoadOntologyResponse\",\"name\":\"LoadOntologyResponse\",\"meaning\":\"Original LoadOntologyResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"6ce99747fdb81c3a9a949961142e610e5618f14799ba50c29ec09726219954dd\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"status\":{\"default\":\"success\",\"title\":\"Status\",\"type\":\"string\"},\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"name\":{\"title\":\"Name\",\"type\":\"string\"},\"nodes_added\":{\"default\":0,\"title\":\"Nodes Added\",\"type\":\"integer\"},\"edges_added\":{\"default\":0,\"title\":\"Edges Added\",\"type\":\"integer\"},\"format\":{\"default\":\"unknown\",\"title\":\"Format\",\"type\":\"string\"}},\"required\":[\"uri\",\"name\"],\"title\":\"LoadOntologyResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"status\",\"meaning\":\"Original status in LoadOntologyResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"success\"},\"source_line\":268,\"validators\":[]},{\"name\":\"uri\",\"meaning\":\"Original uri in LoadOntologyResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":269,\"validators\":[]},{\"name\":\"name\",\"meaning\":\"Original name in LoadOntologyResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":270,\"validators\":[]},{\"name\":\"nodes_added\",\"meaning\":\"Original nodes_added in LoadOntologyResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":271,\"validators\":[]},{\"name\":\"edges_added\",\"meaning\":\"Original edges_added in LoadOntologyResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":272,\"validators\":[]},{\"name\":\"format\",\"meaning\":\"Original format in LoadOntologyResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"unknown\"},\"source_line\":273,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ToggleResponse\",\"name\":\"ToggleResponse\",\"meaning\":\"Original ToggleResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"8007e7cdecf10e4aea79815a96a080da4f6a1a6ffa0e32ba769062325e81c27d\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"enabled\":{\"title\":\"Enabled\",\"type\":\"boolean\"}},\"required\":[\"uri\",\"enabled\"],\"title\":\"ToggleResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in ToggleResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":277,\"validators\":[]},{\"name\":\"enabled\",\"meaning\":\"Original enabled in ToggleResponse\",\"annotation\":\"bool\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":278,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::RefreshResponse\",\"name\":\"RefreshResponse\",\"meaning\":\"Original RefreshResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"792a047541091779f8f4d641653d84853e488e9dc1eb04c3417901d93016c916\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"status\":{\"default\":\"success\",\"title\":\"Status\",\"type\":\"string\"},\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"nodes_added\":{\"default\":0,\"title\":\"Nodes Added\",\"type\":\"integer\"},\"edges_added\":{\"default\":0,\"title\":\"Edges Added\",\"type\":\"integer\"}},\"required\":[\"uri\"],\"title\":\"RefreshResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"status\",\"meaning\":\"Original status in RefreshResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"success\"},\"source_line\":282,\"validators\":[]},{\"name\":\"uri\",\"meaning\":\"Original uri in RefreshResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":283,\"validators\":[]},{\"name\":\"nodes_added\",\"meaning\":\"Original nodes_added in RefreshResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":284,\"validators\":[]},{\"name\":\"edges_added\",\"meaning\":\"Original edges_added in RefreshResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":285,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::OntologyAlignment\",\"name\":\"OntologyAlignment\",\"meaning\":\"Original OntologyAlignment model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"84ed335f3e647b306f77d6bd4f50ed7b46c78c9e9c53a7418526b9f481debd1b\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"source_uri\":{\"title\":\"Source Uri\",\"type\":\"string\"},\"source_label\":{\"default\":\"\",\"title\":\"Source Label\",\"type\":\"string\"},\"target_uri\":{\"title\":\"Target Uri\",\"type\":\"string\"},\"target_label\":{\"default\":\"\",\"title\":\"Target Label\",\"type\":\"string\"},\"relation\":{\"enum\":[\"owl:equivalentClass\",\"owl:equivalentProperty\",\"skos:exactMatch\",\"skos:closeMatch\",\"skos:broadMatch\",\"skos:narrowMatch\",\"skos:relatedMatch\"],\"title\":\"Relation\",\"type\":\"string\"},\"predicate_uri\":{\"title\":\"Predicate Uri\",\"type\":\"string\"},\"confidence\":{\"default\":1.0,\"maximum\":1.0,\"minimum\":0.0,\"title\":\"Confidence\",\"type\":\"number\"},\"provenance\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Provenance\"},\"source\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source\"},\"reviewer\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Reviewer\"},\"created_at\":{\"title\":\"Created At\",\"type\":\"string\"},\"updated_at\":{\"title\":\"Updated At\",\"type\":\"string\"}},\"required\":[\"id\",\"source_uri\",\"target_uri\",\"relation\",\"predicate_uri\",\"created_at\",\"updated_at\"],\"title\":\"OntologyAlignment\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"id\",\"meaning\":\"Original id in OntologyAlignment\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":300,\"validators\":[]},{\"name\":\"source_uri\",\"meaning\":\"Original source_uri in OntologyAlignment\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":301,\"validators\":[]},{\"name\":\"source_label\",\"meaning\":\"Original source_label in OntologyAlignment\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":302,\"validators\":[]},{\"name\":\"target_uri\",\"meaning\":\"Original target_uri in OntologyAlignment\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":303,\"validators\":[]},{\"name\":\"target_label\",\"meaning\":\"Original target_label in OntologyAlignment\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":304,\"validators\":[]},{\"name\":\"relation\",\"meaning\":\"Original relation in OntologyAlignment\",\"annotation\":\"AlignmentRelation\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":305,\"validators\":[]},{\"name\":\"predicate_uri\",\"meaning\":\"Original predicate_uri in OntologyAlignment\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":306,\"validators\":[]},{\"name\":\"confidence\",\"meaning\":\"Original confidence in OntologyAlignment\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":1.0},\"source_line\":307,\"validators\":[]},{\"name\":\"provenance\",\"meaning\":\"Original provenance in OntologyAlignment\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":308,\"validators\":[]},{\"name\":\"source\",\"meaning\":\"Original source in OntologyAlignment\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":309,\"validators\":[]},{\"name\":\"reviewer\",\"meaning\":\"Original reviewer in OntologyAlignment\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":310,\"validators\":[]},{\"name\":\"created_at\",\"meaning\":\"Original created_at in OntologyAlignment\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":311,\"validators\":[]},{\"name\":\"updated_at\",\"meaning\":\"Original updated_at in OntologyAlignment\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":312,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::OntologyAlignmentRequest\",\"name\":\"OntologyAlignmentRequest\",\"meaning\":\"Original OntologyAlignmentRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"204b2df3de1648accf1a269877b8cabdd5aee07165b4b75afc1713b9402cf780\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"source_uri\":{\"title\":\"Source Uri\",\"type\":\"string\"},\"source_label\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Label\"},\"target_uri\":{\"title\":\"Target Uri\",\"type\":\"string\"},\"target_label\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Target Label\"},\"relation\":{\"enum\":[\"owl:equivalentClass\",\"owl:equivalentProperty\",\"skos:exactMatch\",\"skos:closeMatch\",\"skos:broadMatch\",\"skos:narrowMatch\",\"skos:relatedMatch\"],\"title\":\"Relation\",\"type\":\"string\"},\"confidence\":{\"default\":1.0,\"maximum\":1.0,\"minimum\":0.0,\"title\":\"Confidence\",\"type\":\"number\"},\"provenance\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Provenance\"},\"source\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source\"},\"reviewer\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Reviewer\"}},\"required\":[\"source_uri\",\"target_uri\",\"relation\"],\"title\":\"OntologyAlignmentRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"source_uri\",\"meaning\":\"Original source_uri in OntologyAlignmentRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":316,\"validators\":[]},{\"name\":\"source_label\",\"meaning\":\"Original source_label in OntologyAlignmentRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":317,\"validators\":[]},{\"name\":\"target_uri\",\"meaning\":\"Original target_uri in OntologyAlignmentRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":318,\"validators\":[]},{\"name\":\"target_label\",\"meaning\":\"Original target_label in OntologyAlignmentRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":319,\"validators\":[]},{\"name\":\"relation\",\"meaning\":\"Original relation in OntologyAlignmentRequest\",\"annotation\":\"AlignmentRelation\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":320,\"validators\":[]},{\"name\":\"confidence\",\"meaning\":\"Original confidence in OntologyAlignmentRequest\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":1.0},\"source_line\":321,\"validators\":[]},{\"name\":\"provenance\",\"meaning\":\"Original provenance in OntologyAlignmentRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":322,\"validators\":[]},{\"name\":\"source\",\"meaning\":\"Original source in OntologyAlignmentRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":323,\"validators\":[]},{\"name\":\"reviewer\",\"meaning\":\"Original reviewer in OntologyAlignmentRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":324,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::AlignmentSuggestionRequest\",\"name\":\"AlignmentSuggestionRequest\",\"meaning\":\"Original AlignmentSuggestionRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"144b997ec7351b0fd50c65622e3ed16dc0f1ba53c3f4f539209304560c5eeeed\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"source_ontology_uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Ontology Uri\"},\"target_ontology_uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Target Ontology Uri\"},\"threshold\":{\"default\":0.65,\"maximum\":1.0,\"minimum\":0.0,\"title\":\"Threshold\",\"type\":\"number\"},\"limit\":{\"default\":25,\"maximum\":100,\"minimum\":1,\"title\":\"Limit\",\"type\":\"integer\"}},\"title\":\"AlignmentSuggestionRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"source_ontology_uri\",\"meaning\":\"Original source_ontology_uri in AlignmentSuggestionRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":328,\"validators\":[]},{\"name\":\"target_ontology_uri\",\"meaning\":\"Original target_ontology_uri in AlignmentSuggestionRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":329,\"validators\":[]},{\"name\":\"threshold\",\"meaning\":\"Original threshold in AlignmentSuggestionRequest\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.65},\"source_line\":330,\"validators\":[]},{\"name\":\"limit\",\"meaning\":\"Original limit in AlignmentSuggestionRequest\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":25},\"source_line\":331,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::AlignmentSuggestion\",\"name\":\"AlignmentSuggestion\",\"meaning\":\"Original AlignmentSuggestion model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"9c59a70045803570e95f0b659f572f3917e6fcb26ee37a8e9d22798fa53375a1\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"source_uri\":{\"title\":\"Source Uri\",\"type\":\"string\"},\"source_label\":{\"title\":\"Source Label\",\"type\":\"string\"},\"target_uri\":{\"title\":\"Target Uri\",\"type\":\"string\"},\"target_label\":{\"title\":\"Target Label\",\"type\":\"string\"},\"relation\":{\"enum\":[\"owl:equivalentClass\",\"owl:equivalentProperty\",\"skos:exactMatch\",\"skos:closeMatch\",\"skos:broadMatch\",\"skos:narrowMatch\",\"skos:relatedMatch\"],\"title\":\"Relation\",\"type\":\"string\"},\"score\":{\"title\":\"Score\",\"type\":\"number\"},\"label_similarity\":{\"title\":\"Label Similarity\",\"type\":\"number\"},\"embedding_similarity\":{\"anyOf\":[{\"type\":\"number\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Embedding Similarity\"},\"reason\":{\"title\":\"Reason\",\"type\":\"string\"}},\"required\":[\"source_uri\",\"source_label\",\"target_uri\",\"target_label\",\"relation\",\"score\",\"label_similarity\",\"reason\"],\"title\":\"AlignmentSuggestion\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"source_uri\",\"meaning\":\"Original source_uri in AlignmentSuggestion\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":335,\"validators\":[]},{\"name\":\"source_label\",\"meaning\":\"Original source_label in AlignmentSuggestion\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":336,\"validators\":[]},{\"name\":\"target_uri\",\"meaning\":\"Original target_uri in AlignmentSuggestion\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":337,\"validators\":[]},{\"name\":\"target_label\",\"meaning\":\"Original target_label in AlignmentSuggestion\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":338,\"validators\":[]},{\"name\":\"relation\",\"meaning\":\"Original relation in AlignmentSuggestion\",\"annotation\":\"AlignmentRelation\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":339,\"validators\":[]},{\"name\":\"score\",\"meaning\":\"Original score in AlignmentSuggestion\",\"annotation\":\"float\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":340,\"validators\":[]},{\"name\":\"label_similarity\",\"meaning\":\"Original label_similarity in AlignmentSuggestion\",\"annotation\":\"float\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":341,\"validators\":[]},{\"name\":\"embedding_similarity\",\"meaning\":\"Original embedding_similarity in AlignmentSuggestion\",\"annotation\":\"Optional[float]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":342,\"validators\":[]},{\"name\":\"reason\",\"meaning\":\"Original reason in AlignmentSuggestion\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":343,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::HealthDimension\",\"name\":\"HealthDimension\",\"meaning\":\"Original HealthDimension model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"7f61e1f434386932df9f2bb6b0e1261cbd9f090355e440dea417750eef00a77e\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"key\":{\"title\":\"Key\",\"type\":\"string\"},\"label\":{\"title\":\"Label\",\"type\":\"string\"},\"score\":{\"maximum\":100.0,\"minimum\":0.0,\"title\":\"Score\",\"type\":\"number\"},\"status\":{\"default\":\"ok\",\"enum\":[\"ok\",\"warning\",\"critical\",\"unavailable\"],\"title\":\"Status\",\"type\":\"string\"},\"detail\":{\"title\":\"Detail\",\"type\":\"string\"}},\"required\":[\"key\",\"label\",\"score\",\"detail\"],\"title\":\"HealthDimension\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"key\",\"meaning\":\"Original key in HealthDimension\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":347,\"validators\":[]},{\"name\":\"label\",\"meaning\":\"Original label in HealthDimension\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":348,\"validators\":[]},{\"name\":\"score\",\"meaning\":\"Original score in HealthDimension\",\"annotation\":\"float\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":349,\"validators\":[]},{\"name\":\"status\",\"meaning\":\"Original status in HealthDimension\",\"annotation\":\"Literal[\\\"ok\\\", \\\"warning\\\", \\\"critical\\\", \\\"unavailable\\\"]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"ok\"},\"source_line\":350,\"validators\":[]},{\"name\":\"detail\",\"meaning\":\"Original detail in HealthDimension\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":351,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::HealthIssue\",\"name\":\"HealthIssue\",\"meaning\":\"Original HealthIssue model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"d6d49138cf18e23acccdc5b65c3bd9e002142172508f26ae313dd8f31c987771\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"severity\":{\"default\":\"warning\",\"enum\":[\"info\",\"warning\",\"critical\"],\"title\":\"Severity\",\"type\":\"string\"},\"category\":{\"title\":\"Category\",\"type\":\"string\"},\"entity_uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Entity Uri\"},\"entity_label\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Entity Label\"},\"message\":{\"title\":\"Message\",\"type\":\"string\"},\"action\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Action\"}},\"required\":[\"id\",\"category\",\"message\"],\"title\":\"HealthIssue\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"id\",\"meaning\":\"Original id in HealthIssue\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":355,\"validators\":[]},{\"name\":\"severity\",\"meaning\":\"Original severity in HealthIssue\",\"annotation\":\"Literal[\\\"info\\\", \\\"warning\\\", \\\"critical\\\"]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"warning\"},\"source_line\":356,\"validators\":[]},{\"name\":\"category\",\"meaning\":\"Original category in HealthIssue\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":357,\"validators\":[]},{\"name\":\"entity_uri\",\"meaning\":\"Original entity_uri in HealthIssue\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":358,\"validators\":[]},{\"name\":\"entity_label\",\"meaning\":\"Original entity_label in HealthIssue\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":359,\"validators\":[]},{\"name\":\"message\",\"meaning\":\"Original message in HealthIssue\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":360,\"validators\":[]},{\"name\":\"action\",\"meaning\":\"Original action in HealthIssue\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":361,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::OntologyHealthResponse\",\"name\":\"OntologyHealthResponse\",\"meaning\":\"Original OntologyHealthResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"51e14b8e5c172f45ae8554915609d21868194b21b338078a031d33b8978101c5\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"HealthDimension\":{\"properties\":{\"key\":{\"title\":\"Key\",\"type\":\"string\"},\"label\":{\"title\":\"Label\",\"type\":\"string\"},\"score\":{\"maximum\":100.0,\"minimum\":0.0,\"title\":\"Score\",\"type\":\"number\"},\"status\":{\"default\":\"ok\",\"enum\":[\"ok\",\"warning\",\"critical\",\"unavailable\"],\"title\":\"Status\",\"type\":\"string\"},\"detail\":{\"title\":\"Detail\",\"type\":\"string\"}},\"required\":[\"key\",\"label\",\"score\",\"detail\"],\"title\":\"HealthDimension\",\"type\":\"object\"},\"HealthIssue\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"severity\":{\"default\":\"warning\",\"enum\":[\"info\",\"warning\",\"critical\"],\"title\":\"Severity\",\"type\":\"string\"},\"category\":{\"title\":\"Category\",\"type\":\"string\"},\"entity_uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Entity Uri\"},\"entity_label\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Entity Label\"},\"message\":{\"title\":\"Message\",\"type\":\"string\"},\"action\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Action\"}},\"required\":[\"id\",\"category\",\"message\"],\"title\":\"HealthIssue\",\"type\":\"object\"}},\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"name\":{\"title\":\"Name\",\"type\":\"string\"},\"total_score\":{\"maximum\":100.0,\"minimum\":0.0,\"title\":\"Total Score\",\"type\":\"number\"},\"dimensions\":{\"items\":{\"$ref\":\"#/$defs/HealthDimension\"},\"title\":\"Dimensions\",\"type\":\"array\"},\"issues\":{\"items\":{\"$ref\":\"#/$defs/HealthIssue\"},\"title\":\"Issues\",\"type\":\"array\"},\"generated_at\":{\"title\":\"Generated At\",\"type\":\"string\"}},\"required\":[\"uri\",\"name\",\"total_score\",\"dimensions\",\"generated_at\"],\"title\":\"OntologyHealthResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in OntologyHealthResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":365,\"validators\":[]},{\"name\":\"name\",\"meaning\":\"Original name in OntologyHealthResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":366,\"validators\":[]},{\"name\":\"total_score\",\"meaning\":\"Original total_score in OntologyHealthResponse\",\"annotation\":\"float\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":367,\"validators\":[]},{\"name\":\"dimensions\",\"meaning\":\"Original dimensions in OntologyHealthResponse\",\"annotation\":\"List[HealthDimension]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":368,\"validators\":[]},{\"name\":\"issues\",\"meaning\":\"Original issues in OntologyHealthResponse\",\"annotation\":\"List[HealthIssue]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":369,\"validators\":[]},{\"name\":\"generated_at\",\"meaning\":\"Original generated_at in OntologyHealthResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":370,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ShaclGenerateRequest\",\"name\":\"ShaclGenerateRequest\",\"meaning\":\"Original ShaclGenerateRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"be0bd2e8d31bf9beeda4aa102039f2cbad0f3c46abebc4ae56365bfa1318ea3b\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"quality_tier\":{\"default\":\"strict\",\"enum\":[\"standard\",\"strict\"],\"title\":\"Quality Tier\",\"type\":\"string\"}},\"required\":[\"uri\"],\"title\":\"ShaclGenerateRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in ShaclGenerateRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":374,\"validators\":[]},{\"name\":\"quality_tier\",\"meaning\":\"Original quality_tier in ShaclGenerateRequest\",\"annotation\":\"Literal[\\\"standard\\\", \\\"strict\\\"]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"strict\"},\"source_line\":375,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ShaclValidateRequest\",\"name\":\"ShaclValidateRequest\",\"meaning\":\"Original ShaclValidateRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"37c26e0a233d75fbd915e76276ab1f7e1d606e548c45f551f250d61d5e8b0d22\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Uri\"},\"shacl_turtle\":{\"title\":\"Shacl Turtle\",\"type\":\"string\"}},\"required\":[\"shacl_turtle\"],\"title\":\"ShaclValidateRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in ShaclValidateRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":379,\"validators\":[]},{\"name\":\"shacl_turtle\",\"meaning\":\"Original shacl_turtle in ShaclValidateRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":380,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ShaclViolation\",\"name\":\"ShaclViolation\",\"meaning\":\"Original ShaclViolation model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"6eb073bc38376935338a2328960ced528af2edfaf41d3e7f8d7970f8b538ee19\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"node\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Node\"},\"path\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Path\"},\"severity\":{\"default\":\"Violation\",\"title\":\"Severity\",\"type\":\"string\"},\"message\":{\"title\":\"Message\",\"type\":\"string\"},\"focus_node\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Focus Node\"},\"source_shape\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Shape\"}},\"required\":[\"message\"],\"title\":\"ShaclViolation\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"node\",\"meaning\":\"Original node in ShaclViolation\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":384,\"validators\":[]},{\"name\":\"path\",\"meaning\":\"Original path in ShaclViolation\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":385,\"validators\":[]},{\"name\":\"severity\",\"meaning\":\"Original severity in ShaclViolation\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"Violation\"},\"source_line\":386,\"validators\":[]},{\"name\":\"message\",\"meaning\":\"Original message in ShaclViolation\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":387,\"validators\":[]},{\"name\":\"focus_node\",\"meaning\":\"Original focus_node in ShaclViolation\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":388,\"validators\":[]},{\"name\":\"source_shape\",\"meaning\":\"Original source_shape in ShaclViolation\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":389,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ShaclShapeSummary\",\"name\":\"ShaclShapeSummary\",\"meaning\":\"Original ShaclShapeSummary model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"bf9ca7f29bd688812878306ce57d29734912f09dc1cbc1f5c658fa7f70031c5f\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"target_class\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Target Class\"},\"constraint_count\":{\"default\":0,\"title\":\"Constraint Count\",\"type\":\"integer\"},\"constraints\":{\"items\":{\"type\":\"string\"},\"title\":\"Constraints\",\"type\":\"array\"},\"violation_count\":{\"default\":0,\"title\":\"Violation Count\",\"type\":\"integer\"}},\"required\":[\"id\"],\"title\":\"ShaclShapeSummary\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"id\",\"meaning\":\"Original id in ShaclShapeSummary\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":393,\"validators\":[]},{\"name\":\"target_class\",\"meaning\":\"Original target_class in ShaclShapeSummary\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":394,\"validators\":[]},{\"name\":\"constraint_count\",\"meaning\":\"Original constraint_count in ShaclShapeSummary\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":395,\"validators\":[]},{\"name\":\"constraints\",\"meaning\":\"Original constraints in ShaclShapeSummary\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":396,\"validators\":[]},{\"name\":\"violation_count\",\"meaning\":\"Original violation_count in ShaclShapeSummary\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":397,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ShaclGenerateResponse\",\"name\":\"ShaclGenerateResponse\",\"meaning\":\"Original ShaclGenerateResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"e923122ac95efdc051efcca83d4bd699f905c25492b3e63606d82a50bff5a16e\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"shacl_turtle\":{\"title\":\"Shacl Turtle\",\"type\":\"string\"},\"shape_count\":{\"title\":\"Shape Count\",\"type\":\"integer\"},\"generated_at\":{\"title\":\"Generated At\",\"type\":\"string\"}},\"required\":[\"uri\",\"shacl_turtle\",\"shape_count\",\"generated_at\"],\"title\":\"ShaclGenerateResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in ShaclGenerateResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":401,\"validators\":[]},{\"name\":\"shacl_turtle\",\"meaning\":\"Original shacl_turtle in ShaclGenerateResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":402,\"validators\":[]},{\"name\":\"shape_count\",\"meaning\":\"Original shape_count in ShaclGenerateResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":403,\"validators\":[]},{\"name\":\"generated_at\",\"meaning\":\"Original generated_at in ShaclGenerateResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":404,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ShaclShapesResponse\",\"name\":\"ShaclShapesResponse\",\"meaning\":\"Original ShaclShapesResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"d2541aedf27c2fb59faf74c8d56127263eec4325919108f9d0222542f3a2763d\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"ShaclShapeSummary\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"target_class\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Target Class\"},\"constraint_count\":{\"default\":0,\"title\":\"Constraint Count\",\"type\":\"integer\"},\"constraints\":{\"items\":{\"type\":\"string\"},\"title\":\"Constraints\",\"type\":\"array\"},\"violation_count\":{\"default\":0,\"title\":\"Violation Count\",\"type\":\"integer\"}},\"required\":[\"id\"],\"title\":\"ShaclShapeSummary\",\"type\":\"object\"}},\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"shapes\":{\"items\":{\"$ref\":\"#/$defs/ShaclShapeSummary\"},\"title\":\"Shapes\",\"type\":\"array\"},\"shacl_turtle\":{\"title\":\"Shacl Turtle\",\"type\":\"string\"},\"generated_at\":{\"title\":\"Generated At\",\"type\":\"string\"}},\"required\":[\"uri\",\"shapes\",\"shacl_turtle\",\"generated_at\"],\"title\":\"ShaclShapesResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in ShaclShapesResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":408,\"validators\":[]},{\"name\":\"shapes\",\"meaning\":\"Original shapes in ShaclShapesResponse\",\"annotation\":\"List[ShaclShapeSummary]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":409,\"validators\":[]},{\"name\":\"shacl_turtle\",\"meaning\":\"Original shacl_turtle in ShaclShapesResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":410,\"validators\":[]},{\"name\":\"generated_at\",\"meaning\":\"Original generated_at in ShaclShapesResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":411,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ShaclValidationResponse\",\"name\":\"ShaclValidationResponse\",\"meaning\":\"Original ShaclValidationResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"50399cbcf561a8bf856784c5b251d0d5d2f32e193a4a17c30ad35827310fecb7\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"ShaclViolation\":{\"properties\":{\"node\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Node\"},\"path\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Path\"},\"severity\":{\"default\":\"Violation\",\"title\":\"Severity\",\"type\":\"string\"},\"message\":{\"title\":\"Message\",\"type\":\"string\"},\"focus_node\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Focus Node\"},\"source_shape\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Shape\"}},\"required\":[\"message\"],\"title\":\"ShaclViolation\",\"type\":\"object\"}},\"properties\":{\"uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Uri\"},\"conforms\":{\"title\":\"Conforms\",\"type\":\"boolean\"},\"status\":{\"default\":\"success\",\"enum\":[\"success\",\"unavailable\",\"error\"],\"title\":\"Status\",\"type\":\"string\"},\"message\":{\"title\":\"Message\",\"type\":\"string\"},\"violations\":{\"items\":{\"$ref\":\"#/$defs/ShaclViolation\"},\"title\":\"Violations\",\"type\":\"array\"},\"report_text\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Report Text\"}},\"required\":[\"conforms\",\"message\"],\"title\":\"ShaclValidationResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in ShaclValidationResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":415,\"validators\":[]},{\"name\":\"conforms\",\"meaning\":\"Original conforms in ShaclValidationResponse\",\"annotation\":\"bool\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":416,\"validators\":[]},{\"name\":\"status\",\"meaning\":\"Original status in ShaclValidationResponse\",\"annotation\":\"Literal[\\\"success\\\", \\\"unavailable\\\", \\\"error\\\"]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"success\"},\"source_line\":417,\"validators\":[]},{\"name\":\"message\",\"meaning\":\"Original message in ShaclValidationResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":418,\"validators\":[]},{\"name\":\"violations\",\"meaning\":\"Original violations in ShaclValidationResponse\",\"annotation\":\"List[ShaclViolation]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":419,\"validators\":[]},{\"name\":\"report_text\",\"meaning\":\"Original report_text in ShaclValidationResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":420,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::DraftDiff\",\"name\":\"DraftDiff\",\"meaning\":\"Original DraftDiff model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"272728fe2616e4db3b3a79017fb12cfd2cc0d4e420a521dfa076ce6620e18ae9\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"added_classes\":{\"items\":{\"type\":\"string\"},\"title\":\"Added Classes\",\"type\":\"array\"},\"removed_classes\":{\"items\":{\"type\":\"string\"},\"title\":\"Removed Classes\",\"type\":\"array\"},\"modified_classes\":{\"additionalProperties\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Modified Classes\",\"type\":\"object\"},\"added_properties\":{\"items\":{\"type\":\"string\"},\"title\":\"Added Properties\",\"type\":\"array\"},\"removed_properties\":{\"items\":{\"type\":\"string\"},\"title\":\"Removed Properties\",\"type\":\"array\"},\"modified_properties\":{\"additionalProperties\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Modified Properties\",\"type\":\"object\"},\"added_restrictions\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Added Restrictions\",\"type\":\"array\"},\"removed_restrictions\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Removed Restrictions\",\"type\":\"array\"},\"added_axioms\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Added Axioms\",\"type\":\"array\"},\"removed_axioms\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Removed Axioms\",\"type\":\"array\"},\"annotation_changes\":{\"additionalProperties\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Annotation Changes\",\"type\":\"object\"}},\"title\":\"DraftDiff\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"added_classes\",\"meaning\":\"Original added_classes in DraftDiff\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":428,\"validators\":[]},{\"name\":\"removed_classes\",\"meaning\":\"Original removed_classes in DraftDiff\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":429,\"validators\":[]},{\"name\":\"modified_classes\",\"meaning\":\"Original modified_classes in DraftDiff\",\"annotation\":\"Dict[str, Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":430,\"validators\":[]},{\"name\":\"added_properties\",\"meaning\":\"Original added_properties in DraftDiff\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":431,\"validators\":[]},{\"name\":\"removed_properties\",\"meaning\":\"Original removed_properties in DraftDiff\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":432,\"validators\":[]},{\"name\":\"modified_properties\",\"meaning\":\"Original modified_properties in DraftDiff\",\"annotation\":\"Dict[str, Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":433,\"validators\":[]},{\"name\":\"added_restrictions\",\"meaning\":\"Original added_restrictions in DraftDiff\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":434,\"validators\":[]},{\"name\":\"removed_restrictions\",\"meaning\":\"Original removed_restrictions in DraftDiff\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":435,\"validators\":[]},{\"name\":\"added_axioms\",\"meaning\":\"Original added_axioms in DraftDiff\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":436,\"validators\":[]},{\"name\":\"removed_axioms\",\"meaning\":\"Original removed_axioms in DraftDiff\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":437,\"validators\":[]},{\"name\":\"annotation_changes\",\"meaning\":\"Original annotation_changes in DraftDiff\",\"annotation\":\"Dict[str, Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":438,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::DraftRequest\",\"name\":\"DraftRequest\",\"meaning\":\"Original DraftRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"2ddd8388a12ec5208af85b7ed3393a26b79d076c11a6c8156347f006fbe5ee34\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"DraftDiff\":{\"properties\":{\"added_classes\":{\"items\":{\"type\":\"string\"},\"title\":\"Added Classes\",\"type\":\"array\"},\"removed_classes\":{\"items\":{\"type\":\"string\"},\"title\":\"Removed Classes\",\"type\":\"array\"},\"modified_classes\":{\"additionalProperties\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Modified Classes\",\"type\":\"object\"},\"added_properties\":{\"items\":{\"type\":\"string\"},\"title\":\"Added Properties\",\"type\":\"array\"},\"removed_properties\":{\"items\":{\"type\":\"string\"},\"title\":\"Removed Properties\",\"type\":\"array\"},\"modified_properties\":{\"additionalProperties\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Modified Properties\",\"type\":\"object\"},\"added_restrictions\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Added Restrictions\",\"type\":\"array\"},\"removed_restrictions\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Removed Restrictions\",\"type\":\"array\"},\"added_axioms\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Added Axioms\",\"type\":\"array\"},\"removed_axioms\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Removed Axioms\",\"type\":\"array\"},\"annotation_changes\":{\"additionalProperties\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Annotation Changes\",\"type\":\"object\"}},\"title\":\"DraftDiff\",\"type\":\"object\"}},\"properties\":{\"ontology_uri\":{\"title\":\"Ontology Uri\",\"type\":\"string\"},\"diff\":{\"$ref\":\"#/$defs/DraftDiff\"},\"author\":{\"title\":\"Author\",\"type\":\"string\"},\"summary\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Summary\"},\"timestamp\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Timestamp\"}},\"required\":[\"ontology_uri\",\"diff\",\"author\"],\"title\":\"DraftRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"ontology_uri\",\"meaning\":\"Original ontology_uri in DraftRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":442,\"validators\":[]},{\"name\":\"diff\",\"meaning\":\"Original diff in DraftRequest\",\"annotation\":\"DraftDiff\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":443,\"validators\":[]},{\"name\":\"author\",\"meaning\":\"Original author in DraftRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":444,\"validators\":[]},{\"name\":\"summary\",\"meaning\":\"Original summary in DraftRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":445,\"validators\":[]},{\"name\":\"timestamp\",\"meaning\":\"Original timestamp in DraftRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":446,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::DraftResponse\",\"name\":\"DraftResponse\",\"meaning\":\"Original DraftResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"4246e5648acd6da5c2581410b9ca6e390f6e4da8b055dc7d11c4bce39b2e1e3b\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"DraftDiff\":{\"properties\":{\"added_classes\":{\"items\":{\"type\":\"string\"},\"title\":\"Added Classes\",\"type\":\"array\"},\"removed_classes\":{\"items\":{\"type\":\"string\"},\"title\":\"Removed Classes\",\"type\":\"array\"},\"modified_classes\":{\"additionalProperties\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Modified Classes\",\"type\":\"object\"},\"added_properties\":{\"items\":{\"type\":\"string\"},\"title\":\"Added Properties\",\"type\":\"array\"},\"removed_properties\":{\"items\":{\"type\":\"string\"},\"title\":\"Removed Properties\",\"type\":\"array\"},\"modified_properties\":{\"additionalProperties\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Modified Properties\",\"type\":\"object\"},\"added_restrictions\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Added Restrictions\",\"type\":\"array\"},\"removed_restrictions\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Removed Restrictions\",\"type\":\"array\"},\"added_axioms\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Added Axioms\",\"type\":\"array\"},\"removed_axioms\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Removed Axioms\",\"type\":\"array\"},\"annotation_changes\":{\"additionalProperties\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Annotation Changes\",\"type\":\"object\"}},\"title\":\"DraftDiff\",\"type\":\"object\"}},\"properties\":{\"draft_id\":{\"title\":\"Draft Id\",\"type\":\"string\"},\"ontology_uri\":{\"title\":\"Ontology Uri\",\"type\":\"string\"},\"diff\":{\"$ref\":\"#/$defs/DraftDiff\"},\"author\":{\"title\":\"Author\",\"type\":\"string\"},\"summary\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Summary\"},\"created_at\":{\"title\":\"Created At\",\"type\":\"string\"},\"updated_at\":{\"title\":\"Updated At\",\"type\":\"string\"}},\"required\":[\"draft_id\",\"ontology_uri\",\"diff\",\"author\",\"created_at\",\"updated_at\"],\"title\":\"DraftResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"draft_id\",\"meaning\":\"Original draft_id in DraftResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":450,\"validators\":[]},{\"name\":\"ontology_uri\",\"meaning\":\"Original ontology_uri in DraftResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":451,\"validators\":[]},{\"name\":\"diff\",\"meaning\":\"Original diff in DraftResponse\",\"annotation\":\"DraftDiff\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":452,\"validators\":[]},{\"name\":\"author\",\"meaning\":\"Original author in DraftResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":453,\"validators\":[]},{\"name\":\"summary\",\"meaning\":\"Original summary in DraftResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":454,\"validators\":[]},{\"name\":\"created_at\",\"meaning\":\"Original created_at in DraftResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":455,\"validators\":[]},{\"name\":\"updated_at\",\"meaning\":\"Original updated_at in DraftResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":456,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ProposalState\",\"name\":\"ProposalState\",\"meaning\":\"Original ProposalState model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"df6383b0a2852ff09b2d1d2b95d69c18e276761061c73937645b4d4914fbbc97\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"state\":{\"enum\":[\"draft\",\"proposed\",\"approved\",\"published\",\"rejected\"],\"title\":\"State\",\"type\":\"string\"}},\"required\":[\"state\"],\"title\":\"ProposalState\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"state\",\"meaning\":\"Original state in ProposalState\",\"annotation\":\"Literal[\\\"draft\\\", \\\"proposed\\\", \\\"approved\\\", \\\"published\\\", \\\"rejected\\\"]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":460,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ProposalRequest\",\"name\":\"ProposalRequest\",\"meaning\":\"Original ProposalRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"0dcc5c46e9cd401569c9185c53e4b003f6067e631de5bf8e243ed43254e3d0f0\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"draft_id\":{\"title\":\"Draft Id\",\"type\":\"string\"},\"ontology_uri\":{\"title\":\"Ontology Uri\",\"type\":\"string\"},\"summary\":{\"title\":\"Summary\",\"type\":\"string\"},\"reviewer\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Reviewer\"}},\"required\":[\"draft_id\",\"ontology_uri\",\"summary\"],\"title\":\"ProposalRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"draft_id\",\"meaning\":\"Original draft_id in ProposalRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":464,\"validators\":[]},{\"name\":\"ontology_uri\",\"meaning\":\"Original ontology_uri in ProposalRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":465,\"validators\":[]},{\"name\":\"summary\",\"meaning\":\"Original summary in ProposalRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":466,\"validators\":[]},{\"name\":\"reviewer\",\"meaning\":\"Original reviewer in ProposalRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":467,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::ProposalResponse\",\"name\":\"ProposalResponse\",\"meaning\":\"Original ProposalResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"d7d8542d198ff292ea26f7a744852fa2cd858eb0ace97606d7861b46eaced227\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"proposal_id\":{\"title\":\"Proposal Id\",\"type\":\"string\"},\"draft_id\":{\"title\":\"Draft Id\",\"type\":\"string\"},\"ontology_uri\":{\"title\":\"Ontology Uri\",\"type\":\"string\"},\"summary\":{\"title\":\"Summary\",\"type\":\"string\"},\"author\":{\"title\":\"Author\",\"type\":\"string\"},\"reviewer\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Reviewer\"},\"state\":{\"enum\":[\"draft\",\"proposed\",\"approved\",\"published\",\"rejected\"],\"title\":\"State\",\"type\":\"string\"},\"impact_analysis\":{\"additionalProperties\":true,\"title\":\"Impact Analysis\",\"type\":\"object\"},\"shacl_validation\":{\"additionalProperties\":true,\"title\":\"Shacl Validation\",\"type\":\"object\"},\"created_at\":{\"title\":\"Created At\",\"type\":\"string\"},\"updated_at\":{\"title\":\"Updated At\",\"type\":\"string\"},\"comments\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Comments\",\"type\":\"array\"}},\"required\":[\"proposal_id\",\"draft_id\",\"ontology_uri\",\"summary\",\"author\",\"state\",\"created_at\",\"updated_at\"],\"title\":\"ProposalResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"proposal_id\",\"meaning\":\"Original proposal_id in ProposalResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":471,\"validators\":[]},{\"name\":\"draft_id\",\"meaning\":\"Original draft_id in ProposalResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":472,\"validators\":[]},{\"name\":\"ontology_uri\",\"meaning\":\"Original ontology_uri in ProposalResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":473,\"validators\":[]},{\"name\":\"summary\",\"meaning\":\"Original summary in ProposalResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":474,\"validators\":[]},{\"name\":\"author\",\"meaning\":\"Original author in ProposalResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":475,\"validators\":[]},{\"name\":\"reviewer\",\"meaning\":\"Original reviewer in ProposalResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":476,\"validators\":[]},{\"name\":\"state\",\"meaning\":\"Original state in ProposalResponse\",\"annotation\":\"Literal[\\\"draft\\\", \\\"proposed\\\", \\\"approved\\\", \\\"published\\\", \\\"rejected\\\"]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":477,\"validators\":[]},{\"name\":\"impact_analysis\",\"meaning\":\"Original impact_analysis in ProposalResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":478,\"validators\":[]},{\"name\":\"shacl_validation\",\"meaning\":\"Original shacl_validation in ProposalResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":479,\"validators\":[]},{\"name\":\"created_at\",\"meaning\":\"Original created_at in ProposalResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":480,\"validators\":[]},{\"name\":\"updated_at\",\"meaning\":\"Original updated_at in ProposalResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":481,\"validators\":[]},{\"name\":\"comments\",\"meaning\":\"Original comments in ProposalResponse\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":482,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::CommentRequest\",\"name\":\"CommentRequest\",\"meaning\":\"Original CommentRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"0bcb4d407958e4afd24da51feb64ea612e0524f7d6efcfe8e78080f429593613\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"element_uri\":{\"title\":\"Element Uri\",\"type\":\"string\"},\"text\":{\"title\":\"Text\",\"type\":\"string\"},\"author\":{\"title\":\"Author\",\"type\":\"string\"}},\"required\":[\"element_uri\",\"text\",\"author\"],\"title\":\"CommentRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"element_uri\",\"meaning\":\"Original element_uri in CommentRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":486,\"validators\":[]},{\"name\":\"text\",\"meaning\":\"Original text in CommentRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":487,\"validators\":[]},{\"name\":\"author\",\"meaning\":\"Original author in CommentRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":488,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::VersionEntry\",\"name\":\"VersionEntry\",\"meaning\":\"Original VersionEntry model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"fc27d1be62ebb40a07702932189fd383f5b34405605fb49c8ef2f05b07821f43\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"version_id\":{\"title\":\"Version Id\",\"type\":\"string\"},\"ontology_uri\":{\"title\":\"Ontology Uri\",\"type\":\"string\"},\"state\":{\"enum\":[\"draft\",\"published\"],\"title\":\"State\",\"type\":\"string\"},\"author\":{\"title\":\"Author\",\"type\":\"string\"},\"date\":{\"title\":\"Date\",\"type\":\"string\"},\"diff_summary\":{\"additionalProperties\":true,\"title\":\"Diff Summary\",\"type\":\"object\"}},\"required\":[\"version_id\",\"ontology_uri\",\"state\",\"author\",\"date\"],\"title\":\"VersionEntry\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"version_id\",\"meaning\":\"Original version_id in VersionEntry\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":492,\"validators\":[]},{\"name\":\"ontology_uri\",\"meaning\":\"Original ontology_uri in VersionEntry\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":493,\"validators\":[]},{\"name\":\"state\",\"meaning\":\"Original state in VersionEntry\",\"annotation\":\"Literal[\\\"draft\\\", \\\"published\\\"]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":494,\"validators\":[]},{\"name\":\"author\",\"meaning\":\"Original author in VersionEntry\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":495,\"validators\":[]},{\"name\":\"date\",\"meaning\":\"Original date in VersionEntry\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":496,\"validators\":[]},{\"name\":\"diff_summary\",\"meaning\":\"Original diff_summary in VersionEntry\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":497,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::VersionCompareRequest\",\"name\":\"VersionCompareRequest\",\"meaning\":\"Original VersionCompareRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"0de41ac2431da513bc9354dcd552d04446d9c754e3f47fba39f3cbe1cfb955f7\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"version1\":{\"title\":\"Version1\",\"type\":\"string\"},\"version2\":{\"title\":\"Version2\",\"type\":\"string\"}},\"required\":[\"version1\",\"version2\"],\"title\":\"VersionCompareRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"version1\",\"meaning\":\"Original version1 in VersionCompareRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":501,\"validators\":[]},{\"name\":\"version2\",\"meaning\":\"Original version2 in VersionCompareRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":502,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::VersionCompareResponse\",\"name\":\"VersionCompareResponse\",\"meaning\":\"Original VersionCompareResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"cb54fce354d665ea483e8d8f1c9755038901283c078ba5b1f20d628a5a7eb788\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"version1\":{\"title\":\"Version1\",\"type\":\"string\"},\"version2\":{\"title\":\"Version2\",\"type\":\"string\"},\"metadata_changes\":{\"additionalProperties\":true,\"title\":\"Metadata Changes\",\"type\":\"object\"},\"class_changes\":{\"additionalProperties\":true,\"title\":\"Class Changes\",\"type\":\"object\"},\"property_changes\":{\"additionalProperties\":true,\"title\":\"Property Changes\",\"type\":\"object\"},\"restriction_changes\":{\"additionalProperties\":true,\"title\":\"Restriction Changes\",\"type\":\"object\"},\"axiom_changes\":{\"additionalProperties\":true,\"title\":\"Axiom Changes\",\"type\":\"object\"}},\"required\":[\"version1\",\"version2\"],\"title\":\"VersionCompareResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"version1\",\"meaning\":\"Original version1 in VersionCompareResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":506,\"validators\":[]},{\"name\":\"version2\",\"meaning\":\"Original version2 in VersionCompareResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":507,\"validators\":[]},{\"name\":\"metadata_changes\",\"meaning\":\"Original metadata_changes in VersionCompareResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":508,\"validators\":[]},{\"name\":\"class_changes\",\"meaning\":\"Original class_changes in VersionCompareResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":509,\"validators\":[]},{\"name\":\"property_changes\",\"meaning\":\"Original property_changes in VersionCompareResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":510,\"validators\":[]},{\"name\":\"restriction_changes\",\"meaning\":\"Original restriction_changes in VersionCompareResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":511,\"validators\":[]},{\"name\":\"axiom_changes\",\"meaning\":\"Original axiom_changes in VersionCompareResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":512,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::AlignmentRequest\",\"name\":\"AlignmentRequest\",\"meaning\":\"Original AlignmentRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"a09be977b2c04cd3f37ab8dbc145d584bc2fc5c39736cc903cfc9923ee01564c\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"source_uri\":{\"title\":\"Source Uri\",\"type\":\"string\"},\"target_uri\":{\"title\":\"Target Uri\",\"type\":\"string\"},\"predicate\":{\"title\":\"Predicate\",\"type\":\"string\"}},\"required\":[\"source_uri\",\"target_uri\",\"predicate\"],\"title\":\"AlignmentRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"source_uri\",\"meaning\":\"Original source_uri in AlignmentRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":516,\"validators\":[]},{\"name\":\"target_uri\",\"meaning\":\"Original target_uri in AlignmentRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":517,\"validators\":[]},{\"name\":\"predicate\",\"meaning\":\"Original predicate in AlignmentRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":518,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.ontology::AlignmentResponse\",\"name\":\"AlignmentResponse\",\"meaning\":\"Original AlignmentResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/ontology.py\",\"source_sha256\":\"bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f\",\"class_sha256\":\"6955c93265a662313bb445b0069abda410d1d7e8e0f94353a8a3dc66cada80fb\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"source\":{\"title\":\"Source\",\"type\":\"string\"},\"predicate\":{\"title\":\"Predicate\",\"type\":\"string\"},\"target\":{\"title\":\"Target\",\"type\":\"string\"}},\"required\":[\"source\",\"predicate\",\"target\"],\"title\":\"AlignmentResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"source\",\"meaning\":\"Original source in AlignmentResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":522,\"validators\":[]},{\"name\":\"predicate\",\"meaning\":\"Original predicate in AlignmentResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":523,\"validators\":[]},{\"name\":\"target\",\"meaning\":\"Original target in AlignmentResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":524,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.sparql::SparqlRequest\",\"name\":\"SparqlRequest\",\"meaning\":\"Original SparqlRequest model from semantica/explorer/routes/sparql.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/sparql.py\",\"source_sha256\":\"3dd5794cfdac52930b55aa9291fbd6815b335b135e6399eda40b2465a71cecfa\",\"class_sha256\":\"21823985c42698cdeeccb3c9cd184f7010a36f474542373d03366d4d2193427e\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"query\":{\"title\":\"Query\",\"type\":\"string\"}},\"required\":[\"query\"],\"title\":\"SparqlRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"query\",\"meaning\":\"Original query in SparqlRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":107,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.sparql::SparqlResponse\",\"name\":\"SparqlResponse\",\"meaning\":\"Original SparqlResponse model from semantica/explorer/routes/sparql.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/sparql.py\",\"source_sha256\":\"3dd5794cfdac52930b55aa9291fbd6815b335b135e6399eda40b2465a71cecfa\",\"class_sha256\":\"c48db021a0cce0e9029660f303839245eaf9fbb2dfaaf775db97799390b2052d\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"columns\":{\"items\":{\"type\":\"string\"},\"title\":\"Columns\",\"type\":\"array\"},\"rows\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Rows\",\"type\":\"array\"},\"total\":{\"title\":\"Total\",\"type\":\"integer\"},\"truncated\":{\"default\":false,\"title\":\"Truncated\",\"type\":\"boolean\"},\"error\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Error\"},\"error_line\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Error Line\"},\"error_column\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Error Column\"}},\"required\":[\"columns\",\"rows\",\"total\"],\"title\":\"SparqlResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"columns\",\"meaning\":\"Original columns in SparqlResponse\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":111,\"validators\":[]},{\"name\":\"rows\",\"meaning\":\"Original rows in SparqlResponse\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":112,\"validators\":[]},{\"name\":\"total\",\"meaning\":\"Original total in SparqlResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":113,\"validators\":[]},{\"name\":\"truncated\",\"meaning\":\"Original truncated in SparqlResponse\",\"annotation\":\"bool\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":false},\"source_line\":114,\"validators\":[]},{\"name\":\"error\",\"meaning\":\"Original error in SparqlResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":115,\"validators\":[]},{\"name\":\"error_line\",\"meaning\":\"Original error_line in SparqlResponse\",\"annotation\":\"Optional[int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":116,\"validators\":[]},{\"name\":\"error_column\",\"meaning\":\"Original error_column in SparqlResponse\",\"annotation\":\"Optional[int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":117,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.temporal::TemporalSnapshotFastResponse\",\"name\":\"TemporalSnapshotFastResponse\",\"meaning\":\"Original TemporalSnapshotFastResponse model from semantica/explorer/routes/temporal.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/temporal.py\",\"source_sha256\":\"c2c8e6e154c68cfd612ac04672d46a2331d89fe3d8f4213e05ae838c98696b24\",\"class_sha256\":\"833a222f161e9639c9ef895ffcce92834062a3209c6fc0789c45e37f7cf14ad5\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"timestamp\":{\"title\":\"Timestamp\",\"type\":\"string\"},\"active_node_ids\":{\"items\":{\"type\":\"string\"},\"title\":\"Active Node Ids\",\"type\":\"array\"},\"active_node_count\":{\"title\":\"Active Node Count\",\"type\":\"integer\"}},\"required\":[\"timestamp\",\"active_node_ids\",\"active_node_count\"],\"title\":\"TemporalSnapshotFastResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"timestamp\",\"meaning\":\"Original timestamp in TemporalSnapshotFastResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":29,\"validators\":[]},{\"name\":\"active_node_ids\",\"meaning\":\"Original active_node_ids in TemporalSnapshotFastResponse\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":30,\"validators\":[]},{\"name\":\"active_node_count\",\"meaning\":\"Original active_node_count in TemporalSnapshotFastResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":31,\"validators\":[]}]},{\"id\":\"semantica.explorer.routes.temporal::TemporalBoundsResponse\",\"name\":\"TemporalBoundsResponse\",\"meaning\":\"Original TemporalBoundsResponse model from semantica/explorer/routes/temporal.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/routes/temporal.py\",\"source_sha256\":\"c2c8e6e154c68cfd612ac04672d46a2331d89fe3d8f4213e05ae838c98696b24\",\"class_sha256\":\"c77c479d283b44d6df7c852e15868acaa60e0d0ca5c88d79b4cf6b1a68324a0b\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"min\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Min\"},\"max\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Max\"}},\"title\":\"TemporalBoundsResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"min\",\"meaning\":\"Original min in TemporalBoundsResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":35,\"validators\":[]},{\"name\":\"max\",\"meaning\":\"Original max in TemporalBoundsResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":36,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ErrorResponse\",\"name\":\"ErrorResponse\",\"meaning\":\"Original ErrorResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"45057e448f68248387676616bae6eba3d4d20dbeb905a60cf7d9adfe0ef5cd0f\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"detail\":{\"title\":\"Detail\",\"type\":\"string\"},\"status_code\":{\"default\":500,\"title\":\"Status Code\",\"type\":\"integer\"}},\"required\":[\"detail\"],\"title\":\"ErrorResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"detail\",\"meaning\":\"Original detail in ErrorResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":12,\"validators\":[]},{\"name\":\"status_code\",\"meaning\":\"Original status_code in ErrorResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":500},\"source_line\":13,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::NodeResponse\",\"name\":\"NodeResponse\",\"meaning\":\"Original NodeResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"19324cd81bbbde803b858f9e974aacf5a019b36c611f4c04f238c81c33283230\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"content\":{\"default\":\"\",\"title\":\"Content\",\"type\":\"string\"},\"properties\":{\"type\":\"object\",\"additionalProperties\":true,\"title\":\"Properties\"},\"valid_from\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid From\"},\"valid_until\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid Until\"}},\"required\":[\"id\",\"type\"],\"title\":\"NodeResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"id\",\"meaning\":\"Original id in NodeResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":17,\"validators\":[]},{\"name\":\"type\",\"meaning\":\"Original type in NodeResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":18,\"validators\":[]},{\"name\":\"content\",\"meaning\":\"Original content in NodeResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":19,\"validators\":[]},{\"name\":\"properties\",\"meaning\":\"Original properties in NodeResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":20,\"validators\":[]},{\"name\":\"valid_from\",\"meaning\":\"Original valid_from in NodeResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":21,\"validators\":[]},{\"name\":\"valid_until\",\"meaning\":\"Original valid_until in NodeResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":22,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::EdgeResponse\",\"name\":\"EdgeResponse\",\"meaning\":\"Original EdgeResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"571f462324245093cf601eabb266c0d11fd9cd26da3ca8ed52149c5675ef1550\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"familyId\":{\"title\":\"Familyid\",\"type\":\"string\"},\"source\":{\"title\":\"Source\",\"type\":\"string\"},\"target\":{\"title\":\"Target\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"weight\":{\"default\":1.0,\"title\":\"Weight\",\"type\":\"number\"},\"properties\":{\"type\":\"object\",\"additionalProperties\":true,\"title\":\"Properties\"},\"valid_from\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid From\"},\"valid_until\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid Until\"}},\"required\":[\"id\",\"familyId\",\"source\",\"target\",\"type\"],\"title\":\"EdgeResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"id\",\"meaning\":\"Original id in EdgeResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":26,\"validators\":[]},{\"name\":\"familyId\",\"meaning\":\"Original familyId in EdgeResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":27,\"validators\":[]},{\"name\":\"source\",\"meaning\":\"Original source in EdgeResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":28,\"validators\":[]},{\"name\":\"target\",\"meaning\":\"Original target in EdgeResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":29,\"validators\":[]},{\"name\":\"type\",\"meaning\":\"Original type in EdgeResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":30,\"validators\":[]},{\"name\":\"weight\",\"meaning\":\"Original weight in EdgeResponse\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":1.0},\"source_line\":31,\"validators\":[]},{\"name\":\"properties\",\"meaning\":\"Original properties in EdgeResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":32,\"validators\":[]},{\"name\":\"valid_from\",\"meaning\":\"Original valid_from in EdgeResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":33,\"validators\":[]},{\"name\":\"valid_until\",\"meaning\":\"Original valid_until in EdgeResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":34,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::NodeListResponse\",\"name\":\"NodeListResponse\",\"meaning\":\"Original NodeListResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"c3e878fcf9aa2b650b6bbb283205d075d87dd0fc6f55ccc8dbac73d9f0a43e74\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"NodeResponse\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"content\":{\"default\":\"\",\"title\":\"Content\",\"type\":\"string\"},\"properties\":{\"type\":\"object\",\"additionalProperties\":true,\"title\":\"Properties\"},\"valid_from\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid From\"},\"valid_until\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid Until\"}},\"required\":[\"id\",\"type\"],\"title\":\"NodeResponse\",\"type\":\"object\"}},\"properties\":{\"nodes\":{\"items\":{\"$ref\":\"#/$defs/NodeResponse\"},\"title\":\"Nodes\",\"type\":\"array\"},\"total\":{\"title\":\"Total\",\"type\":\"integer\"},\"skip\":{\"default\":0,\"title\":\"Skip\",\"type\":\"integer\"},\"limit\":{\"default\":100,\"title\":\"Limit\",\"type\":\"integer\"},\"next_cursor\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Next Cursor\"},\"has_more\":{\"default\":false,\"title\":\"Has More\",\"type\":\"boolean\"}},\"required\":[\"nodes\",\"total\"],\"title\":\"NodeListResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"nodes\",\"meaning\":\"Original nodes in NodeListResponse\",\"annotation\":\"List[NodeResponse]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":38,\"validators\":[]},{\"name\":\"total\",\"meaning\":\"Original total in NodeListResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":39,\"validators\":[]},{\"name\":\"skip\",\"meaning\":\"Original skip in NodeListResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":40,\"validators\":[]},{\"name\":\"limit\",\"meaning\":\"Original limit in NodeListResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":100},\"source_line\":41,\"validators\":[]},{\"name\":\"next_cursor\",\"meaning\":\"Original next_cursor in NodeListResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":42,\"validators\":[]},{\"name\":\"has_more\",\"meaning\":\"Original has_more in NodeListResponse\",\"annotation\":\"bool\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":false},\"source_line\":43,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::EdgeListResponse\",\"name\":\"EdgeListResponse\",\"meaning\":\"Original EdgeListResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"41405933c23709d59d00b6b47fdd1958f5d11752ee645d93c862c2663a811f16\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"EdgeResponse\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"familyId\":{\"title\":\"Familyid\",\"type\":\"string\"},\"source\":{\"title\":\"Source\",\"type\":\"string\"},\"target\":{\"title\":\"Target\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"weight\":{\"default\":1.0,\"title\":\"Weight\",\"type\":\"number\"},\"properties\":{\"type\":\"object\",\"additionalProperties\":true,\"title\":\"Properties\"},\"valid_from\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid From\"},\"valid_until\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid Until\"}},\"required\":[\"id\",\"familyId\",\"source\",\"target\",\"type\"],\"title\":\"EdgeResponse\",\"type\":\"object\"}},\"properties\":{\"edges\":{\"items\":{\"$ref\":\"#/$defs/EdgeResponse\"},\"title\":\"Edges\",\"type\":\"array\"},\"total\":{\"title\":\"Total\",\"type\":\"integer\"},\"skip\":{\"default\":0,\"title\":\"Skip\",\"type\":\"integer\"},\"limit\":{\"default\":100,\"title\":\"Limit\",\"type\":\"integer\"},\"next_cursor\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Next Cursor\"},\"has_more\":{\"default\":false,\"title\":\"Has More\",\"type\":\"boolean\"}},\"required\":[\"edges\",\"total\"],\"title\":\"EdgeListResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"edges\",\"meaning\":\"Original edges in EdgeListResponse\",\"annotation\":\"List[EdgeResponse]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":47,\"validators\":[]},{\"name\":\"total\",\"meaning\":\"Original total in EdgeListResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":48,\"validators\":[]},{\"name\":\"skip\",\"meaning\":\"Original skip in EdgeListResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":49,\"validators\":[]},{\"name\":\"limit\",\"meaning\":\"Original limit in EdgeListResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":100},\"source_line\":50,\"validators\":[]},{\"name\":\"next_cursor\",\"meaning\":\"Original next_cursor in EdgeListResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":51,\"validators\":[]},{\"name\":\"has_more\",\"meaning\":\"Original has_more in EdgeListResponse\",\"annotation\":\"bool\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":false},\"source_line\":52,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::NeighborResponse\",\"name\":\"NeighborResponse\",\"meaning\":\"Original NeighborResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"5b9f1eb8ecce390a3c7934f4b69bf61a7b944c260b8c999d7a2d1d35c58f3052\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"content\":{\"default\":\"\",\"title\":\"Content\",\"type\":\"string\"},\"relationship\":{\"default\":\"\",\"title\":\"Relationship\",\"type\":\"string\"},\"weight\":{\"default\":1.0,\"title\":\"Weight\",\"type\":\"number\"},\"hop\":{\"default\":1,\"title\":\"Hop\",\"type\":\"integer\"}},\"required\":[\"id\",\"type\"],\"title\":\"NeighborResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"id\",\"meaning\":\"Original id in NeighborResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":56,\"validators\":[]},{\"name\":\"type\",\"meaning\":\"Original type in NeighborResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":57,\"validators\":[]},{\"name\":\"content\",\"meaning\":\"Original content in NeighborResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":58,\"validators\":[]},{\"name\":\"relationship\",\"meaning\":\"Original relationship in NeighborResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":59,\"validators\":[]},{\"name\":\"weight\",\"meaning\":\"Original weight in NeighborResponse\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":1.0},\"source_line\":60,\"validators\":[]},{\"name\":\"hop\",\"meaning\":\"Original hop in NeighborResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":1},\"source_line\":61,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::PathResponse\",\"name\":\"PathResponse\",\"meaning\":\"Original PathResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"c54531585f8634acf8ea2e97875d74566723bfbebcde0c7276538791f78070da\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"source\":{\"title\":\"Source\",\"type\":\"string\"},\"target\":{\"title\":\"Target\",\"type\":\"string\"},\"algorithm\":{\"title\":\"Algorithm\",\"type\":\"string\"},\"path\":{\"items\":{\"type\":\"string\"},\"title\":\"Path\",\"type\":\"array\"},\"edge_ids\":{\"items\":{\"type\":\"string\"},\"title\":\"Edge Ids\",\"type\":\"array\"},\"total_weight\":{\"default\":0.0,\"title\":\"Total Weight\",\"type\":\"number\"},\"directed\":{\"default\":true,\"title\":\"Directed\",\"type\":\"boolean\"},\"hop_count\":{\"default\":0,\"title\":\"Hop Count\",\"type\":\"integer\"},\"distance_band\":{\"default\":\"direct\",\"title\":\"Distance Band\",\"type\":\"string\"},\"semantic_similarity\":{\"anyOf\":[{\"type\":\"number\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Semantic Similarity\"},\"path_coherence_score\":{\"anyOf\":[{\"type\":\"number\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Path Coherence Score\"},\"confidence_decay\":{\"anyOf\":[{\"type\":\"number\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Confidence Decay\"},\"bottleneck_node\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Bottleneck Node\"},\"alternative_path_count\":{\"default\":0,\"title\":\"Alternative Path Count\",\"type\":\"integer\"},\"interpretation\":{\"default\":\"\",\"title\":\"Interpretation\",\"type\":\"string\"}},\"required\":[\"source\",\"target\",\"algorithm\",\"path\"],\"title\":\"PathResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"source\",\"meaning\":\"Original source in PathResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":65,\"validators\":[]},{\"name\":\"target\",\"meaning\":\"Original target in PathResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":66,\"validators\":[]},{\"name\":\"algorithm\",\"meaning\":\"Original algorithm in PathResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":67,\"validators\":[]},{\"name\":\"path\",\"meaning\":\"Original path in PathResponse\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":68,\"validators\":[]},{\"name\":\"edge_ids\",\"meaning\":\"Original edge_ids in PathResponse\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":69,\"validators\":[]},{\"name\":\"total_weight\",\"meaning\":\"Original total_weight in PathResponse\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.0},\"source_line\":70,\"validators\":[]},{\"name\":\"directed\",\"meaning\":\"Original directed in PathResponse\",\"annotation\":\"bool\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":true},\"source_line\":71,\"validators\":[]},{\"name\":\"hop_count\",\"meaning\":\"Original hop_count in PathResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":72,\"validators\":[]},{\"name\":\"distance_band\",\"meaning\":\"Original distance_band in PathResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"direct\"},\"source_line\":73,\"validators\":[]},{\"name\":\"semantic_similarity\",\"meaning\":\"Original semantic_similarity in PathResponse\",\"annotation\":\"Optional[float]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":75,\"validators\":[]},{\"name\":\"path_coherence_score\",\"meaning\":\"Original path_coherence_score in PathResponse\",\"annotation\":\"Optional[float]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":76,\"validators\":[]},{\"name\":\"confidence_decay\",\"meaning\":\"Original confidence_decay in PathResponse\",\"annotation\":\"Optional[float]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":77,\"validators\":[]},{\"name\":\"bottleneck_node\",\"meaning\":\"Original bottleneck_node in PathResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":78,\"validators\":[]},{\"name\":\"alternative_path_count\",\"meaning\":\"Original alternative_path_count in PathResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":79,\"validators\":[]},{\"name\":\"interpretation\",\"meaning\":\"Original interpretation in PathResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":80,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::GraphStatsResponse\",\"name\":\"GraphStatsResponse\",\"meaning\":\"Original GraphStatsResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"5fc491d9f4e1a93ced2163596dffc9d6855f6bdd845ee74cb016f3a4a649dfb3\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"node_count\":{\"title\":\"Node Count\",\"type\":\"integer\"},\"edge_count\":{\"title\":\"Edge Count\",\"type\":\"integer\"},\"node_types\":{\"additionalProperties\":{\"type\":\"integer\"},\"title\":\"Node Types\",\"type\":\"object\"},\"edge_types\":{\"additionalProperties\":{\"type\":\"integer\"},\"title\":\"Edge Types\",\"type\":\"object\"},\"density\":{\"default\":0.0,\"title\":\"Density\",\"type\":\"number\"}},\"required\":[\"node_count\",\"edge_count\"],\"title\":\"GraphStatsResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"node_count\",\"meaning\":\"Original node_count in GraphStatsResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":84,\"validators\":[]},{\"name\":\"edge_count\",\"meaning\":\"Original edge_count in GraphStatsResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":85,\"validators\":[]},{\"name\":\"node_types\",\"meaning\":\"Original node_types in GraphStatsResponse\",\"annotation\":\"Dict[str, int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":86,\"validators\":[]},{\"name\":\"edge_types\",\"meaning\":\"Original edge_types in GraphStatsResponse\",\"annotation\":\"Dict[str, int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":87,\"validators\":[]},{\"name\":\"density\",\"meaning\":\"Original density in GraphStatsResponse\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.0},\"source_line\":88,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::SearchRequest\",\"name\":\"SearchRequest\",\"meaning\":\"Original SearchRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"f815ca7e7c4a9d08cc4bc60a496ab8ae3173ce1bb0b06c07a7f1767292568e30\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"query\":{\"title\":\"Query\",\"type\":\"string\"},\"filters\":{\"additionalProperties\":true,\"title\":\"Filters\",\"type\":\"object\"},\"limit\":{\"default\":20,\"maximum\":200,\"minimum\":1,\"title\":\"Limit\",\"type\":\"integer\"},\"anchor_node\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Anchor Node\"},\"max_hops\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Max Hops\"},\"min_semantic_similarity\":{\"anyOf\":[{\"type\":\"number\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Min Semantic Similarity\"},\"rank_by\":{\"default\":\"relevance\",\"enum\":[\"relevance\",\"proximity\",\"hybrid\"],\"title\":\"Rank By\",\"type\":\"string\"}},\"required\":[\"query\"],\"title\":\"SearchRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"query\",\"meaning\":\"Original query in SearchRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":92,\"validators\":[]},{\"name\":\"filters\",\"meaning\":\"Original filters in SearchRequest\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":93,\"validators\":[]},{\"name\":\"limit\",\"meaning\":\"Original limit in SearchRequest\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":20},\"source_line\":94,\"validators\":[]},{\"name\":\"anchor_node\",\"meaning\":\"Original anchor_node in SearchRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":96,\"validators\":[]},{\"name\":\"max_hops\",\"meaning\":\"Original max_hops in SearchRequest\",\"annotation\":\"Optional[int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":97,\"validators\":[]},{\"name\":\"min_semantic_similarity\",\"meaning\":\"Original min_semantic_similarity in SearchRequest\",\"annotation\":\"Optional[float]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":98,\"validators\":[]},{\"name\":\"rank_by\",\"meaning\":\"Original rank_by in SearchRequest\",\"annotation\":\"Literal[\\\"relevance\\\", \\\"proximity\\\", \\\"hybrid\\\"]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"relevance\"},\"source_line\":99,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::SearchResultItem\",\"name\":\"SearchResultItem\",\"meaning\":\"Original SearchResultItem model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"df34d06ac32ca41bd514865fc4abff6cb174c43d44108e76b2d50f17ea7e8829\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"NodeResponse\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"content\":{\"default\":\"\",\"title\":\"Content\",\"type\":\"string\"},\"properties\":{\"type\":\"object\",\"additionalProperties\":true,\"title\":\"Properties\"},\"valid_from\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid From\"},\"valid_until\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid Until\"}},\"required\":[\"id\",\"type\"],\"title\":\"NodeResponse\",\"type\":\"object\"}},\"properties\":{\"node\":{\"$ref\":\"#/$defs/NodeResponse\"},\"score\":{\"default\":0.0,\"title\":\"Score\",\"type\":\"number\"},\"hop_distance\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Hop Distance\"},\"semantic_similarity\":{\"anyOf\":[{\"type\":\"number\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Semantic Similarity\"}},\"required\":[\"node\"],\"title\":\"SearchResultItem\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"node\",\"meaning\":\"Original node in SearchResultItem\",\"annotation\":\"NodeResponse\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":103,\"validators\":[]},{\"name\":\"score\",\"meaning\":\"Original score in SearchResultItem\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.0},\"source_line\":104,\"validators\":[]},{\"name\":\"hop_distance\",\"meaning\":\"Original hop_distance in SearchResultItem\",\"annotation\":\"Optional[int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":106,\"validators\":[]},{\"name\":\"semantic_similarity\",\"meaning\":\"Original semantic_similarity in SearchResultItem\",\"annotation\":\"Optional[float]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":107,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::SearchResultResponse\",\"name\":\"SearchResultResponse\",\"meaning\":\"Original SearchResultResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"e0137ecf9a5264fabe48e6d2faac8cb45c144fbfe76e2c45aa474156b5d32e2e\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"NodeResponse\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"content\":{\"default\":\"\",\"title\":\"Content\",\"type\":\"string\"},\"properties\":{\"type\":\"object\",\"additionalProperties\":true,\"title\":\"Properties\"},\"valid_from\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid From\"},\"valid_until\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid Until\"}},\"required\":[\"id\",\"type\"],\"title\":\"NodeResponse\",\"type\":\"object\"},\"SearchResultItem\":{\"properties\":{\"node\":{\"$ref\":\"#/$defs/NodeResponse\"},\"score\":{\"default\":0.0,\"title\":\"Score\",\"type\":\"number\"},\"hop_distance\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Hop Distance\"},\"semantic_similarity\":{\"anyOf\":[{\"type\":\"number\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Semantic Similarity\"}},\"required\":[\"node\"],\"title\":\"SearchResultItem\",\"type\":\"object\"}},\"properties\":{\"results\":{\"items\":{\"$ref\":\"#/$defs/SearchResultItem\"},\"title\":\"Results\",\"type\":\"array\"},\"total\":{\"title\":\"Total\",\"type\":\"integer\"},\"query\":{\"title\":\"Query\",\"type\":\"string\"}},\"required\":[\"results\",\"total\",\"query\"],\"title\":\"SearchResultResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"results\",\"meaning\":\"Original results in SearchResultResponse\",\"annotation\":\"List[SearchResultItem]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":111,\"validators\":[]},{\"name\":\"total\",\"meaning\":\"Original total in SearchResultResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":112,\"validators\":[]},{\"name\":\"query\",\"meaning\":\"Original query in SearchResultResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":113,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::AnalyticsResponse\",\"name\":\"AnalyticsResponse\",\"meaning\":\"Original AnalyticsResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"86775c3db54e62c3406c4e9c97a318fc9ca5b569b358b55d809a8d1671161ef9\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"centrality\":{\"anyOf\":[{\"additionalProperties\":true,\"type\":\"object\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Centrality\"},\"community\":{\"anyOf\":[{\"additionalProperties\":true,\"type\":\"object\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Community\"},\"connectivity\":{\"anyOf\":[{\"additionalProperties\":true,\"type\":\"object\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Connectivity\"}},\"title\":\"AnalyticsResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"centrality\",\"meaning\":\"Original centrality in AnalyticsResponse\",\"annotation\":\"Optional[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":117,\"validators\":[]},{\"name\":\"community\",\"meaning\":\"Original community in AnalyticsResponse\",\"annotation\":\"Optional[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":118,\"validators\":[]},{\"name\":\"connectivity\",\"meaning\":\"Original connectivity in AnalyticsResponse\",\"annotation\":\"Optional[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":119,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ValidationIssue\",\"name\":\"ValidationIssue\",\"meaning\":\"Original ValidationIssue model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"eca63f419572c4d2836a92e736fdebce4ac6a24323c278af1da1aca8e0d53821\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"severity\":{\"title\":\"Severity\",\"type\":\"string\"},\"message\":{\"title\":\"Message\",\"type\":\"string\"},\"node_id\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Node Id\"},\"edge_source\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Edge Source\"},\"edge_target\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Edge Target\"}},\"required\":[\"severity\",\"message\"],\"title\":\"ValidationIssue\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"severity\",\"meaning\":\"Original severity in ValidationIssue\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":123,\"validators\":[]},{\"name\":\"message\",\"meaning\":\"Original message in ValidationIssue\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":124,\"validators\":[]},{\"name\":\"node_id\",\"meaning\":\"Original node_id in ValidationIssue\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":125,\"validators\":[]},{\"name\":\"edge_source\",\"meaning\":\"Original edge_source in ValidationIssue\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":126,\"validators\":[]},{\"name\":\"edge_target\",\"meaning\":\"Original edge_target in ValidationIssue\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":127,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ValidationReportResponse\",\"name\":\"ValidationReportResponse\",\"meaning\":\"Original ValidationReportResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"1a2b5c8ab31da50d51c0febf4e79b86912adc962769501070b8c626976ef1ecf\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"ValidationIssue\":{\"properties\":{\"severity\":{\"title\":\"Severity\",\"type\":\"string\"},\"message\":{\"title\":\"Message\",\"type\":\"string\"},\"node_id\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Node Id\"},\"edge_source\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Edge Source\"},\"edge_target\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Edge Target\"}},\"required\":[\"severity\",\"message\"],\"title\":\"ValidationIssue\",\"type\":\"object\"}},\"properties\":{\"valid\":{\"title\":\"Valid\",\"type\":\"boolean\"},\"error_count\":{\"default\":0,\"title\":\"Error Count\",\"type\":\"integer\"},\"warning_count\":{\"default\":0,\"title\":\"Warning Count\",\"type\":\"integer\"},\"issues\":{\"items\":{\"$ref\":\"#/$defs/ValidationIssue\"},\"title\":\"Issues\",\"type\":\"array\"}},\"required\":[\"valid\"],\"title\":\"ValidationReportResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"valid\",\"meaning\":\"Original valid in ValidationReportResponse\",\"annotation\":\"bool\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":131,\"validators\":[]},{\"name\":\"error_count\",\"meaning\":\"Original error_count in ValidationReportResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":132,\"validators\":[]},{\"name\":\"warning_count\",\"meaning\":\"Original warning_count in ValidationReportResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":133,\"validators\":[]},{\"name\":\"issues\",\"meaning\":\"Original issues in ValidationReportResponse\",\"annotation\":\"List[ValidationIssue]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":134,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::DecisionResponse\",\"name\":\"DecisionResponse\",\"meaning\":\"Original DecisionResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"fd5a312fb931bcc67d0070ee61a2984e0cec5ea69a8cc2ad1e56aad3e2dc447c\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"decision_id\":{\"title\":\"Decision Id\",\"type\":\"string\"},\"category\":{\"default\":\"\",\"title\":\"Category\",\"type\":\"string\"},\"scenario\":{\"default\":\"\",\"title\":\"Scenario\",\"type\":\"string\"},\"reasoning\":{\"default\":\"\",\"title\":\"Reasoning\",\"type\":\"string\"},\"outcome\":{\"default\":\"\",\"title\":\"Outcome\",\"type\":\"string\"},\"confidence\":{\"default\":0.0,\"title\":\"Confidence\",\"type\":\"number\"},\"timestamp\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Timestamp\"},\"metadata\":{\"additionalProperties\":true,\"title\":\"Metadata\",\"type\":\"object\"}},\"required\":[\"decision_id\"],\"title\":\"DecisionResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"decision_id\",\"meaning\":\"Original decision_id in DecisionResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":138,\"validators\":[]},{\"name\":\"category\",\"meaning\":\"Original category in DecisionResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":139,\"validators\":[]},{\"name\":\"scenario\",\"meaning\":\"Original scenario in DecisionResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":140,\"validators\":[]},{\"name\":\"reasoning\",\"meaning\":\"Original reasoning in DecisionResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":141,\"validators\":[]},{\"name\":\"outcome\",\"meaning\":\"Original outcome in DecisionResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":142,\"validators\":[]},{\"name\":\"confidence\",\"meaning\":\"Original confidence in DecisionResponse\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.0},\"source_line\":143,\"validators\":[]},{\"name\":\"timestamp\",\"meaning\":\"Original timestamp in DecisionResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":144,\"validators\":[\"decision_timestamp\"]},{\"name\":\"metadata\",\"meaning\":\"Original metadata in DecisionResponse\",\"annotation\":\"Dict[str, Any]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":145,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::CausalChainResponse\",\"name\":\"CausalChainResponse\",\"meaning\":\"Original CausalChainResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"16f430670c24926057529c444c825e6643286fc9ca6454956afa42fc32d64657\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"decision_id\":{\"title\":\"Decision Id\",\"type\":\"string\"},\"chain\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Chain\",\"type\":\"array\"}},\"required\":[\"decision_id\"],\"title\":\"CausalChainResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"decision_id\",\"meaning\":\"Original decision_id in CausalChainResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":180,\"validators\":[]},{\"name\":\"chain\",\"meaning\":\"Original chain in CausalChainResponse\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":181,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ComplianceResponse\",\"name\":\"ComplianceResponse\",\"meaning\":\"Original ComplianceResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"25414297ef7366253908beeb8bae02ae4aee4dd37c34d458f5c04df8f74034a6\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"decision_id\":{\"title\":\"Decision Id\",\"type\":\"string\"},\"compliant\":{\"default\":true,\"title\":\"Compliant\",\"type\":\"boolean\"},\"violations\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Violations\",\"type\":\"array\"}},\"required\":[\"decision_id\"],\"title\":\"ComplianceResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"decision_id\",\"meaning\":\"Original decision_id in ComplianceResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":185,\"validators\":[]},{\"name\":\"compliant\",\"meaning\":\"Original compliant in ComplianceResponse\",\"annotation\":\"bool\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":true},\"source_line\":186,\"validators\":[]},{\"name\":\"violations\",\"meaning\":\"Original violations in ComplianceResponse\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":187,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::TemporalSnapshotResponse\",\"name\":\"TemporalSnapshotResponse\",\"meaning\":\"Original TemporalSnapshotResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"516988011a6709e8d243119c8150bf996acec0e91a232a60b762f18e9c417c74\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"NodeResponse\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"content\":{\"default\":\"\",\"title\":\"Content\",\"type\":\"string\"},\"properties\":{\"type\":\"object\",\"additionalProperties\":true,\"title\":\"Properties\"},\"valid_from\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid From\"},\"valid_until\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Valid Until\"}},\"required\":[\"id\",\"type\"],\"title\":\"NodeResponse\",\"type\":\"object\"}},\"properties\":{\"timestamp\":{\"title\":\"Timestamp\",\"type\":\"string\"},\"active_nodes\":{\"items\":{\"$ref\":\"#/$defs/NodeResponse\"},\"title\":\"Active Nodes\",\"type\":\"array\"},\"active_node_count\":{\"title\":\"Active Node Count\",\"type\":\"integer\"}},\"required\":[\"timestamp\",\"active_nodes\",\"active_node_count\"],\"title\":\"TemporalSnapshotResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"timestamp\",\"meaning\":\"Original timestamp in TemporalSnapshotResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":191,\"validators\":[]},{\"name\":\"active_nodes\",\"meaning\":\"Original active_nodes in TemporalSnapshotResponse\",\"annotation\":\"List[NodeResponse]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":192,\"validators\":[]},{\"name\":\"active_node_count\",\"meaning\":\"Original active_node_count in TemporalSnapshotResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":193,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::TemporalDiffResponse\",\"name\":\"TemporalDiffResponse\",\"meaning\":\"Original TemporalDiffResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"f22633ab5279398f1af5f10fbd5e40fd090858f657a86b98cf3e331e454a32a4\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"from_time\":{\"title\":\"From Time\",\"type\":\"string\"},\"to_time\":{\"title\":\"To Time\",\"type\":\"string\"},\"added_nodes\":{\"items\":{\"type\":\"string\"},\"title\":\"Added Nodes\",\"type\":\"array\"},\"removed_nodes\":{\"items\":{\"type\":\"string\"},\"title\":\"Removed Nodes\",\"type\":\"array\"}},\"required\":[\"from_time\",\"to_time\"],\"title\":\"TemporalDiffResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"from_time\",\"meaning\":\"Original from_time in TemporalDiffResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":197,\"validators\":[]},{\"name\":\"to_time\",\"meaning\":\"Original to_time in TemporalDiffResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":198,\"validators\":[]},{\"name\":\"added_nodes\",\"meaning\":\"Original added_nodes in TemporalDiffResponse\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":199,\"validators\":[]},{\"name\":\"removed_nodes\",\"meaning\":\"Original removed_nodes in TemporalDiffResponse\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":200,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::TemporalPatternResponse\",\"name\":\"TemporalPatternResponse\",\"meaning\":\"Original TemporalPatternResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"3e73e15b30aaead58ba441e570755e04dd75a27c762d67cf41b40f5d6d6d2eaf\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"patterns\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Patterns\",\"type\":\"array\"}},\"title\":\"TemporalPatternResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"patterns\",\"meaning\":\"Original patterns in TemporalPatternResponse\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":204,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::EnrichExtractRequest\",\"name\":\"EnrichExtractRequest\",\"meaning\":\"Original EnrichExtractRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"72d827898b6cde7a4d1e4bd885c2ebc1b3bb6f9305d3d15a02466fae7d159770\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"text\":{\"maxLength\":10000,\"title\":\"Text\",\"type\":\"string\"}},\"required\":[\"text\"],\"title\":\"EnrichExtractRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"text\",\"meaning\":\"Original text in EnrichExtractRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":210,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::EnrichExtractResponse\",\"name\":\"EnrichExtractResponse\",\"meaning\":\"Original EnrichExtractResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"c9153da9fb2bb3be963ccdbf6144675de38641e00f105de0b9c189414a79436b\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"entities\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Entities\",\"type\":\"array\"},\"relations\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Relations\",\"type\":\"array\"}},\"title\":\"EnrichExtractResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"entities\",\"meaning\":\"Original entities in EnrichExtractResponse\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":214,\"validators\":[]},{\"name\":\"relations\",\"meaning\":\"Original relations in EnrichExtractResponse\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":215,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::LinkPredictionRequest\",\"name\":\"LinkPredictionRequest\",\"meaning\":\"Original LinkPredictionRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"13242148d5357ab53b352d1234e365b334172ecd8c9807952bd176602926710e\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"node_id\":{\"title\":\"Node Id\",\"type\":\"string\"},\"top_n\":{\"default\":10,\"maximum\":200,\"minimum\":1,\"title\":\"Top N\",\"type\":\"integer\"},\"candidate_type\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Candidate Type\"},\"min_score\":{\"default\":0.0,\"minimum\":0.0,\"title\":\"Min Score\",\"type\":\"number\"}},\"required\":[\"node_id\"],\"title\":\"LinkPredictionRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"node_id\",\"meaning\":\"Original node_id in LinkPredictionRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":219,\"validators\":[]},{\"name\":\"top_n\",\"meaning\":\"Original top_n in LinkPredictionRequest\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":10},\"source_line\":220,\"validators\":[]},{\"name\":\"candidate_type\",\"meaning\":\"Original candidate_type in LinkPredictionRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":221,\"validators\":[]},{\"name\":\"min_score\",\"meaning\":\"Original min_score in LinkPredictionRequest\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.0},\"source_line\":222,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::LinkPredictionResponse\",\"name\":\"LinkPredictionResponse\",\"meaning\":\"Original LinkPredictionResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"517bf23a830b08d31ca3070f489189e19b6fc4767e01017b247ff3b60e781a45\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"node_id\":{\"title\":\"Node Id\",\"type\":\"string\"},\"predictions\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Predictions\",\"type\":\"array\"}},\"required\":[\"node_id\"],\"title\":\"LinkPredictionResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"node_id\",\"meaning\":\"Original node_id in LinkPredictionResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":226,\"validators\":[]},{\"name\":\"predictions\",\"meaning\":\"Original predictions in LinkPredictionResponse\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":227,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::DedupRequest\",\"name\":\"DedupRequest\",\"meaning\":\"Original DedupRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"2ef0eabe406d4d74abdd1bd6a6c993e68e504d12c7e9766fb0c1e738ec229ff4\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"threshold\":{\"default\":0.8,\"maximum\":1.0,\"minimum\":0.0,\"title\":\"Threshold\",\"type\":\"number\"}},\"title\":\"DedupRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"threshold\",\"meaning\":\"Original threshold in DedupRequest\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.8},\"source_line\":231,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::DedupResponse\",\"name\":\"DedupResponse\",\"meaning\":\"Original DedupResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"9f46d34e1d2c00fcabd42b270e302fd051dd4cc093f9e37ba956a4ef773d7681\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"duplicates\":{\"items\":{\"additionalProperties\":true,\"type\":\"object\"},\"title\":\"Duplicates\",\"type\":\"array\"},\"total_flagged\":{\"default\":0,\"title\":\"Total Flagged\",\"type\":\"integer\"}},\"title\":\"DedupResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"duplicates\",\"meaning\":\"Original duplicates in DedupResponse\",\"annotation\":\"List[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":235,\"validators\":[]},{\"name\":\"total_flagged\",\"meaning\":\"Original total_flagged in DedupResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":236,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ReasoningRequest\",\"name\":\"ReasoningRequest\",\"meaning\":\"Original ReasoningRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"e071ca2d41efc3729ba01b262bd2e4bf4c6a26ed4314c601ca9de4ec208cc4d3\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"facts\":{\"items\":{\"type\":\"string\"},\"title\":\"Facts\",\"type\":\"array\"},\"rules\":{\"items\":{\"type\":\"string\"},\"title\":\"Rules\",\"type\":\"array\"},\"mode\":{\"default\":\"forward\",\"title\":\"Mode\",\"type\":\"string\"},\"apply_to_graph\":{\"default\":false,\"title\":\"Apply To Graph\",\"type\":\"boolean\"},\"inferred_edge_type\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Inferred Edge Type\"}},\"required\":[\"facts\",\"rules\"],\"title\":\"ReasoningRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"facts\",\"meaning\":\"Original facts in ReasoningRequest\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":240,\"validators\":[]},{\"name\":\"rules\",\"meaning\":\"Original rules in ReasoningRequest\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":241,\"validators\":[]},{\"name\":\"mode\",\"meaning\":\"Original mode in ReasoningRequest\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"forward\"},\"source_line\":242,\"validators\":[]},{\"name\":\"apply_to_graph\",\"meaning\":\"Original apply_to_graph in ReasoningRequest\",\"annotation\":\"bool\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":false},\"source_line\":243,\"validators\":[]},{\"name\":\"inferred_edge_type\",\"meaning\":\"Original inferred_edge_type in ReasoningRequest\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":244,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ReasoningResponse\",\"name\":\"ReasoningResponse\",\"meaning\":\"Original ReasoningResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"7eafa8c3173716b29a9cf97e9c81d82e78b78ff5ecf966d9b91e94343f8b1db5\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"inferred_facts\":{\"items\":{\"type\":\"string\"},\"title\":\"Inferred Facts\",\"type\":\"array\"},\"rules_fired\":{\"default\":0,\"title\":\"Rules Fired\",\"type\":\"integer\"},\"added_edges\":{\"default\":0,\"title\":\"Added Edges\",\"type\":\"integer\"},\"mutated\":{\"default\":false,\"title\":\"Mutated\",\"type\":\"boolean\"}},\"title\":\"ReasoningResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"inferred_facts\",\"meaning\":\"Original inferred_facts in ReasoningResponse\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":248,\"validators\":[]},{\"name\":\"rules_fired\",\"meaning\":\"Original rules_fired in ReasoningResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":249,\"validators\":[]},{\"name\":\"added_edges\",\"meaning\":\"Original added_edges in ReasoningResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":250,\"validators\":[]},{\"name\":\"mutated\",\"meaning\":\"Original mutated in ReasoningResponse\",\"annotation\":\"bool\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":false},\"source_line\":251,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ExportRequest\",\"name\":\"ExportRequest\",\"meaning\":\"Original ExportRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"1973c25f650cb55730de77c832496442cf3f846a14488b009d76e7d3f4ac1f6b\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"format\":{\"default\":\"json\",\"title\":\"Format\",\"type\":\"string\"},\"node_ids\":{\"anyOf\":[{\"items\":{\"type\":\"string\"},\"type\":\"array\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Node Ids\"}},\"title\":\"ExportRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"format\",\"meaning\":\"Original format in ExportRequest\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"json\"},\"source_line\":255,\"validators\":[]},{\"name\":\"node_ids\",\"meaning\":\"Original node_ids in ExportRequest\",\"annotation\":\"Optional[List[str]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":256,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ExportResponse\",\"name\":\"ExportResponse\",\"meaning\":\"Original ExportResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"5b1031f528294619ce0564e46c1e82ad458447862fc61d843455a7d857e3d740\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"format\":{\"title\":\"Format\",\"type\":\"string\"},\"content_type\":{\"title\":\"Content Type\",\"type\":\"string\"},\"filename\":{\"title\":\"Filename\",\"type\":\"string\"},\"size_bytes\":{\"default\":0,\"title\":\"Size Bytes\",\"type\":\"integer\"}},\"required\":[\"format\",\"content_type\",\"filename\"],\"title\":\"ExportResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"format\",\"meaning\":\"Original format in ExportResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":260,\"validators\":[]},{\"name\":\"content_type\",\"meaning\":\"Original content_type in ExportResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":261,\"validators\":[]},{\"name\":\"filename\",\"meaning\":\"Original filename in ExportResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":262,\"validators\":[]},{\"name\":\"size_bytes\",\"meaning\":\"Original size_bytes in ExportResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":263,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ImportResponse\",\"name\":\"ImportResponse\",\"meaning\":\"Original ImportResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"0e08f7d8e7e23c2a8feda6c9f7724b1199714027a62ca6ad9909dac8c85ea9eb\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"status\":{\"default\":\"success\",\"title\":\"Status\",\"type\":\"string\"},\"message\":{\"default\":\"Import successful\",\"title\":\"Message\",\"type\":\"string\"},\"nodes_added\":{\"default\":0,\"title\":\"Nodes Added\",\"type\":\"integer\"},\"edges_added\":{\"default\":0,\"title\":\"Edges Added\",\"type\":\"integer\"},\"nodes_imported\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Nodes Imported\"},\"edges_imported\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Edges Imported\"}},\"title\":\"ImportResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"status\",\"meaning\":\"Original status in ImportResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"success\"},\"source_line\":267,\"validators\":[]},{\"name\":\"message\",\"meaning\":\"Original message in ImportResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"Import successful\"},\"source_line\":268,\"validators\":[]},{\"name\":\"nodes_added\",\"meaning\":\"Original nodes_added in ImportResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":269,\"validators\":[]},{\"name\":\"edges_added\",\"meaning\":\"Original edges_added in ImportResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":270,\"validators\":[]},{\"name\":\"nodes_imported\",\"meaning\":\"Original nodes_imported in ImportResponse\",\"annotation\":\"Optional[int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":271,\"validators\":[]},{\"name\":\"edges_imported\",\"meaning\":\"Original edges_imported in ImportResponse\",\"annotation\":\"Optional[int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":272,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::StandardMessageResponse\",\"name\":\"StandardMessageResponse\",\"meaning\":\"Original StandardMessageResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"adcbb4b793e59cf0776203ddff3b1ed92a075cd77fb1797bf45af1ef6be9586a\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"status\":{\"title\":\"Status\",\"type\":\"string\"},\"message\":{\"title\":\"Message\",\"type\":\"string\"}},\"required\":[\"status\",\"message\"],\"title\":\"StandardMessageResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"status\",\"meaning\":\"Original status in StandardMessageResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":276,\"validators\":[]},{\"name\":\"message\",\"meaning\":\"Original message in StandardMessageResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":277,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::AnnotationCreate\",\"name\":\"AnnotationCreate\",\"meaning\":\"Original AnnotationCreate model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"8103f23b82c47ddcb7d8803532f3604f04fcc251d1020319ed3f8d130fbfb81e\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"node_id\":{\"title\":\"Node Id\",\"type\":\"string\"},\"content\":{\"title\":\"Content\",\"type\":\"string\"},\"tags\":{\"items\":{\"type\":\"string\"},\"title\":\"Tags\",\"type\":\"array\"},\"visibility\":{\"default\":\"public\",\"title\":\"Visibility\",\"type\":\"string\"}},\"required\":[\"node_id\",\"content\"],\"title\":\"AnnotationCreate\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"node_id\",\"meaning\":\"Original node_id in AnnotationCreate\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":281,\"validators\":[]},{\"name\":\"content\",\"meaning\":\"Original content in AnnotationCreate\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":282,\"validators\":[]},{\"name\":\"tags\",\"meaning\":\"Original tags in AnnotationCreate\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":283,\"validators\":[]},{\"name\":\"visibility\",\"meaning\":\"Original visibility in AnnotationCreate\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"public\"},\"source_line\":284,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::AnnotationResponse\",\"name\":\"AnnotationResponse\",\"meaning\":\"Original AnnotationResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"a447946dcc5977e60e9f21d6ba145d298a3dc627fbf14b951f4af2f34c8bd2aa\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"annotation_id\":{\"title\":\"Annotation Id\",\"type\":\"string\"},\"node_id\":{\"title\":\"Node Id\",\"type\":\"string\"},\"content\":{\"title\":\"Content\",\"type\":\"string\"},\"tags\":{\"items\":{\"type\":\"string\"},\"title\":\"Tags\",\"type\":\"array\"},\"visibility\":{\"default\":\"public\",\"title\":\"Visibility\",\"type\":\"string\"},\"created_at\":{\"default\":\"\",\"title\":\"Created At\",\"type\":\"string\"}},\"required\":[\"annotation_id\",\"node_id\",\"content\"],\"title\":\"AnnotationResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"annotation_id\",\"meaning\":\"Original annotation_id in AnnotationResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":288,\"validators\":[]},{\"name\":\"node_id\",\"meaning\":\"Original node_id in AnnotationResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":289,\"validators\":[]},{\"name\":\"content\",\"meaning\":\"Original content in AnnotationResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":290,\"validators\":[]},{\"name\":\"tags\",\"meaning\":\"Original tags in AnnotationResponse\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":291,\"validators\":[]},{\"name\":\"visibility\",\"meaning\":\"Original visibility in AnnotationResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"public\"},\"source_line\":292,\"validators\":[]},{\"name\":\"created_at\",\"meaning\":\"Original created_at in AnnotationResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":293,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::VocabularyScheme\",\"name\":\"VocabularyScheme\",\"meaning\":\"Original VocabularyScheme model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"bf34f2fdfe76a8109d138365dc47ef738eb1822a8b351895401e97ebe35ed596\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"label\":{\"title\":\"Label\",\"type\":\"string\"},\"description\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Description\"}},\"required\":[\"uri\",\"label\"],\"title\":\"VocabularyScheme\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in VocabularyScheme\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":297,\"validators\":[]},{\"name\":\"label\",\"meaning\":\"Original label in VocabularyScheme\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":298,\"validators\":[]},{\"name\":\"description\",\"meaning\":\"Original description in VocabularyScheme\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":299,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ConceptSummary\",\"name\":\"ConceptSummary\",\"meaning\":\"Original ConceptSummary model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"014435618a6f113fd249023f8ac9a42de608ac9e7236d3bfae3393bba6a1668b\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"pref_label\":{\"title\":\"Pref Label\",\"type\":\"string\"},\"alt_labels\":{\"items\":{\"type\":\"string\"},\"title\":\"Alt Labels\",\"type\":\"array\"},\"description\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Description\"},\"notation\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Notation\"},\"scheme_uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Scheme Uri\"},\"parent_uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Parent Uri\"}},\"required\":[\"uri\",\"pref_label\"],\"title\":\"ConceptSummary\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in ConceptSummary\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":303,\"validators\":[]},{\"name\":\"pref_label\",\"meaning\":\"Original pref_label in ConceptSummary\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":304,\"validators\":[]},{\"name\":\"alt_labels\",\"meaning\":\"Original alt_labels in ConceptSummary\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":305,\"validators\":[]},{\"name\":\"description\",\"meaning\":\"Original description in ConceptSummary\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":306,\"validators\":[]},{\"name\":\"notation\",\"meaning\":\"Original notation in ConceptSummary\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":307,\"validators\":[]},{\"name\":\"scheme_uri\",\"meaning\":\"Original scheme_uri in ConceptSummary\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":308,\"validators\":[]},{\"name\":\"parent_uri\",\"meaning\":\"Original parent_uri in ConceptSummary\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":309,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ConceptNode\",\"name\":\"ConceptNode\",\"meaning\":\"Original ConceptNode model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"2703c24350f3918cf56fd6e28832b4c1cfcd633ec9fd6245cc9fe8632248a237\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"ConceptNode\":{\"properties\":{\"uri\":{\"title\":\"Uri\",\"type\":\"string\"},\"pref_label\":{\"title\":\"Pref Label\",\"type\":\"string\"},\"alt_labels\":{\"items\":{\"type\":\"string\"},\"title\":\"Alt Labels\",\"type\":\"array\"},\"description\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Description\"},\"notation\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Notation\"},\"scheme_uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Scheme Uri\"},\"parent_uri\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Parent Uri\"},\"children\":{\"anyOf\":[{\"items\":{\"$ref\":\"#/$defs/ConceptNode\"},\"type\":\"array\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Children\"}},\"required\":[\"uri\",\"pref_label\"],\"title\":\"ConceptNode\",\"type\":\"object\"}},\"$ref\":\"#/$defs/ConceptNode\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"uri\",\"meaning\":\"Original uri in ConceptNode\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":313,\"validators\":[]},{\"name\":\"pref_label\",\"meaning\":\"Original pref_label in ConceptNode\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":314,\"validators\":[]},{\"name\":\"alt_labels\",\"meaning\":\"Original alt_labels in ConceptNode\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":315,\"validators\":[]},{\"name\":\"description\",\"meaning\":\"Original description in ConceptNode\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":316,\"validators\":[]},{\"name\":\"notation\",\"meaning\":\"Original notation in ConceptNode\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":317,\"validators\":[]},{\"name\":\"scheme_uri\",\"meaning\":\"Original scheme_uri in ConceptNode\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":318,\"validators\":[]},{\"name\":\"parent_uri\",\"meaning\":\"Original parent_uri in ConceptNode\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":319,\"validators\":[]},{\"name\":\"children\",\"meaning\":\"Original children in ConceptNode\",\"annotation\":\"Optional[List[\\\"ConceptNode\\\"]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":320,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::VocabularyImportResponse\",\"name\":\"VocabularyImportResponse\",\"meaning\":\"Original VocabularyImportResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"753a59f0d3ac2cd193f87c359d8985c7c75416de179e8283a64c1cbca846463a\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"status\":{\"default\":\"success\",\"title\":\"Status\",\"type\":\"string\"},\"filename\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Filename\"},\"nodes_added\":{\"default\":0,\"title\":\"Nodes Added\",\"type\":\"integer\"},\"edges_added\":{\"default\":0,\"title\":\"Edges Added\",\"type\":\"integer\"},\"format\":{\"title\":\"Format\",\"type\":\"string\"}},\"required\":[\"format\"],\"title\":\"VocabularyImportResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"status\",\"meaning\":\"Original status in VocabularyImportResponse\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"success\"},\"source_line\":324,\"validators\":[]},{\"name\":\"filename\",\"meaning\":\"Original filename in VocabularyImportResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":325,\"validators\":[]},{\"name\":\"nodes_added\",\"meaning\":\"Original nodes_added in VocabularyImportResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":326,\"validators\":[]},{\"name\":\"edges_added\",\"meaning\":\"Original edges_added in VocabularyImportResponse\",\"annotation\":\"int\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":327,\"validators\":[]},{\"name\":\"format\",\"meaning\":\"Original format in VocabularyImportResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":328,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::MergeRequest\",\"name\":\"MergeRequest\",\"meaning\":\"Original MergeRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"a38364687bebe87351c1bc81d40a7e70c6bfbf443aad6be92c19b55d4424143e\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"primary_id\":{\"title\":\"Primary Id\",\"type\":\"string\"},\"duplicate_ids\":{\"items\":{\"type\":\"string\"},\"title\":\"Duplicate Ids\",\"type\":\"array\"}},\"required\":[\"primary_id\",\"duplicate_ids\"],\"title\":\"MergeRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"primary_id\",\"meaning\":\"Original primary_id in MergeRequest\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":332,\"validators\":[]},{\"name\":\"duplicate_ids\",\"meaning\":\"Original duplicate_ids in MergeRequest\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":333,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::MergeResponse\",\"name\":\"MergeResponse\",\"meaning\":\"Original MergeResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"375906d419914970ee1722debca1de0e98f3658929cad5c3647534d517d4c072\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"merged_into\":{\"title\":\"Merged Into\",\"type\":\"string\"},\"removed_ids\":{\"items\":{\"type\":\"string\"},\"title\":\"Removed Ids\",\"type\":\"array\"},\"edges_updated\":{\"title\":\"Edges Updated\",\"type\":\"integer\"}},\"required\":[\"merged_into\",\"removed_ids\",\"edges_updated\"],\"title\":\"MergeResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"merged_into\",\"meaning\":\"Original merged_into in MergeResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":337,\"validators\":[]},{\"name\":\"removed_ids\",\"meaning\":\"Original removed_ids in MergeResponse\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":338,\"validators\":[]},{\"name\":\"edges_updated\",\"meaning\":\"Original edges_updated in MergeResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":339,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ProvenanceNode\",\"name\":\"ProvenanceNode\",\"meaning\":\"Original ProvenanceNode model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"d8dad363726ff0d6ba28bef812a7a0ab4379fc833718194e8bc3687f969fb604\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"label\":{\"title\":\"Label\",\"type\":\"string\"},\"prov_type\":{\"title\":\"Prov Type\",\"type\":\"string\"},\"parent_id\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Parent Id\"},\"source_document\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Document\"},\"source_location\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Location\"},\"source_quote\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Quote\"},\"confidence\":{\"anyOf\":[{\"type\":\"number\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Confidence\"},\"checksum\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Checksum\"}},\"required\":[\"id\",\"label\",\"prov_type\"],\"title\":\"ProvenanceNode\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"id\",\"meaning\":\"Original id in ProvenanceNode\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":343,\"validators\":[]},{\"name\":\"label\",\"meaning\":\"Original label in ProvenanceNode\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":344,\"validators\":[]},{\"name\":\"prov_type\",\"meaning\":\"Original prov_type in ProvenanceNode\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":345,\"validators\":[]},{\"name\":\"parent_id\",\"meaning\":\"Original parent_id in ProvenanceNode\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":346,\"validators\":[]},{\"name\":\"source_document\",\"meaning\":\"Original source_document in ProvenanceNode\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":347,\"validators\":[]},{\"name\":\"source_location\",\"meaning\":\"Original source_location in ProvenanceNode\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":348,\"validators\":[]},{\"name\":\"source_quote\",\"meaning\":\"Original source_quote in ProvenanceNode\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":349,\"validators\":[]},{\"name\":\"confidence\",\"meaning\":\"Original confidence in ProvenanceNode\",\"annotation\":\"Optional[float]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":350,\"validators\":[]},{\"name\":\"checksum\",\"meaning\":\"Original checksum in ProvenanceNode\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":351,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ProvenanceEdge\",\"name\":\"ProvenanceEdge\",\"meaning\":\"Original ProvenanceEdge model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"f916c8fea8e09ed76397bc99f8488e7558f1f70880aba710931681977ba7ac17\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"source\":{\"title\":\"Source\",\"type\":\"string\"},\"target\":{\"title\":\"Target\",\"type\":\"string\"},\"label\":{\"title\":\"Label\",\"type\":\"string\"},\"direction\":{\"title\":\"Direction\",\"type\":\"string\"}},\"required\":[\"id\",\"source\",\"target\",\"label\",\"direction\"],\"title\":\"ProvenanceEdge\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"id\",\"meaning\":\"Original id in ProvenanceEdge\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":355,\"validators\":[]},{\"name\":\"source\",\"meaning\":\"Original source in ProvenanceEdge\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":356,\"validators\":[]},{\"name\":\"target\",\"meaning\":\"Original target in ProvenanceEdge\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":357,\"validators\":[]},{\"name\":\"label\",\"meaning\":\"Original label in ProvenanceEdge\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":358,\"validators\":[]},{\"name\":\"direction\",\"meaning\":\"Original direction in ProvenanceEdge\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":359,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::ProvenanceResponse\",\"name\":\"ProvenanceResponse\",\"meaning\":\"Original ProvenanceResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"985eaa552b5e789e664f444c41adb20b30aa4fe3570098f613261d2f2cee2b23\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"ProvenanceEdge\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"source\":{\"title\":\"Source\",\"type\":\"string\"},\"target\":{\"title\":\"Target\",\"type\":\"string\"},\"label\":{\"title\":\"Label\",\"type\":\"string\"},\"direction\":{\"title\":\"Direction\",\"type\":\"string\"}},\"required\":[\"id\",\"source\",\"target\",\"label\",\"direction\"],\"title\":\"ProvenanceEdge\",\"type\":\"object\"},\"ProvenanceNode\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"label\":{\"title\":\"Label\",\"type\":\"string\"},\"prov_type\":{\"title\":\"Prov Type\",\"type\":\"string\"},\"parent_id\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Parent Id\"},\"source_document\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Document\"},\"source_location\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Location\"},\"source_quote\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source Quote\"},\"confidence\":{\"anyOf\":[{\"type\":\"number\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Confidence\"},\"checksum\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Checksum\"}},\"required\":[\"id\",\"label\",\"prov_type\"],\"title\":\"ProvenanceNode\",\"type\":\"object\"}},\"properties\":{\"nodes\":{\"items\":{\"$ref\":\"#/$defs/ProvenanceNode\"},\"title\":\"Nodes\",\"type\":\"array\"},\"edges\":{\"items\":{\"$ref\":\"#/$defs/ProvenanceEdge\"},\"title\":\"Edges\",\"type\":\"array\"},\"source\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Source\"}},\"required\":[\"nodes\",\"edges\"],\"title\":\"ProvenanceResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"nodes\",\"meaning\":\"Original nodes in ProvenanceResponse\",\"annotation\":\"List[ProvenanceNode]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":363,\"validators\":[]},{\"name\":\"edges\",\"meaning\":\"Original edges in ProvenanceResponse\",\"annotation\":\"List[ProvenanceEdge]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":364,\"validators\":[]},{\"name\":\"source\",\"meaning\":\"Original source in ProvenanceResponse\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":365,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::DistanceMatrixRequest\",\"name\":\"DistanceMatrixRequest\",\"meaning\":\"Original DistanceMatrixRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"40f3b2ad0b7fc1d0c356ddec8b930f903237bb9d401affec1228ab6fecd8c574\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"node_ids\":{\"items\":{\"type\":\"string\"},\"title\":\"Node Ids\",\"type\":\"array\"},\"metric\":{\"default\":\"hops\",\"enum\":[\"hops\",\"weighted\",\"semantic\"],\"title\":\"Metric\",\"type\":\"string\"}},\"required\":[\"node_ids\"],\"title\":\"DistanceMatrixRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"node_ids\",\"meaning\":\"Original node_ids in DistanceMatrixRequest\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":373,\"validators\":[]},{\"name\":\"metric\",\"meaning\":\"Original metric in DistanceMatrixRequest\",\"annotation\":\"Literal[\\\"hops\\\", \\\"weighted\\\", \\\"semantic\\\"]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"hops\"},\"source_line\":374,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::DistanceMatrixResponse\",\"name\":\"DistanceMatrixResponse\",\"meaning\":\"Original DistanceMatrixResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"657d5c9540f55289c9ad5bff39442b4d9616a9cf4f5a389f06682ea150a1700b\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"nodes\":{\"items\":{\"type\":\"string\"},\"title\":\"Nodes\",\"type\":\"array\"},\"metric\":{\"title\":\"Metric\",\"type\":\"string\"},\"matrix\":{\"items\":{\"items\":{\"anyOf\":[{\"type\":\"number\"},{\"type\":\"null\"}]},\"type\":\"array\"},\"title\":\"Matrix\",\"type\":\"array\"},\"unreachable_pairs\":{\"items\":{\"maxItems\":2,\"minItems\":2,\"prefixItems\":[{\"type\":\"string\"},{\"type\":\"string\"}],\"type\":\"array\"},\"title\":\"Unreachable Pairs\",\"type\":\"array\"},\"computation_time_ms\":{\"title\":\"Computation Time Ms\",\"type\":\"number\"}},\"required\":[\"nodes\",\"metric\",\"matrix\",\"computation_time_ms\"],\"title\":\"DistanceMatrixResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"nodes\",\"meaning\":\"Original nodes in DistanceMatrixResponse\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":378,\"validators\":[]},{\"name\":\"metric\",\"meaning\":\"Original metric in DistanceMatrixResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":379,\"validators\":[]},{\"name\":\"matrix\",\"meaning\":\"Original matrix in DistanceMatrixResponse\",\"annotation\":\"List[List[Optional[float]]]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":380,\"validators\":[]},{\"name\":\"unreachable_pairs\",\"meaning\":\"Original unreachable_pairs in DistanceMatrixResponse\",\"annotation\":\"List[Tuple[str, str]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":381,\"validators\":[]},{\"name\":\"computation_time_ms\",\"meaning\":\"Original computation_time_ms in DistanceMatrixResponse\",\"annotation\":\"float\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":382,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::SemanticNeighborItem\",\"name\":\"SemanticNeighborItem\",\"meaning\":\"Original SemanticNeighborItem model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"c3458a25e654ee4ead17c01f9b69e872322c391a6cde44f626b02c424b3f3646\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"content\":{\"default\":\"\",\"title\":\"Content\",\"type\":\"string\"},\"similarity\":{\"title\":\"Similarity\",\"type\":\"number\"},\"hop_distance\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Hop Distance\"}},\"required\":[\"id\",\"type\",\"similarity\"],\"title\":\"SemanticNeighborItem\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"id\",\"meaning\":\"Original id in SemanticNeighborItem\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":390,\"validators\":[]},{\"name\":\"type\",\"meaning\":\"Original type in SemanticNeighborItem\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":391,\"validators\":[]},{\"name\":\"content\",\"meaning\":\"Original content in SemanticNeighborItem\",\"annotation\":\"str\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"\"},\"source_line\":392,\"validators\":[]},{\"name\":\"similarity\",\"meaning\":\"Original similarity in SemanticNeighborItem\",\"annotation\":\"float\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":393,\"validators\":[]},{\"name\":\"hop_distance\",\"meaning\":\"Original hop_distance in SemanticNeighborItem\",\"annotation\":\"Optional[int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":394,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::SemanticNeighborhoodResponse\",\"name\":\"SemanticNeighborhoodResponse\",\"meaning\":\"Original SemanticNeighborhoodResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"e1f370422324446e7b8f8ad273cd4a53f13732d438f9a36b0f1864e2d99d776e\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"SemanticNeighborItem\":{\"properties\":{\"id\":{\"title\":\"Id\",\"type\":\"string\"},\"type\":{\"title\":\"Type\",\"type\":\"string\"},\"content\":{\"default\":\"\",\"title\":\"Content\",\"type\":\"string\"},\"similarity\":{\"title\":\"Similarity\",\"type\":\"number\"},\"hop_distance\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Hop Distance\"}},\"required\":[\"id\",\"type\",\"similarity\"],\"title\":\"SemanticNeighborItem\",\"type\":\"object\"}},\"properties\":{\"anchor_node\":{\"title\":\"Anchor Node\",\"type\":\"string\"},\"neighbors\":{\"items\":{\"$ref\":\"#/$defs/SemanticNeighborItem\"},\"title\":\"Neighbors\",\"type\":\"array\"},\"total\":{\"title\":\"Total\",\"type\":\"integer\"}},\"required\":[\"anchor_node\",\"neighbors\",\"total\"],\"title\":\"SemanticNeighborhoodResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"anchor_node\",\"meaning\":\"Original anchor_node in SemanticNeighborhoodResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":398,\"validators\":[]},{\"name\":\"neighbors\",\"meaning\":\"Original neighbors in SemanticNeighborhoodResponse\",\"annotation\":\"List[SemanticNeighborItem]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":399,\"validators\":[]},{\"name\":\"total\",\"meaning\":\"Original total in SemanticNeighborhoodResponse\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":400,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::CausalDistanceReport\",\"name\":\"CausalDistanceReport\",\"meaning\":\"Original CausalDistanceReport model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"6a24191722879d9aaade07300bb9dc5b6f8b30c4b849fc5d4e5984fc496600c1\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"source_id\":{\"title\":\"Source Id\",\"type\":\"string\"},\"target_id\":{\"title\":\"Target Id\",\"type\":\"string\"},\"causal_path\":{\"items\":{\"type\":\"string\"},\"title\":\"Causal Path\",\"type\":\"array\"},\"causal_hop_count\":{\"title\":\"Causal Hop Count\",\"type\":\"integer\"},\"intermediate_decisions\":{\"items\":{\"type\":\"string\"},\"title\":\"Intermediate Decisions\",\"type\":\"array\"},\"confidence_decay\":{\"title\":\"Confidence Decay\",\"type\":\"number\"},\"weakest_link\":{\"anyOf\":[{\"additionalProperties\":true,\"type\":\"object\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Weakest Link\"},\"interpretation\":{\"title\":\"Interpretation\",\"type\":\"string\"}},\"required\":[\"source_id\",\"target_id\",\"causal_path\",\"causal_hop_count\",\"intermediate_decisions\",\"confidence_decay\",\"interpretation\"],\"title\":\"CausalDistanceReport\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"source_id\",\"meaning\":\"Original source_id in CausalDistanceReport\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":408,\"validators\":[]},{\"name\":\"target_id\",\"meaning\":\"Original target_id in CausalDistanceReport\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":409,\"validators\":[]},{\"name\":\"causal_path\",\"meaning\":\"Original causal_path in CausalDistanceReport\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":410,\"validators\":[]},{\"name\":\"causal_hop_count\",\"meaning\":\"Original causal_hop_count in CausalDistanceReport\",\"annotation\":\"int\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":411,\"validators\":[]},{\"name\":\"intermediate_decisions\",\"meaning\":\"Original intermediate_decisions in CausalDistanceReport\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":412,\"validators\":[]},{\"name\":\"confidence_decay\",\"meaning\":\"Original confidence_decay in CausalDistanceReport\",\"annotation\":\"float\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":413,\"validators\":[]},{\"name\":\"weakest_link\",\"meaning\":\"Original weakest_link in CausalDistanceReport\",\"annotation\":\"Optional[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":414,\"validators\":[]},{\"name\":\"interpretation\",\"meaning\":\"Original interpretation in CausalDistanceReport\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":415,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::DistanceSnapshot\",\"name\":\"DistanceSnapshot\",\"meaning\":\"Original DistanceSnapshot model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"0c0b008c358d63bf3dd879b41318bb2e032b42d3b7200d9a6be008f70bedaf3b\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"timestamp\":{\"format\":\"date-time\",\"title\":\"Timestamp\",\"type\":\"string\"},\"hop_count\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Hop Count\"},\"distance_band\":{\"title\":\"Distance Band\",\"type\":\"string\"}},\"required\":[\"timestamp\",\"distance_band\"],\"title\":\"DistanceSnapshot\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"timestamp\",\"meaning\":\"Original timestamp in DistanceSnapshot\",\"annotation\":\"datetime\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":423,\"validators\":[]},{\"name\":\"hop_count\",\"meaning\":\"Original hop_count in DistanceSnapshot\",\"annotation\":\"Optional[int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":424,\"validators\":[]},{\"name\":\"distance_band\",\"meaning\":\"Original distance_band in DistanceSnapshot\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":425,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::DistanceEvent\",\"name\":\"DistanceEvent\",\"meaning\":\"Original DistanceEvent model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"e01a3a0694fd2666754764afcd6e24902d5ea0d897014fdd0d7553c43eb510eb\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"timestamp\":{\"format\":\"date-time\",\"title\":\"Timestamp\",\"type\":\"string\"},\"event_type\":{\"enum\":[\"convergence\",\"divergence\",\"disconnected\",\"reconnected\"],\"title\":\"Event Type\",\"type\":\"string\"},\"hop_count_before\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Hop Count Before\"},\"hop_count_after\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Hop Count After\"},\"description\":{\"title\":\"Description\",\"type\":\"string\"}},\"required\":[\"timestamp\",\"event_type\",\"description\"],\"title\":\"DistanceEvent\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"timestamp\",\"meaning\":\"Original timestamp in DistanceEvent\",\"annotation\":\"datetime\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":429,\"validators\":[]},{\"name\":\"event_type\",\"meaning\":\"Original event_type in DistanceEvent\",\"annotation\":\"Literal[\\\"convergence\\\", \\\"divergence\\\", \\\"disconnected\\\", \\\"reconnected\\\"]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":430,\"validators\":[]},{\"name\":\"hop_count_before\",\"meaning\":\"Original hop_count_before in DistanceEvent\",\"annotation\":\"Optional[int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":431,\"validators\":[]},{\"name\":\"hop_count_after\",\"meaning\":\"Original hop_count_after in DistanceEvent\",\"annotation\":\"Optional[int]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":432,\"validators\":[]},{\"name\":\"description\",\"meaning\":\"Original description in DistanceEvent\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":433,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::DistanceHistoryResponse\",\"name\":\"DistanceHistoryResponse\",\"meaning\":\"Original DistanceHistoryResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"f4c08a0b53d9cced7e58312550d51bc3f5f2dfe76f58e0ac6087d4a31c71f414\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"DistanceEvent\":{\"properties\":{\"timestamp\":{\"format\":\"date-time\",\"title\":\"Timestamp\",\"type\":\"string\"},\"event_type\":{\"enum\":[\"convergence\",\"divergence\",\"disconnected\",\"reconnected\"],\"title\":\"Event Type\",\"type\":\"string\"},\"hop_count_before\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Hop Count Before\"},\"hop_count_after\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Hop Count After\"},\"description\":{\"title\":\"Description\",\"type\":\"string\"}},\"required\":[\"timestamp\",\"event_type\",\"description\"],\"title\":\"DistanceEvent\",\"type\":\"object\"},\"DistanceSnapshot\":{\"properties\":{\"timestamp\":{\"format\":\"date-time\",\"title\":\"Timestamp\",\"type\":\"string\"},\"hop_count\":{\"anyOf\":[{\"type\":\"integer\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Hop Count\"},\"distance_band\":{\"title\":\"Distance Band\",\"type\":\"string\"}},\"required\":[\"timestamp\",\"distance_band\"],\"title\":\"DistanceSnapshot\",\"type\":\"object\"}},\"properties\":{\"source_id\":{\"title\":\"Source Id\",\"type\":\"string\"},\"target_id\":{\"title\":\"Target Id\",\"type\":\"string\"},\"metric\":{\"title\":\"Metric\",\"type\":\"string\"},\"history\":{\"items\":{\"$ref\":\"#/$defs/DistanceSnapshot\"},\"title\":\"History\",\"type\":\"array\"},\"events\":{\"items\":{\"$ref\":\"#/$defs/DistanceEvent\"},\"title\":\"Events\",\"type\":\"array\"}},\"required\":[\"source_id\",\"target_id\",\"metric\",\"history\",\"events\"],\"title\":\"DistanceHistoryResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"source_id\",\"meaning\":\"Original source_id in DistanceHistoryResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":437,\"validators\":[]},{\"name\":\"target_id\",\"meaning\":\"Original target_id in DistanceHistoryResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":438,\"validators\":[]},{\"name\":\"metric\",\"meaning\":\"Original metric in DistanceHistoryResponse\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":439,\"validators\":[]},{\"name\":\"history\",\"meaning\":\"Original history in DistanceHistoryResponse\",\"annotation\":\"List[DistanceSnapshot]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":440,\"validators\":[]},{\"name\":\"events\",\"meaning\":\"Original events in DistanceHistoryResponse\",\"annotation\":\"List[DistanceEvent]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":441,\"validators\":[]}]},{\"id\":\"semantica.explorer.schemas::DistanceExportRequest\",\"name\":\"DistanceExportRequest\",\"meaning\":\"Original DistanceExportRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/explorer/schemas.py\",\"source_sha256\":\"37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237\",\"class_sha256\":\"b58721e65d85733cda45b7ee3125adfc510387e98991178bbd5ab95bfb033e23\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"format\":{\"default\":\"csv\",\"enum\":[\"csv\",\"jsonl\"],\"title\":\"Format\",\"type\":\"string\"},\"node_subset\":{\"anyOf\":[{\"items\":{\"type\":\"string\"},\"type\":\"array\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Node Subset\"},\"include\":{\"items\":{\"type\":\"string\"},\"title\":\"Include\",\"type\":\"array\"}},\"title\":\"DistanceExportRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"format\",\"meaning\":\"Original format in DistanceExportRequest\",\"annotation\":\"Literal[\\\"csv\\\", \\\"jsonl\\\"]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":\"csv\"},\"source_line\":449,\"validators\":[]},{\"name\":\"node_subset\",\"meaning\":\"Original node_subset in DistanceExportRequest\",\"annotation\":\"Optional[List[str]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":450,\"validators\":[]},{\"name\":\"include\",\"meaning\":\"Original include in DistanceExportRequest\",\"annotation\":\"List[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[\"source_id\",\"target_id\",\"hop_count\",\"distance_band\"]},\"source_line\":451,\"validators\":[]}]},{\"id\":\"semantica.semantic_extract.schemas::EntityOut\",\"name\":\"EntityOut\",\"meaning\":\"Original EntityOut model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/semantic_extract/schemas.py\",\"source_sha256\":\"deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d\",\"class_sha256\":\"a01e96bbfc9edc9f121eb5619f9f837db69f1a82061dd3a2b0ce95fa9161fd5c\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"description\":\"Canonical schema for entity extraction output.\",\"properties\":{\"text\":{\"description\":\"The text content of the entity\",\"title\":\"Text\",\"type\":\"string\"},\"label\":{\"description\":\"The type or label of the entity (e.g., PERSON, ORG)\",\"title\":\"Label\",\"type\":\"string\"},\"start_char\":{\"default\":0,\"description\":\"Start character index\",\"title\":\"Start Char\",\"type\":\"integer\"},\"end_char\":{\"default\":0,\"description\":\"End character index\",\"title\":\"End Char\",\"type\":\"integer\"},\"confidence\":{\"default\":0.9,\"description\":\"Confidence score between 0 and 1\",\"title\":\"Confidence\",\"type\":\"number\"},\"metadata\":{\"additionalProperties\":true,\"description\":\"Additional metadata including provenance\",\"title\":\"Metadata\",\"type\":\"object\"}},\"required\":[\"text\",\"label\"],\"title\":\"EntityOut\",\"type\":\"object\"},\"config\":{\"populate_by_name\":true,\"extra\":\"ignore\",\"validate_by_alias\":true,\"validate_by_name\":true},\"validators\":[\"aliases_before\"],\"aliases\":{\"label\":[\"type\"],\"text\":[\"value\",\"span\"]},\"properties\":{},\"fields\":[{\"name\":\"text\",\"meaning\":\"The text content of the entity\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":8,\"validators\":[\"python_string_strip\"]},{\"name\":\"label\",\"meaning\":\"The type or label of the entity (e.g., PERSON, ORG)\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":9,\"validators\":[]},{\"name\":\"start\",\"meaning\":\"Start character index\",\"annotation\":\"int\",\"required\":false,\"alias\":\"start_char\",\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":10,\"validators\":[]},{\"name\":\"end\",\"meaning\":\"End character index\",\"annotation\":\"int\",\"required\":false,\"alias\":\"end_char\",\"default\":{\"kind\":\"value\",\"value\":0},\"source_line\":11,\"validators\":[]},{\"name\":\"confidence\",\"meaning\":\"Confidence score between 0 and 1\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.9},\"source_line\":12,\"validators\":[\"confidence_09\"]},{\"name\":\"metadata\",\"meaning\":\"Additional metadata including provenance\",\"annotation\":\"dict\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":13,\"validators\":[]}]},{\"id\":\"semantica.semantic_extract.schemas::RelationOut\",\"name\":\"RelationOut\",\"meaning\":\"Original RelationOut model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/semantic_extract/schemas.py\",\"source_sha256\":\"deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d\",\"class_sha256\":\"cc884973447ccf6739aacd2f858bd7add8dc416191b8452de00329b728a70787\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"description\":\"Canonical schema for relation extraction output.\",\"properties\":{\"subject\":{\"description\":\"Source entity text\",\"title\":\"Subject\",\"type\":\"string\"},\"object\":{\"description\":\"Target entity text\",\"title\":\"Object\",\"type\":\"string\"},\"predicate\":{\"description\":\"Relation type or predicate\",\"title\":\"Predicate\",\"type\":\"string\"},\"confidence\":{\"default\":0.9,\"description\":\"Confidence score between 0 and 1\",\"title\":\"Confidence\",\"type\":\"number\"},\"metadata\":{\"additionalProperties\":true,\"description\":\"Additional metadata including provenance\",\"title\":\"Metadata\",\"type\":\"object\"}},\"required\":[\"subject\",\"object\",\"predicate\"],\"title\":\"RelationOut\",\"type\":\"object\"},\"config\":{\"populate_by_name\":true,\"extra\":\"ignore\",\"validate_by_alias\":true,\"validate_by_name\":true},\"validators\":[\"aliases_before\"],\"aliases\":{\"subject\":[\"source\"],\"object\":[\"target\"],\"predicate\":[\"label\"]},\"properties\":{\"source\":\"subject\",\"target\":\"object\",\"label\":\"predicate\"},\"fields\":[{\"name\":\"subject\",\"meaning\":\"Source entity text\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":53,\"validators\":[]},{\"name\":\"object\",\"meaning\":\"Target entity text\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":54,\"validators\":[]},{\"name\":\"predicate\",\"meaning\":\"Relation type or predicate\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":55,\"validators\":[]},{\"name\":\"confidence\",\"meaning\":\"Confidence score between 0 and 1\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.9},\"source_line\":56,\"validators\":[\"confidence_09\"]},{\"name\":\"metadata\",\"meaning\":\"Additional metadata including provenance\",\"annotation\":\"dict\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":57,\"validators\":[]}]},{\"id\":\"semantica.semantic_extract.schemas::TripletOut\",\"name\":\"TripletOut\",\"meaning\":\"Original TripletOut model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/semantic_extract/schemas.py\",\"source_sha256\":\"deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d\",\"class_sha256\":\"a31ab3bdd2cb623f9082b782f287ccd3f954a893a97da5585ea22382c995e845\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"description\":\"Canonical schema for triplet extraction output.\",\"properties\":{\"subject\":{\"description\":\"Subject of the triplet\",\"title\":\"Subject\",\"type\":\"string\"},\"predicate\":{\"description\":\"Predicate or relation\",\"title\":\"Predicate\",\"type\":\"string\"},\"object\":{\"description\":\"Object of the triplet\",\"title\":\"Object\",\"type\":\"string\"},\"confidence\":{\"default\":0.9,\"description\":\"Confidence score between 0 and 1\",\"title\":\"Confidence\",\"type\":\"number\"},\"metadata\":{\"additionalProperties\":true,\"description\":\"Additional metadata including provenance\",\"title\":\"Metadata\",\"type\":\"object\"}},\"required\":[\"subject\",\"predicate\",\"object\"],\"title\":\"TripletOut\",\"type\":\"object\"},\"config\":{\"populate_by_name\":true,\"extra\":\"ignore\",\"validate_by_alias\":true,\"validate_by_name\":true},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"subject\",\"meaning\":\"Subject of the triplet\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":99,\"validators\":[]},{\"name\":\"predicate\",\"meaning\":\"Predicate or relation\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":100,\"validators\":[]},{\"name\":\"object\",\"meaning\":\"Object of the triplet\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":101,\"validators\":[]},{\"name\":\"confidence\",\"meaning\":\"Confidence score between 0 and 1\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.9},\"source_line\":102,\"validators\":[\"confidence_09\"]},{\"name\":\"metadata\",\"meaning\":\"Additional metadata including provenance\",\"annotation\":\"dict\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":103,\"validators\":[]}]},{\"id\":\"semantica.semantic_extract.schemas::EntitiesResponse\",\"name\":\"EntitiesResponse\",\"meaning\":\"Original EntitiesResponse model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/semantic_extract/schemas.py\",\"source_sha256\":\"deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d\",\"class_sha256\":\"f9b09770b854b30e6fca15eda5356b987436e506abd8892302bcef9420a087d5\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"EntityOut\":{\"description\":\"Canonical schema for entity extraction output.\",\"properties\":{\"text\":{\"description\":\"The text content of the entity\",\"title\":\"Text\",\"type\":\"string\"},\"label\":{\"description\":\"The type or label of the entity (e.g., PERSON, ORG)\",\"title\":\"Label\",\"type\":\"string\"},\"start_char\":{\"default\":0,\"description\":\"Start character index\",\"title\":\"Start Char\",\"type\":\"integer\"},\"end_char\":{\"default\":0,\"description\":\"End character index\",\"title\":\"End Char\",\"type\":\"integer\"},\"confidence\":{\"default\":0.9,\"description\":\"Confidence score between 0 and 1\",\"title\":\"Confidence\",\"type\":\"number\"},\"metadata\":{\"additionalProperties\":true,\"description\":\"Additional metadata including provenance\",\"title\":\"Metadata\",\"type\":\"object\"}},\"required\":[\"text\",\"label\"],\"title\":\"EntityOut\",\"type\":\"object\"}},\"description\":\"Wrapper for list of entities.\",\"properties\":{\"entities\":{\"items\":{\"$ref\":\"#/$defs/EntityOut\"},\"title\":\"Entities\",\"type\":\"array\"}},\"title\":\"EntitiesResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"entities\",\"meaning\":\"Original entities in EntitiesResponse\",\"annotation\":\"List[EntityOut]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":119,\"validators\":[]}]},{\"id\":\"semantica.semantic_extract.schemas::RelationsResponse\",\"name\":\"RelationsResponse\",\"meaning\":\"Original RelationsResponse model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/semantic_extract/schemas.py\",\"source_sha256\":\"deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d\",\"class_sha256\":\"d13b38b4692f3fe0a1f9579b74c8edc6ab0204769fbdc5b569c506bd916916fe\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"RelationOut\":{\"description\":\"Canonical schema for relation extraction output.\",\"properties\":{\"subject\":{\"description\":\"Source entity text\",\"title\":\"Subject\",\"type\":\"string\"},\"object\":{\"description\":\"Target entity text\",\"title\":\"Object\",\"type\":\"string\"},\"predicate\":{\"description\":\"Relation type or predicate\",\"title\":\"Predicate\",\"type\":\"string\"},\"confidence\":{\"default\":0.9,\"description\":\"Confidence score between 0 and 1\",\"title\":\"Confidence\",\"type\":\"number\"},\"metadata\":{\"additionalProperties\":true,\"description\":\"Additional metadata including provenance\",\"title\":\"Metadata\",\"type\":\"object\"}},\"required\":[\"subject\",\"object\",\"predicate\"],\"title\":\"RelationOut\",\"type\":\"object\"}},\"description\":\"Wrapper for list of relations.\",\"properties\":{\"relations\":{\"items\":{\"$ref\":\"#/$defs/RelationOut\"},\"title\":\"Relations\",\"type\":\"array\"}},\"title\":\"RelationsResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"relations\",\"meaning\":\"Original relations in RelationsResponse\",\"annotation\":\"List[RelationOut]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":123,\"validators\":[]}]},{\"id\":\"semantica.semantic_extract.schemas::TripletsResponse\",\"name\":\"TripletsResponse\",\"meaning\":\"Original TripletsResponse model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/semantic_extract/schemas.py\",\"source_sha256\":\"deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d\",\"class_sha256\":\"839afdbceed0cee360449fde8de6ab92cf803045d9c1ba622115ab05b18b7df3\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"TripletOut\":{\"description\":\"Canonical schema for triplet extraction output.\",\"properties\":{\"subject\":{\"description\":\"Subject of the triplet\",\"title\":\"Subject\",\"type\":\"string\"},\"predicate\":{\"description\":\"Predicate or relation\",\"title\":\"Predicate\",\"type\":\"string\"},\"object\":{\"description\":\"Object of the triplet\",\"title\":\"Object\",\"type\":\"string\"},\"confidence\":{\"default\":0.9,\"description\":\"Confidence score between 0 and 1\",\"title\":\"Confidence\",\"type\":\"number\"},\"metadata\":{\"additionalProperties\":true,\"description\":\"Additional metadata including provenance\",\"title\":\"Metadata\",\"type\":\"object\"}},\"required\":[\"subject\",\"predicate\",\"object\"],\"title\":\"TripletOut\",\"type\":\"object\"}},\"description\":\"Wrapper for list of triplets.\",\"properties\":{\"triplets\":{\"items\":{\"$ref\":\"#/$defs/TripletOut\"},\"title\":\"Triplets\",\"type\":\"array\"}},\"title\":\"TripletsResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"triplets\",\"meaning\":\"Original triplets in TripletsResponse\",\"annotation\":\"List[TripletOut]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":127,\"validators\":[]}]},{\"id\":\"semantica.semantic_extract.schemas::RelationWithTemporalOut\",\"name\":\"RelationWithTemporalOut\",\"meaning\":\"Original RelationWithTemporalOut model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/semantic_extract/schemas.py\",\"source_sha256\":\"deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d\",\"class_sha256\":\"6c35112e8aa1fececd792485969e7d0d770f4e1848114288b6b424817b1efb7e\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"description\":\"Schema for relation extraction output with temporal validity bounds.\",\"properties\":{\"subject\":{\"description\":\"Source entity text\",\"title\":\"Subject\",\"type\":\"string\"},\"object\":{\"description\":\"Target entity text\",\"title\":\"Object\",\"type\":\"string\"},\"predicate\":{\"description\":\"Relation type or predicate\",\"title\":\"Predicate\",\"type\":\"string\"},\"confidence\":{\"default\":0.9,\"description\":\"Confidence score between 0 and 1\",\"title\":\"Confidence\",\"type\":\"number\"},\"metadata\":{\"additionalProperties\":true,\"description\":\"Additional metadata including provenance\",\"title\":\"Metadata\",\"type\":\"object\"},\"valid_from\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"ISO 8601 date or natural-language phrase for when this relation became valid. Null if no temporal signal in text.\",\"title\":\"Valid From\"},\"valid_until\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"ISO 8601 date or phrase for when this relation ceased to be valid. Null if open-ended or not stated.\",\"title\":\"Valid Until\"},\"temporal_confidence\":{\"default\":0.0,\"description\":\"Confidence that temporal information was present and correctly extracted. 0.0 if no temporal signal.\",\"title\":\"Temporal Confidence\",\"type\":\"number\"},\"temporal_source_text\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"Exact verbatim substring from the source text containing the temporal signal. Null when temporal_confidence is 0.0.\",\"title\":\"Temporal Source Text\"}},\"required\":[\"subject\",\"object\",\"predicate\"],\"title\":\"RelationWithTemporalOut\",\"type\":\"object\"},\"config\":{\"populate_by_name\":true,\"extra\":\"ignore\",\"validate_by_alias\":true,\"validate_by_name\":true},\"validators\":[\"aliases_before\"],\"aliases\":{\"subject\":[\"source\"],\"object\":[\"target\"],\"predicate\":[\"label\"]},\"properties\":{},\"fields\":[{\"name\":\"subject\",\"meaning\":\"Source entity text\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":134,\"validators\":[]},{\"name\":\"object\",\"meaning\":\"Target entity text\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":135,\"validators\":[]},{\"name\":\"predicate\",\"meaning\":\"Relation type or predicate\",\"annotation\":\"str\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":136,\"validators\":[]},{\"name\":\"confidence\",\"meaning\":\"Confidence score between 0 and 1\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.9},\"source_line\":137,\"validators\":[\"confidence_0\"]},{\"name\":\"metadata\",\"meaning\":\"Additional metadata including provenance\",\"annotation\":\"dict\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":{}},\"source_line\":138,\"validators\":[]},{\"name\":\"valid_from\",\"meaning\":\"ISO 8601 date or natural-language phrase for when this relation became valid. Null if no temporal signal in text.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":140,\"validators\":[]},{\"name\":\"valid_until\",\"meaning\":\"ISO 8601 date or phrase for when this relation ceased to be valid. Null if open-ended or not stated.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":144,\"validators\":[]},{\"name\":\"temporal_confidence\",\"meaning\":\"Confidence that temporal information was present and correctly extracted. 0.0 if no temporal signal.\",\"annotation\":\"float\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":0.0},\"source_line\":148,\"validators\":[\"confidence_0\"]},{\"name\":\"temporal_source_text\",\"meaning\":\"Exact verbatim substring from the source text containing the temporal signal. Null when temporal_confidence is 0.0.\",\"annotation\":\"Optional[str]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":152,\"validators\":[]}]},{\"id\":\"semantica.semantic_extract.schemas::RelationsWithTemporalResponse\",\"name\":\"RelationsWithTemporalResponse\",\"meaning\":\"Original RelationsWithTemporalResponse model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/semantic_extract/schemas.py\",\"source_sha256\":\"deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d\",\"class_sha256\":\"03eef9327c4a200448cfbf5e58143586d88330d8abb2d53c7a2ef3cd1c957d26\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"$defs\":{\"RelationWithTemporalOut\":{\"description\":\"Schema for relation extraction output with temporal validity bounds.\",\"properties\":{\"subject\":{\"description\":\"Source entity text\",\"title\":\"Subject\",\"type\":\"string\"},\"object\":{\"description\":\"Target entity text\",\"title\":\"Object\",\"type\":\"string\"},\"predicate\":{\"description\":\"Relation type or predicate\",\"title\":\"Predicate\",\"type\":\"string\"},\"confidence\":{\"default\":0.9,\"description\":\"Confidence score between 0 and 1\",\"title\":\"Confidence\",\"type\":\"number\"},\"metadata\":{\"additionalProperties\":true,\"description\":\"Additional metadata including provenance\",\"title\":\"Metadata\",\"type\":\"object\"},\"valid_from\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"ISO 8601 date or natural-language phrase for when this relation became valid. Null if no temporal signal in text.\",\"title\":\"Valid From\"},\"valid_until\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"ISO 8601 date or phrase for when this relation ceased to be valid. Null if open-ended or not stated.\",\"title\":\"Valid Until\"},\"temporal_confidence\":{\"default\":0.0,\"description\":\"Confidence that temporal information was present and correctly extracted. 0.0 if no temporal signal.\",\"title\":\"Temporal Confidence\",\"type\":\"number\"},\"temporal_source_text\":{\"anyOf\":[{\"type\":\"string\"},{\"type\":\"null\"}],\"default\":null,\"description\":\"Exact verbatim substring from the source text containing the temporal signal. Null when temporal_confidence is 0.0.\",\"title\":\"Temporal Source Text\"}},\"required\":[\"subject\",\"object\",\"predicate\"],\"title\":\"RelationWithTemporalOut\",\"type\":\"object\"}},\"description\":\"Wrapper for list of relations with temporal validity bounds.\",\"properties\":{\"relations\":{\"items\":{\"$ref\":\"#/$defs/RelationWithTemporalOut\"},\"title\":\"Relations\",\"type\":\"array\"}},\"title\":\"RelationsWithTemporalResponse\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"relations\",\"meaning\":\"Original relations in RelationsWithTemporalResponse\",\"annotation\":\"List[RelationWithTemporalOut]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":[]},\"source_line\":184,\"validators\":[]}]},{\"id\":\"semantica.server::BuildRequest\",\"name\":\"BuildRequest\",\"meaning\":\"Original BuildRequest model from semantica/server.py; fields and methods retain explicit Pydantic profile\",\"source\":\"semantica/server.py\",\"source_sha256\":\"41cd1a55c79dc6f11f0e544d8725823a6f06f967608cdacdd01e69eb2d72bd0f\",\"class_sha256\":\"08ba7ff8ab3b8bb6af617d6e2e257486d58014044a3c4501197e40e5bd3102fe\",\"profile\":\"semantica.pydantic-json/1\",\"json_schema\":{\"properties\":{\"sources\":{\"items\":{\"type\":\"string\"},\"title\":\"Sources\",\"type\":\"array\"},\"config\":{\"anyOf\":[{\"additionalProperties\":true,\"type\":\"object\"},{\"type\":\"null\"}],\"default\":null,\"title\":\"Config\"}},\"required\":[\"sources\"],\"title\":\"BuildRequest\",\"type\":\"object\"},\"config\":{},\"validators\":[],\"aliases\":{},\"properties\":{},\"fields\":[{\"name\":\"sources\",\"meaning\":\"Original sources in BuildRequest\",\"annotation\":\"List[str]\",\"required\":true,\"alias\":null,\"default\":{\"kind\":\"required\"},\"source_line\":131,\"validators\":[]},{\"name\":\"config\",\"meaning\":\"Original config in BuildRequest\",\"annotation\":\"Optional[Dict[str, Any]]\",\"required\":false,\"alias\":null,\"default\":{\"kind\":\"value\",\"value\":null},\"source_line\":132,\"validators\":[]}]}]"
}


/// Original SemanticaDecisionToolInput model from integrations/crewai/decision_tool.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput {
    /// Which decision-intelligence operation to run. One of: 'record_decision', 'find_precedents', 'trace_causal_chain', 'analyze_impact', 'check_policy'.
    #[serde(rename = "action")]
    pub action: String,
    /// Domain category, e.g. 'loan_approval'. Used by 'record_decision'.
    #[serde(rename = "category")]
    pub category: Option<String>,
    /// Short description of the situation. Used by 'record_decision' and 'find_precedents'.
    #[serde(rename = "scenario")]
    pub scenario: Option<String>,
    /// Why this outcome was chosen. Used by 'record_decision'.
    #[serde(rename = "reasoning")]
    pub reasoning: Option<String>,
    /// The decision result. Used by 'record_decision'.
    #[serde(rename = "outcome")]
    pub outcome: Option<String>,
    /// Confidence score in [0, 1]. Used by 'record_decision'.
    #[serde(rename = "confidence")]
    pub confidence: f64,
    /// Comma-separated entity names. Used by 'record_decision'.
    #[serde(rename = "entities")]
    pub entities: Option<String>,
    /// Identifier of a decision. Used by 'trace_causal_chain' and 'analyze_impact'.
    #[serde(rename = "decision_id")]
    pub decision_id: Option<String>,
    /// Maximum chain depth. Used by 'trace_causal_chain'.
    #[serde(rename = "depth", deserialize_with = "crate::json::deserialize_preserved")]
    pub depth: serde_json::Number,
    /// JSON object describing a proposed decision. Used by 'check_policy'.
    #[serde(rename = "decision_data")]
    pub decision_data: Option<String>,
    /// JSON list of rule strings like 'confidence >= 0.7'. Used by 'check_policy'.
    #[serde(rename = "policy_rules")]
    pub policy_rules: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput {}
impl crate::SemanticModelSnapshot for SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput {
    const MODEL_ID: &'static str = "integrations.crewai.decision_tool::SemanticaDecisionToolInput";
    const SOURCE_SHA256: &'static str = "62ebeb4769503670627a3243fdfeb537588c4249349bf4045a3f08c040c5d8c1";
    const CLASS_SHA256: &'static str = "e08a7b36fd732eed52116ba4d4600051ed6376e9bc2a7116f3e7458ad25f733d";
}

/// Original SemanticaKGToolInput model from integrations/crewai/kg_tool.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput {
    /// Which graph operation to run. One of: 'extract_entities', 'extract_relations', 'add_to_graph', 'query_graph', 'find_related'.
    #[serde(rename = "action")]
    pub action: String,
    /// Input text. Used by 'extract_entities', 'extract_relations' and 'add_to_graph'.
    #[serde(rename = "text")]
    pub text: Option<String>,
    /// Search query. Used by 'query_graph'.
    #[serde(rename = "query")]
    pub query: Option<String>,
    /// Root entity name. Used by 'find_related'.
    #[serde(rename = "entity")]
    pub entity: Option<String>,
    /// Maximum relationship hops. Used by 'find_related'.
    #[serde(rename = "hops", deserialize_with = "crate::json::deserialize_preserved")]
    pub hops: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput {}
impl crate::SemanticModelSnapshot for SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput {
    const MODEL_ID: &'static str = "integrations.crewai.kg_tool::SemanticaKGToolInput";
    const SOURCE_SHA256: &'static str = "3ddca1c02e9243e81d843495502624e825376b35d1b40b6ea98a0a13b5347da3";
    const CLASS_SHA256: &'static str = "a00eb2979f33464523f55a4c11efd7f97747bc231e33e18a5cf9bb3c9cdab22d";
}

/// Original AlignmentRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest {
    /// Original source_uri in AlignmentRequest
    #[serde(rename = "source_uri")]
    pub source_uri: String,
    /// Original target_uri in AlignmentRequest
    #[serde(rename = "target_uri")]
    pub target_uri: String,
    /// Original predicate in AlignmentRequest
    #[serde(rename = "predicate")]
    pub predicate: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::AlignmentRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "a09be977b2c04cd3f37ab8dbc145d584bc2fc5c39736cc903cfc9923ee01564c";
}

/// Original AlignmentResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse {
    /// Original source in AlignmentResponse
    #[serde(rename = "source")]
    pub source: String,
    /// Original predicate in AlignmentResponse
    #[serde(rename = "predicate")]
    pub predicate: String,
    /// Original target in AlignmentResponse
    #[serde(rename = "target")]
    pub target: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::AlignmentResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "6955c93265a662313bb445b0069abda410d1d7e8e0f94353a8a3dc66cada80fb";
}

/// Original AlignmentSuggestion model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion {
    /// Original source_uri in AlignmentSuggestion
    #[serde(rename = "source_uri")]
    pub source_uri: String,
    /// Original source_label in AlignmentSuggestion
    #[serde(rename = "source_label")]
    pub source_label: String,
    /// Original target_uri in AlignmentSuggestion
    #[serde(rename = "target_uri")]
    pub target_uri: String,
    /// Original target_label in AlignmentSuggestion
    #[serde(rename = "target_label")]
    pub target_label: String,
    /// Original relation in AlignmentSuggestion
    #[serde(rename = "relation")]
    pub relation: String,
    /// Original score in AlignmentSuggestion
    #[serde(rename = "score")]
    pub score: f64,
    /// Original label_similarity in AlignmentSuggestion
    #[serde(rename = "label_similarity")]
    pub label_similarity: f64,
    /// Original embedding_similarity in AlignmentSuggestion
    #[serde(rename = "embedding_similarity")]
    pub embedding_similarity: Option<f64>,
    /// Original reason in AlignmentSuggestion
    #[serde(rename = "reason")]
    pub reason: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::AlignmentSuggestion";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "9c59a70045803570e95f0b659f572f3917e6fcb26ee37a8e9d22798fa53375a1";
}

/// Original AlignmentSuggestionRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest {
    /// Original source_ontology_uri in AlignmentSuggestionRequest
    #[serde(rename = "source_ontology_uri")]
    pub source_ontology_uri: Option<String>,
    /// Original target_ontology_uri in AlignmentSuggestionRequest
    #[serde(rename = "target_ontology_uri")]
    pub target_ontology_uri: Option<String>,
    /// Original threshold in AlignmentSuggestionRequest
    #[serde(rename = "threshold")]
    pub threshold: f64,
    /// Original limit in AlignmentSuggestionRequest
    #[serde(rename = "limit", deserialize_with = "crate::json::deserialize_preserved")]
    pub limit: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::AlignmentSuggestionRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "144b997ec7351b0fd50c65622e3ed16dc0f1ba53c3f4f539209304560c5eeeed";
}

/// Original CommentRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyCommentRequest {
    /// Original element_uri in CommentRequest
    #[serde(rename = "element_uri")]
    pub element_uri: String,
    /// Original text in CommentRequest
    #[serde(rename = "text")]
    pub text: String,
    /// Original author in CommentRequest
    #[serde(rename = "author")]
    pub author: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyCommentRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyCommentRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::CommentRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "0bcb4d407958e4afd24da51feb64ea612e0524f7d6efcfe8e78080f429593613";
}

/// Original CreateOntologyRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest {
    /// Original mode in CreateOntologyRequest
    #[serde(rename = "mode")]
    pub mode: String,
    /// Original namespace in CreateOntologyRequest
    #[serde(rename = "namespace")]
    pub namespace: String,
    /// Original name in CreateOntologyRequest
    #[serde(rename = "name")]
    pub name: String,
    /// Original description in CreateOntologyRequest
    #[serde(rename = "description")]
    pub description: Option<String>,
    /// Original tags in CreateOntologyRequest
    #[serde(rename = "tags")]
    pub tags: Vec<String>,
    /// Original sample_data in CreateOntologyRequest
    #[serde(rename = "sample_data")]
    pub sample_data: Option<String>,
    /// Original schema_text in CreateOntologyRequest
    #[serde(rename = "schema_text")]
    pub schema_text: Option<String>,
    /// Original provider in CreateOntologyRequest
    #[serde(rename = "provider")]
    pub provider: Option<String>,
    /// Original model in CreateOntologyRequest
    #[serde(rename = "model")]
    pub model: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::CreateOntologyRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "8f6afe37e672edf26772a48567ef282a2862cd5cfa82b0454602d56d5f836cb5";
}

/// Original DraftDiff model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyDraftDiff {
    /// Original added_classes in DraftDiff
    #[serde(rename = "added_classes")]
    pub added_classes: Vec<String>,
    /// Original removed_classes in DraftDiff
    #[serde(rename = "removed_classes")]
    pub removed_classes: Vec<String>,
    /// Original modified_classes in DraftDiff
    #[serde(rename = "modified_classes", deserialize_with = "crate::json::deserialize_preserved")]
    pub modified_classes: crate::json::OrderedObject<String, crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original added_properties in DraftDiff
    #[serde(rename = "added_properties")]
    pub added_properties: Vec<String>,
    /// Original removed_properties in DraftDiff
    #[serde(rename = "removed_properties")]
    pub removed_properties: Vec<String>,
    /// Original modified_properties in DraftDiff
    #[serde(rename = "modified_properties", deserialize_with = "crate::json::deserialize_preserved")]
    pub modified_properties: crate::json::OrderedObject<String, crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original added_restrictions in DraftDiff
    #[serde(rename = "added_restrictions", deserialize_with = "crate::json::deserialize_preserved")]
    pub added_restrictions: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original removed_restrictions in DraftDiff
    #[serde(rename = "removed_restrictions", deserialize_with = "crate::json::deserialize_preserved")]
    pub removed_restrictions: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original added_axioms in DraftDiff
    #[serde(rename = "added_axioms", deserialize_with = "crate::json::deserialize_preserved")]
    pub added_axioms: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original removed_axioms in DraftDiff
    #[serde(rename = "removed_axioms", deserialize_with = "crate::json::deserialize_preserved")]
    pub removed_axioms: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original annotation_changes in DraftDiff
    #[serde(rename = "annotation_changes", deserialize_with = "crate::json::deserialize_preserved")]
    pub annotation_changes: crate::json::OrderedObject<String, crate::json::OrderedObject<String, serde_json::Value>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyDraftDiff {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyDraftDiff {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::DraftDiff";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "272728fe2616e4db3b3a79017fb12cfd2cc0d4e420a521dfa076ce6620e18ae9";
}

/// Original DraftRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyDraftRequest {
    /// Original ontology_uri in DraftRequest
    #[serde(rename = "ontology_uri")]
    pub ontology_uri: String,
    /// Original diff in DraftRequest
    #[serde(rename = "diff")]
    pub diff: SemanticModelSemanticaExplorerRoutesOntologyDraftDiff,
    /// Original author in DraftRequest
    #[serde(rename = "author")]
    pub author: String,
    /// Original summary in DraftRequest
    #[serde(rename = "summary")]
    pub summary: Option<String>,
    /// Original timestamp in DraftRequest
    #[serde(rename = "timestamp")]
    pub timestamp: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyDraftRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyDraftRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::DraftRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "2ddd8388a12ec5208af85b7ed3393a26b79d076c11a6c8156347f006fbe5ee34";
}

/// Original DraftResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyDraftResponse {
    /// Original draft_id in DraftResponse
    #[serde(rename = "draft_id")]
    pub draft_id: String,
    /// Original ontology_uri in DraftResponse
    #[serde(rename = "ontology_uri")]
    pub ontology_uri: String,
    /// Original diff in DraftResponse
    #[serde(rename = "diff")]
    pub diff: SemanticModelSemanticaExplorerRoutesOntologyDraftDiff,
    /// Original author in DraftResponse
    #[serde(rename = "author")]
    pub author: String,
    /// Original summary in DraftResponse
    #[serde(rename = "summary")]
    pub summary: Option<String>,
    /// Original created_at in DraftResponse
    #[serde(rename = "created_at")]
    pub created_at: String,
    /// Original updated_at in DraftResponse
    #[serde(rename = "updated_at")]
    pub updated_at: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyDraftResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyDraftResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::DraftResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "4246e5648acd6da5c2581410b9ca6e390f6e4da8b055dc7d11c4bce39b2e1e3b";
}

/// Original EntityDetailResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse {
    /// Original uri in EntityDetailResponse
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original label in EntityDetailResponse
    #[serde(rename = "label")]
    pub label: String,
    /// Original type in EntityDetailResponse
    #[serde(rename = "type")]
    pub field_type: String,
    /// Original entity_type in EntityDetailResponse
    #[serde(rename = "entity_type")]
    pub entity_type: String,
    /// Original definition in EntityDetailResponse
    #[serde(rename = "definition")]
    pub definition: Option<String>,
    /// Original source_ontology in EntityDetailResponse
    #[serde(rename = "source_ontology")]
    pub source_ontology: Option<String>,
    /// Original superclasses in EntityDetailResponse
    #[serde(rename = "superclasses")]
    pub superclasses: Vec<String>,
    /// Original subclasses in EntityDetailResponse
    #[serde(rename = "subclasses")]
    pub subclasses: Vec<String>,
    /// Original domain in EntityDetailResponse
    #[serde(rename = "domain")]
    pub domain: Vec<String>,
    /// Original range in EntityDetailResponse
    #[serde(rename = "range")]
    pub range: Vec<String>,
    /// Original instance_count in EntityDetailResponse
    #[serde(rename = "instance_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub instance_count: serde_json::Number,
    /// Original properties in EntityDetailResponse
    #[serde(rename = "properties", deserialize_with = "crate::json::deserialize_preserved")]
    pub properties: crate::json::OrderedObject<String, serde_json::Value>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::EntityDetailResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "6aeeccbe0791654364195c02207f614062fa396bc6ae0267a10e76be6609d233";
}

/// Original HealthDimension model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyHealthDimension {
    /// Original key in HealthDimension
    #[serde(rename = "key")]
    pub key: String,
    /// Original label in HealthDimension
    #[serde(rename = "label")]
    pub label: String,
    /// Original score in HealthDimension
    #[serde(rename = "score")]
    pub score: f64,
    /// Original status in HealthDimension
    #[serde(rename = "status")]
    pub status: String,
    /// Original detail in HealthDimension
    #[serde(rename = "detail")]
    pub detail: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyHealthDimension {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyHealthDimension {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::HealthDimension";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "7f61e1f434386932df9f2bb6b0e1261cbd9f090355e440dea417750eef00a77e";
}

/// Original HealthIssue model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyHealthIssue {
    /// Original id in HealthIssue
    #[serde(rename = "id")]
    pub id: String,
    /// Original severity in HealthIssue
    #[serde(rename = "severity")]
    pub severity: String,
    /// Original category in HealthIssue
    #[serde(rename = "category")]
    pub category: String,
    /// Original entity_uri in HealthIssue
    #[serde(rename = "entity_uri")]
    pub entity_uri: Option<String>,
    /// Original entity_label in HealthIssue
    #[serde(rename = "entity_label")]
    pub entity_label: Option<String>,
    /// Original message in HealthIssue
    #[serde(rename = "message")]
    pub message: String,
    /// Original action in HealthIssue
    #[serde(rename = "action")]
    pub action: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyHealthIssue {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyHealthIssue {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::HealthIssue";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "d6d49138cf18e23acccdc5b65c3bd9e002142172508f26ae313dd8f31c987771";
}

/// Original LoadOntologyRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest {
    /// Original url in LoadOntologyRequest
    #[serde(rename = "url")]
    pub url: Option<String>,
    /// Original content in LoadOntologyRequest
    #[serde(rename = "content")]
    pub content: Option<String>,
    /// Original format in LoadOntologyRequest
    #[serde(rename = "format")]
    pub format: Option<String>,
    /// Original name in LoadOntologyRequest
    #[serde(rename = "name")]
    pub name: Option<String>,
    /// Original description in LoadOntologyRequest
    #[serde(rename = "description")]
    pub description: Option<String>,
    /// Original tags in LoadOntologyRequest
    #[serde(rename = "tags")]
    pub tags: Vec<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::LoadOntologyRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "d831cb63bba224401d4e64c4c95362702ab1c571c0cdd572ddf435abeb0b0009";
}

/// Original LoadOntologyResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse {
    /// Original status in LoadOntologyResponse
    #[serde(rename = "status")]
    pub status: String,
    /// Original uri in LoadOntologyResponse
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original name in LoadOntologyResponse
    #[serde(rename = "name")]
    pub name: String,
    /// Original nodes_added in LoadOntologyResponse
    #[serde(rename = "nodes_added", deserialize_with = "crate::json::deserialize_preserved")]
    pub nodes_added: serde_json::Number,
    /// Original edges_added in LoadOntologyResponse
    #[serde(rename = "edges_added", deserialize_with = "crate::json::deserialize_preserved")]
    pub edges_added: serde_json::Number,
    /// Original format in LoadOntologyResponse
    #[serde(rename = "format")]
    pub format: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::LoadOntologyResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "6ce99747fdb81c3a9a949961142e610e5618f14799ba50c29ec09726219954dd";
}

/// Original OntologyAlignment model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment {
    /// Original id in OntologyAlignment
    #[serde(rename = "id")]
    pub id: String,
    /// Original source_uri in OntologyAlignment
    #[serde(rename = "source_uri")]
    pub source_uri: String,
    /// Original source_label in OntologyAlignment
    #[serde(rename = "source_label")]
    pub source_label: String,
    /// Original target_uri in OntologyAlignment
    #[serde(rename = "target_uri")]
    pub target_uri: String,
    /// Original target_label in OntologyAlignment
    #[serde(rename = "target_label")]
    pub target_label: String,
    /// Original relation in OntologyAlignment
    #[serde(rename = "relation")]
    pub relation: String,
    /// Original predicate_uri in OntologyAlignment
    #[serde(rename = "predicate_uri")]
    pub predicate_uri: String,
    /// Original confidence in OntologyAlignment
    #[serde(rename = "confidence")]
    pub confidence: f64,
    /// Original provenance in OntologyAlignment
    #[serde(rename = "provenance")]
    pub provenance: Option<String>,
    /// Original source in OntologyAlignment
    #[serde(rename = "source")]
    pub source: Option<String>,
    /// Original reviewer in OntologyAlignment
    #[serde(rename = "reviewer")]
    pub reviewer: Option<String>,
    /// Original created_at in OntologyAlignment
    #[serde(rename = "created_at")]
    pub created_at: String,
    /// Original updated_at in OntologyAlignment
    #[serde(rename = "updated_at")]
    pub updated_at: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::OntologyAlignment";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "84ed335f3e647b306f77d6bd4f50ed7b46c78c9e9c53a7418526b9f481debd1b";
}

/// Original OntologyAlignmentRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest {
    /// Original source_uri in OntologyAlignmentRequest
    #[serde(rename = "source_uri")]
    pub source_uri: String,
    /// Original source_label in OntologyAlignmentRequest
    #[serde(rename = "source_label")]
    pub source_label: Option<String>,
    /// Original target_uri in OntologyAlignmentRequest
    #[serde(rename = "target_uri")]
    pub target_uri: String,
    /// Original target_label in OntologyAlignmentRequest
    #[serde(rename = "target_label")]
    pub target_label: Option<String>,
    /// Original relation in OntologyAlignmentRequest
    #[serde(rename = "relation")]
    pub relation: String,
    /// Original confidence in OntologyAlignmentRequest
    #[serde(rename = "confidence")]
    pub confidence: f64,
    /// Original provenance in OntologyAlignmentRequest
    #[serde(rename = "provenance")]
    pub provenance: Option<String>,
    /// Original source in OntologyAlignmentRequest
    #[serde(rename = "source")]
    pub source: Option<String>,
    /// Original reviewer in OntologyAlignmentRequest
    #[serde(rename = "reviewer")]
    pub reviewer: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::OntologyAlignmentRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "204b2df3de1648accf1a269877b8cabdd5aee07165b4b75afc1713b9402cf780";
}

/// Original OntologyEntry model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry {
    /// Original uri in OntologyEntry
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original name in OntologyEntry
    #[serde(rename = "name")]
    pub name: String,
    /// Original description in OntologyEntry
    #[serde(rename = "description")]
    pub description: Option<String>,
    /// Original format in OntologyEntry
    #[serde(rename = "format")]
    pub format: String,
    /// Original status in OntologyEntry
    #[serde(rename = "status")]
    pub status: String,
    /// Original source_url in OntologyEntry
    #[serde(rename = "source_url")]
    pub source_url: Option<String>,
    /// Original version in OntologyEntry
    #[serde(rename = "version")]
    pub version: Option<String>,
    /// Original class_count in OntologyEntry
    #[serde(rename = "class_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub class_count: serde_json::Number,
    /// Original concept_count in OntologyEntry
    #[serde(rename = "concept_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub concept_count: serde_json::Number,
    /// Original property_count in OntologyEntry
    #[serde(rename = "property_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub property_count: serde_json::Number,
    /// Original loaded_at in OntologyEntry
    #[serde(rename = "loaded_at")]
    pub loaded_at: String,
    /// Original enabled in OntologyEntry
    #[serde(rename = "enabled")]
    pub enabled: bool,
    /// Original tags in OntologyEntry
    #[serde(rename = "tags")]
    pub tags: Vec<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::OntologyEntry";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "aca87ba6fc316ac641f0c921842407c6d8dd958262541b42bf12f49556cc4637";
}

/// Original OntologyHealthResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse {
    /// Original uri in OntologyHealthResponse
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original name in OntologyHealthResponse
    #[serde(rename = "name")]
    pub name: String,
    /// Original total_score in OntologyHealthResponse
    #[serde(rename = "total_score")]
    pub total_score: f64,
    /// Original dimensions in OntologyHealthResponse
    #[serde(rename = "dimensions")]
    pub dimensions: Vec<SemanticModelSemanticaExplorerRoutesOntologyHealthDimension>,
    /// Original issues in OntologyHealthResponse
    #[serde(rename = "issues")]
    pub issues: Vec<SemanticModelSemanticaExplorerRoutesOntologyHealthIssue>,
    /// Original generated_at in OntologyHealthResponse
    #[serde(rename = "generated_at")]
    pub generated_at: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::OntologyHealthResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "51e14b8e5c172f45ae8554915609d21868194b21b338078a031d33b8978101c5";
}

/// Original OntologyPreview model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview {
    /// Original uri in OntologyPreview
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original name in OntologyPreview
    #[serde(rename = "name")]
    pub name: String,
    /// Original description in OntologyPreview
    #[serde(rename = "description")]
    pub description: Option<String>,
    /// Original namespace in OntologyPreview
    #[serde(rename = "namespace")]
    pub namespace: Option<String>,
    /// Original version in OntologyPreview
    #[serde(rename = "version")]
    pub version: Option<String>,
    /// Original license in OntologyPreview
    #[serde(rename = "license")]
    pub license: Option<String>,
    /// Original format in OntologyPreview
    #[serde(rename = "format")]
    pub format: String,
    /// Original estimated_triples in OntologyPreview
    #[serde(rename = "estimated_triples", deserialize_with = "crate::json::deserialize_preserved")]
    pub estimated_triples: serde_json::Number,
    /// Original source_url in OntologyPreview
    #[serde(rename = "source_url")]
    pub source_url: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::OntologyPreview";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "376052f3e8773b3a97773f4fb81345e72e222d2ebf9f9f922bec21b6b6567e70";
}

/// Original OntologySearchResult model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult {
    /// Original uri in OntologySearchResult
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original label in OntologySearchResult
    #[serde(rename = "label")]
    pub label: String,
    /// Original type in OntologySearchResult
    #[serde(rename = "type")]
    pub field_type: String,
    /// Original entity_type in OntologySearchResult
    #[serde(rename = "entity_type")]
    pub entity_type: String,
    /// Original definition in OntologySearchResult
    #[serde(rename = "definition")]
    pub definition: Option<String>,
    /// Original source_ontology in OntologySearchResult
    #[serde(rename = "source_ontology")]
    pub source_ontology: Option<String>,
    /// Original namespace_prefix in OntologySearchResult
    #[serde(rename = "namespace_prefix")]
    pub namespace_prefix: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::OntologySearchResult";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "c10e5f2281fb5f6c65bccb838435fc4e441de48bccfe9904431d2e06026f3e20";
}

/// Original PreviewOntologyRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest {
    /// Original url in PreviewOntologyRequest
    #[serde(rename = "url")]
    pub url: Option<String>,
    /// Original content in PreviewOntologyRequest
    #[serde(rename = "content")]
    pub content: Option<String>,
    /// Original format in PreviewOntologyRequest
    #[serde(rename = "format")]
    pub format: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::PreviewOntologyRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "21bfc57f7cccd5bccbfd07fc349f3c305c42585b877adf46dcad3c2273242453";
}

/// Original ProposalRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyProposalRequest {
    /// Original draft_id in ProposalRequest
    #[serde(rename = "draft_id")]
    pub draft_id: String,
    /// Original ontology_uri in ProposalRequest
    #[serde(rename = "ontology_uri")]
    pub ontology_uri: String,
    /// Original summary in ProposalRequest
    #[serde(rename = "summary")]
    pub summary: String,
    /// Original reviewer in ProposalRequest
    #[serde(rename = "reviewer")]
    pub reviewer: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyProposalRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyProposalRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ProposalRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "0dcc5c46e9cd401569c9185c53e4b003f6067e631de5bf8e243ed43254e3d0f0";
}

/// Original ProposalResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyProposalResponse {
    /// Original proposal_id in ProposalResponse
    #[serde(rename = "proposal_id")]
    pub proposal_id: String,
    /// Original draft_id in ProposalResponse
    #[serde(rename = "draft_id")]
    pub draft_id: String,
    /// Original ontology_uri in ProposalResponse
    #[serde(rename = "ontology_uri")]
    pub ontology_uri: String,
    /// Original summary in ProposalResponse
    #[serde(rename = "summary")]
    pub summary: String,
    /// Original author in ProposalResponse
    #[serde(rename = "author")]
    pub author: String,
    /// Original reviewer in ProposalResponse
    #[serde(rename = "reviewer")]
    pub reviewer: Option<String>,
    /// Original state in ProposalResponse
    #[serde(rename = "state")]
    pub state: String,
    /// Original impact_analysis in ProposalResponse
    #[serde(rename = "impact_analysis", deserialize_with = "crate::json::deserialize_preserved")]
    pub impact_analysis: crate::json::OrderedObject<String, serde_json::Value>,
    /// Original shacl_validation in ProposalResponse
    #[serde(rename = "shacl_validation", deserialize_with = "crate::json::deserialize_preserved")]
    pub shacl_validation: crate::json::OrderedObject<String, serde_json::Value>,
    /// Original created_at in ProposalResponse
    #[serde(rename = "created_at")]
    pub created_at: String,
    /// Original updated_at in ProposalResponse
    #[serde(rename = "updated_at")]
    pub updated_at: String,
    /// Original comments in ProposalResponse
    #[serde(rename = "comments", deserialize_with = "crate::json::deserialize_preserved")]
    pub comments: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyProposalResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyProposalResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ProposalResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "d7d8542d198ff292ea26f7a744852fa2cd858eb0ace97606d7861b46eaced227";
}

/// Original ProposalState model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyProposalState {
    /// Original state in ProposalState
    #[serde(rename = "state")]
    pub state: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyProposalState {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyProposalState {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ProposalState";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "df6383b0a2852ff09b2d1d2b95d69c18e276761061c73937645b4d4914fbbc97";
}

/// Original RefreshResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse {
    /// Original status in RefreshResponse
    #[serde(rename = "status")]
    pub status: String,
    /// Original uri in RefreshResponse
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original nodes_added in RefreshResponse
    #[serde(rename = "nodes_added", deserialize_with = "crate::json::deserialize_preserved")]
    pub nodes_added: serde_json::Number,
    /// Original edges_added in RefreshResponse
    #[serde(rename = "edges_added", deserialize_with = "crate::json::deserialize_preserved")]
    pub edges_added: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::RefreshResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "792a047541091779f8f4d641653d84853e488e9dc1eb04c3417901d93016c916";
}

/// Original SKOSConceptDetail model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail {
    /// Original uri in SKOSConceptDetail
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original pref_label in SKOSConceptDetail
    #[serde(rename = "pref_label")]
    pub pref_label: String,
    /// Original alt_labels in SKOSConceptDetail
    #[serde(rename = "alt_labels")]
    pub alt_labels: Vec<String>,
    /// Original hidden_labels in SKOSConceptDetail
    #[serde(rename = "hidden_labels")]
    pub hidden_labels: Vec<String>,
    /// Original definition in SKOSConceptDetail
    #[serde(rename = "definition")]
    pub definition: Option<String>,
    /// Original scope_note in SKOSConceptDetail
    #[serde(rename = "scope_note")]
    pub scope_note: Option<String>,
    /// Original editorial_note in SKOSConceptDetail
    #[serde(rename = "editorial_note")]
    pub editorial_note: Option<String>,
    /// Original broader in SKOSConceptDetail
    #[serde(rename = "broader")]
    pub broader: Vec<String>,
    /// Original narrower in SKOSConceptDetail
    #[serde(rename = "narrower")]
    pub narrower: Vec<String>,
    /// Original related in SKOSConceptDetail
    #[serde(rename = "related")]
    pub related: Vec<String>,
    /// Original exact_match in SKOSConceptDetail
    #[serde(rename = "exact_match")]
    pub exact_match: Vec<String>,
    /// Original close_match in SKOSConceptDetail
    #[serde(rename = "close_match")]
    pub close_match: Vec<String>,
    /// Original broad_match in SKOSConceptDetail
    #[serde(rename = "broad_match")]
    pub broad_match: Vec<String>,
    /// Original narrow_match in SKOSConceptDetail
    #[serde(rename = "narrow_match")]
    pub narrow_match: Vec<String>,
    /// Original scheme_uri in SKOSConceptDetail
    #[serde(rename = "scheme_uri")]
    pub scheme_uri: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::SKOSConceptDetail";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "491eb90a7c33740526e2f8f4597c33c3226324e97b04580be08582721fea90fd";
}

/// Original SKOSConceptSearchRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest {
    /// Original query in SKOSConceptSearchRequest
    #[serde(rename = "query")]
    pub query: String,
    /// Original scheme_uri in SKOSConceptSearchRequest
    #[serde(rename = "scheme_uri")]
    pub scheme_uri: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::SKOSConceptSearchRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "7d85a427ae1ba7b9e70c8cf4632405bed4f2bcb79ce278ffde5bf68a07e99e5a";
}

/// Original SKOSScheme model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologySKOSScheme {
    /// Original uri in SKOSScheme
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original title in SKOSScheme
    #[serde(rename = "title")]
    pub title: String,
    /// Original description in SKOSScheme
    #[serde(rename = "description")]
    pub description: Option<String>,
    /// Original concept_count in SKOSScheme
    #[serde(rename = "concept_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub concept_count: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologySKOSScheme {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologySKOSScheme {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::SKOSScheme";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "fbb9a87c7e57f12db755e98fa1f03ba8c52875b04b042ec774932d4463577fae";
}

/// Original ShaclGenerateRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest {
    /// Original uri in ShaclGenerateRequest
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original quality_tier in ShaclGenerateRequest
    #[serde(rename = "quality_tier")]
    pub quality_tier: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ShaclGenerateRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "be0bd2e8d31bf9beeda4aa102039f2cbad0f3c46abebc4ae56365bfa1318ea3b";
}

/// Original ShaclGenerateResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse {
    /// Original uri in ShaclGenerateResponse
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original shacl_turtle in ShaclGenerateResponse
    #[serde(rename = "shacl_turtle")]
    pub shacl_turtle: String,
    /// Original shape_count in ShaclGenerateResponse
    #[serde(rename = "shape_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub shape_count: serde_json::Number,
    /// Original generated_at in ShaclGenerateResponse
    #[serde(rename = "generated_at")]
    pub generated_at: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ShaclGenerateResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "e923122ac95efdc051efcca83d4bd699f905c25492b3e63606d82a50bff5a16e";
}

/// Original ShaclShapeSummary model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary {
    /// Original id in ShaclShapeSummary
    #[serde(rename = "id")]
    pub id: String,
    /// Original target_class in ShaclShapeSummary
    #[serde(rename = "target_class")]
    pub target_class: Option<String>,
    /// Original constraint_count in ShaclShapeSummary
    #[serde(rename = "constraint_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub constraint_count: serde_json::Number,
    /// Original constraints in ShaclShapeSummary
    #[serde(rename = "constraints")]
    pub constraints: Vec<String>,
    /// Original violation_count in ShaclShapeSummary
    #[serde(rename = "violation_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub violation_count: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ShaclShapeSummary";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "bf9ca7f29bd688812878306ce57d29734912f09dc1cbc1f5c658fa7f70031c5f";
}

/// Original ShaclShapesResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse {
    /// Original uri in ShaclShapesResponse
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original shapes in ShaclShapesResponse
    #[serde(rename = "shapes")]
    pub shapes: Vec<SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary>,
    /// Original shacl_turtle in ShaclShapesResponse
    #[serde(rename = "shacl_turtle")]
    pub shacl_turtle: String,
    /// Original generated_at in ShaclShapesResponse
    #[serde(rename = "generated_at")]
    pub generated_at: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ShaclShapesResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "d2541aedf27c2fb59faf74c8d56127263eec4325919108f9d0222542f3a2763d";
}

/// Original ShaclValidateRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest {
    /// Original uri in ShaclValidateRequest
    #[serde(rename = "uri")]
    pub uri: Option<String>,
    /// Original shacl_turtle in ShaclValidateRequest
    #[serde(rename = "shacl_turtle")]
    pub shacl_turtle: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ShaclValidateRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "37c26e0a233d75fbd915e76276ab1f7e1d606e548c45f551f250d61d5e8b0d22";
}

/// Original ShaclValidationResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse {
    /// Original uri in ShaclValidationResponse
    #[serde(rename = "uri")]
    pub uri: Option<String>,
    /// Original conforms in ShaclValidationResponse
    #[serde(rename = "conforms")]
    pub conforms: bool,
    /// Original status in ShaclValidationResponse
    #[serde(rename = "status")]
    pub status: String,
    /// Original message in ShaclValidationResponse
    #[serde(rename = "message")]
    pub message: String,
    /// Original violations in ShaclValidationResponse
    #[serde(rename = "violations")]
    pub violations: Vec<SemanticModelSemanticaExplorerRoutesOntologyShaclViolation>,
    /// Original report_text in ShaclValidationResponse
    #[serde(rename = "report_text")]
    pub report_text: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ShaclValidationResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "50399cbcf561a8bf856784c5b251d0d5d2f32e193a4a17c30ad35827310fecb7";
}

/// Original ShaclViolation model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyShaclViolation {
    /// Original node in ShaclViolation
    #[serde(rename = "node")]
    pub node: Option<String>,
    /// Original path in ShaclViolation
    #[serde(rename = "path")]
    pub path: Option<String>,
    /// Original severity in ShaclViolation
    #[serde(rename = "severity")]
    pub severity: String,
    /// Original message in ShaclViolation
    #[serde(rename = "message")]
    pub message: String,
    /// Original focus_node in ShaclViolation
    #[serde(rename = "focus_node")]
    pub focus_node: Option<String>,
    /// Original source_shape in ShaclViolation
    #[serde(rename = "source_shape")]
    pub source_shape: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyShaclViolation {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyShaclViolation {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ShaclViolation";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "6eb073bc38376935338a2328960ced528af2edfaf41d3e7f8d7970f8b538ee19";
}

/// Original ToggleResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyToggleResponse {
    /// Original uri in ToggleResponse
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original enabled in ToggleResponse
    #[serde(rename = "enabled")]
    pub enabled: bool,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyToggleResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyToggleResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::ToggleResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "8007e7cdecf10e4aea79815a96a080da4f6a1a6ffa0e32ba769062325e81c27d";
}

/// Original VersionCompareRequest model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest {
    /// Original version1 in VersionCompareRequest
    #[serde(rename = "version1")]
    pub version1: String,
    /// Original version2 in VersionCompareRequest
    #[serde(rename = "version2")]
    pub version2: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::VersionCompareRequest";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "0de41ac2431da513bc9354dcd552d04446d9c754e3f47fba39f3cbe1cfb955f7";
}

/// Original VersionCompareResponse model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse {
    /// Original version1 in VersionCompareResponse
    #[serde(rename = "version1")]
    pub version1: String,
    /// Original version2 in VersionCompareResponse
    #[serde(rename = "version2")]
    pub version2: String,
    /// Original metadata_changes in VersionCompareResponse
    #[serde(rename = "metadata_changes", deserialize_with = "crate::json::deserialize_preserved")]
    pub metadata_changes: crate::json::OrderedObject<String, serde_json::Value>,
    /// Original class_changes in VersionCompareResponse
    #[serde(rename = "class_changes", deserialize_with = "crate::json::deserialize_preserved")]
    pub class_changes: crate::json::OrderedObject<String, serde_json::Value>,
    /// Original property_changes in VersionCompareResponse
    #[serde(rename = "property_changes", deserialize_with = "crate::json::deserialize_preserved")]
    pub property_changes: crate::json::OrderedObject<String, serde_json::Value>,
    /// Original restriction_changes in VersionCompareResponse
    #[serde(rename = "restriction_changes", deserialize_with = "crate::json::deserialize_preserved")]
    pub restriction_changes: crate::json::OrderedObject<String, serde_json::Value>,
    /// Original axiom_changes in VersionCompareResponse
    #[serde(rename = "axiom_changes", deserialize_with = "crate::json::deserialize_preserved")]
    pub axiom_changes: crate::json::OrderedObject<String, serde_json::Value>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::VersionCompareResponse";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "cb54fce354d665ea483e8d8f1c9755038901283c078ba5b1f20d628a5a7eb788";
}

/// Original VersionEntry model from semantica/explorer/routes/ontology.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesOntologyVersionEntry {
    /// Original version_id in VersionEntry
    #[serde(rename = "version_id")]
    pub version_id: String,
    /// Original ontology_uri in VersionEntry
    #[serde(rename = "ontology_uri")]
    pub ontology_uri: String,
    /// Original state in VersionEntry
    #[serde(rename = "state")]
    pub state: String,
    /// Original author in VersionEntry
    #[serde(rename = "author")]
    pub author: String,
    /// Original date in VersionEntry
    #[serde(rename = "date")]
    pub date: String,
    /// Original diff_summary in VersionEntry
    #[serde(rename = "diff_summary", deserialize_with = "crate::json::deserialize_preserved")]
    pub diff_summary: crate::json::OrderedObject<String, serde_json::Value>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesOntologyVersionEntry {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesOntologyVersionEntry {
    const MODEL_ID: &'static str = "semantica.explorer.routes.ontology::VersionEntry";
    const SOURCE_SHA256: &'static str = "bec53351cd08862fda71b128fc111fd21cd25e98f5d2ba850988389a397c5d5f";
    const CLASS_SHA256: &'static str = "fc27d1be62ebb40a07702932189fd383f5b34405605fb49c8ef2f05b07821f43";
}

/// Original SparqlRequest model from semantica/explorer/routes/sparql.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest {
    /// Original query in SparqlRequest
    #[serde(rename = "query")]
    pub query: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest {
    const MODEL_ID: &'static str = "semantica.explorer.routes.sparql::SparqlRequest";
    const SOURCE_SHA256: &'static str = "3dd5794cfdac52930b55aa9291fbd6815b335b135e6399eda40b2465a71cecfa";
    const CLASS_SHA256: &'static str = "21823985c42698cdeeccb3c9cd184f7010a36f474542373d03366d4d2193427e";
}

/// Original SparqlResponse model from semantica/explorer/routes/sparql.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse {
    /// Original columns in SparqlResponse
    #[serde(rename = "columns")]
    pub columns: Vec<String>,
    /// Original rows in SparqlResponse
    #[serde(rename = "rows", deserialize_with = "crate::json::deserialize_preserved")]
    pub rows: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original total in SparqlResponse
    #[serde(rename = "total", deserialize_with = "crate::json::deserialize_preserved")]
    pub total: serde_json::Number,
    /// Original truncated in SparqlResponse
    #[serde(rename = "truncated")]
    pub truncated: bool,
    /// Original error in SparqlResponse
    #[serde(rename = "error")]
    pub error: Option<String>,
    /// Original error_line in SparqlResponse
    #[serde(rename = "error_line", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub error_line: Option<serde_json::Number>,
    /// Original error_column in SparqlResponse
    #[serde(rename = "error_column", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub error_column: Option<serde_json::Number>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.sparql::SparqlResponse";
    const SOURCE_SHA256: &'static str = "3dd5794cfdac52930b55aa9291fbd6815b335b135e6399eda40b2465a71cecfa";
    const CLASS_SHA256: &'static str = "c48db021a0cce0e9029660f303839245eaf9fbb2dfaaf775db97799390b2052d";
}

/// Original TemporalBoundsResponse model from semantica/explorer/routes/temporal.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse {
    /// Original min in TemporalBoundsResponse
    #[serde(rename = "min")]
    pub min: Option<String>,
    /// Original max in TemporalBoundsResponse
    #[serde(rename = "max")]
    pub max: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.temporal::TemporalBoundsResponse";
    const SOURCE_SHA256: &'static str = "c2c8e6e154c68cfd612ac04672d46a2331d89fe3d8f4213e05ae838c98696b24";
    const CLASS_SHA256: &'static str = "c77c479d283b44d6df7c852e15868acaa60e0d0ca5c88d79b4cf6b1a68324a0b";
}

/// Original TemporalSnapshotFastResponse model from semantica/explorer/routes/temporal.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse {
    /// Original timestamp in TemporalSnapshotFastResponse
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    /// Original active_node_ids in TemporalSnapshotFastResponse
    #[serde(rename = "active_node_ids")]
    pub active_node_ids: Vec<String>,
    /// Original active_node_count in TemporalSnapshotFastResponse
    #[serde(rename = "active_node_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub active_node_count: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse {
    const MODEL_ID: &'static str = "semantica.explorer.routes.temporal::TemporalSnapshotFastResponse";
    const SOURCE_SHA256: &'static str = "c2c8e6e154c68cfd612ac04672d46a2331d89fe3d8f4213e05ae838c98696b24";
    const CLASS_SHA256: &'static str = "833a222f161e9639c9ef895ffcce92834062a3209c6fc0789c45e37f7cf14ad5";
}

/// Original AnalyticsResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasAnalyticsResponse {
    /// Original centrality in AnalyticsResponse
    #[serde(rename = "centrality", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub centrality: Option<crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original community in AnalyticsResponse
    #[serde(rename = "community", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub community: Option<crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original connectivity in AnalyticsResponse
    #[serde(rename = "connectivity", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub connectivity: Option<crate::json::OrderedObject<String, serde_json::Value>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasAnalyticsResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasAnalyticsResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::AnalyticsResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "86775c3db54e62c3406c4e9c97a318fc9ca5b569b358b55d809a8d1671161ef9";
}

/// Original AnnotationCreate model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasAnnotationCreate {
    /// Original node_id in AnnotationCreate
    #[serde(rename = "node_id")]
    pub node_id: String,
    /// Original content in AnnotationCreate
    #[serde(rename = "content")]
    pub content: String,
    /// Original tags in AnnotationCreate
    #[serde(rename = "tags")]
    pub tags: Vec<String>,
    /// Original visibility in AnnotationCreate
    #[serde(rename = "visibility")]
    pub visibility: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasAnnotationCreate {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasAnnotationCreate {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::AnnotationCreate";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "8103f23b82c47ddcb7d8803532f3604f04fcc251d1020319ed3f8d130fbfb81e";
}

/// Original AnnotationResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasAnnotationResponse {
    /// Original annotation_id in AnnotationResponse
    #[serde(rename = "annotation_id")]
    pub annotation_id: String,
    /// Original node_id in AnnotationResponse
    #[serde(rename = "node_id")]
    pub node_id: String,
    /// Original content in AnnotationResponse
    #[serde(rename = "content")]
    pub content: String,
    /// Original tags in AnnotationResponse
    #[serde(rename = "tags")]
    pub tags: Vec<String>,
    /// Original visibility in AnnotationResponse
    #[serde(rename = "visibility")]
    pub visibility: String,
    /// Original created_at in AnnotationResponse
    #[serde(rename = "created_at")]
    pub created_at: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasAnnotationResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasAnnotationResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::AnnotationResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "a447946dcc5977e60e9f21d6ba145d298a3dc627fbf14b951f4af2f34c8bd2aa";
}

/// Original CausalChainResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasCausalChainResponse {
    /// Original decision_id in CausalChainResponse
    #[serde(rename = "decision_id")]
    pub decision_id: String,
    /// Original chain in CausalChainResponse
    #[serde(rename = "chain", deserialize_with = "crate::json::deserialize_preserved")]
    pub chain: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasCausalChainResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasCausalChainResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::CausalChainResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "16f430670c24926057529c444c825e6643286fc9ca6454956afa42fc32d64657";
}

/// Original CausalDistanceReport model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasCausalDistanceReport {
    /// Original source_id in CausalDistanceReport
    #[serde(rename = "source_id")]
    pub source_id: String,
    /// Original target_id in CausalDistanceReport
    #[serde(rename = "target_id")]
    pub target_id: String,
    /// Original causal_path in CausalDistanceReport
    #[serde(rename = "causal_path")]
    pub causal_path: Vec<String>,
    /// Original causal_hop_count in CausalDistanceReport
    #[serde(rename = "causal_hop_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub causal_hop_count: serde_json::Number,
    /// Original intermediate_decisions in CausalDistanceReport
    #[serde(rename = "intermediate_decisions")]
    pub intermediate_decisions: Vec<String>,
    /// Original confidence_decay in CausalDistanceReport
    #[serde(rename = "confidence_decay")]
    pub confidence_decay: f64,
    /// Original weakest_link in CausalDistanceReport
    #[serde(rename = "weakest_link", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub weakest_link: Option<crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original interpretation in CausalDistanceReport
    #[serde(rename = "interpretation")]
    pub interpretation: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasCausalDistanceReport {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasCausalDistanceReport {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::CausalDistanceReport";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "6a24191722879d9aaade07300bb9dc5b6f8b30c4b849fc5d4e5984fc496600c1";
}

/// Original ComplianceResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasComplianceResponse {
    /// Original decision_id in ComplianceResponse
    #[serde(rename = "decision_id")]
    pub decision_id: String,
    /// Original compliant in ComplianceResponse
    #[serde(rename = "compliant")]
    pub compliant: bool,
    /// Original violations in ComplianceResponse
    #[serde(rename = "violations", deserialize_with = "crate::json::deserialize_preserved")]
    pub violations: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasComplianceResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasComplianceResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ComplianceResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "25414297ef7366253908beeb8bae02ae4aee4dd37c34d458f5c04df8f74034a6";
}

/// Original ConceptNode model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasConceptNode {
    /// Original uri in ConceptNode
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original pref_label in ConceptNode
    #[serde(rename = "pref_label")]
    pub pref_label: String,
    /// Original alt_labels in ConceptNode
    #[serde(rename = "alt_labels")]
    pub alt_labels: Vec<String>,
    /// Original description in ConceptNode
    #[serde(rename = "description")]
    pub description: Option<String>,
    /// Original notation in ConceptNode
    #[serde(rename = "notation")]
    pub notation: Option<String>,
    /// Original scheme_uri in ConceptNode
    #[serde(rename = "scheme_uri")]
    pub scheme_uri: Option<String>,
    /// Original parent_uri in ConceptNode
    #[serde(rename = "parent_uri")]
    pub parent_uri: Option<String>,
    /// Original children in ConceptNode
    #[serde(rename = "children")]
    pub children: Option<Vec<SemanticModelSemanticaExplorerSchemasConceptNode>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasConceptNode {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasConceptNode {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ConceptNode";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "2703c24350f3918cf56fd6e28832b4c1cfcd633ec9fd6245cc9fe8632248a237";
}

/// Original ConceptSummary model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasConceptSummary {
    /// Original uri in ConceptSummary
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original pref_label in ConceptSummary
    #[serde(rename = "pref_label")]
    pub pref_label: String,
    /// Original alt_labels in ConceptSummary
    #[serde(rename = "alt_labels")]
    pub alt_labels: Vec<String>,
    /// Original description in ConceptSummary
    #[serde(rename = "description")]
    pub description: Option<String>,
    /// Original notation in ConceptSummary
    #[serde(rename = "notation")]
    pub notation: Option<String>,
    /// Original scheme_uri in ConceptSummary
    #[serde(rename = "scheme_uri")]
    pub scheme_uri: Option<String>,
    /// Original parent_uri in ConceptSummary
    #[serde(rename = "parent_uri")]
    pub parent_uri: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasConceptSummary {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasConceptSummary {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ConceptSummary";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "014435618a6f113fd249023f8ac9a42de608ac9e7236d3bfae3393bba6a1668b";
}

/// Original DecisionResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasDecisionResponse {
    /// Original decision_id in DecisionResponse
    #[serde(rename = "decision_id")]
    pub decision_id: String,
    /// Original category in DecisionResponse
    #[serde(rename = "category")]
    pub category: String,
    /// Original scenario in DecisionResponse
    #[serde(rename = "scenario")]
    pub scenario: String,
    /// Original reasoning in DecisionResponse
    #[serde(rename = "reasoning")]
    pub reasoning: String,
    /// Original outcome in DecisionResponse
    #[serde(rename = "outcome")]
    pub outcome: String,
    /// Original confidence in DecisionResponse
    #[serde(rename = "confidence")]
    pub confidence: f64,
    /// Original timestamp in DecisionResponse
    #[serde(rename = "timestamp")]
    pub timestamp: Option<String>,
    /// Original metadata in DecisionResponse
    #[serde(rename = "metadata", deserialize_with = "crate::json::deserialize_preserved")]
    pub metadata: crate::json::OrderedObject<String, serde_json::Value>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasDecisionResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasDecisionResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::DecisionResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "fd5a312fb931bcc67d0070ee61a2984e0cec5ea69a8cc2ad1e56aad3e2dc447c";
}

/// Original DedupRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasDedupRequest {
    /// Original threshold in DedupRequest
    #[serde(rename = "threshold")]
    pub threshold: f64,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasDedupRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasDedupRequest {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::DedupRequest";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "2ef0eabe406d4d74abdd1bd6a6c993e68e504d12c7e9766fb0c1e738ec229ff4";
}

/// Original DedupResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasDedupResponse {
    /// Original duplicates in DedupResponse
    #[serde(rename = "duplicates", deserialize_with = "crate::json::deserialize_preserved")]
    pub duplicates: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original total_flagged in DedupResponse
    #[serde(rename = "total_flagged", deserialize_with = "crate::json::deserialize_preserved")]
    pub total_flagged: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasDedupResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasDedupResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::DedupResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "9f46d34e1d2c00fcabd42b270e302fd051dd4cc093f9e37ba956a4ef773d7681";
}

/// Original DistanceEvent model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasDistanceEvent {
    /// Original timestamp in DistanceEvent
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    /// Original event_type in DistanceEvent
    #[serde(rename = "event_type")]
    pub event_type: String,
    /// Original hop_count_before in DistanceEvent
    #[serde(rename = "hop_count_before", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub hop_count_before: Option<serde_json::Number>,
    /// Original hop_count_after in DistanceEvent
    #[serde(rename = "hop_count_after", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub hop_count_after: Option<serde_json::Number>,
    /// Original description in DistanceEvent
    #[serde(rename = "description")]
    pub description: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasDistanceEvent {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasDistanceEvent {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::DistanceEvent";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "e01a3a0694fd2666754764afcd6e24902d5ea0d897014fdd0d7553c43eb510eb";
}

/// Original DistanceExportRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasDistanceExportRequest {
    /// Original format in DistanceExportRequest
    #[serde(rename = "format")]
    pub format: String,
    /// Original node_subset in DistanceExportRequest
    #[serde(rename = "node_subset")]
    pub node_subset: Option<Vec<String>>,
    /// Original include in DistanceExportRequest
    #[serde(rename = "include")]
    pub include: Vec<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasDistanceExportRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasDistanceExportRequest {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::DistanceExportRequest";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "b58721e65d85733cda45b7ee3125adfc510387e98991178bbd5ab95bfb033e23";
}

/// Original DistanceHistoryResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse {
    /// Original source_id in DistanceHistoryResponse
    #[serde(rename = "source_id")]
    pub source_id: String,
    /// Original target_id in DistanceHistoryResponse
    #[serde(rename = "target_id")]
    pub target_id: String,
    /// Original metric in DistanceHistoryResponse
    #[serde(rename = "metric")]
    pub metric: String,
    /// Original history in DistanceHistoryResponse
    #[serde(rename = "history")]
    pub history: Vec<SemanticModelSemanticaExplorerSchemasDistanceSnapshot>,
    /// Original events in DistanceHistoryResponse
    #[serde(rename = "events")]
    pub events: Vec<SemanticModelSemanticaExplorerSchemasDistanceEvent>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::DistanceHistoryResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "f4c08a0b53d9cced7e58312550d51bc3f5f2dfe76f58e0ac6087d4a31c71f414";
}

/// Original DistanceMatrixRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest {
    /// Original node_ids in DistanceMatrixRequest
    #[serde(rename = "node_ids")]
    pub node_ids: Vec<String>,
    /// Original metric in DistanceMatrixRequest
    #[serde(rename = "metric")]
    pub metric: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::DistanceMatrixRequest";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "40f3b2ad0b7fc1d0c356ddec8b930f903237bb9d401affec1228ab6fecd8c574";
}

/// Original DistanceMatrixResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse {
    /// Original nodes in DistanceMatrixResponse
    #[serde(rename = "nodes")]
    pub nodes: Vec<String>,
    /// Original metric in DistanceMatrixResponse
    #[serde(rename = "metric")]
    pub metric: String,
    /// Original matrix in DistanceMatrixResponse
    #[serde(rename = "matrix")]
    pub matrix: Vec<Vec<Option<f64>>>,
    /// Original unreachable_pairs in DistanceMatrixResponse
    #[serde(rename = "unreachable_pairs")]
    pub unreachable_pairs: Vec<(String, String)>,
    /// Original computation_time_ms in DistanceMatrixResponse
    #[serde(rename = "computation_time_ms")]
    pub computation_time_ms: f64,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::DistanceMatrixResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "657d5c9540f55289c9ad5bff39442b4d9616a9cf4f5a389f06682ea150a1700b";
}

/// Original DistanceSnapshot model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasDistanceSnapshot {
    /// Original timestamp in DistanceSnapshot
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    /// Original hop_count in DistanceSnapshot
    #[serde(rename = "hop_count", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub hop_count: Option<serde_json::Number>,
    /// Original distance_band in DistanceSnapshot
    #[serde(rename = "distance_band")]
    pub distance_band: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasDistanceSnapshot {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasDistanceSnapshot {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::DistanceSnapshot";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "0c0b008c358d63bf3dd879b41318bb2e032b42d3b7200d9a6be008f70bedaf3b";
}

/// Original EdgeListResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasEdgeListResponse {
    /// Original edges in EdgeListResponse
    #[serde(rename = "edges")]
    pub edges: Vec<SemanticModelSemanticaExplorerSchemasEdgeResponse>,
    /// Original total in EdgeListResponse
    #[serde(rename = "total", deserialize_with = "crate::json::deserialize_preserved")]
    pub total: serde_json::Number,
    /// Original skip in EdgeListResponse
    #[serde(rename = "skip", deserialize_with = "crate::json::deserialize_preserved")]
    pub skip: serde_json::Number,
    /// Original limit in EdgeListResponse
    #[serde(rename = "limit", deserialize_with = "crate::json::deserialize_preserved")]
    pub limit: serde_json::Number,
    /// Original next_cursor in EdgeListResponse
    #[serde(rename = "next_cursor")]
    pub next_cursor: Option<String>,
    /// Original has_more in EdgeListResponse
    #[serde(rename = "has_more")]
    pub has_more: bool,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasEdgeListResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasEdgeListResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::EdgeListResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "41405933c23709d59d00b6b47fdd1958f5d11752ee645d93c862c2663a811f16";
}

/// Original EdgeResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasEdgeResponse {
    /// Original id in EdgeResponse
    #[serde(rename = "id")]
    pub id: String,
    /// Original familyId in EdgeResponse
    #[serde(rename = "familyId")]
    pub family_id: String,
    /// Original source in EdgeResponse
    #[serde(rename = "source")]
    pub source: String,
    /// Original target in EdgeResponse
    #[serde(rename = "target")]
    pub target: String,
    /// Original type in EdgeResponse
    #[serde(rename = "type")]
    pub field_type: String,
    /// Original weight in EdgeResponse
    #[serde(rename = "weight")]
    pub weight: f64,
    /// Original properties in EdgeResponse
    #[serde(rename = "properties", deserialize_with = "crate::json::deserialize_preserved")]
    pub properties: crate::json::OrderedObject<String, serde_json::Value>,
    /// Original valid_from in EdgeResponse
    #[serde(rename = "valid_from")]
    pub valid_from: Option<String>,
    /// Original valid_until in EdgeResponse
    #[serde(rename = "valid_until")]
    pub valid_until: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasEdgeResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasEdgeResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::EdgeResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "571f462324245093cf601eabb266c0d11fd9cd26da3ca8ed52149c5675ef1550";
}

/// Original EnrichExtractRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasEnrichExtractRequest {
    /// Original text in EnrichExtractRequest
    #[serde(rename = "text")]
    pub text: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasEnrichExtractRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasEnrichExtractRequest {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::EnrichExtractRequest";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "72d827898b6cde7a4d1e4bd885c2ebc1b3bb6f9305d3d15a02466fae7d159770";
}

/// Original EnrichExtractResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasEnrichExtractResponse {
    /// Original entities in EnrichExtractResponse
    #[serde(rename = "entities", deserialize_with = "crate::json::deserialize_preserved")]
    pub entities: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
    /// Original relations in EnrichExtractResponse
    #[serde(rename = "relations", deserialize_with = "crate::json::deserialize_preserved")]
    pub relations: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasEnrichExtractResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasEnrichExtractResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::EnrichExtractResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "c9153da9fb2bb3be963ccdbf6144675de38641e00f105de0b9c189414a79436b";
}

/// Original ErrorResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasErrorResponse {
    /// Original detail in ErrorResponse
    #[serde(rename = "detail")]
    pub detail: String,
    /// Original status_code in ErrorResponse
    #[serde(rename = "status_code", deserialize_with = "crate::json::deserialize_preserved")]
    pub status_code: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasErrorResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasErrorResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ErrorResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "45057e448f68248387676616bae6eba3d4d20dbeb905a60cf7d9adfe0ef5cd0f";
}

/// Original ExportRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasExportRequest {
    /// Original format in ExportRequest
    #[serde(rename = "format")]
    pub format: String,
    /// Original node_ids in ExportRequest
    #[serde(rename = "node_ids")]
    pub node_ids: Option<Vec<String>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasExportRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasExportRequest {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ExportRequest";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "1973c25f650cb55730de77c832496442cf3f846a14488b009d76e7d3f4ac1f6b";
}

/// Original ExportResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasExportResponse {
    /// Original format in ExportResponse
    #[serde(rename = "format")]
    pub format: String,
    /// Original content_type in ExportResponse
    #[serde(rename = "content_type")]
    pub content_type: String,
    /// Original filename in ExportResponse
    #[serde(rename = "filename")]
    pub filename: String,
    /// Original size_bytes in ExportResponse
    #[serde(rename = "size_bytes", deserialize_with = "crate::json::deserialize_preserved")]
    pub size_bytes: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasExportResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasExportResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ExportResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "5b1031f528294619ce0564e46c1e82ad458447862fc61d843455a7d857e3d740";
}

/// Original GraphStatsResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasGraphStatsResponse {
    /// Original node_count in GraphStatsResponse
    #[serde(rename = "node_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub node_count: serde_json::Number,
    /// Original edge_count in GraphStatsResponse
    #[serde(rename = "edge_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub edge_count: serde_json::Number,
    /// Original node_types in GraphStatsResponse
    #[serde(rename = "node_types", deserialize_with = "crate::json::deserialize_preserved")]
    pub node_types: crate::json::OrderedObject<String, serde_json::Number>,
    /// Original edge_types in GraphStatsResponse
    #[serde(rename = "edge_types", deserialize_with = "crate::json::deserialize_preserved")]
    pub edge_types: crate::json::OrderedObject<String, serde_json::Number>,
    /// Original density in GraphStatsResponse
    #[serde(rename = "density")]
    pub density: f64,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasGraphStatsResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasGraphStatsResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::GraphStatsResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "5fc491d9f4e1a93ced2163596dffc9d6855f6bdd845ee74cb016f3a4a649dfb3";
}

/// Original ImportResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasImportResponse {
    /// Original status in ImportResponse
    #[serde(rename = "status")]
    pub status: String,
    /// Original message in ImportResponse
    #[serde(rename = "message")]
    pub message: String,
    /// Original nodes_added in ImportResponse
    #[serde(rename = "nodes_added", deserialize_with = "crate::json::deserialize_preserved")]
    pub nodes_added: serde_json::Number,
    /// Original edges_added in ImportResponse
    #[serde(rename = "edges_added", deserialize_with = "crate::json::deserialize_preserved")]
    pub edges_added: serde_json::Number,
    /// Original nodes_imported in ImportResponse
    #[serde(rename = "nodes_imported", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub nodes_imported: Option<serde_json::Number>,
    /// Original edges_imported in ImportResponse
    #[serde(rename = "edges_imported", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub edges_imported: Option<serde_json::Number>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasImportResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasImportResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ImportResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "0e08f7d8e7e23c2a8feda6c9f7724b1199714027a62ca6ad9909dac8c85ea9eb";
}

/// Original LinkPredictionRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasLinkPredictionRequest {
    /// Original node_id in LinkPredictionRequest
    #[serde(rename = "node_id")]
    pub node_id: String,
    /// Original top_n in LinkPredictionRequest
    #[serde(rename = "top_n", deserialize_with = "crate::json::deserialize_preserved")]
    pub top_n: serde_json::Number,
    /// Original candidate_type in LinkPredictionRequest
    #[serde(rename = "candidate_type")]
    pub candidate_type: Option<String>,
    /// Original min_score in LinkPredictionRequest
    #[serde(rename = "min_score")]
    pub min_score: f64,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasLinkPredictionRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasLinkPredictionRequest {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::LinkPredictionRequest";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "13242148d5357ab53b352d1234e365b334172ecd8c9807952bd176602926710e";
}

/// Original LinkPredictionResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasLinkPredictionResponse {
    /// Original node_id in LinkPredictionResponse
    #[serde(rename = "node_id")]
    pub node_id: String,
    /// Original predictions in LinkPredictionResponse
    #[serde(rename = "predictions", deserialize_with = "crate::json::deserialize_preserved")]
    pub predictions: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasLinkPredictionResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasLinkPredictionResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::LinkPredictionResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "517bf23a830b08d31ca3070f489189e19b6fc4767e01017b247ff3b60e781a45";
}

/// Original MergeRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasMergeRequest {
    /// Original primary_id in MergeRequest
    #[serde(rename = "primary_id")]
    pub primary_id: String,
    /// Original duplicate_ids in MergeRequest
    #[serde(rename = "duplicate_ids")]
    pub duplicate_ids: Vec<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasMergeRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasMergeRequest {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::MergeRequest";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "a38364687bebe87351c1bc81d40a7e70c6bfbf443aad6be92c19b55d4424143e";
}

/// Original MergeResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasMergeResponse {
    /// Original merged_into in MergeResponse
    #[serde(rename = "merged_into")]
    pub merged_into: String,
    /// Original removed_ids in MergeResponse
    #[serde(rename = "removed_ids")]
    pub removed_ids: Vec<String>,
    /// Original edges_updated in MergeResponse
    #[serde(rename = "edges_updated", deserialize_with = "crate::json::deserialize_preserved")]
    pub edges_updated: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasMergeResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasMergeResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::MergeResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "375906d419914970ee1722debca1de0e98f3658929cad5c3647534d517d4c072";
}

/// Original NeighborResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasNeighborResponse {
    /// Original id in NeighborResponse
    #[serde(rename = "id")]
    pub id: String,
    /// Original type in NeighborResponse
    #[serde(rename = "type")]
    pub field_type: String,
    /// Original content in NeighborResponse
    #[serde(rename = "content")]
    pub content: String,
    /// Original relationship in NeighborResponse
    #[serde(rename = "relationship")]
    pub relationship: String,
    /// Original weight in NeighborResponse
    #[serde(rename = "weight")]
    pub weight: f64,
    /// Original hop in NeighborResponse
    #[serde(rename = "hop", deserialize_with = "crate::json::deserialize_preserved")]
    pub hop: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasNeighborResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasNeighborResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::NeighborResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "5b9f1eb8ecce390a3c7934f4b69bf61a7b944c260b8c999d7a2d1d35c58f3052";
}

/// Original NodeListResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasNodeListResponse {
    /// Original nodes in NodeListResponse
    #[serde(rename = "nodes")]
    pub nodes: Vec<SemanticModelSemanticaExplorerSchemasNodeResponse>,
    /// Original total in NodeListResponse
    #[serde(rename = "total", deserialize_with = "crate::json::deserialize_preserved")]
    pub total: serde_json::Number,
    /// Original skip in NodeListResponse
    #[serde(rename = "skip", deserialize_with = "crate::json::deserialize_preserved")]
    pub skip: serde_json::Number,
    /// Original limit in NodeListResponse
    #[serde(rename = "limit", deserialize_with = "crate::json::deserialize_preserved")]
    pub limit: serde_json::Number,
    /// Original next_cursor in NodeListResponse
    #[serde(rename = "next_cursor")]
    pub next_cursor: Option<String>,
    /// Original has_more in NodeListResponse
    #[serde(rename = "has_more")]
    pub has_more: bool,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasNodeListResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasNodeListResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::NodeListResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "c3e878fcf9aa2b650b6bbb283205d075d87dd0fc6f55ccc8dbac73d9f0a43e74";
}

/// Original NodeResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasNodeResponse {
    /// Original id in NodeResponse
    #[serde(rename = "id")]
    pub id: String,
    /// Original type in NodeResponse
    #[serde(rename = "type")]
    pub field_type: String,
    /// Original content in NodeResponse
    #[serde(rename = "content")]
    pub content: String,
    /// Original properties in NodeResponse
    #[serde(rename = "properties", deserialize_with = "crate::json::deserialize_preserved")]
    pub properties: crate::json::OrderedObject<String, serde_json::Value>,
    /// Original valid_from in NodeResponse
    #[serde(rename = "valid_from")]
    pub valid_from: Option<String>,
    /// Original valid_until in NodeResponse
    #[serde(rename = "valid_until")]
    pub valid_until: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasNodeResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasNodeResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::NodeResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "19324cd81bbbde803b858f9e974aacf5a019b36c611f4c04f238c81c33283230";
}

/// Original PathResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasPathResponse {
    /// Original source in PathResponse
    #[serde(rename = "source")]
    pub source: String,
    /// Original target in PathResponse
    #[serde(rename = "target")]
    pub target: String,
    /// Original algorithm in PathResponse
    #[serde(rename = "algorithm")]
    pub algorithm: String,
    /// Original path in PathResponse
    #[serde(rename = "path")]
    pub path: Vec<String>,
    /// Original edge_ids in PathResponse
    #[serde(rename = "edge_ids")]
    pub edge_ids: Vec<String>,
    /// Original total_weight in PathResponse
    #[serde(rename = "total_weight")]
    pub total_weight: f64,
    /// Original directed in PathResponse
    #[serde(rename = "directed")]
    pub directed: bool,
    /// Original hop_count in PathResponse
    #[serde(rename = "hop_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub hop_count: serde_json::Number,
    /// Original distance_band in PathResponse
    #[serde(rename = "distance_band")]
    pub distance_band: String,
    /// Original semantic_similarity in PathResponse
    #[serde(rename = "semantic_similarity")]
    pub semantic_similarity: Option<f64>,
    /// Original path_coherence_score in PathResponse
    #[serde(rename = "path_coherence_score")]
    pub path_coherence_score: Option<f64>,
    /// Original confidence_decay in PathResponse
    #[serde(rename = "confidence_decay")]
    pub confidence_decay: Option<f64>,
    /// Original bottleneck_node in PathResponse
    #[serde(rename = "bottleneck_node")]
    pub bottleneck_node: Option<String>,
    /// Original alternative_path_count in PathResponse
    #[serde(rename = "alternative_path_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub alternative_path_count: serde_json::Number,
    /// Original interpretation in PathResponse
    #[serde(rename = "interpretation")]
    pub interpretation: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasPathResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasPathResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::PathResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "c54531585f8634acf8ea2e97875d74566723bfbebcde0c7276538791f78070da";
}

/// Original ProvenanceEdge model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasProvenanceEdge {
    /// Original id in ProvenanceEdge
    #[serde(rename = "id")]
    pub id: String,
    /// Original source in ProvenanceEdge
    #[serde(rename = "source")]
    pub source: String,
    /// Original target in ProvenanceEdge
    #[serde(rename = "target")]
    pub target: String,
    /// Original label in ProvenanceEdge
    #[serde(rename = "label")]
    pub label: String,
    /// Original direction in ProvenanceEdge
    #[serde(rename = "direction")]
    pub direction: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasProvenanceEdge {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasProvenanceEdge {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ProvenanceEdge";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "f916c8fea8e09ed76397bc99f8488e7558f1f70880aba710931681977ba7ac17";
}

/// Original ProvenanceNode model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasProvenanceNode {
    /// Original id in ProvenanceNode
    #[serde(rename = "id")]
    pub id: String,
    /// Original label in ProvenanceNode
    #[serde(rename = "label")]
    pub label: String,
    /// Original prov_type in ProvenanceNode
    #[serde(rename = "prov_type")]
    pub prov_type: String,
    /// Original parent_id in ProvenanceNode
    #[serde(rename = "parent_id")]
    pub parent_id: Option<String>,
    /// Original source_document in ProvenanceNode
    #[serde(rename = "source_document")]
    pub source_document: Option<String>,
    /// Original source_location in ProvenanceNode
    #[serde(rename = "source_location")]
    pub source_location: Option<String>,
    /// Original source_quote in ProvenanceNode
    #[serde(rename = "source_quote")]
    pub source_quote: Option<String>,
    /// Original confidence in ProvenanceNode
    #[serde(rename = "confidence")]
    pub confidence: Option<f64>,
    /// Original checksum in ProvenanceNode
    #[serde(rename = "checksum")]
    pub checksum: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasProvenanceNode {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasProvenanceNode {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ProvenanceNode";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "d8dad363726ff0d6ba28bef812a7a0ab4379fc833718194e8bc3687f969fb604";
}

/// Original ProvenanceResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasProvenanceResponse {
    /// Original nodes in ProvenanceResponse
    #[serde(rename = "nodes")]
    pub nodes: Vec<SemanticModelSemanticaExplorerSchemasProvenanceNode>,
    /// Original edges in ProvenanceResponse
    #[serde(rename = "edges")]
    pub edges: Vec<SemanticModelSemanticaExplorerSchemasProvenanceEdge>,
    /// Original source in ProvenanceResponse
    #[serde(rename = "source")]
    pub source: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasProvenanceResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasProvenanceResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ProvenanceResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "985eaa552b5e789e664f444c41adb20b30aa4fe3570098f613261d2f2cee2b23";
}

/// Original ReasoningRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasReasoningRequest {
    /// Original facts in ReasoningRequest
    #[serde(rename = "facts")]
    pub facts: Vec<String>,
    /// Original rules in ReasoningRequest
    #[serde(rename = "rules")]
    pub rules: Vec<String>,
    /// Original mode in ReasoningRequest
    #[serde(rename = "mode")]
    pub mode: String,
    /// Original apply_to_graph in ReasoningRequest
    #[serde(rename = "apply_to_graph")]
    pub apply_to_graph: bool,
    /// Original inferred_edge_type in ReasoningRequest
    #[serde(rename = "inferred_edge_type")]
    pub inferred_edge_type: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasReasoningRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasReasoningRequest {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ReasoningRequest";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "e071ca2d41efc3729ba01b262bd2e4bf4c6a26ed4314c601ca9de4ec208cc4d3";
}

/// Original ReasoningResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasReasoningResponse {
    /// Original inferred_facts in ReasoningResponse
    #[serde(rename = "inferred_facts")]
    pub inferred_facts: Vec<String>,
    /// Original rules_fired in ReasoningResponse
    #[serde(rename = "rules_fired", deserialize_with = "crate::json::deserialize_preserved")]
    pub rules_fired: serde_json::Number,
    /// Original added_edges in ReasoningResponse
    #[serde(rename = "added_edges", deserialize_with = "crate::json::deserialize_preserved")]
    pub added_edges: serde_json::Number,
    /// Original mutated in ReasoningResponse
    #[serde(rename = "mutated")]
    pub mutated: bool,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasReasoningResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasReasoningResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ReasoningResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "7eafa8c3173716b29a9cf97e9c81d82e78b78ff5ecf966d9b91e94343f8b1db5";
}

/// Original SearchRequest model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasSearchRequest {
    /// Original query in SearchRequest
    #[serde(rename = "query")]
    pub query: String,
    /// Original filters in SearchRequest
    #[serde(rename = "filters", deserialize_with = "crate::json::deserialize_preserved")]
    pub filters: crate::json::OrderedObject<String, serde_json::Value>,
    /// Original limit in SearchRequest
    #[serde(rename = "limit", deserialize_with = "crate::json::deserialize_preserved")]
    pub limit: serde_json::Number,
    /// Original anchor_node in SearchRequest
    #[serde(rename = "anchor_node")]
    pub anchor_node: Option<String>,
    /// Original max_hops in SearchRequest
    #[serde(rename = "max_hops", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub max_hops: Option<serde_json::Number>,
    /// Original min_semantic_similarity in SearchRequest
    #[serde(rename = "min_semantic_similarity")]
    pub min_semantic_similarity: Option<f64>,
    /// Original rank_by in SearchRequest
    #[serde(rename = "rank_by")]
    pub rank_by: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasSearchRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasSearchRequest {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::SearchRequest";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "f815ca7e7c4a9d08cc4bc60a496ab8ae3173ce1bb0b06c07a7f1767292568e30";
}

/// Original SearchResultItem model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasSearchResultItem {
    /// Original node in SearchResultItem
    #[serde(rename = "node")]
    pub node: SemanticModelSemanticaExplorerSchemasNodeResponse,
    /// Original score in SearchResultItem
    #[serde(rename = "score")]
    pub score: f64,
    /// Original hop_distance in SearchResultItem
    #[serde(rename = "hop_distance", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub hop_distance: Option<serde_json::Number>,
    /// Original semantic_similarity in SearchResultItem
    #[serde(rename = "semantic_similarity")]
    pub semantic_similarity: Option<f64>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasSearchResultItem {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasSearchResultItem {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::SearchResultItem";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "df34d06ac32ca41bd514865fc4abff6cb174c43d44108e76b2d50f17ea7e8829";
}

/// Original SearchResultResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasSearchResultResponse {
    /// Original results in SearchResultResponse
    #[serde(rename = "results")]
    pub results: Vec<SemanticModelSemanticaExplorerSchemasSearchResultItem>,
    /// Original total in SearchResultResponse
    #[serde(rename = "total", deserialize_with = "crate::json::deserialize_preserved")]
    pub total: serde_json::Number,
    /// Original query in SearchResultResponse
    #[serde(rename = "query")]
    pub query: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasSearchResultResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasSearchResultResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::SearchResultResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "e0137ecf9a5264fabe48e6d2faac8cb45c144fbfe76e2c45aa474156b5d32e2e";
}

/// Original SemanticNeighborItem model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasSemanticNeighborItem {
    /// Original id in SemanticNeighborItem
    #[serde(rename = "id")]
    pub id: String,
    /// Original type in SemanticNeighborItem
    #[serde(rename = "type")]
    pub field_type: String,
    /// Original content in SemanticNeighborItem
    #[serde(rename = "content")]
    pub content: String,
    /// Original similarity in SemanticNeighborItem
    #[serde(rename = "similarity")]
    pub similarity: f64,
    /// Original hop_distance in SemanticNeighborItem
    #[serde(rename = "hop_distance", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub hop_distance: Option<serde_json::Number>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasSemanticNeighborItem {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasSemanticNeighborItem {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::SemanticNeighborItem";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "c3458a25e654ee4ead17c01f9b69e872322c391a6cde44f626b02c424b3f3646";
}

/// Original SemanticNeighborhoodResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse {
    /// Original anchor_node in SemanticNeighborhoodResponse
    #[serde(rename = "anchor_node")]
    pub anchor_node: String,
    /// Original neighbors in SemanticNeighborhoodResponse
    #[serde(rename = "neighbors")]
    pub neighbors: Vec<SemanticModelSemanticaExplorerSchemasSemanticNeighborItem>,
    /// Original total in SemanticNeighborhoodResponse
    #[serde(rename = "total", deserialize_with = "crate::json::deserialize_preserved")]
    pub total: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::SemanticNeighborhoodResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "e1f370422324446e7b8f8ad273cd4a53f13732d438f9a36b0f1864e2d99d776e";
}

/// Original StandardMessageResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasStandardMessageResponse {
    /// Original status in StandardMessageResponse
    #[serde(rename = "status")]
    pub status: String,
    /// Original message in StandardMessageResponse
    #[serde(rename = "message")]
    pub message: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasStandardMessageResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasStandardMessageResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::StandardMessageResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "adcbb4b793e59cf0776203ddff3b1ed92a075cd77fb1797bf45af1ef6be9586a";
}

/// Original TemporalDiffResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasTemporalDiffResponse {
    /// Original from_time in TemporalDiffResponse
    #[serde(rename = "from_time")]
    pub from_time: String,
    /// Original to_time in TemporalDiffResponse
    #[serde(rename = "to_time")]
    pub to_time: String,
    /// Original added_nodes in TemporalDiffResponse
    #[serde(rename = "added_nodes")]
    pub added_nodes: Vec<String>,
    /// Original removed_nodes in TemporalDiffResponse
    #[serde(rename = "removed_nodes")]
    pub removed_nodes: Vec<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasTemporalDiffResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasTemporalDiffResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::TemporalDiffResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "f22633ab5279398f1af5f10fbd5e40fd090858f657a86b98cf3e331e454a32a4";
}

/// Original TemporalPatternResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasTemporalPatternResponse {
    /// Original patterns in TemporalPatternResponse
    #[serde(rename = "patterns", deserialize_with = "crate::json::deserialize_preserved")]
    pub patterns: Vec<crate::json::OrderedObject<String, serde_json::Value>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasTemporalPatternResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasTemporalPatternResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::TemporalPatternResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "3e73e15b30aaead58ba441e570755e04dd75a27c762d67cf41b40f5d6d6d2eaf";
}

/// Original TemporalSnapshotResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse {
    /// Original timestamp in TemporalSnapshotResponse
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    /// Original active_nodes in TemporalSnapshotResponse
    #[serde(rename = "active_nodes")]
    pub active_nodes: Vec<SemanticModelSemanticaExplorerSchemasNodeResponse>,
    /// Original active_node_count in TemporalSnapshotResponse
    #[serde(rename = "active_node_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub active_node_count: serde_json::Number,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::TemporalSnapshotResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "516988011a6709e8d243119c8150bf996acec0e91a232a60b762f18e9c417c74";
}

/// Original ValidationIssue model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasValidationIssue {
    /// Original severity in ValidationIssue
    #[serde(rename = "severity")]
    pub severity: String,
    /// Original message in ValidationIssue
    #[serde(rename = "message")]
    pub message: String,
    /// Original node_id in ValidationIssue
    #[serde(rename = "node_id")]
    pub node_id: Option<String>,
    /// Original edge_source in ValidationIssue
    #[serde(rename = "edge_source")]
    pub edge_source: Option<String>,
    /// Original edge_target in ValidationIssue
    #[serde(rename = "edge_target")]
    pub edge_target: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasValidationIssue {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasValidationIssue {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ValidationIssue";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "eca63f419572c4d2836a92e736fdebce4ac6a24323c278af1da1aca8e0d53821";
}

/// Original ValidationReportResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasValidationReportResponse {
    /// Original valid in ValidationReportResponse
    #[serde(rename = "valid")]
    pub valid: bool,
    /// Original error_count in ValidationReportResponse
    #[serde(rename = "error_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub error_count: serde_json::Number,
    /// Original warning_count in ValidationReportResponse
    #[serde(rename = "warning_count", deserialize_with = "crate::json::deserialize_preserved")]
    pub warning_count: serde_json::Number,
    /// Original issues in ValidationReportResponse
    #[serde(rename = "issues")]
    pub issues: Vec<SemanticModelSemanticaExplorerSchemasValidationIssue>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasValidationReportResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasValidationReportResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::ValidationReportResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "1a2b5c8ab31da50d51c0febf4e79b86912adc962769501070b8c626976ef1ecf";
}

/// Original VocabularyImportResponse model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasVocabularyImportResponse {
    /// Original status in VocabularyImportResponse
    #[serde(rename = "status")]
    pub status: String,
    /// Original filename in VocabularyImportResponse
    #[serde(rename = "filename")]
    pub filename: Option<String>,
    /// Original nodes_added in VocabularyImportResponse
    #[serde(rename = "nodes_added", deserialize_with = "crate::json::deserialize_preserved")]
    pub nodes_added: serde_json::Number,
    /// Original edges_added in VocabularyImportResponse
    #[serde(rename = "edges_added", deserialize_with = "crate::json::deserialize_preserved")]
    pub edges_added: serde_json::Number,
    /// Original format in VocabularyImportResponse
    #[serde(rename = "format")]
    pub format: String,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasVocabularyImportResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasVocabularyImportResponse {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::VocabularyImportResponse";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "753a59f0d3ac2cd193f87c359d8985c7c75416de179e8283a64c1cbca846463a";
}

/// Original VocabularyScheme model from semantica/explorer/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaExplorerSchemasVocabularyScheme {
    /// Original uri in VocabularyScheme
    #[serde(rename = "uri")]
    pub uri: String,
    /// Original label in VocabularyScheme
    #[serde(rename = "label")]
    pub label: String,
    /// Original description in VocabularyScheme
    #[serde(rename = "description")]
    pub description: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaExplorerSchemasVocabularyScheme {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaExplorerSchemasVocabularyScheme {
    const MODEL_ID: &'static str = "semantica.explorer.schemas::VocabularyScheme";
    const SOURCE_SHA256: &'static str = "37903bd99739297e450a3db57d8637c12ed56a4e02b48205466a185a47a50237";
    const CLASS_SHA256: &'static str = "bf34f2fdfe76a8109d138365dc47ef738eb1822a8b351895401e97ebe35ed596";
}

/// Original EntitiesResponse model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse {
    /// Original entities in EntitiesResponse
    #[serde(rename = "entities")]
    pub entities: Vec<SemanticModelSemanticaSemanticExtractSchemasEntityOut>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse {
    const MODEL_ID: &'static str = "semantica.semantic_extract.schemas::EntitiesResponse";
    const SOURCE_SHA256: &'static str = "deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d";
    const CLASS_SHA256: &'static str = "f9b09770b854b30e6fca15eda5356b987436e506abd8892302bcef9420a087d5";
}

/// Original EntityOut model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaSemanticExtractSchemasEntityOut {
    /// The text content of the entity
    #[serde(rename = "text")]
    pub text: String,
    /// The type or label of the entity (e.g., PERSON, ORG)
    #[serde(rename = "label")]
    pub label: String,
    /// Start character index
    #[serde(rename = "start", deserialize_with = "crate::json::deserialize_preserved")]
    pub start: serde_json::Number,
    /// End character index
    #[serde(rename = "end", deserialize_with = "crate::json::deserialize_preserved")]
    pub end: serde_json::Number,
    /// Confidence score between 0 and 1
    #[serde(rename = "confidence")]
    pub confidence: f64,
    /// Additional metadata including provenance
    #[serde(rename = "metadata", deserialize_with = "crate::json::deserialize_preserved")]
    pub metadata: crate::json::OrderedObject<String, serde_json::Value>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaSemanticExtractSchemasEntityOut {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaSemanticExtractSchemasEntityOut {
    const MODEL_ID: &'static str = "semantica.semantic_extract.schemas::EntityOut";
    const SOURCE_SHA256: &'static str = "deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d";
    const CLASS_SHA256: &'static str = "a01e96bbfc9edc9f121eb5619f9f837db69f1a82061dd3a2b0ce95fa9161fd5c";
}

/// Original RelationOut model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaSemanticExtractSchemasRelationOut {
    /// Source entity text
    #[serde(rename = "subject")]
    pub subject: String,
    /// Target entity text
    #[serde(rename = "object")]
    pub object: String,
    /// Relation type or predicate
    #[serde(rename = "predicate")]
    pub predicate: String,
    /// Confidence score between 0 and 1
    #[serde(rename = "confidence")]
    pub confidence: f64,
    /// Additional metadata including provenance
    #[serde(rename = "metadata", deserialize_with = "crate::json::deserialize_preserved")]
    pub metadata: crate::json::OrderedObject<String, serde_json::Value>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaSemanticExtractSchemasRelationOut {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaSemanticExtractSchemasRelationOut {
    const MODEL_ID: &'static str = "semantica.semantic_extract.schemas::RelationOut";
    const SOURCE_SHA256: &'static str = "deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d";
    const CLASS_SHA256: &'static str = "cc884973447ccf6739aacd2f858bd7add8dc416191b8452de00329b728a70787";
}

/// Original RelationWithTemporalOut model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut {
    /// Source entity text
    #[serde(rename = "subject")]
    pub subject: String,
    /// Target entity text
    #[serde(rename = "object")]
    pub object: String,
    /// Relation type or predicate
    #[serde(rename = "predicate")]
    pub predicate: String,
    /// Confidence score between 0 and 1
    #[serde(rename = "confidence")]
    pub confidence: f64,
    /// Additional metadata including provenance
    #[serde(rename = "metadata", deserialize_with = "crate::json::deserialize_preserved")]
    pub metadata: crate::json::OrderedObject<String, serde_json::Value>,
    /// ISO 8601 date or natural-language phrase for when this relation became valid. Null if no temporal signal in text.
    #[serde(rename = "valid_from")]
    pub valid_from: Option<String>,
    /// ISO 8601 date or phrase for when this relation ceased to be valid. Null if open-ended or not stated.
    #[serde(rename = "valid_until")]
    pub valid_until: Option<String>,
    /// Confidence that temporal information was present and correctly extracted. 0.0 if no temporal signal.
    #[serde(rename = "temporal_confidence")]
    pub temporal_confidence: f64,
    /// Exact verbatim substring from the source text containing the temporal signal. Null when temporal_confidence is 0.0.
    #[serde(rename = "temporal_source_text")]
    pub temporal_source_text: Option<String>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut {
    const MODEL_ID: &'static str = "semantica.semantic_extract.schemas::RelationWithTemporalOut";
    const SOURCE_SHA256: &'static str = "deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d";
    const CLASS_SHA256: &'static str = "6c35112e8aa1fececd792485969e7d0d770f4e1848114288b6b424817b1efb7e";
}

/// Original RelationsResponse model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaSemanticExtractSchemasRelationsResponse {
    /// Original relations in RelationsResponse
    #[serde(rename = "relations")]
    pub relations: Vec<SemanticModelSemanticaSemanticExtractSchemasRelationOut>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaSemanticExtractSchemasRelationsResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaSemanticExtractSchemasRelationsResponse {
    const MODEL_ID: &'static str = "semantica.semantic_extract.schemas::RelationsResponse";
    const SOURCE_SHA256: &'static str = "deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d";
    const CLASS_SHA256: &'static str = "d13b38b4692f3fe0a1f9579b74c8edc6ab0204769fbdc5b569c506bd916916fe";
}

/// Original RelationsWithTemporalResponse model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse {
    /// Original relations in RelationsWithTemporalResponse
    #[serde(rename = "relations")]
    pub relations: Vec<SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse {
    const MODEL_ID: &'static str = "semantica.semantic_extract.schemas::RelationsWithTemporalResponse";
    const SOURCE_SHA256: &'static str = "deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d";
    const CLASS_SHA256: &'static str = "03eef9327c4a200448cfbf5e58143586d88330d8abb2d53c7a2ef3cd1c957d26";
}

/// Original TripletOut model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaSemanticExtractSchemasTripletOut {
    /// Subject of the triplet
    #[serde(rename = "subject")]
    pub subject: String,
    /// Predicate or relation
    #[serde(rename = "predicate")]
    pub predicate: String,
    /// Object of the triplet
    #[serde(rename = "object")]
    pub object: String,
    /// Confidence score between 0 and 1
    #[serde(rename = "confidence")]
    pub confidence: f64,
    /// Additional metadata including provenance
    #[serde(rename = "metadata", deserialize_with = "crate::json::deserialize_preserved")]
    pub metadata: crate::json::OrderedObject<String, serde_json::Value>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaSemanticExtractSchemasTripletOut {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaSemanticExtractSchemasTripletOut {
    const MODEL_ID: &'static str = "semantica.semantic_extract.schemas::TripletOut";
    const SOURCE_SHA256: &'static str = "deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d";
    const CLASS_SHA256: &'static str = "a31ab3bdd2cb623f9082b782f287ccd3f954a893a97da5585ea22382c995e845";
}

/// Original TripletsResponse model from semantica/semantic_extract/schemas.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaSemanticExtractSchemasTripletsResponse {
    /// Original triplets in TripletsResponse
    #[serde(rename = "triplets")]
    pub triplets: Vec<SemanticModelSemanticaSemanticExtractSchemasTripletOut>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaSemanticExtractSchemasTripletsResponse {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaSemanticExtractSchemasTripletsResponse {
    const MODEL_ID: &'static str = "semantica.semantic_extract.schemas::TripletsResponse";
    const SOURCE_SHA256: &'static str = "deafb123dbbb18dca55cec352c7669f400d7dafc0e1650679657d9dc95fee70d";
    const CLASS_SHA256: &'static str = "839afdbceed0cee360449fde8de6ab92cf803045d9c1ba622115ab05b18b7df3";
}

/// Original BuildRequest model from semantica/server.py; fields and methods retain explicit Pydantic profile; strong view only after original schema validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModelSemanticaServerBuildRequest {
    /// Original sources in BuildRequest
    #[serde(rename = "sources")]
    pub sources: Vec<String>,
    /// Original config in BuildRequest
    #[serde(rename = "config", default, deserialize_with = "crate::json::deserialize_preserved")]
    pub config: Option<crate::json::OrderedObject<String, serde_json::Value>>,
}
impl crate::models::sealed::Sealed for SemanticModelSemanticaServerBuildRequest {}
impl crate::SemanticModelSnapshot for SemanticModelSemanticaServerBuildRequest {
    const MODEL_ID: &'static str = "semantica.server::BuildRequest";
    const SOURCE_SHA256: &'static str = "41cd1a55c79dc6f11f0e544d8725823a6f06f967608cdacdd01e69eb2d72bd0f";
    const CLASS_SHA256: &'static str = "08ba7ff8ab3b8bb6af617d6e2e257486d58014044a3c4501197e40e5bd3102fe";
}

/// Complete generated choice of strong original model views. Variants are data,
/// never proof of validation or engineering adoption; runtime owns validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum SemanticRegisteredModelSnapshot {
    SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput(SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput),
    SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput(SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput),
    SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest(SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest),
    SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse(SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse),
    SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion(SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion),
    SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest(SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest),
    SemanticModelSemanticaExplorerRoutesOntologyCommentRequest(SemanticModelSemanticaExplorerRoutesOntologyCommentRequest),
    SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest(SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest),
    SemanticModelSemanticaExplorerRoutesOntologyDraftDiff(SemanticModelSemanticaExplorerRoutesOntologyDraftDiff),
    SemanticModelSemanticaExplorerRoutesOntologyDraftRequest(SemanticModelSemanticaExplorerRoutesOntologyDraftRequest),
    SemanticModelSemanticaExplorerRoutesOntologyDraftResponse(SemanticModelSemanticaExplorerRoutesOntologyDraftResponse),
    SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse(SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse),
    SemanticModelSemanticaExplorerRoutesOntologyHealthDimension(SemanticModelSemanticaExplorerRoutesOntologyHealthDimension),
    SemanticModelSemanticaExplorerRoutesOntologyHealthIssue(SemanticModelSemanticaExplorerRoutesOntologyHealthIssue),
    SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest(SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest),
    SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse(SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse),
    SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment(SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment),
    SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest(SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest),
    SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry(SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry),
    SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse(SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse),
    SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview(SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview),
    SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult(SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult),
    SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest(SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest),
    SemanticModelSemanticaExplorerRoutesOntologyProposalRequest(SemanticModelSemanticaExplorerRoutesOntologyProposalRequest),
    SemanticModelSemanticaExplorerRoutesOntologyProposalResponse(SemanticModelSemanticaExplorerRoutesOntologyProposalResponse),
    SemanticModelSemanticaExplorerRoutesOntologyProposalState(SemanticModelSemanticaExplorerRoutesOntologyProposalState),
    SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse(SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse),
    SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail(SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail),
    SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest(SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest),
    SemanticModelSemanticaExplorerRoutesOntologySKOSScheme(SemanticModelSemanticaExplorerRoutesOntologySKOSScheme),
    SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest(SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest),
    SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse(SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse),
    SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary(SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary),
    SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse(SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse),
    SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest(SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest),
    SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse(SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse),
    SemanticModelSemanticaExplorerRoutesOntologyShaclViolation(SemanticModelSemanticaExplorerRoutesOntologyShaclViolation),
    SemanticModelSemanticaExplorerRoutesOntologyToggleResponse(SemanticModelSemanticaExplorerRoutesOntologyToggleResponse),
    SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest(SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest),
    SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse(SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse),
    SemanticModelSemanticaExplorerRoutesOntologyVersionEntry(SemanticModelSemanticaExplorerRoutesOntologyVersionEntry),
    SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest(SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest),
    SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse(SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse),
    SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse(SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse),
    SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse(SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse),
    SemanticModelSemanticaExplorerSchemasAnalyticsResponse(SemanticModelSemanticaExplorerSchemasAnalyticsResponse),
    SemanticModelSemanticaExplorerSchemasAnnotationCreate(SemanticModelSemanticaExplorerSchemasAnnotationCreate),
    SemanticModelSemanticaExplorerSchemasAnnotationResponse(SemanticModelSemanticaExplorerSchemasAnnotationResponse),
    SemanticModelSemanticaExplorerSchemasCausalChainResponse(SemanticModelSemanticaExplorerSchemasCausalChainResponse),
    SemanticModelSemanticaExplorerSchemasCausalDistanceReport(SemanticModelSemanticaExplorerSchemasCausalDistanceReport),
    SemanticModelSemanticaExplorerSchemasComplianceResponse(SemanticModelSemanticaExplorerSchemasComplianceResponse),
    SemanticModelSemanticaExplorerSchemasConceptNode(SemanticModelSemanticaExplorerSchemasConceptNode),
    SemanticModelSemanticaExplorerSchemasConceptSummary(SemanticModelSemanticaExplorerSchemasConceptSummary),
    SemanticModelSemanticaExplorerSchemasDecisionResponse(SemanticModelSemanticaExplorerSchemasDecisionResponse),
    SemanticModelSemanticaExplorerSchemasDedupRequest(SemanticModelSemanticaExplorerSchemasDedupRequest),
    SemanticModelSemanticaExplorerSchemasDedupResponse(SemanticModelSemanticaExplorerSchemasDedupResponse),
    SemanticModelSemanticaExplorerSchemasDistanceEvent(SemanticModelSemanticaExplorerSchemasDistanceEvent),
    SemanticModelSemanticaExplorerSchemasDistanceExportRequest(SemanticModelSemanticaExplorerSchemasDistanceExportRequest),
    SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse(SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse),
    SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest(SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest),
    SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse(SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse),
    SemanticModelSemanticaExplorerSchemasDistanceSnapshot(SemanticModelSemanticaExplorerSchemasDistanceSnapshot),
    SemanticModelSemanticaExplorerSchemasEdgeListResponse(SemanticModelSemanticaExplorerSchemasEdgeListResponse),
    SemanticModelSemanticaExplorerSchemasEdgeResponse(SemanticModelSemanticaExplorerSchemasEdgeResponse),
    SemanticModelSemanticaExplorerSchemasEnrichExtractRequest(SemanticModelSemanticaExplorerSchemasEnrichExtractRequest),
    SemanticModelSemanticaExplorerSchemasEnrichExtractResponse(SemanticModelSemanticaExplorerSchemasEnrichExtractResponse),
    SemanticModelSemanticaExplorerSchemasErrorResponse(SemanticModelSemanticaExplorerSchemasErrorResponse),
    SemanticModelSemanticaExplorerSchemasExportRequest(SemanticModelSemanticaExplorerSchemasExportRequest),
    SemanticModelSemanticaExplorerSchemasExportResponse(SemanticModelSemanticaExplorerSchemasExportResponse),
    SemanticModelSemanticaExplorerSchemasGraphStatsResponse(SemanticModelSemanticaExplorerSchemasGraphStatsResponse),
    SemanticModelSemanticaExplorerSchemasImportResponse(SemanticModelSemanticaExplorerSchemasImportResponse),
    SemanticModelSemanticaExplorerSchemasLinkPredictionRequest(SemanticModelSemanticaExplorerSchemasLinkPredictionRequest),
    SemanticModelSemanticaExplorerSchemasLinkPredictionResponse(SemanticModelSemanticaExplorerSchemasLinkPredictionResponse),
    SemanticModelSemanticaExplorerSchemasMergeRequest(SemanticModelSemanticaExplorerSchemasMergeRequest),
    SemanticModelSemanticaExplorerSchemasMergeResponse(SemanticModelSemanticaExplorerSchemasMergeResponse),
    SemanticModelSemanticaExplorerSchemasNeighborResponse(SemanticModelSemanticaExplorerSchemasNeighborResponse),
    SemanticModelSemanticaExplorerSchemasNodeListResponse(SemanticModelSemanticaExplorerSchemasNodeListResponse),
    SemanticModelSemanticaExplorerSchemasNodeResponse(SemanticModelSemanticaExplorerSchemasNodeResponse),
    SemanticModelSemanticaExplorerSchemasPathResponse(SemanticModelSemanticaExplorerSchemasPathResponse),
    SemanticModelSemanticaExplorerSchemasProvenanceEdge(SemanticModelSemanticaExplorerSchemasProvenanceEdge),
    SemanticModelSemanticaExplorerSchemasProvenanceNode(SemanticModelSemanticaExplorerSchemasProvenanceNode),
    SemanticModelSemanticaExplorerSchemasProvenanceResponse(SemanticModelSemanticaExplorerSchemasProvenanceResponse),
    SemanticModelSemanticaExplorerSchemasReasoningRequest(SemanticModelSemanticaExplorerSchemasReasoningRequest),
    SemanticModelSemanticaExplorerSchemasReasoningResponse(SemanticModelSemanticaExplorerSchemasReasoningResponse),
    SemanticModelSemanticaExplorerSchemasSearchRequest(SemanticModelSemanticaExplorerSchemasSearchRequest),
    SemanticModelSemanticaExplorerSchemasSearchResultItem(SemanticModelSemanticaExplorerSchemasSearchResultItem),
    SemanticModelSemanticaExplorerSchemasSearchResultResponse(SemanticModelSemanticaExplorerSchemasSearchResultResponse),
    SemanticModelSemanticaExplorerSchemasSemanticNeighborItem(SemanticModelSemanticaExplorerSchemasSemanticNeighborItem),
    SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse(SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse),
    SemanticModelSemanticaExplorerSchemasStandardMessageResponse(SemanticModelSemanticaExplorerSchemasStandardMessageResponse),
    SemanticModelSemanticaExplorerSchemasTemporalDiffResponse(SemanticModelSemanticaExplorerSchemasTemporalDiffResponse),
    SemanticModelSemanticaExplorerSchemasTemporalPatternResponse(SemanticModelSemanticaExplorerSchemasTemporalPatternResponse),
    SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse(SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse),
    SemanticModelSemanticaExplorerSchemasValidationIssue(SemanticModelSemanticaExplorerSchemasValidationIssue),
    SemanticModelSemanticaExplorerSchemasValidationReportResponse(SemanticModelSemanticaExplorerSchemasValidationReportResponse),
    SemanticModelSemanticaExplorerSchemasVocabularyImportResponse(SemanticModelSemanticaExplorerSchemasVocabularyImportResponse),
    SemanticModelSemanticaExplorerSchemasVocabularyScheme(SemanticModelSemanticaExplorerSchemasVocabularyScheme),
    SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse(SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse),
    SemanticModelSemanticaSemanticExtractSchemasEntityOut(SemanticModelSemanticaSemanticExtractSchemasEntityOut),
    SemanticModelSemanticaSemanticExtractSchemasRelationOut(SemanticModelSemanticaSemanticExtractSchemasRelationOut),
    SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut(SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut),
    SemanticModelSemanticaSemanticExtractSchemasRelationsResponse(SemanticModelSemanticaSemanticExtractSchemasRelationsResponse),
    SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse(SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse),
    SemanticModelSemanticaSemanticExtractSchemasTripletOut(SemanticModelSemanticaSemanticExtractSchemasTripletOut),
    SemanticModelSemanticaSemanticExtractSchemasTripletsResponse(SemanticModelSemanticaSemanticExtractSchemasTripletsResponse),
    SemanticModelSemanticaServerBuildRequest(SemanticModelSemanticaServerBuildRequest),
}
impl SemanticRegisteredModelSnapshot {
    pub fn model_id(&self) -> &'static str { match self {
        Self::SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput(_) => <SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput(_) => <SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion(_) => <SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyCommentRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyCommentRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyDraftDiff(_) => <SemanticModelSemanticaExplorerRoutesOntologyDraftDiff as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyDraftRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyDraftRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyDraftResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyDraftResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyHealthDimension(_) => <SemanticModelSemanticaExplorerRoutesOntologyHealthDimension as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyHealthIssue(_) => <SemanticModelSemanticaExplorerRoutesOntologyHealthIssue as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment(_) => <SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry(_) => <SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview(_) => <SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult(_) => <SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyProposalRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyProposalRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyProposalResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyProposalResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyProposalState(_) => <SemanticModelSemanticaExplorerRoutesOntologyProposalState as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail(_) => <SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologySKOSScheme(_) => <SemanticModelSemanticaExplorerRoutesOntologySKOSScheme as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary(_) => <SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclViolation(_) => <SemanticModelSemanticaExplorerRoutesOntologyShaclViolation as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyToggleResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyToggleResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest(_) => <SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse(_) => <SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesOntologyVersionEntry(_) => <SemanticModelSemanticaExplorerRoutesOntologyVersionEntry as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest(_) => <SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse(_) => <SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse(_) => <SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse(_) => <SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasAnalyticsResponse(_) => <SemanticModelSemanticaExplorerSchemasAnalyticsResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasAnnotationCreate(_) => <SemanticModelSemanticaExplorerSchemasAnnotationCreate as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasAnnotationResponse(_) => <SemanticModelSemanticaExplorerSchemasAnnotationResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasCausalChainResponse(_) => <SemanticModelSemanticaExplorerSchemasCausalChainResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasCausalDistanceReport(_) => <SemanticModelSemanticaExplorerSchemasCausalDistanceReport as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasComplianceResponse(_) => <SemanticModelSemanticaExplorerSchemasComplianceResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasConceptNode(_) => <SemanticModelSemanticaExplorerSchemasConceptNode as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasConceptSummary(_) => <SemanticModelSemanticaExplorerSchemasConceptSummary as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasDecisionResponse(_) => <SemanticModelSemanticaExplorerSchemasDecisionResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasDedupRequest(_) => <SemanticModelSemanticaExplorerSchemasDedupRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasDedupResponse(_) => <SemanticModelSemanticaExplorerSchemasDedupResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasDistanceEvent(_) => <SemanticModelSemanticaExplorerSchemasDistanceEvent as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasDistanceExportRequest(_) => <SemanticModelSemanticaExplorerSchemasDistanceExportRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse(_) => <SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest(_) => <SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse(_) => <SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasDistanceSnapshot(_) => <SemanticModelSemanticaExplorerSchemasDistanceSnapshot as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasEdgeListResponse(_) => <SemanticModelSemanticaExplorerSchemasEdgeListResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasEdgeResponse(_) => <SemanticModelSemanticaExplorerSchemasEdgeResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasEnrichExtractRequest(_) => <SemanticModelSemanticaExplorerSchemasEnrichExtractRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasEnrichExtractResponse(_) => <SemanticModelSemanticaExplorerSchemasEnrichExtractResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasErrorResponse(_) => <SemanticModelSemanticaExplorerSchemasErrorResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasExportRequest(_) => <SemanticModelSemanticaExplorerSchemasExportRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasExportResponse(_) => <SemanticModelSemanticaExplorerSchemasExportResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasGraphStatsResponse(_) => <SemanticModelSemanticaExplorerSchemasGraphStatsResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasImportResponse(_) => <SemanticModelSemanticaExplorerSchemasImportResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasLinkPredictionRequest(_) => <SemanticModelSemanticaExplorerSchemasLinkPredictionRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasLinkPredictionResponse(_) => <SemanticModelSemanticaExplorerSchemasLinkPredictionResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasMergeRequest(_) => <SemanticModelSemanticaExplorerSchemasMergeRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasMergeResponse(_) => <SemanticModelSemanticaExplorerSchemasMergeResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasNeighborResponse(_) => <SemanticModelSemanticaExplorerSchemasNeighborResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasNodeListResponse(_) => <SemanticModelSemanticaExplorerSchemasNodeListResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasNodeResponse(_) => <SemanticModelSemanticaExplorerSchemasNodeResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasPathResponse(_) => <SemanticModelSemanticaExplorerSchemasPathResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasProvenanceEdge(_) => <SemanticModelSemanticaExplorerSchemasProvenanceEdge as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasProvenanceNode(_) => <SemanticModelSemanticaExplorerSchemasProvenanceNode as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasProvenanceResponse(_) => <SemanticModelSemanticaExplorerSchemasProvenanceResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasReasoningRequest(_) => <SemanticModelSemanticaExplorerSchemasReasoningRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasReasoningResponse(_) => <SemanticModelSemanticaExplorerSchemasReasoningResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasSearchRequest(_) => <SemanticModelSemanticaExplorerSchemasSearchRequest as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasSearchResultItem(_) => <SemanticModelSemanticaExplorerSchemasSearchResultItem as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasSearchResultResponse(_) => <SemanticModelSemanticaExplorerSchemasSearchResultResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasSemanticNeighborItem(_) => <SemanticModelSemanticaExplorerSchemasSemanticNeighborItem as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse(_) => <SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasStandardMessageResponse(_) => <SemanticModelSemanticaExplorerSchemasStandardMessageResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasTemporalDiffResponse(_) => <SemanticModelSemanticaExplorerSchemasTemporalDiffResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasTemporalPatternResponse(_) => <SemanticModelSemanticaExplorerSchemasTemporalPatternResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse(_) => <SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasValidationIssue(_) => <SemanticModelSemanticaExplorerSchemasValidationIssue as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasValidationReportResponse(_) => <SemanticModelSemanticaExplorerSchemasValidationReportResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasVocabularyImportResponse(_) => <SemanticModelSemanticaExplorerSchemasVocabularyImportResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaExplorerSchemasVocabularyScheme(_) => <SemanticModelSemanticaExplorerSchemasVocabularyScheme as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse(_) => <SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaSemanticExtractSchemasEntityOut(_) => <SemanticModelSemanticaSemanticExtractSchemasEntityOut as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaSemanticExtractSchemasRelationOut(_) => <SemanticModelSemanticaSemanticExtractSchemasRelationOut as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut(_) => <SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaSemanticExtractSchemasRelationsResponse(_) => <SemanticModelSemanticaSemanticExtractSchemasRelationsResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse(_) => <SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaSemanticExtractSchemasTripletOut(_) => <SemanticModelSemanticaSemanticExtractSchemasTripletOut as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaSemanticExtractSchemasTripletsResponse(_) => <SemanticModelSemanticaSemanticExtractSchemasTripletsResponse as crate::SemanticModelSnapshot>::MODEL_ID,
        Self::SemanticModelSemanticaServerBuildRequest(_) => <SemanticModelSemanticaServerBuildRequest as crate::SemanticModelSnapshot>::MODEL_ID,
    } }
    pub fn source_identity(&self) -> (&'static str, &'static str) { match self {
        Self::SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput(_) => (<SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput(_) => (<SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion(_) => (<SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyCommentRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyCommentRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyCommentRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyDraftDiff(_) => (<SemanticModelSemanticaExplorerRoutesOntologyDraftDiff as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyDraftDiff as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyDraftRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyDraftRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyDraftRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyDraftResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyDraftResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyDraftResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyHealthDimension(_) => (<SemanticModelSemanticaExplorerRoutesOntologyHealthDimension as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyHealthDimension as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyHealthIssue(_) => (<SemanticModelSemanticaExplorerRoutesOntologyHealthIssue as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyHealthIssue as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment(_) => (<SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry(_) => (<SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview(_) => (<SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult(_) => (<SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyProposalRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyProposalRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyProposalRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyProposalResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyProposalResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyProposalResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyProposalState(_) => (<SemanticModelSemanticaExplorerRoutesOntologyProposalState as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyProposalState as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail(_) => (<SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologySKOSScheme(_) => (<SemanticModelSemanticaExplorerRoutesOntologySKOSScheme as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologySKOSScheme as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary(_) => (<SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyShaclViolation(_) => (<SemanticModelSemanticaExplorerRoutesOntologyShaclViolation as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyShaclViolation as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyToggleResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyToggleResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyToggleResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest(_) => (<SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse(_) => (<SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesOntologyVersionEntry(_) => (<SemanticModelSemanticaExplorerRoutesOntologyVersionEntry as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesOntologyVersionEntry as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest(_) => (<SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse(_) => (<SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse(_) => (<SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse(_) => (<SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasAnalyticsResponse(_) => (<SemanticModelSemanticaExplorerSchemasAnalyticsResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasAnalyticsResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasAnnotationCreate(_) => (<SemanticModelSemanticaExplorerSchemasAnnotationCreate as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasAnnotationCreate as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasAnnotationResponse(_) => (<SemanticModelSemanticaExplorerSchemasAnnotationResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasAnnotationResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasCausalChainResponse(_) => (<SemanticModelSemanticaExplorerSchemasCausalChainResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasCausalChainResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasCausalDistanceReport(_) => (<SemanticModelSemanticaExplorerSchemasCausalDistanceReport as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasCausalDistanceReport as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasComplianceResponse(_) => (<SemanticModelSemanticaExplorerSchemasComplianceResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasComplianceResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasConceptNode(_) => (<SemanticModelSemanticaExplorerSchemasConceptNode as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasConceptNode as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasConceptSummary(_) => (<SemanticModelSemanticaExplorerSchemasConceptSummary as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasConceptSummary as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasDecisionResponse(_) => (<SemanticModelSemanticaExplorerSchemasDecisionResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasDecisionResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasDedupRequest(_) => (<SemanticModelSemanticaExplorerSchemasDedupRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasDedupRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasDedupResponse(_) => (<SemanticModelSemanticaExplorerSchemasDedupResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasDedupResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasDistanceEvent(_) => (<SemanticModelSemanticaExplorerSchemasDistanceEvent as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasDistanceEvent as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasDistanceExportRequest(_) => (<SemanticModelSemanticaExplorerSchemasDistanceExportRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasDistanceExportRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse(_) => (<SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest(_) => (<SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse(_) => (<SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasDistanceSnapshot(_) => (<SemanticModelSemanticaExplorerSchemasDistanceSnapshot as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasDistanceSnapshot as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasEdgeListResponse(_) => (<SemanticModelSemanticaExplorerSchemasEdgeListResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasEdgeListResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasEdgeResponse(_) => (<SemanticModelSemanticaExplorerSchemasEdgeResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasEdgeResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasEnrichExtractRequest(_) => (<SemanticModelSemanticaExplorerSchemasEnrichExtractRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasEnrichExtractRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasEnrichExtractResponse(_) => (<SemanticModelSemanticaExplorerSchemasEnrichExtractResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasEnrichExtractResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasErrorResponse(_) => (<SemanticModelSemanticaExplorerSchemasErrorResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasErrorResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasExportRequest(_) => (<SemanticModelSemanticaExplorerSchemasExportRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasExportRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasExportResponse(_) => (<SemanticModelSemanticaExplorerSchemasExportResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasExportResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasGraphStatsResponse(_) => (<SemanticModelSemanticaExplorerSchemasGraphStatsResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasGraphStatsResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasImportResponse(_) => (<SemanticModelSemanticaExplorerSchemasImportResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasImportResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasLinkPredictionRequest(_) => (<SemanticModelSemanticaExplorerSchemasLinkPredictionRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasLinkPredictionRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasLinkPredictionResponse(_) => (<SemanticModelSemanticaExplorerSchemasLinkPredictionResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasLinkPredictionResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasMergeRequest(_) => (<SemanticModelSemanticaExplorerSchemasMergeRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasMergeRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasMergeResponse(_) => (<SemanticModelSemanticaExplorerSchemasMergeResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasMergeResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasNeighborResponse(_) => (<SemanticModelSemanticaExplorerSchemasNeighborResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasNeighborResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasNodeListResponse(_) => (<SemanticModelSemanticaExplorerSchemasNodeListResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasNodeListResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasNodeResponse(_) => (<SemanticModelSemanticaExplorerSchemasNodeResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasNodeResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasPathResponse(_) => (<SemanticModelSemanticaExplorerSchemasPathResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasPathResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasProvenanceEdge(_) => (<SemanticModelSemanticaExplorerSchemasProvenanceEdge as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasProvenanceEdge as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasProvenanceNode(_) => (<SemanticModelSemanticaExplorerSchemasProvenanceNode as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasProvenanceNode as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasProvenanceResponse(_) => (<SemanticModelSemanticaExplorerSchemasProvenanceResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasProvenanceResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasReasoningRequest(_) => (<SemanticModelSemanticaExplorerSchemasReasoningRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasReasoningRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasReasoningResponse(_) => (<SemanticModelSemanticaExplorerSchemasReasoningResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasReasoningResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasSearchRequest(_) => (<SemanticModelSemanticaExplorerSchemasSearchRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasSearchRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasSearchResultItem(_) => (<SemanticModelSemanticaExplorerSchemasSearchResultItem as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasSearchResultItem as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasSearchResultResponse(_) => (<SemanticModelSemanticaExplorerSchemasSearchResultResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasSearchResultResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasSemanticNeighborItem(_) => (<SemanticModelSemanticaExplorerSchemasSemanticNeighborItem as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasSemanticNeighborItem as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse(_) => (<SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasStandardMessageResponse(_) => (<SemanticModelSemanticaExplorerSchemasStandardMessageResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasStandardMessageResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasTemporalDiffResponse(_) => (<SemanticModelSemanticaExplorerSchemasTemporalDiffResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasTemporalDiffResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasTemporalPatternResponse(_) => (<SemanticModelSemanticaExplorerSchemasTemporalPatternResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasTemporalPatternResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse(_) => (<SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasValidationIssue(_) => (<SemanticModelSemanticaExplorerSchemasValidationIssue as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasValidationIssue as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasValidationReportResponse(_) => (<SemanticModelSemanticaExplorerSchemasValidationReportResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasValidationReportResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasVocabularyImportResponse(_) => (<SemanticModelSemanticaExplorerSchemasVocabularyImportResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasVocabularyImportResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaExplorerSchemasVocabularyScheme(_) => (<SemanticModelSemanticaExplorerSchemasVocabularyScheme as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaExplorerSchemasVocabularyScheme as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse(_) => (<SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaSemanticExtractSchemasEntityOut(_) => (<SemanticModelSemanticaSemanticExtractSchemasEntityOut as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaSemanticExtractSchemasEntityOut as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaSemanticExtractSchemasRelationOut(_) => (<SemanticModelSemanticaSemanticExtractSchemasRelationOut as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaSemanticExtractSchemasRelationOut as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut(_) => (<SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaSemanticExtractSchemasRelationsResponse(_) => (<SemanticModelSemanticaSemanticExtractSchemasRelationsResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaSemanticExtractSchemasRelationsResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse(_) => (<SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaSemanticExtractSchemasTripletOut(_) => (<SemanticModelSemanticaSemanticExtractSchemasTripletOut as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaSemanticExtractSchemasTripletOut as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaSemanticExtractSchemasTripletsResponse(_) => (<SemanticModelSemanticaSemanticExtractSchemasTripletsResponse as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaSemanticExtractSchemasTripletsResponse as crate::SemanticModelSnapshot>::CLASS_SHA256),
        Self::SemanticModelSemanticaServerBuildRequest(_) => (<SemanticModelSemanticaServerBuildRequest as crate::SemanticModelSnapshot>::SOURCE_SHA256, <SemanticModelSemanticaServerBuildRequest as crate::SemanticModelSnapshot>::CLASS_SHA256),
    } }
}
/// Representation conversion only. Callers must validate using the same schema
/// runtime first; serde deserialization does not run original model validators.
pub fn semantic_model_snapshot_from_normalized_json(model_id: &str, input: serde_json::Value) -> Result<SemanticRegisteredModelSnapshot, serde_json::Error> {
    match model_id {
        "integrations.crewai.decision_tool::SemanticaDecisionToolInput" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelIntegrationsCrewaiDecisionToolSemanticaDecisionToolInput),
        "integrations.crewai.kg_tool::SemanticaKGToolInput" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelIntegrationsCrewaiKgToolSemanticaKGToolInput),
        "semantica.explorer.routes.ontology::AlignmentRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyAlignmentRequest),
        "semantica.explorer.routes.ontology::AlignmentResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyAlignmentResponse),
        "semantica.explorer.routes.ontology::AlignmentSuggestion" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestion),
        "semantica.explorer.routes.ontology::AlignmentSuggestionRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyAlignmentSuggestionRequest),
        "semantica.explorer.routes.ontology::CommentRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyCommentRequest),
        "semantica.explorer.routes.ontology::CreateOntologyRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyCreateOntologyRequest),
        "semantica.explorer.routes.ontology::DraftDiff" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyDraftDiff),
        "semantica.explorer.routes.ontology::DraftRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyDraftRequest),
        "semantica.explorer.routes.ontology::DraftResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyDraftResponse),
        "semantica.explorer.routes.ontology::EntityDetailResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyEntityDetailResponse),
        "semantica.explorer.routes.ontology::HealthDimension" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyHealthDimension),
        "semantica.explorer.routes.ontology::HealthIssue" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyHealthIssue),
        "semantica.explorer.routes.ontology::LoadOntologyRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyRequest),
        "semantica.explorer.routes.ontology::LoadOntologyResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyLoadOntologyResponse),
        "semantica.explorer.routes.ontology::OntologyAlignment" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignment),
        "semantica.explorer.routes.ontology::OntologyAlignmentRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyOntologyAlignmentRequest),
        "semantica.explorer.routes.ontology::OntologyEntry" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyOntologyEntry),
        "semantica.explorer.routes.ontology::OntologyHealthResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyOntologyHealthResponse),
        "semantica.explorer.routes.ontology::OntologyPreview" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyOntologyPreview),
        "semantica.explorer.routes.ontology::OntologySearchResult" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyOntologySearchResult),
        "semantica.explorer.routes.ontology::PreviewOntologyRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyPreviewOntologyRequest),
        "semantica.explorer.routes.ontology::ProposalRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyProposalRequest),
        "semantica.explorer.routes.ontology::ProposalResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyProposalResponse),
        "semantica.explorer.routes.ontology::ProposalState" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyProposalState),
        "semantica.explorer.routes.ontology::RefreshResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyRefreshResponse),
        "semantica.explorer.routes.ontology::SKOSConceptDetail" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologySKOSConceptDetail),
        "semantica.explorer.routes.ontology::SKOSConceptSearchRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologySKOSConceptSearchRequest),
        "semantica.explorer.routes.ontology::SKOSScheme" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologySKOSScheme),
        "semantica.explorer.routes.ontology::ShaclGenerateRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateRequest),
        "semantica.explorer.routes.ontology::ShaclGenerateResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyShaclGenerateResponse),
        "semantica.explorer.routes.ontology::ShaclShapeSummary" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyShaclShapeSummary),
        "semantica.explorer.routes.ontology::ShaclShapesResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyShaclShapesResponse),
        "semantica.explorer.routes.ontology::ShaclValidateRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyShaclValidateRequest),
        "semantica.explorer.routes.ontology::ShaclValidationResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyShaclValidationResponse),
        "semantica.explorer.routes.ontology::ShaclViolation" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyShaclViolation),
        "semantica.explorer.routes.ontology::ToggleResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyToggleResponse),
        "semantica.explorer.routes.ontology::VersionCompareRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyVersionCompareRequest),
        "semantica.explorer.routes.ontology::VersionCompareResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyVersionCompareResponse),
        "semantica.explorer.routes.ontology::VersionEntry" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesOntologyVersionEntry),
        "semantica.explorer.routes.sparql::SparqlRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesSparqlSparqlRequest),
        "semantica.explorer.routes.sparql::SparqlResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesSparqlSparqlResponse),
        "semantica.explorer.routes.temporal::TemporalBoundsResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesTemporalTemporalBoundsResponse),
        "semantica.explorer.routes.temporal::TemporalSnapshotFastResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerRoutesTemporalTemporalSnapshotFastResponse),
        "semantica.explorer.schemas::AnalyticsResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasAnalyticsResponse),
        "semantica.explorer.schemas::AnnotationCreate" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasAnnotationCreate),
        "semantica.explorer.schemas::AnnotationResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasAnnotationResponse),
        "semantica.explorer.schemas::CausalChainResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasCausalChainResponse),
        "semantica.explorer.schemas::CausalDistanceReport" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasCausalDistanceReport),
        "semantica.explorer.schemas::ComplianceResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasComplianceResponse),
        "semantica.explorer.schemas::ConceptNode" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasConceptNode),
        "semantica.explorer.schemas::ConceptSummary" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasConceptSummary),
        "semantica.explorer.schemas::DecisionResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasDecisionResponse),
        "semantica.explorer.schemas::DedupRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasDedupRequest),
        "semantica.explorer.schemas::DedupResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasDedupResponse),
        "semantica.explorer.schemas::DistanceEvent" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasDistanceEvent),
        "semantica.explorer.schemas::DistanceExportRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasDistanceExportRequest),
        "semantica.explorer.schemas::DistanceHistoryResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasDistanceHistoryResponse),
        "semantica.explorer.schemas::DistanceMatrixRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasDistanceMatrixRequest),
        "semantica.explorer.schemas::DistanceMatrixResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasDistanceMatrixResponse),
        "semantica.explorer.schemas::DistanceSnapshot" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasDistanceSnapshot),
        "semantica.explorer.schemas::EdgeListResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasEdgeListResponse),
        "semantica.explorer.schemas::EdgeResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasEdgeResponse),
        "semantica.explorer.schemas::EnrichExtractRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasEnrichExtractRequest),
        "semantica.explorer.schemas::EnrichExtractResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasEnrichExtractResponse),
        "semantica.explorer.schemas::ErrorResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasErrorResponse),
        "semantica.explorer.schemas::ExportRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasExportRequest),
        "semantica.explorer.schemas::ExportResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasExportResponse),
        "semantica.explorer.schemas::GraphStatsResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasGraphStatsResponse),
        "semantica.explorer.schemas::ImportResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasImportResponse),
        "semantica.explorer.schemas::LinkPredictionRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasLinkPredictionRequest),
        "semantica.explorer.schemas::LinkPredictionResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasLinkPredictionResponse),
        "semantica.explorer.schemas::MergeRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasMergeRequest),
        "semantica.explorer.schemas::MergeResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasMergeResponse),
        "semantica.explorer.schemas::NeighborResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasNeighborResponse),
        "semantica.explorer.schemas::NodeListResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasNodeListResponse),
        "semantica.explorer.schemas::NodeResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasNodeResponse),
        "semantica.explorer.schemas::PathResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasPathResponse),
        "semantica.explorer.schemas::ProvenanceEdge" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasProvenanceEdge),
        "semantica.explorer.schemas::ProvenanceNode" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasProvenanceNode),
        "semantica.explorer.schemas::ProvenanceResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasProvenanceResponse),
        "semantica.explorer.schemas::ReasoningRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasReasoningRequest),
        "semantica.explorer.schemas::ReasoningResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasReasoningResponse),
        "semantica.explorer.schemas::SearchRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasSearchRequest),
        "semantica.explorer.schemas::SearchResultItem" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasSearchResultItem),
        "semantica.explorer.schemas::SearchResultResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasSearchResultResponse),
        "semantica.explorer.schemas::SemanticNeighborItem" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasSemanticNeighborItem),
        "semantica.explorer.schemas::SemanticNeighborhoodResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasSemanticNeighborhoodResponse),
        "semantica.explorer.schemas::StandardMessageResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasStandardMessageResponse),
        "semantica.explorer.schemas::TemporalDiffResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasTemporalDiffResponse),
        "semantica.explorer.schemas::TemporalPatternResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasTemporalPatternResponse),
        "semantica.explorer.schemas::TemporalSnapshotResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasTemporalSnapshotResponse),
        "semantica.explorer.schemas::ValidationIssue" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasValidationIssue),
        "semantica.explorer.schemas::ValidationReportResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasValidationReportResponse),
        "semantica.explorer.schemas::VocabularyImportResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasVocabularyImportResponse),
        "semantica.explorer.schemas::VocabularyScheme" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaExplorerSchemasVocabularyScheme),
        "semantica.semantic_extract.schemas::EntitiesResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaSemanticExtractSchemasEntitiesResponse),
        "semantica.semantic_extract.schemas::EntityOut" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaSemanticExtractSchemasEntityOut),
        "semantica.semantic_extract.schemas::RelationOut" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaSemanticExtractSchemasRelationOut),
        "semantica.semantic_extract.schemas::RelationWithTemporalOut" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaSemanticExtractSchemasRelationWithTemporalOut),
        "semantica.semantic_extract.schemas::RelationsResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaSemanticExtractSchemasRelationsResponse),
        "semantica.semantic_extract.schemas::RelationsWithTemporalResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaSemanticExtractSchemasRelationsWithTemporalResponse),
        "semantica.semantic_extract.schemas::TripletOut" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaSemanticExtractSchemasTripletOut),
        "semantica.semantic_extract.schemas::TripletsResponse" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaSemanticExtractSchemasTripletsResponse),
        "semantica.server::BuildRequest" => serde_json::from_value(input).map(SemanticRegisteredModelSnapshot::SemanticModelSemanticaServerBuildRequest),
        _ => Err(<serde_json::Error as serde::de::Error>::custom("schema_model_unknown")),
    }
}

pub trait SemanticBookCatalogPort {
    fn read(&self, input: &SemanticBookCatalogRequest) -> Result<SemanticBookCatalogResult, SemanticRuntimeFailure>;
}

pub trait SemanticDatalogGraphPort {
    fn datalog_graph(&self, input: &SemanticDatalogGraphRequest) -> Result<SemanticDatalogGraphResult, SemanticRuntimeFailure>;
}

pub trait SemanticDatalogPort {
    fn datalog(&self, input: &SemanticDatalogRequest) -> Result<SemanticDatalogResult, SemanticRuntimeFailure>;
}

pub trait SemanticDatalogSessionPort {
    fn datalog_session(&self, input: &SemanticDatalogSessionRequest) -> Result<SemanticDatalogSessionResult, SemanticRuntimeFailure>;
}

pub trait SemanticDatasetIdentityDiagnosticPort {
    fn diagnose_identity(&self, input: &SemanticRdfLibSnapshotRequest) -> Result<SemanticDatasetIdentityDiagnostic, SemanticRuntimeFailure>;
}

pub trait SemanticEmbeddingGenerationPort {
    fn embed(&self, input: &SemanticEmbeddingRequest) -> Result<SemanticProviderResult, SemanticProviderFailure>;
}

pub trait SemanticEmbeddingPoolingPort {
    fn pool(&self, input: &SemanticEmbeddingPoolRequest) -> Result<SemanticEmbeddingPoolResult, SemanticProviderFailure>;
}

pub trait SemanticExplorerGraphStatsPort {
    fn graph_stats(&self, input: &SemanticExplorerGraphStatsRequest) -> Result<SemanticExplorerGraphStats, SemanticRuntimeFailure>;
}

pub trait SemanticExtractionPort {
    fn extract(&self, input: &SemanticExtractionRequest) -> Result<SemanticExtractionResult, SemanticRuntimeFailure>;
}

pub trait SemanticMcpPort {
    fn dispatch(&self, input: &SemanticMcpRequest) -> Result<SemanticMcpResult, SemanticRuntimeFailure>;
}

pub trait SemanticNamedGraphValidationPort {
    fn validate_named_graph(&self, input: &SemanticNamedGraphValidationRequest) -> Result<SemanticNamedGraphValidationResult, SemanticRuntimeFailure>;
}

pub trait SemanticNativeInferencePort {
    fn execute(&self, input: &SemanticNativeRequest) -> Result<SemanticNativeResult, SemanticProviderFailure>;
}

pub trait SemanticNativeResultGuardPort {
    fn check_native_result(&self, input: &SemanticNativeResult) -> Result<SemanticProviderResultGuard, SemanticProviderFailure>;
}

pub trait SemanticPromotedReviewPort1 {
    fn review_promoted_current(&self, input: &SemanticPromotedReviewRequest1) -> Result<SemanticPromotedReviewResult1, SemanticRuntimeFailure>;
}

pub trait SemanticProvenancePort {
    fn project(&self, input: &SemanticProvenanceBatchRequest) -> Result<SemanticProvenanceProjection, SemanticRuntimeFailure>;
}

pub trait SemanticProviderConstructionPort {
    fn construct(&self, input: &SemanticProviderConstructionRequest) -> Result<SemanticProviderInstance, SemanticProviderFactoryFailure>;
}

pub trait SemanticProviderFactoryPort {
    fn apply(&self, input: &SemanticProviderFactoryRequest) -> Result<SemanticProviderFactoryResult, SemanticProviderFactoryFailure>;
}

pub trait SemanticProviderGenerationPort {
    fn generate(&self, input: &SemanticProviderRequest) -> Result<SemanticProviderResult, SemanticProviderFailure>;
}

pub trait SemanticProviderResultGuardPort {
    fn check_result(&self, input: &SemanticProviderResult) -> Result<SemanticProviderResultGuard, SemanticProviderFailure>;
}

pub trait SemanticProviderTransportPort {
    fn post(&self, input: &SemanticProviderWireRequest) -> Result<SemanticProviderWireResponse, SemanticProviderFailure>;
}

pub trait SemanticQueryOptionsPortV2 {
    fn query_with_options_v2(&self, input: &SemanticQueryOptionsRequestV2) -> Result<SemanticQueryOptionsResultV2, SemanticRuntimeFailure>;
}

pub trait SemanticQueryOptionsPortV3 {
    fn query_with_options_v3(&self, input: &SemanticQueryOptionsRequestV3) -> Result<SemanticQueryOptionsResultV3, SemanticRuntimeFailure>;
}

pub trait SemanticQueryOptionsPortV4 {
    fn query_with_options_v4(&self, input: &SemanticQueryOptionsRequestV4) -> Result<SemanticQueryOptionsResultV4, SemanticRuntimeFailure>;
}

pub trait SemanticQueryPort {
    fn query(&self, input: &SemanticQueryRequest) -> Result<SemanticQueryResult, SemanticRuntimeFailure>;
}

pub trait SemanticRdfLibSnapshotPort {
    fn snapshot_legacy(&self, input: &SemanticRdfLibSnapshotRequest) -> Result<SemanticRdfLibDatasetSnapshot, SemanticRuntimeFailure>;
}

pub trait SemanticReasonPort {
    fn reason(&self, input: &SemanticReasonRequest) -> Result<SemanticReasonResult, SemanticRuntimeFailure>;
}

pub trait SemanticRefineryPort {
    fn execute(&self, input: &SemanticRefineryRequest) -> Result<SemanticRefineryResult, SemanticRefineryFailure>;
}

pub trait SemanticSchemaValidationPort {
    fn validate_schema(&self, input: &SemanticSchemaValidationRequest) -> Result<SemanticSchemaValidationResult, SemanticSchemaFailure>;
}

pub trait SemanticServiceBudgetPort {
    fn charge(&self, input: &SemanticServiceBudgetCharge) -> Result<SemanticServiceBudgetReceipt, SemanticRuntimeFailure>;
}

pub trait SemanticServiceQueryPort {
    fn query(&self, input: &SemanticServiceQueryRequest) -> Result<SemanticServiceQueryResult, SemanticRuntimeFailure>;
}

pub trait SemanticSnapshotChecksumPort {
    fn snapshot_checksum(&self, input: &SemanticSnapshotChecksumRequest) -> Result<SemanticSnapshotChecksumResult, SemanticRuntimeFailure>;
}

pub trait SemanticSourceSessionPortV1 {
    fn execute_source_session(&self, input: &SemanticSourceSessionRequest) -> Result<SemanticSourceSessionResult, SemanticRuntimeFailure>;
}

pub trait SemanticSourceSessionPortV2 {
    fn execute_source_session2(&self, input: &SemanticSourceSessionRequest2) -> Result<SemanticSourceSessionResult2, SemanticRuntimeFailure>;
}

pub trait SemanticSourceSessionPortV3 {
    fn execute_source_session3(&self, input: &SemanticSourceSessionRequest3) -> Result<SemanticSourceSessionResult3, SemanticRuntimeFailure>;
}

pub trait SemanticSourceSessionPortV4 {
    fn execute_source_session4(&self, input: &SemanticSourceSessionRequest4) -> Result<SemanticSourceSessionResult4, SemanticRuntimeFailure>;
}

pub trait SemanticTemporalConsistencyPort {
    fn temporal_consistency(&self, input: &SemanticTemporalConsistencyRequest) -> Result<SemanticTemporalConsistencyResult, SemanticRuntimeFailure>;
}

pub trait SemanticTemporalFactsPort {
    fn temporal_facts(&self, input: &SemanticTemporalFactsRequest) -> Result<SemanticTemporalFactsResult, SemanticRuntimeFailure>;
}

pub trait SemanticTemporalPathsPort {
    fn temporal_paths(&self, input: &SemanticTemporalPathsRequest) -> Result<SemanticTemporalPathsResult, SemanticRuntimeFailure>;
}

pub trait SemanticTemporalPatternsPort {
    fn temporal_patterns(&self, input: &SemanticTemporalPatternsRequest) -> Result<SemanticTemporalPatternsResult, SemanticRuntimeFailure>;
}

pub trait SemanticTemporalPort {
    fn temporal(&self, input: &SemanticTemporalRequest) -> Result<SemanticTemporalResult, SemanticRuntimeFailure>;
}

pub trait SemanticTemporalRangePort {
    fn temporal_range(&self, input: &SemanticTemporalRangeRequest) -> Result<SemanticTemporalRangeResult, SemanticRuntimeFailure>;
}

pub trait SemanticTemporalSnapshotPort {
    fn temporal_snapshot(&self, input: &SemanticTemporalSnapshotRequest) -> Result<SemanticTemporalSnapshotResult, SemanticRuntimeFailure>;
}

pub trait SemanticTemporalVersionDiffPort {
    fn temporal_version_diff(&self, input: &SemanticTemporalVersionDiffRequest) -> Result<SemanticTemporalVersionDiffResult, SemanticRuntimeFailure>;
}

pub trait SemanticTemporalVersionRecordsPort {
    fn temporal_version_records(&self, input: &SemanticTemporalVersionRecordRequest) -> Result<SemanticTemporalVersionRecordResult, SemanticRuntimeFailure>;
}

pub trait SemanticTypedDispatchPort {
    fn dispatch_typed(&self, input: &SemanticTypedDispatchRequest) -> Result<SemanticTypedDispatchResult, SemanticProviderFailure>;
}

pub trait SemanticTypedGenerationPort {
    fn generate_typed(&self, input: &SemanticTypedGenerationRequest) -> Result<SemanticTypedGenerationResult, SemanticProviderFailure>;
}

pub trait SemanticTypedRelationPort8 {
    fn extract_typed_relations8(&self, input: &SemanticTypedRelationRequest8) -> Result<SemanticTypedRelationResult8, SemanticProviderFailure>;
}

