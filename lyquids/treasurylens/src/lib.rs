//! TreasuryLens: snapshot-bound historical ERC-20 outflow certified by a 3-of-4 committee.
//! The LDK protocol pattern is attributed in docs/ATTRIBUTION.md.
use lyquid::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use treasurylens_core::{
    Observation, QuerySpec, Summary, TRANSFER_TOPIC, select_quorum, summarize_logs,
};

const CHAIN_ID: u64 = 31337;
const THRESHOLD: usize = 3;
const CONFIRMATIONS: u64 = 2;
const MAX_RECORDS: usize = 200;
const MAX_LOGS: usize = 10_000;
const MAX_EVIDENCE_BYTES: usize = 1024 * 1024;
const REPLAY_SCOPE: &str = "source-replay-not-new-certificate";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Defaults {
    token: String,
    treasury: String,
    from_block: u64,
    to_block: u64,
    budget: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    query: QuerySpec,
    summary: Summary,
    supporters: Vec<String>,
    certifiers: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeProposal {
    summary: Option<Summary>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QueryRequest {
    token: String,
    treasury: String,
    from_block: u64,
    to_block: u64,
    #[serde(default)]
    budget: Option<String>,
}

state! {
    network initializer: Address = Address::ZERO;
    network oracle treasury;
    network committee: Vec<NodeID> = Vec::new();
    network defaults: Defaults = Defaults::default();
    network records: HashMap<String, Record> = new_hashmap();
    network record_order: Vec<String> = Vec::new();
    instance last_observation: Option<Summary> = None;
    instance fault: String = String::new();
}

fn err(message: impl ToString) -> LyquidError {
    LyquidError::LyquorRuntime(message.to_string())
}
fn rpc(method: &str, params: Value) -> LyquidResult<Value> {
    let response = lyquor_api::http_request(
        http::Request {
            method: http::Method::Post,
            url: lyquid::well_known::SEQUENCER_RPC_API.into(),
            headers: vec![http::Header {
                name: "content-type".into(),
                value: b"application/json".to_vec(),
            }],
            body: Some(
                json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
                    .to_string()
                    .into_bytes(),
            ),
        },
        Some(http::RequestOptions {
            timeout_ms: Some(3000),
        }),
    )?;
    if response.status != 200 {
        return Err(err(format!("RPC HTTP {}", response.status)));
    }
    let body: Value = serde_json::from_slice(&response.body).map_err(err)?;
    if !body["error"].is_null() {
        return Err(err(format!("RPC error: {}", body["error"])));
    }
    body.get("result")
        .cloned()
        .ok_or_else(|| err("RPC result missing"))
}
fn quantity(v: &Value) -> LyquidResult<u64> {
    treasurylens_core::parse_quantity(v.as_str().ok_or_else(|| err("invalid RPC quantity"))?)
        .map_err(err)
}
fn block_hash(number: u64) -> LyquidResult<String> {
    let block = rpc(
        "eth_getBlockByNumber",
        json!([format!("0x{number:x}"), false]),
    )?;
    if quantity(&block["number"])? != number {
        return Err(err("RPC returned the wrong block height"));
    }
    let hash = block["hash"]
        .as_str()
        .ok_or_else(|| err("block not available"))?;
    treasurylens_core::normalize_hash(hash).map_err(err)
}
fn validate_chain() -> LyquidResult<()> {
    if quantity(&rpc("eth_chainId", json!([]))?)? != CHAIN_ID {
        return Err(err("unexpected source chain"));
    }
    Ok(())
}
fn source_logs(query: &QuerySpec) -> LyquidResult<Vec<Value>> {
    let query = query.normalized().map_err(err)?;
    if query.chain_id != CHAIN_ID {
        return Err(err("unsupported query chain"));
    }
    validate_chain()?;
    if rpc(
        "eth_getCode",
        json!([query.token, format!("0x{:x}", query.to_block)]),
    )?
    .as_str()
    .is_none_or(|code| code == "0x" || code == "0x0")
    {
        return Err(err("no token contract at snapshot"));
    }
    let latest = quantity(&rpc("eth_blockNumber", json!([]))?)?;
    if latest < query.to_block.saturating_add(CONFIRMATIONS) {
        return Err(err("query endpoint needs two successor blocks"));
    }
    if block_hash(query.to_block)? != query.snapshot_hash {
        return Err(err("snapshot mismatch before reading"));
    }
    // Exact ERC-20 address + Transfer(from=treasury) filter. Recipient is unrestricted.
    let from_topic = format!("0x{:0>64}", &query.treasury[2..]);
    let result = rpc(
        "eth_getLogs",
        json!([{"address":query.token,"fromBlock":format!("0x{:x}",query.from_block),
        "toBlock":format!("0x{:x}",query.to_block),"topics":[TRANSFER_TOPIC,from_topic]}]),
    )?;
    let logs = result
        .as_array()
        .ok_or_else(|| err("RPC logs must be an array"))?;
    if logs.len() > MAX_LOGS {
        return Err(err("query exceeds 10000 log limit"));
    }
    if block_hash(query.to_block)? != query.snapshot_hash {
        return Err(err("snapshot changed while reading"));
    }
    Ok(logs.clone())
}
fn observe(query: &QuerySpec) -> LyquidResult<Summary> {
    let logs = source_logs(query)?;
    summarize_logs(query, &logs).map_err(err)
}

// Optional display metadata from the source RPC. Never included in a certificate
// or used to round/convert the certified integer total.
fn token_metadata(query: &QuerySpec) -> Option<Value> {
    let encoded = rpc(
        "eth_call",
        json!([{"to":query.token,"data":"0x313ce567"},
        format!("0x{:x}",query.to_block)]),
    )
    .ok()?;
    let encoded = encoded.as_str()?.strip_prefix("0x")?;
    if encoded.len() != 64 || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let decimals = U256::from_str_radix(encoded, 16).ok()?;
    if decimals > U256::from(255) {
        return None;
    }
    let decimals: u8 = decimals.to_string().parse().ok()?;
    Some(
        json!({"token":query.token,"decimals":decimals,"block_number":query.to_block,
        "scope":"source-rpc-not-certified"}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordRequest {
    query_id: String,
}

fn parse_record_request(req: http::Request) -> Result<String, http::Response> {
    let bytes = req.body.unwrap_or_default();
    if bytes.len() > 8192 {
        return Err(json_response(
            413,
            json!({"status":"unavailable","error":"request too large"}),
        ));
    }
    let input: RecordRequest = serde_json::from_slice(&bytes).map_err(|error| {
        json_response(
            400,
            json!({"status":"unavailable","error":error.to_string()}),
        )
    })?;
    treasurylens_core::normalize_hash(&input.query_id).map_err(|error| {
        json_response(
            400,
            json!({"status":"unavailable","error":error.to_string()}),
        )
    })
}
fn json_response(status: u16, body: Value) -> http::Response {
    http::Response {
        status,
        headers: vec![
            http::Header {
                name: "content-type".into(),
                value: b"application/json; charset=utf-8".to_vec(),
            },
            http::Header {
                name: "cache-control".into(),
                value: b"no-store".to_vec(),
            },
            http::Header {
                name: "x-content-type-options".into(),
                value: b"nosniff".to_vec(),
            },
        ],
        body: body.to_string().into_bytes(),
    }
}

#[method::network(export = eth)]
fn constructor(ctx: &mut _) {
    *ctx.network.initializer = ctx.caller;
}

#[method::network(export = eth, eth_guard = creator)]
fn configure_committee(ctx: &mut _, node_ids: Vec<NodeID>) -> LyquidResult<bool> {
    if ctx.caller != *ctx.network.initializer {
        return Err(err("initializer only"));
    }
    if !ctx.network.committee.is_empty() {
        return Err(err("committee already configured"));
    }
    let unique: std::collections::BTreeSet<_> = node_ids.iter().map(ToString::to_string).collect();
    if node_ids.len() != 4 || unique.len() != 4 {
        return Err(err("exactly four distinct nodes required"));
    }
    let target = OracleTarget {
        seq_id: lyquor_api::sequence_backend_id()?,
        target: OracleServiceTarget::LVM(ctx.lyquid_id),
    };
    let oracle = ctx.network.treasury.clone();
    if !oracle.initialize(&mut ctx, target, node_ids.clone(), THRESHOLD as u16) {
        return Err(err("oracle initialization rejected"));
    }
    *ctx.network.committee = node_ids;
    Ok(true)
}

#[method::network(export = eth, eth_guard = creator)]
fn set_defaults(
    ctx: &mut _,
    token: String,
    treasury: String,
    from_block: u64,
    to_block: u64,
    budget: String,
) -> LyquidResult<bool> {
    if ctx.caller != *ctx.network.initializer {
        return Err(err("initializer only"));
    }
    let token = treasurylens_core::normalize_address(&token).map_err(err)?;
    let treasury = treasurylens_core::normalize_address(&treasury).map_err(err)?;
    if from_block > to_block {
        return Err(err("invalid block range"));
    }
    let _: U256 = budget.parse().map_err(err)?;
    *ctx.network.defaults = Defaults {
        token,
        treasury,
        from_block,
        to_block,
        budget,
    };
    Ok(true)
}

#[method::network(export = eth)]
fn get_node_ids(ctx: &_) -> LyquidResult<Vec<String>> {
    Ok(ctx
        .network
        .committee
        .iter()
        .map(ToString::to_string)
        .collect())
}
#[method::instance(export = eth)]
fn get_observation_json(ctx: &_) -> LyquidResult<String> {
    serde_json::to_string(&*ctx.instance.last_observation.read()).map_err(err)
}

#[method::network(export = eth)]
fn get_result_json(ctx: &_, query_id: String) -> LyquidResult<String> {
    Ok(ctx
        .network
        .records
        .get(&query_id)
        .map(|r| serde_json::to_string(r).unwrap_or_default())
        .unwrap_or_else(|| "null".into()))
}

#[method::instance(group = oracle::two_phase::treasury)]
fn propose(ctx: &mut _, query_json: String, target: OracleTarget) -> LyquidResult<NodeProposal> {
    (|| -> LyquidResult<NodeProposal> {
        let expected = OracleTarget {
            seq_id: lyquor_api::sequence_backend_id()?,
            target: OracleServiceTarget::LVM(ctx.lyquid_id),
        };
        if target != expected {
            return Err(err("wrong oracle target"));
        }
        let query: QuerySpec = serde_json::from_str(&query_json).map_err(err)?;
        let summary = match observe(&query) {
            Ok(value) => value,
            Err(error) => {
                return Ok(NodeProposal {
                    summary: None,
                    error: Some(error.to_string()),
                });
            }
        };
        #[cfg(feature = "test-controls")]
        let summary = {
            let mut result = summary;
            match ctx.instance.fault.read().as_str() {
                "wrong_total" => {
                    let v: U256 = result.total.parse().map_err(err)?;
                    result.total = v
                        .checked_add(U256::from(1))
                        .ok_or_else(|| err("fault overflow"))?
                        .to_string();
                }
                "stale_snapshot" => result.snapshot_hash = format!("0x{}", "11".repeat(32)),
                "unavailable" => {
                    return Ok(NodeProposal {
                        summary: None,
                        error: Some("injected source-read failure".into()),
                    });
                }
                _ => {}
            }
            result
        };
        *ctx.instance.last_observation.write() = Some(summary.clone());
        Ok(NodeProposal {
            summary: Some(summary),
            error: None,
        })
    })()
}

#[method::instance(group = oracle::two_phase::treasury)]
fn aggregate(ctx: &_) -> LyquidResult<Option<CertifiedCallParams>> {
    let init = decode_by_fields!(ctx.init,query_json:String,target:OracleTarget)
        .ok_or(LyquidError::LyquorInput)?;
    let expected = OracleTarget {
        seq_id: lyquor_api::sequence_backend_id()?,
        target: OracleServiceTarget::LVM(ctx.lyquid_id),
    };
    if init.target != expected {
        return Ok(None);
    }
    let query: QuerySpec = serde_json::from_str(&init.query_json).map_err(err)?;
    let query = query.normalized().map_err(err)?;
    if query.chain_id != CHAIN_ID {
        return Ok(None);
    }
    let mut observations = Vec::new();
    for input in ctx.inputs {
        // The LDK verifies each signed input belongs to the active committee.
        // Aggregation deliberately has no network-state access.
        if let Some(proposal) = lyquor_primitives::decode_object::<NodeProposal>(&input.input) {
            if let Some(summary) = proposal.summary {
                observations.push(Observation {
                    node_id: input.from.to_string(),
                    summary,
                });
            }
        }
    }
    let Some(quorum) = select_quorum(&query, &observations, THRESHOLD).map_err(err)? else {
        // A signed source-read failure is a response, not an absent packet. All four
        // replies with no valid quorum can deterministically certify rejection.
        if ctx.inputs.len() == 4 {
            return Ok(Some(CertifiedCallParams {
                origin: Address::ZERO,
                method: "reject_query".into(),
                input: encode_by_fields!(query_id:String=query.query_id().map_err(err)?).into(),
                target: init.target,
            }));
        }
        return Ok(None);
    };
    // Keep output deterministic as new responses arrive: the record commits a fixed-size sorted quorum.
    let supporters = quorum.supporting_nodes;
    let record = Record {
        query,
        summary: quorum.summary,
        supporters,
        certifiers: Vec::new(),
    };
    Ok(Some(CertifiedCallParams {
        origin: Address::ZERO,
        method: "commit_result".into(),
        input: encode_by_fields!(record:Record=record).into(),
        target: init.target,
    }))
}

// A rejection is certified to terminate aggregation; the HTTP path reports it
// without submitting a shared-state update. It never creates a confirmed result.
#[method::network(group = oracle::certified::treasury::two_phase)]
fn reject_query(_ctx: &mut _, _query_id: String) -> LyquidResult<bool> {
    Ok(false)
}

#[method::network(group = oracle::certified::treasury::two_phase)]
fn commit_result(ctx: &mut _, record: Record) -> LyquidResult<bool> {
    let mut record = record;
    let id = record.query.query_id().map_err(err)?;
    if record.summary.query_id != id || record.summary.snapshot_hash != record.query.snapshot_hash {
        return Err(err("invalid result identity"));
    }
    let allowed: std::collections::BTreeSet<_> = ctx
        .network
        .committee
        .iter()
        .map(ToString::to_string)
        .collect();
    let supporters: std::collections::BTreeSet<_> = record.supporters.iter().cloned().collect();
    if supporters.len() < THRESHOLD || !supporters.is_subset(&allowed) {
        return Err(err("invalid supporting committee"));
    }
    let mut certifiers: Vec<String> = ctx
        .cert
        .signers
        .clone()
        .into_iter()
        .filter_map(|id| ctx.signer_node_id(id as u64))
        .map(|id| id.to_string())
        .collect();
    certifiers.sort();
    certifiers.dedup();
    if certifiers.len() < THRESHOLD {
        return Err(err("insufficient certificate signers"));
    }
    record.certifiers = certifiers;
    if let Some(existing) = ctx.network.records.get(&id) {
        if existing.summary != record.summary {
            return Err(err("conflicting duplicate query"));
        }
        return Ok(true);
    }
    if ctx.network.record_order.len() >= MAX_RECORDS {
        let oldest = ctx.network.record_order.remove(0);
        ctx.network.records.remove(&oldest);
    }
    ctx.network.record_order.push(id.clone());
    ctx.network.records.insert(id, record);
    Ok(true)
}

#[method::instance(export = http, method="GET", path_prefix="/api/status")]
fn http_status(ctx: &_, _req: http::Request) -> LyquidResult<http::Response> {
    let target = OracleTarget {
        seq_id: lyquor_api::sequence_backend_id()?,
        target: OracleServiceTarget::LVM(ctx.lyquid_id),
    };
    let active = ctx
        .network
        .treasury
        .config_current(&ctx, target)
        .committee
        .len()
        == 4;
    let latest = rpc("eth_blockNumber", json!([])).and_then(|v| quantity(&v));
    Ok(json_response(
        200,
        json!({"app":"TreasuryLens","lyquid_id":ctx.lyquid_id.to_string(),
        "threshold":THRESHOLD,"committee":ctx.network.committee.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "active":active,"chain_id":CHAIN_ID,"latest_block":latest.ok(),"confirmations":CONFIRMATIONS,
        "defaults":&*ctx.network.defaults,"result_retention":MAX_RECORDS,"test_controls":cfg!(feature="test-controls")}),
    ))
}
#[method::instance(export = http, method="GET", path_prefix="/api/results")]
fn http_results(ctx: &_, _req: http::Request) -> LyquidResult<http::Response> {
    let mut results: Vec<_> = ctx.network.records.values().cloned().collect();
    results.sort_by_key(|r| r.summary.query_id.clone());
    Ok(json_response(200, json!({"results":results})))
}
// Replay the retained query on this node's current source RPC. A match is
// fresh content evidence from one instance, not a new committee certificate.
#[method::instance(export = http, method="POST", path_prefix="/api/audit")]
fn http_audit(ctx: &_, req: http::Request) -> LyquidResult<http::Response> {
    let id = match parse_record_request(req) {
        Ok(id) => id,
        Err(response) => return Ok(response),
    };
    let checked_at_ms = lyquor_api::systime()?;
    let Some(record) = ctx.network.records.get(&id).cloned() else {
        return Ok(json_response(
            404,
            json!({"status":"unavailable","query_id":id,"record":null,
            "scope":REPLAY_SCOPE,"checked_at_ms":checked_at_ms,"error":"no retained certified result"}),
        ));
    };
    if let Err(error) = validate_chain() {
        return Ok(json_response(
            200,
            json!({"status":"unavailable","record":record,
            "scope":REPLAY_SCOPE,"checked_at_ms":checked_at_ms,"error":error.to_string()}),
        ));
    }
    match block_hash(record.query.to_block) {
        Ok(current) if current != record.query.snapshot_hash => {
            return Ok(json_response(
                200,
                json!({"status":"mismatch","record":record,"current_hash":current,
                "scope":REPLAY_SCOPE,"checked_at_ms":checked_at_ms,"error":"source snapshot has changed; no replay details released"}),
            ));
        }
        Err(error) => {
            return Ok(json_response(
                200,
                json!({"status":"unavailable","record":record,
            "scope":REPLAY_SCOPE,"checked_at_ms":checked_at_ms,"error":error.to_string()}),
            ));
        }
        _ => {}
    }
    let logs = match source_logs(&record.query) {
        Ok(logs) => logs,
        Err(error) => {
            // The source might have changed during the fetch. Distinguish an
            // observed changed hash from an RPC/availability failure.
            let current = block_hash(record.query.to_block).ok();
            let changed = current
                .as_ref()
                .is_some_and(|hash| hash != &record.query.snapshot_hash);
            return Ok(json_response(
                200,
                json!({"status":if changed {"mismatch"} else {"unavailable"},
                "record":record,"current_hash":current,"scope":REPLAY_SCOPE,
                "checked_at_ms":checked_at_ms,"error":error.to_string()}),
            ));
        }
    };
    let audit = match treasurylens_core::verify_logs(&record.query, &record.summary, &logs) {
        Ok(audit) => audit,
        Err(error) => {
            return Ok(json_response(
                200,
                json!({"status":"mismatch","record":record,
            "scope":REPLAY_SCOPE,"checked_at_ms":checked_at_ms,"error":error.to_string()}),
            ));
        }
    };
    // Leave space for query/summary/schema fields in a <=1MiB import bundle.
    if serde_json::to_vec(&logs).map_err(err)?.len() > MAX_EVIDENCE_BYTES - 8192 {
        return Ok(json_response(
            200,
            json!({"status":"unavailable","record":record,
            "scope":REPLAY_SCOPE,"checked_at_ms":checked_at_ms,
            "error":"replay log evidence exceeds the export/import size limit; use a smaller query interval"}),
        ));
    }
    let metadata = token_metadata(&record.query);
    let mut body = json!({"status":"matched","record":record,"audit":audit,"logs":logs,
        "scope":REPLAY_SCOPE,"checked_at_ms":checked_at_ms,"certificate_verified":false});
    if let Some(metadata) = metadata {
        body["token_metadata"] = metadata;
    }
    Ok(json_response(200, body))
}

// Check the current source endpoint only; do not retract or mutate records.
#[method::instance(export = http, method="POST", path_prefix="/api/revalidate")]
fn http_revalidate(ctx: &_, req: http::Request) -> LyquidResult<http::Response> {
    let id = match parse_record_request(req) {
        Ok(id) => id,
        Err(response) => return Ok(response),
    };
    let checked_at_ms = lyquor_api::systime()?;
    let warning = "Single-source endpoint-hash check only; two successor blocks are a depth heuristic, not strong finality. The stored certificate is unchanged.";
    let Some(record) = ctx.network.records.get(&id) else {
        return Ok(json_response(
            404,
            json!({"query_id":id,"status":"unavailable","expected_hash":null,
            "checked_at_ms":checked_at_ms,"warning":warning,"error":"no retained certified result"}),
        ));
    };
    let current = (|| -> LyquidResult<(String, u64)> {
        validate_chain()?;
        let hash = block_hash(record.query.to_block)?;
        let latest = quantity(&rpc("eth_blockNumber", json!([]))?)?;
        Ok((hash, latest.saturating_sub(record.query.to_block)))
    })();
    match current {
        Ok((hash, depth)) => Ok(json_response(
            200,
            json!({"query_id":id,
            "status":if hash==record.query.snapshot_hash {"canonical"} else {"changed"},
            "expected_hash":record.query.snapshot_hash,"current_hash":hash,
            "checked_at_ms":checked_at_ms,"confirmations_now":depth,"warning":warning}),
        )),
        Err(error) => Ok(json_response(
            200,
            json!({"query_id":id,"status":"unavailable",
            "expected_hash":record.query.snapshot_hash,"checked_at_ms":checked_at_ms,
            "warning":warning,"error":error.to_string()}),
        )),
    }
}

// Stateless supplied-content verification. No RPC reads, persisted-result
// authentication or cryptographic verification of committee signatures.
#[method::instance(export = http, method="POST", path_prefix="/api/verify-evidence")]
fn http_verify_evidence(_ctx: &_, req: http::Request) -> LyquidResult<http::Response> {
    let bytes = req.body.unwrap_or_default();
    if bytes.len() > MAX_EVIDENCE_BYTES {
        return Ok(json_response(
            413,
            json!({"status":"invalid","certificate_verified":false,
            "scope":"supplied-contents-only","error":"evidence exceeds 1 MiB limit"}),
        ));
    }
    let bundle: treasurylens_core::EvidenceBundle = match serde_json::from_slice(&bytes) {
        Ok(bundle) => bundle,
        Err(error) => {
            return Ok(json_response(
                400,
                json!({"status":"invalid","certificate_verified":false,
            "scope":"supplied-contents-only","error":error.to_string()}),
            ));
        }
    };
    if bundle.logs.len() > MAX_LOGS {
        return Ok(json_response(
            413,
            json!({"status":"invalid","certificate_verified":false,
            "scope":"supplied-contents-only","error":"evidence exceeds 10000 log limit"}),
        ));
    }
    match treasurylens_core::verify_evidence(&bundle) {
        Ok(audit) => Ok(json_response(
            200,
            json!({"status":"valid","audit":audit,
            "certificate_verified":false,"scope":"supplied-contents-only"}),
        )),
        Err(error) => Ok(json_response(
            200,
            json!({"status":"invalid","certificate_verified":false,
            "scope":"supplied-contents-only","error":error.to_string()}),
        )),
    }
}

#[method::instance(export = http, method="POST", path_prefix="/api/query")]
fn http_query(ctx: &mut _, req: http::Request) -> LyquidResult<http::Response> {
    let start = lyquor_api::systime()?;
    let bytes = req.body.unwrap_or_default();
    if bytes.len() > 8192 {
        return Ok(json_response(
            413,
            json!({"status":"failed","error":"request too large"}),
        ));
    }
    let request: QueryRequest = match serde_json::from_slice(&bytes) {
        Ok(q) => q,
        Err(e) => {
            return Ok(json_response(
                400,
                json!({"status":"failed","error":e.to_string()}),
            ));
        }
    };
    if let Some(budget) = &request.budget {
        if budget.parse::<U256>().is_err() {
            return Ok(json_response(
                400,
                json!({"status":"failed","error":"budget must be an unsigned 256-bit integer"}),
            ));
        }
    }
    // Validate public input before issuing any source-chain request.
    let provisional = QuerySpec {
        chain_id: CHAIN_ID,
        token: request.token,
        treasury: request.treasury,
        from_block: request.from_block,
        to_block: request.to_block,
        snapshot_hash: format!("0x{}", "00".repeat(32)),
        rule_version: 1,
    };
    let mut query = match provisional.normalized() {
        Ok(q) => q,
        Err(e) => {
            return Ok(json_response(
                400,
                json!({"status":"failed","error":e.to_string()}),
            ));
        }
    };
    // Check maturity before requesting an endpoint header: future blocks return
    // null from eth_getBlockByNumber and should produce an actionable error.
    // Resolve the snapshot before deriving the query ID or using cached results.
    let preflight = (|| -> LyquidResult<()> {
        validate_chain()?;
        let head = quantity(&rpc("eth_blockNumber", json!([]))?)?;
        if head
            .checked_sub(query.to_block)
            .is_none_or(|depth| depth < CONFIRMATIONS)
        {
            return Err(err(format!(
                "query endpoint needs two successor blocks; latest source block is {head}"
            )));
        }
        query.snapshot_hash = block_hash(query.to_block)?;
        let code = rpc(
            "eth_getCode",
            json!([query.token, format!("0x{:x}", query.to_block)]),
        )?;
        if code.as_str().is_none_or(|c| c == "0x" || c == "0x0") {
            return Err(err("no token contract at snapshot"));
        }
        Ok(())
    })();
    if let Err(e) = preflight {
        return Ok(json_response(
            422,
            json!({"status":"failed","error":e.to_string()}),
        ));
    }
    let id = query.query_id().map_err(err)?;
    if let Some(record) = ctx.network.records.get(&id) {
        return Ok(json_response(
            200,
            json!({"query_id":id,"status":"confirmed","record":record,"elapsed_ms":lyquor_api::systime()?.saturating_sub(start)}),
        ));
    }
    let target = OracleTarget {
        seq_id: lyquor_api::sequence_backend_id()?,
        target: OracleServiceTarget::LVM(ctx.lyquid_id),
    };
    let oracle = ctx.network.treasury.clone();
    let call=oracle.propose_and_certify(&mut ctx,target,
        encode_by_fields!(query_json:String=serde_json::to_string(&query).map_err(err)?,target:OracleTarget=target).into(),
        None,Some(10000));
    match call {
        Ok(Some(call)) if call.method == "reject_query" => Ok(json_response(
            422,
            json!({"status":"failed","query_id":id,
            "error":"committee responses contain no matching 3-node result for this snapshot",
            "elapsed_ms":lyquor_api::systime()?.saturating_sub(start)}),
        )),
        Ok(Some(call)) => match submit_certified_call!(call) {
            Ok(_) => Ok(json_response(
                202,
                json!({"status":"submitted","query_id":id,"elapsed_ms":lyquor_api::systime()?.saturating_sub(start)}),
            )),
            Err(e) => Ok(json_response(
                502,
                json!({"status":"failed","query_id":id,"error":e.to_string()}),
            )),
        },
        Ok(None) => Ok(json_response(
            422,
            json!({"status":"failed","query_id":id,"error":"no matching 3-node quorum or oracle not active","elapsed_ms":lyquor_api::systime()?.saturating_sub(start)}),
        )),
        Err(e) => Ok(json_response(
            502,
            json!({"status":"failed","query_id":id,"error":e.to_string()}),
        )),
    }
}

#[cfg(feature = "test-controls")]
#[method::instance(export = eth)]
fn set_fault(ctx: &mut _, mode: String) -> LyquidResult<bool> {
    if !["", "wrong_total", "stale_snapshot", "unavailable"].contains(&mode.as_str()) {
        return Err(err("unknown test fault"));
    }
    *ctx.instance.fault.write() = mode;
    Ok(true)
}
