use super::*;
use crate::{RULE_VERSION, TRANSFER_TOPIC, summarize_logs};
use serde_json::json;

fn address(byte: u8) -> String {
    format!("0x{}", format!("{byte:02x}").repeat(20))
}

fn hash(byte: u8) -> String {
    format!("0x{}", format!("{byte:02x}").repeat(32))
}

fn query() -> QuerySpec {
    QuerySpec {
        chain_id: 31337,
        token: address(0x11),
        treasury: address(0x22),
        from_block: 10,
        to_block: 20,
        snapshot_hash: hash(0x20),
        rule_version: RULE_VERSION,
    }
}

fn event(block: u64, index: u64, recipient: u8, amount: U256) -> Value {
    let q = query();
    json!({
        "address": q.token,
        "topics": [
            TRANSFER_TOPIC,
            format!("0x{}{}", "0".repeat(24), &q.treasury[2..]),
            format!("0x{}{}", "0".repeat(24), &address(recipient)[2..]),
        ],
        "data": format!("0x{amount:064x}"),
        "blockNumber": format!("0x{block:x}"),
        "blockHash": if block == 20 { q.snapshot_hash } else { hash(block as u8) },
        "transactionHash": hash((index + 0x40) as u8),
        "logIndex": format!("0x{index:x}"),
        "removed": false,
    })
}

fn bundle(logs: Vec<Value>) -> EvidenceBundle {
    EvidenceBundle {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        query: query(),
        summary: summarize_logs(&query(), &logs).unwrap(),
        logs,
    }
}

#[test]
fn recipient_and_block_drilldowns_reconcile_to_gross_summary() {
    let logs = [
        event(10, 0, 0x33, U256::from(7)),
        event(10, 1, 0x44, U256::from(9)),
        event(20, 0, 0x33, U256::from(3)),
        event(20, 1, 0x55, U256::from(9)),
    ];
    // One transaction hash cannot cross blocks; give second block distinct txs.
    let logs: Vec<_> = logs
        .into_iter()
        .enumerate()
        .map(|(i, mut log)| {
            log["transactionHash"] = json!(hash(0x60 + i as u8));
            log
        })
        .collect();
    let report = audit_logs(&query(), &logs).unwrap();
    assert_eq!(report.summary.total, "28");
    assert_eq!(report.summary.transfer_count, 4);
    assert_eq!(
        report.recipients,
        vec![
            RecipientTotal {
                address: address(0x33),
                total: "10".into(),
                count: 2,
                share_bps: 3571
            },
            RecipientTotal {
                address: address(0x44),
                total: "9".into(),
                count: 1,
                share_bps: 3214
            },
            RecipientTotal {
                address: address(0x55),
                total: "9".into(),
                count: 1,
                share_bps: 3214
            },
        ]
    );
    assert_eq!(
        report.blocks,
        vec![
            BlockTotal {
                block_number: 10,
                total: "16".into(),
                count: 2
            },
            BlockTotal {
                block_number: 20,
                total: "12".into(),
                count: 2
            },
        ]
    );
    assert_eq!(report.top_recipient_share_bps, 3571);
    assert_eq!(report.self_transfer_total, "0");
    assert_eq!(report.self_transfer_count, 0);
}

#[test]
fn basis_points_widen_intermediate_at_uint256_max_instead_of_overflowing() {
    let half = U256::MAX / U256::from(2);
    let remainder = U256::MAX - half;
    let report = audit_logs(
        &query(),
        &[event(10, 0, 0x33, half), event(10, 1, 0x44, remainder)],
    )
    .unwrap();
    assert_eq!(report.summary.total, U256::MAX.to_string());
    assert_eq!(report.recipients[0].address, address(0x44));
    assert_eq!(report.recipients[0].share_bps, 5000);
    assert_eq!(report.recipients[1].share_bps, 4999);
    assert_eq!(report.top_recipient_share_bps, 5000);
    assert_eq!(share_bps(U256::MAX, U256::MAX).unwrap(), 10_000);
    assert!(share_bps(U256::from(2), U256::from(1)).is_err());
}

#[test]
fn fractional_basis_points_are_floored_not_rounded() {
    let report = audit_logs(
        &query(),
        &[
            event(10, 0, 0x33, U256::from(1)),
            event(10, 1, 0x44, U256::from(1)),
            event(10, 2, 0x55, U256::from(1)),
        ],
    )
    .unwrap();
    assert!(
        report
            .recipients
            .iter()
            .all(|recipient| recipient.share_bps == 3333)
    );
    assert_eq!(
        report
            .recipients
            .iter()
            .map(|recipient| recipient.share_bps as u64)
            .sum::<u64>(),
        9999
    );
}

#[test]
fn self_transfers_are_explained_without_subtracting_from_original_statistic() {
    let report = audit_logs(
        &query(),
        &[
            event(10, 0, 0x22, U256::from(12)),
            event(10, 1, 0x22, U256::ZERO),
            event(10, 2, 0x33, U256::from(3)),
        ],
    )
    .unwrap();
    assert_eq!(report.summary.total, "15");
    assert_eq!(report.summary.transfer_count, 3);
    assert_eq!(report.self_transfer_total, "12");
    assert_eq!(report.self_transfer_count, 2);
    assert_eq!(report.recipients[0].address, query().treasury);
    assert_eq!(report.recipients[0].share_bps, 8000);
}

