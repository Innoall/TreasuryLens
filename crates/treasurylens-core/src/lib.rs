//! Deterministic query, log validation and quorum policy for TreasuryLens.
//!
//! No network calls, clocks, randomness or application authority live here. The
//! Lyquid adapter authenticates identities and obtains RPC responses. A matching
//! quorum is evidence of agreement, not independent proof of chain truth.

use alloy_primitives::{U256, keccak256};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub mod analytics;
pub use analytics::{
    AuditReport, BlockTotal, EVIDENCE_SCHEMA_VERSION, EvidenceBundle, MAX_EVIDENCE_LOGS,
    RecipientTotal, audit_logs, verify_evidence, verify_logs,
};

pub const RULE_VERSION: u16 = 1;
pub const MAX_BLOCK_RANGE: u64 = 10_000;
pub const TRANSFER_TOPIC: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
const QUERY_DOMAIN: &[u8] = b"TreasuryLens/query/v1\0";
const LOGS_DOMAIN: &[u8] = b"TreasuryLens/logs/v1\0";

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreError(pub String);

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for CoreError {}

fn error(message: impl Into<String>) -> CoreError {
    CoreError(message.into())
}

/// The snapshot is the identity of `to_block` and anchors the requested history.
/// Callers must validate that it belongs to their intended canonical chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuerySpec {
    pub chain_id: u64,
    pub token: String,
    pub treasury: String,
    pub from_block: u64,
    pub to_block: u64,
    pub snapshot_hash: String,
    pub rule_version: u16,
}

impl QuerySpec {
    /// Validate and normalize human-entered addresses and hashes once at ingress.
    pub fn normalized(&self) -> CoreResult<Self> {
        if self.rule_version != RULE_VERSION {
            return Err(error("unsupported rule_version"));
        }
        if self.from_block > self.to_block {
            return Err(error("from_block must not exceed to_block"));
        }
        // The subtraction is safe after ordering, and avoids overflowing at MAX.
        if self.to_block - self.from_block >= MAX_BLOCK_RANGE {
            return Err(error("query range exceeds 10000 blocks"));
        }
        Ok(Self {
            chain_id: self.chain_id,
            token: normalize_address(&self.token)?,
            treasury: normalize_address(&self.treasury)?,
            from_block: self.from_block,
            to_block: self.to_block,
            snapshot_hash: normalize_hash(&self.snapshot_hash)?,
            rule_version: self.rule_version,
        })
    }

    /// Domain-separated, fixed-width encoding. All policy fields enter the ID.
    pub fn query_id(&self) -> CoreResult<String> {
        let q = self.normalized()?;
        let mut bytes = QUERY_DOMAIN.to_vec();
        bytes.extend_from_slice(&q.chain_id.to_be_bytes());
        bytes.extend(hex_bytes(&q.token)?);
        bytes.extend(hex_bytes(&q.treasury)?);
        bytes.extend_from_slice(&q.from_block.to_be_bytes());
        bytes.extend_from_slice(&q.to_block.to_be_bytes());
        bytes.extend(hex_bytes(&q.snapshot_hash)?);
        bytes.extend_from_slice(&q.rule_version.to_be_bytes());
        Ok(hash(&bytes))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub query_id: String,
    pub snapshot_hash: String,
    /// Decimal uint256 in smallest token units. No floating-point arithmetic.
    pub total: String,
    pub transfer_count: u64,
    pub logs_digest: String,
}

impl Summary {
    /// Validate an untrusted node's summary before counting it toward quorum.
    pub fn validate_for(&self, query: &QuerySpec) -> CoreResult<()> {
        let query = query.normalized()?;
        if self.query_id != query.query_id()? || self.snapshot_hash != query.snapshot_hash {
            return Err(error("summary does not match query and snapshot"));
        }
        if normalize_hash(&self.logs_digest)? != self.logs_digest {
            return Err(error("logs_digest must be canonical lowercase hex"));
        }
        let total = parse_total(&self.total)?;
        if self.transfer_count == 0
            && (total != U256::ZERO || self.logs_digest != hash(LOGS_DOMAIN))
        {
            return Err(error("empty summary has inconsistent total or logs_digest"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    /// Must be authenticated by the transport; this field alone proves nothing.
    pub node_id: String,
    pub summary: Summary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuorumResult {
    pub summary: Summary,
    pub supporting_nodes: Vec<String>,
}

/// An event whose address, topics, metadata and data all passed validation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferRecord {
    pub block_number: u64,
    pub block_hash: String,
    pub transaction_hash: String,
    pub log_index: u64,
    pub token: String,
    pub from: String,
    pub to: String,
    pub amount: String,
}

pub fn normalize_address(value: &str) -> CoreResult<String> {
    normalize_fixed_hex(value, 20, "address")
}

pub fn normalize_hash(value: &str) -> CoreResult<String> {
    normalize_fixed_hex(value, 32, "hash")
}

fn normalize_fixed_hex(value: &str, length: usize, name: &str) -> CoreResult<String> {
    let value = value.trim();
    let body = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .ok_or_else(|| error(format!("{name} requires 0x prefix")))?;
    if body.len() != length * 2 || !body.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(error(format!("{name} must contain {length} bytes of hex")));
    }
    Ok(format!("0x{}", body.to_ascii_lowercase()))
}

fn hex_bytes(value: &str) -> CoreResult<Vec<u8>> {
    let body = value
        .strip_prefix("0x")
        .ok_or_else(|| error("missing hex prefix"))?;
    if !body.len().is_multiple_of(2) {
        return Err(error("odd-length hex"));
    }
    (0..body.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&body[i..i + 2], 16).map_err(|_| error("invalid hex")))
        .collect()
}

fn hash(bytes: &[u8]) -> String {
    format!("{:#x}", keccak256(bytes))
}

fn parse_total(total: &str) -> CoreResult<U256> {
    if total.is_empty()
        || !total.bytes().all(|c| c.is_ascii_digit())
        || (total.len() > 1 && total.starts_with('0'))
    {
        return Err(error("total must be canonical unsigned decimal"));
    }
    U256::from_str_radix(total, 10).map_err(|_| error("total exceeds uint256"))
}

/// Parse a canonical JSON-RPC quantity, bounded to the u64 fields we use.
pub fn parse_quantity(value: &str) -> CoreResult<u64> {
    let body = value
        .strip_prefix("0x")
        .ok_or_else(|| error("RPC quantity requires lowercase 0x prefix"))?;
    if body.is_empty()
        || body.len() > 16
        || !body.bytes().all(|c| c.is_ascii_hexdigit())
        || (body.len() > 1 && body.starts_with('0'))
    {
        return Err(error("malformed or overflowing RPC quantity"));
    }
    u64::from_str_radix(body, 16).map_err(|_| error("RPC quantity exceeds u64"))
}

fn string_field<'a>(log: &'a Value, key: &str) -> CoreResult<&'a str> {
    log.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| error(format!("log requires string field {key}")))
}

