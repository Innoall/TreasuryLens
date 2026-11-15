//! Replayable drilldown derived from validated canonical events.
//!
//! An evidence bundle binds content to the supplied summary. It contains no
//! cryptographic proof of chain membership or independently trusted certificate.

use crate::{
    CoreResult, QuerySpec, Summary, TransferRecord, error, parse_logs, parse_total,
    summarize_records,
};
use alloy_primitives::{U256, U512};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const EVIDENCE_SCHEMA_VERSION: u16 = 1;
pub const MAX_EVIDENCE_LOGS: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecipientTotal {
    pub address: String,
    pub total: String,
    pub count: u64,
    /// Floor of recipient amount / gross transfer amount * 10,000.
    /// Defined as zero when every event amount is zero.
    pub share_bps: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BlockTotal {
    pub block_number: u64,
    pub total: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuditReport {
    pub summary: Summary,
    /// Canonical chain order; exact duplicate source logs appear only once.
    pub transfers: Vec<TransferRecord>,
    /// Sorted by descending numeric total, then ascending canonical address.
    pub recipients: Vec<RecipientTotal>,
    /// Only blocks containing a matching event, in ascending block order.
    pub blocks: Vec<BlockTotal>,
    /// Included in summary.total; this separate subtotal explains self-transfers.
    pub self_transfer_total: String,
    pub self_transfer_count: u64,
    pub top_recipient_share_bps: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceBundle {
    pub schema_version: u16,
    pub query: QuerySpec,
    pub summary: Summary,
    /// Raw JSON-RPC logs, checked again rather than trusting exported analytics.
    pub logs: Vec<Value>,
}

/// Exact integer fraction with a widened intermediate; `U256::MAX * 10,000`
/// does not fit in uint256. The result is always 0..=10,000 because part<=total.
fn share_bps(part: U256, total: U256) -> CoreResult<u16> {
    if part > total {
        return Err(error("recipient amount exceeds transfer total"));
    }
    if total == U256::ZERO {
        return Ok(0);
    }
    let numerator = U512::from(part)
        .checked_mul(U512::from(10_000))
        .ok_or_else(|| error("basis-point intermediate overflow"))?;
    let bps = numerator / U512::from(total);
    u16::try_from(bps).map_err(|_| error("basis-point result exceeds u16"))
}

fn add_group(group: &mut (U256, u64), amount: U256) -> CoreResult<()> {
    group.0 = group
        .0
        .checked_add(amount)
        .ok_or_else(|| error("group sum exceeds uint256"))?;
    group.1 = group
        .1
        .checked_add(1)
        .ok_or_else(|| error("group count exceeds u64"))?;
    Ok(())
}

/// Parse once, then derive the summary and every visible aggregate from the
/// exact same canonical event list. No browser-supplied budget enters evidence.
pub fn audit_logs(query: &QuerySpec, logs: &[Value]) -> CoreResult<AuditReport> {
    if logs.len() > MAX_EVIDENCE_LOGS {
        return Err(error("audit exceeds 10000 raw logs"));
    }
    let query = query.normalized()?;
    let transfers = parse_logs(&query, logs)?;
    let summary = summarize_records(&query, &transfers)?;
    let total = parse_total(&summary.total)?;
    let mut recipient_totals: BTreeMap<String, (U256, u64)> = BTreeMap::new();
    let mut block_totals: BTreeMap<u64, (U256, u64)> = BTreeMap::new();
    let mut self_transfers = (U256::ZERO, 0_u64);
    for transfer in &transfers {
        let amount = parse_total(&transfer.amount)?;
        add_group(
            recipient_totals.entry(transfer.to.clone()).or_default(),
            amount,
        )?;
        add_group(
            block_totals.entry(transfer.block_number).or_default(),
            amount,
        )?;
        if transfer.from == transfer.to {
            add_group(&mut self_transfers, amount)?;
        }
    }
    let mut sorted_recipients: Vec<_> = recipient_totals.into_iter().collect();
    sorted_recipients.sort_by(|(address_a, (amount_a, _)), (address_b, (amount_b, _))| {
        amount_b
            .cmp(amount_a)
            .then_with(|| address_a.cmp(address_b))
    });
    let recipients = sorted_recipients
        .into_iter()
        .map(|(address, (amount, count))| {
            Ok(RecipientTotal {
                address,
                total: amount.to_string(),
                count,
                share_bps: share_bps(amount, total)?,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    let blocks = block_totals
        .into_iter()
        .map(|(block_number, (amount, count))| BlockTotal {
            block_number,
            total: amount.to_string(),
            count,
        })
        .collect();
    let top_recipient_share_bps = recipients.first().map(|r| r.share_bps).unwrap_or(0);
    Ok(AuditReport {
        summary,
        transfers,
        recipients,
        blocks,
        self_transfer_total: self_transfers.0.to_string(),
        self_transfer_count: self_transfers.1,
        top_recipient_share_bps,
    })
}

/// Recompute exact events and every summary field against a known summary.
/// Callers wanting certificate anchoring must obtain `expected` through their
/// trusted persisted-result path rather than accepting it from the uploader.
pub fn verify_logs(
    query: &QuerySpec,
    expected: &Summary,
    logs: &[Value],
) -> CoreResult<AuditReport> {
    expected.validate_for(query)?;
    let report = audit_logs(query, logs)?;
    if report.summary != *expected {
        return Err(error("evidence logs do not reproduce the expected summary"));
    }
    Ok(report)
}

/// Verify the internal consistency of a versioned exported bundle. Success does
/// not authenticate the uploader, prove log completeness/chain membership, or
/// verify committee signatures; an attacker can rewrite a bundle and recompute
/// its summary. Compare the verified summary to a trusted record separately.
pub fn verify_evidence(bundle: &EvidenceBundle) -> CoreResult<AuditReport> {
    if bundle.schema_version != EVIDENCE_SCHEMA_VERSION {
        return Err(error("unsupported evidence schema_version"));
    }
    verify_logs(&bundle.query, &bundle.summary, &bundle.logs)
}

#[cfg(test)]
mod tests;