#[test]
fn zero_value_events_retain_drilldown_counts_without_dividing_by_zero() {
    let report = audit_logs(
        &query(),
        &[
            event(10, 0, 0x44, U256::ZERO),
            event(10, 1, 0x33, U256::ZERO),
        ],
    )
    .unwrap();
    assert_eq!(report.summary.total, "0");
    assert_eq!(report.summary.transfer_count, 2);
    assert_eq!(report.blocks[0].count, 2);
    assert_eq!(report.recipients[0].address, address(0x33));
    assert!(
        report
            .recipients
            .iter()
            .all(|recipient| recipient.share_bps == 0)
    );
    assert_eq!(report.top_recipient_share_bps, 0);
}

#[test]
fn duplicate_input_and_response_order_do_not_inflate_any_drilldown() {
    let a = event(10, 0, 0x33, U256::from(1));
    let b = event(10, 1, 0x22, U256::from(2));
    let normal = audit_logs(&query(), &[a.clone(), b.clone()]).unwrap();
    let repeated = audit_logs(&query(), &[b.clone(), a.clone(), b, a]).unwrap();
    assert_eq!(normal, repeated);
    assert_eq!(normal.self_transfer_count, 1);
    assert_eq!(normal.transfers.len(), 2);
}

#[test]
fn empty_evidence_bundle_is_valid_and_has_no_fake_drilldown_rows() {
    let evidence = bundle(vec![]);
    let report = verify_evidence(&evidence).unwrap();
    assert_eq!(report.summary.total, "0");
    assert_eq!(report.summary.transfer_count, 0);
    assert!(report.transfers.is_empty());
    assert!(report.blocks.is_empty());
    assert!(report.recipients.is_empty());
    assert_eq!(report.self_transfer_total, "0");
    assert_eq!(report.top_recipient_share_bps, 0);
}

#[test]
fn bundle_json_roundtrip_is_replayable() {
    let original = bundle(vec![event(20, 0, 0x33, U256::from(1))]);
    let bytes = serde_json::to_vec(&original).unwrap();
    let decoded: EvidenceBundle = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
    assert_eq!(
        verify_evidence(&original).unwrap(),
        verify_evidence(&decoded).unwrap()
    );
}

#[test]
fn tampering_amount_or_recipient_fails_full_expected_summary_comparison() {
    let original = bundle(vec![event(10, 0, 0x33, U256::from(1))]);
    let mut amount_tamper = original.clone();
    amount_tamper.logs[0]["data"] = json!(format!("0x{:064x}", 2));
    assert!(verify_evidence(&amount_tamper).is_err());
    let mut recipient_tamper = original;
    recipient_tamper.logs[0]["topics"][2] =
        json!(format!("0x{}{}", "0".repeat(24), &address(0x44)[2..]));
    assert!(verify_evidence(&recipient_tamper).is_err());
}

#[test]
fn tampering_event_identity_fails_even_when_amount_and_count_are_unchanged() {
    let original = bundle(vec![event(10, 0, 0x33, U256::from(1))]);
    for field in ["blockHash", "transactionHash"] {
        let mut tampered = original.clone();
        tampered.logs[0][field] = json!(hash(0xEE));
        assert!(verify_evidence(&tampered).is_err(), "{field}");
    }
}

#[test]
fn wrong_snapshot_query_or_endpoint_log_is_rejected() {
    let original = bundle(vec![event(20, 0, 0x33, U256::from(1))]);
    let mut query_tamper = original.clone();
    query_tamper.query.snapshot_hash = hash(0xEE);
    assert!(verify_evidence(&query_tamper).is_err());
    let mut log_tamper = original;
    log_tamper.logs[0]["blockHash"] = json!(hash(0xEE));
    assert!(verify_evidence(&log_tamper).is_err());
}

#[test]
fn missing_log_cannot_reproduce_expected_summary() {
    let mut original = bundle(vec![
        event(10, 0, 0x33, U256::from(1)),
        event(10, 1, 0x44, U256::from(2)),
    ]);
    original.logs.remove(0);
    assert!(verify_evidence(&original).is_err());
    original.logs.clear();
    assert!(verify_evidence(&original).is_err());
}

#[test]
fn forged_self_consistent_bundle_requires_external_anchor_to_detect() {
    let original = bundle(vec![event(10, 0, 0x33, U256::from(1))]);
    let mut rewritten = original.clone();
    rewritten.logs[0]["data"] = json!(format!("0x{:064x}", 2));
    rewritten.summary = summarize_logs(&rewritten.query, &rewritten.logs).unwrap();
    // Content verification alone cannot know that a different summary was certified.
    assert!(verify_evidence(&rewritten).is_ok());
    // Replay against the actual known summary detects the changed evidence.
    assert!(verify_logs(&original.query, &original.summary, &rewritten.logs).is_err());
}

#[test]
fn unsupported_schema_unknown_fields_and_excessive_log_inputs_are_rejected() {
    let mut unsupported = bundle(vec![]);
    unsupported.schema_version = 2;
    assert!(verify_evidence(&unsupported).is_err());
    let mut unknown = serde_json::to_value(bundle(vec![])).unwrap();
    unknown["trust_me"] = json!(true);
    assert!(serde_json::from_value::<EvidenceBundle>(unknown).is_err());
    let excessive = vec![Value::Null; MAX_EVIDENCE_LOGS + 1];
    assert!(
        audit_logs(&query(), &excessive)
            .unwrap_err()
            .to_string()
            .contains("10000")
    );
}

#[test]
fn analytics_preserves_checked_uint256_overflow_rejection() {
    let logs = [
        event(10, 0, 0x33, U256::MAX),
        event(10, 1, 0x44, U256::from(1)),
    ];
    assert!(audit_logs(&query(), &logs).is_err());
}