fn topic_address(topic: &str) -> CoreResult<String> {
    let normalized = normalize_hash(topic)?;
    if normalized[2..26].bytes().any(|c| c != b'0') {
        return Err(error("indexed address must have zero padding"));
    }
    normalize_address(&format!("0x{}", &normalized[26..]))
}

fn parse_record(query: &QuerySpec, log: &Value) -> CoreResult<TransferRecord> {
    if !log.is_object() {
        return Err(error("log must be an object"));
    }
    // A missing removed field is permitted by some providers, but a malformed
    // or true one is not. Pending logs fail required mined metadata below.
    match log.get("removed") {
        None | Some(Value::Bool(false)) => {}
        _ => return Err(error("removed or malformed removed log")),
    }
    let token = normalize_address(string_field(log, "address")?)?;
    if token != query.token {
        return Err(error("log address is outside requested token"));
    }
    let topics = log
        .get("topics")
        .and_then(Value::as_array)
        .ok_or_else(|| error("log requires topics array"))?;
    if topics.len() != 3 {
        return Err(error("ERC-20 Transfer log requires exactly 3 topics"));
    }
    let topic = |index: usize| -> CoreResult<&str> {
        topics[index]
            .as_str()
            .ok_or_else(|| error("log topics must be strings"))
    };
    if normalize_hash(topic(0)?)? != TRANSFER_TOPIC {
        return Err(error("log is not ERC-20 Transfer"));
    }
    let from = topic_address(topic(1)?)?;
    if from != query.treasury {
        return Err(error("log sender is outside requested treasury"));
    }
    let to = topic_address(topic(2)?)?;
    let data = normalize_fixed_hex(string_field(log, "data")?, 32, "Transfer data")?;
    let amount =
        U256::from_str_radix(&data[2..], 16).map_err(|_| error("invalid Transfer amount"))?;
    let block_number = parse_quantity(string_field(log, "blockNumber")?)?;
    if block_number < query.from_block || block_number > query.to_block {
        return Err(error("log is outside requested block range"));
    }
    let block_hash = normalize_hash(string_field(log, "blockHash")?)?;
    if block_number == query.to_block && block_hash != query.snapshot_hash {
        return Err(error("endpoint log does not match snapshot"));
    }
    let transaction_hash = normalize_hash(string_field(log, "transactionHash")?)?;
    let log_index = parse_quantity(string_field(log, "logIndex")?)?;
    Ok(TransferRecord {
        block_number,
        block_hash,
        transaction_hash,
        log_index,
        token,
        from,
        to,
        amount: amount.to_string(),
    })
}

/// Validate a prefiltered eth_getLogs result and return canonical deduplicated
/// events in chain order. Unexpected scope is an error, never silently ignored.
/// RPC chain ID and headers before/after fetching are the adapter's concern.
pub fn parse_logs(query: &QuerySpec, logs: &[Value]) -> CoreResult<Vec<TransferRecord>> {
    let query = query.normalized()?;
    let mut by_key: BTreeMap<(String, String, u64), TransferRecord> = BTreeMap::new();
    let mut by_position: BTreeMap<(u64, u64), (String, String)> = BTreeMap::new();
    let mut block_hashes: BTreeMap<u64, String> = BTreeMap::new();
    let mut transaction_blocks: BTreeMap<String, (u64, String)> = BTreeMap::new();
    for log in logs {
        let record = parse_record(&query, log)?;
        if let Some(previous) = block_hashes.insert(record.block_number, record.block_hash.clone())
        {
            if previous != record.block_hash {
                return Err(error("conflicting block hashes within query response"));
            }
        }
        let transaction_block = (record.block_number, record.block_hash.clone());
        if let Some(previous) =
            transaction_blocks.insert(record.transaction_hash.clone(), transaction_block.clone())
        {
            if previous != transaction_block {
                return Err(error("one transaction appears in multiple blocks"));
            }
        }
        let position = (record.block_number, record.log_index);
        let identity = (record.block_hash.clone(), record.transaction_hash.clone());
        if let Some(previous) = by_position.insert(position, identity.clone()) {
            if previous != identity {
                return Err(error("conflicting event identities at block log position"));
            }
        }
        let key = (
            record.block_hash.clone(),
            record.transaction_hash.clone(),
            record.log_index,
        );
        if let Some(previous) = by_key.get(&key) {
            if previous != &record {
                return Err(error("conflicting duplicate log"));
            }
        } else {
            by_key.insert(key, record);
        }
    }
    let mut records: Vec<_> = by_key.into_values().collect();
    records.sort_by(|a, b| {
        (a.block_number, a.log_index, &a.transaction_hash).cmp(&(
            b.block_number,
            b.log_index,
            &b.transaction_hash,
        ))
    });
    Ok(records)
}

/// Canonical records use fixed-width fields under a domain separator. The digest
/// binds event identities, destinations and amounts, not merely the sum.
pub fn summarize_logs(query: &QuerySpec, logs: &[Value]) -> CoreResult<Summary> {
    let query = query.normalized()?;
    let records = parse_logs(&query, logs)?;
    summarize_records(&query, &records)
}

fn summarize_records(query: &QuerySpec, records: &[TransferRecord]) -> CoreResult<Summary> {
    let mut total = U256::ZERO;
    let mut bytes = LOGS_DOMAIN.to_vec();
    for record in records {
        let amount = parse_total(&record.amount)?;
        total = total
            .checked_add(amount)
            .ok_or_else(|| error("transfer sum exceeds uint256"))?;
        bytes.extend_from_slice(&record.block_number.to_be_bytes());
        bytes.extend(hex_bytes(&record.block_hash)?);
        bytes.extend(hex_bytes(&record.transaction_hash)?);
        bytes.extend_from_slice(&record.log_index.to_be_bytes());
        bytes.extend(hex_bytes(&record.token)?);
        bytes.extend(hex_bytes(&record.from)?);
        bytes.extend(hex_bytes(&record.to)?);
        bytes.extend_from_slice(&amount.to_be_bytes::<32>());
    }
    Ok(Summary {
        query_id: query.query_id()?,
        snapshot_hash: query.snapshot_hash.clone(),
        total: total.to_string(),
        transfer_count: u64::try_from(records.len())
            .map_err(|_| error("too many transfer records"))?,
        logs_digest: hash(&bytes),
    })
}

/// Select one result supported by `threshold` distinct authenticated identities.
/// Invalid/out-of-snapshot summaries are ineligible. Exact duplicate responses
/// have no effect; conflicting responses from one identity fail closed.
///
/// The caller must bind node IDs to its authorized committee. A string supplied
/// by an arbitrary client is not a vote. The caller also fixes the threshold;
/// never accept a user-supplied lower threshold for a trusted application policy.
pub fn select_quorum(
    query: &QuerySpec,
    observations: &[Observation],
    threshold: usize,
) -> CoreResult<Option<QuorumResult>> {
    let query = query.normalized()?;
    if threshold == 0 {
        return Err(error("quorum threshold must be positive"));
    }
    let mut identities: BTreeMap<&str, &Summary> = BTreeMap::new();
    for observation in observations {
        let identity = observation.node_id.as_str();
        if identity.is_empty()
            || identity.len() > 128
            || !identity.bytes().all(|c| c.is_ascii_graphic())
        {
            return Err(error("invalid node identity"));
        }
        if let Some(previous) = identities.insert(identity, &observation.summary) {
            if previous != &observation.summary {
                return Err(error("conflicting observations from one node identity"));
            }
        }
    }
    let mut groups: BTreeMap<Summary, BTreeSet<String>> = BTreeMap::new();
    for (identity, summary) in identities {
        if summary.validate_for(&query).is_ok() {
            groups
                .entry(summary.clone())
                .or_default()
                .insert(identity.to_owned());
        }
    }
    let mut winner = None;
    for (summary, nodes) in groups {
        if nodes.len() >= threshold {
            if winner.is_some() {
                return Err(error("ambiguous quorum: multiple results meet threshold"));
            }
            winner = Some(QuorumResult {
                summary,
                supporting_nodes: nodes.into_iter().collect(),
            });
        }
    }
    Ok(winner)
}

#[cfg(test)]
mod tests;
