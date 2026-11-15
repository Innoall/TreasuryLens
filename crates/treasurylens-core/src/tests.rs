use super::*;
use serde_json::json;

fn address(byte: u8) -> String {
    format!("0x{:02x}", byte)
        .chars()
        .take(2)
        .collect::<String>()
        + &format!("{:02x}", byte).repeat(20)
}

fn fixed_hash(byte: u8) -> String {
    format!("0x{}", format!("{byte:02x}").repeat(32))
}

fn query() -> QuerySpec {
    QuerySpec {
        chain_id: 31337,
        token: address(0x11),
        treasury: address(0x22),
        from_block: 10,
        to_block: 20,
        snapshot_hash: fixed_hash(0x20),
        rule_version: RULE_VERSION,
    }
}

fn topic_address(address: &str) -> String {
    format!("0x{}{}", "0".repeat(24), &address[2..])
}

fn log(block: u64, index: u64, amount: U256) -> Value {
    let q = query();
    json!({
        "address": q.token,
        "topics": [TRANSFER_TOPIC, topic_address(&q.treasury), topic_address(&address(0x33))],
        "data": format!("0x{amount:064x}"),
        "blockNumber": format!("0x{block:x}"),
        "blockHash": if block == 20 { q.snapshot_hash } else { fixed_hash(block as u8) },
        "transactionHash": fixed_hash((index + 0x40) as u8),
        "logIndex": format!("0x{index:x}"),
        "removed": false,
    })
}

fn observation(node: &str, summary: &Summary) -> Observation {
    Observation {
        node_id: node.into(),
        summary: summary.clone(),
    }
}

#[test]
fn canonical_query_normalizes_case_without_losing_identity() {
    let q = query();
    let mut user = q.clone();
    user.token = format!(" {} ", q.token.to_uppercase());
    user.treasury = q.treasury.to_uppercase();
    user.snapshot_hash = q.snapshot_hash.to_uppercase();
    assert_eq!(user.normalized().unwrap(), q);
    assert_eq!(q.query_id().unwrap(), user.query_id().unwrap());
}

#[test]
fn query_id_binds_all_policy_fields() {
    let q = query();
    let original = q.query_id().unwrap();
    let mut changed = Vec::new();
    let mut next = q.clone();
    next.chain_id += 1;
    changed.push(next);
    let mut next = q.clone();
    next.token = address(0x12);
    changed.push(next);
    let mut next = q.clone();
    next.treasury = address(0x23);
    changed.push(next);
    let mut next = q.clone();
    next.from_block += 1;
    changed.push(next);
    let mut next = q.clone();
    next.to_block += 1;
    changed.push(next);
    let mut next = q.clone();
    next.snapshot_hash = fixed_hash(0x21);
    changed.push(next);
    for next in changed {
        assert_ne!(original, next.query_id().unwrap());
    }
    let mut unsupported = q;
    unsupported.rule_version += 1;
    assert!(unsupported.query_id().is_err());
}

#[test]
fn inclusive_range_boundary_and_u64_max_are_safe() {
    let mut q = query();
    q.from_block = 0;
    q.to_block = 9_999;
    assert!(q.normalized().is_ok());
    q.to_block = 10_000;
    assert!(q.normalized().is_err());
    q.from_block = u64::MAX;
    q.to_block = u64::MAX;
    assert!(q.normalized().is_ok());
    q.from_block = 1;
    q.to_block = 0;
    assert!(q.normalized().is_err());
}

#[test]
fn invalid_address_hash_and_rule_are_rejected() {
    for input in [
        "0x1234",
        "11",
        "0xGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG",
        "0xéééééééééééééééééééé",
    ] {
        assert!(normalize_address(input).is_err(), "{input}");
    }
    let mut q = query();
    q.snapshot_hash = "0xdeadbeef".into();
    assert!(q.normalized().is_err());
    q = query();
    q.rule_version = 0;
    assert!(q.normalized().is_err());
}

#[test]
fn rpc_quantity_checks_canonical_encoding_and_overflow() {
    assert_eq!(parse_quantity("0x0").unwrap(), 0);
    assert_eq!(parse_quantity("0xffffffffffffffff").unwrap(), u64::MAX);
    for input in [
        "0x",
        "0x00",
        "0x01",
        "1",
        "-1",
        "0X1",
        "0x10000000000000000",
        "0xgg",
    ] {
        assert!(parse_quantity(input).is_err(), "{input}");
    }
}

#[test]
fn sums_integers_without_float_precision_loss() {
    let huge = U256::from_str_radix("9007199254740993123456789", 10).unwrap();
    let result = summarize_logs(&query(), &[log(10, 0, huge), log(20, 1, U256::from(7))]).unwrap();
    assert_eq!(result.total, "9007199254740993123456796");
    assert_eq!(result.transfer_count, 2);
    result.validate_for(&query()).unwrap();
}

#[test]
fn empty_range_is_a_valid_confirmable_zero_summary() {
    let summary = summarize_logs(&query(), &[]).unwrap();
    assert_eq!(summary.total, "0");
    assert_eq!(summary.transfer_count, 0);
    assert_eq!(summary.logs_digest, hash(LOGS_DOMAIN));
    summary.validate_for(&query()).unwrap();
    let votes: Vec<_> = ["a", "b", "c"].map(|n| observation(n, &summary)).into();
    assert!(select_quorum(&query(), &votes, 3).unwrap().is_some());
}

#[test]
fn zero_amount_transfer_still_counts_as_event() {
    let summary = summarize_logs(&query(), &[log(10, 0, U256::ZERO)]).unwrap();
    assert_eq!(summary.total, "0");
    assert_eq!(summary.transfer_count, 1);
    assert_ne!(summary.logs_digest, hash(LOGS_DOMAIN));
}

#[test]
fn duplicates_are_counted_once_and_input_order_does_not_change_digest() {
    let a = log(10, 0, U256::from(10));
    let b = log(20, 1, U256::from(20));
    let normal = summarize_logs(&query(), &[a.clone(), b.clone()]).unwrap();
    let reordered = summarize_logs(&query(), &[b.clone(), a.clone(), b, a]).unwrap();
    assert_eq!(normal, reordered);
    assert_eq!(normal.total, "30");
    assert_eq!(normal.transfer_count, 2);
}

#[test]
fn conflicting_duplicate_amount_or_destination_fails_closed() {
    let original = log(10, 0, U256::from(10));
    let mut changed = original.clone();
    changed["data"] = json!(format!("0x{:064x}", 11));
    assert!(summarize_logs(&query(), &[original.clone(), changed]).is_err());
    let mut changed = original.clone();
    changed["topics"][2] = json!(topic_address(&address(0x44)));
    assert!(summarize_logs(&query(), &[original, changed]).is_err());
}

#[test]
fn conflicting_hashes_or_identities_at_one_position_are_rejected() {
    let original = log(10, 0, U256::from(10));
    let mut changed = log(10, 1, U256::from(10));
    changed["blockHash"] = json!(fixed_hash(0xFF));
    assert!(summarize_logs(&query(), &[original.clone(), changed]).is_err());
    let mut changed = original.clone();
    changed["transactionHash"] = json!(fixed_hash(0xFF));
    assert!(summarize_logs(&query(), &[original, changed]).is_err());
}

#[test]
fn endpoint_log_must_match_query_snapshot() {
    let mut changed = log(20, 0, U256::from(10));
    changed["blockHash"] = json!(fixed_hash(0xFF));
    assert!(summarize_logs(&query(), &[changed]).is_err());
}

#[test]
fn removed_pending_and_malformed_logs_are_rejected() {
    let original = log(10, 0, U256::from(10));
    for field in ["blockHash", "transactionHash", "blockNumber", "logIndex"] {
        let mut malformed = original.clone();
        malformed[field] = Value::Null;
        assert!(summarize_logs(&query(), &[malformed]).is_err(), "{field}");
    }
    for value in [json!(true), json!("false"), Value::Null] {
        let mut malformed = original.clone();
        malformed["removed"] = value;
        assert!(summarize_logs(&query(), &[malformed]).is_err());
    }
    assert!(summarize_logs(&query(), &[json!(null)]).is_err());
}

#[test]
fn event_scope_is_validated_instead_of_trusting_rpc_filter() {
    let original = log(10, 0, U256::from(10));
    let mut malformed = original.clone();
    malformed["address"] = json!(address(0x99));
    assert!(summarize_logs(&query(), &[malformed]).is_err());
    let mut malformed = original.clone();
    malformed["topics"][0] = json!(fixed_hash(0x99));
    assert!(summarize_logs(&query(), &[malformed]).is_err());
    let mut malformed = original.clone();
    malformed["topics"][1] = json!(topic_address(&address(0x99)));
    assert!(summarize_logs(&query(), &[malformed]).is_err());
    for block in [9, 21] {
        assert!(summarize_logs(&query(), &[log(block, 0, U256::from(1))]).is_err());
    }
}

#[test]
fn exact_event_shape_and_indexed_address_padding_are_required() {
    let original = log(10, 0, U256::from(10));
    let mut malformed = original.clone();
    malformed["topics"]
        .as_array_mut()
        .unwrap()
        .push(json!(fixed_hash(0)));
    assert!(summarize_logs(&query(), &[malformed]).is_err());
    let mut malformed = original.clone();
    malformed["topics"][1] = json!(fixed_hash(0x22));
    assert!(summarize_logs(&query(), &[malformed]).is_err());
    for data in ["0x01", "0x", "1"] {
        let mut malformed = original.clone();
        malformed["data"] = json!(data);
        assert!(summarize_logs(&query(), &[malformed]).is_err());
    }
}

#[test]
fn uint256_max_is_valid_but_sum_overflow_is_rejected() {
    let max = log(10, 0, U256::MAX);
    let summary = summarize_logs(&query(), &[max.clone()]).unwrap();
    assert_eq!(summary.total, U256::MAX.to_string());
    assert!(summarize_logs(&query(), &[max, log(11, 1, U256::from(1))]).is_err());
}

#[test]
fn digest_binds_destination_and_event_identity_not_only_total() {
    let original = log(10, 0, U256::from(10));
    let normal = summarize_logs(&query(), &[original.clone()]).unwrap();
    let mut changed = original.clone();
    changed["topics"][2] = json!(topic_address(&address(0x44)));
    let next = summarize_logs(&query(), &[changed]).unwrap();
    assert_eq!(normal.total, next.total);
    assert_ne!(normal.logs_digest, next.logs_digest);
    let mut changed = original;
    changed["transactionHash"] = json!(fixed_hash(0x55));
    assert_ne!(
        normal.logs_digest,
        summarize_logs(&query(), &[changed]).unwrap().logs_digest
    );
}

#[test]
fn three_distinct_matching_nodes_produce_sorted_supporters() {
    let normal = summarize_logs(&query(), &[log(10, 0, U256::from(10))]).unwrap();
    let wrong = summarize_logs(&query(), &[log(10, 0, U256::from(100))]).unwrap();
    let votes = [
        observation("d", &wrong),
        observation("c", &normal),
        observation("a", &normal),
        observation("b", &normal),
    ];
    let result = select_quorum(&query(), &votes, 3).unwrap().unwrap();
    assert_eq!(result.summary, normal);
    assert_eq!(result.supporting_nodes, vec!["a", "b", "c"]);
}

#[test]
fn duplicate_identity_cannot_manufacture_threshold() {
    let normal = summarize_logs(&query(), &[]).unwrap();
    let votes = vec![observation("a", &normal); 4];
    assert!(select_quorum(&query(), &votes, 3).unwrap().is_none());
    let votes = [
        observation("a", &normal),
        observation("a", &normal),
        observation("b", &normal),
    ];
    assert!(select_quorum(&query(), &votes, 3).unwrap().is_none());
}

#[test]
fn conflicting_observations_from_one_identity_fail_closed() {
    let normal = summarize_logs(&query(), &[]).unwrap();
    let wrong = summarize_logs(&query(), &[log(10, 0, U256::from(1))]).unwrap();
    let votes = [
        observation("a", &normal),
        observation("b", &normal),
        observation("c", &normal),
        observation("a", &wrong),
    ];
    assert!(select_quorum(&query(), &votes, 3).is_err());
}

#[test]
fn other_query_snapshot_or_digest_is_not_counted_toward_quorum() {
    let normal = summarize_logs(&query(), &[log(10, 0, U256::from(10))]).unwrap();
    for field in ["snapshot", "query", "digest"] {
        let mut changed = normal.clone();
        match field {
            "snapshot" => changed.snapshot_hash = fixed_hash(0xFF),
            "query" => changed.query_id = fixed_hash(0xFF),
            _ => changed.logs_digest = fixed_hash(0xFF),
        }
        let votes = [
            observation("a", &normal),
            observation("b", &normal),
            observation("c", &changed),
        ];
        assert!(
            select_quorum(&query(), &votes, 3).unwrap().is_none(),
            "{field}"
        );
    }
}

#[test]
fn noncanonical_or_overflowing_summary_total_is_ineligible() {
    let normal = summarize_logs(&query(), &[log(10, 0, U256::from(10))]).unwrap();
    for total in [
        "01",
        "-1",
        "1.0",
        "0x1",
        "",
        " 1",
        "115792089237316195423570985008687907853269984665640564039457584007913129639936",
    ] {
        let mut changed = normal.clone();
        changed.total = total.into();
        assert!(changed.validate_for(&query()).is_err(), "{total}");
        let votes = [
            observation("a", &normal),
            observation("b", &normal),
            observation("c", &changed),
        ];
        assert!(select_quorum(&query(), &votes, 3).unwrap().is_none());
    }
}

#[test]
fn empty_summary_cannot_claim_nonzero_total_or_arbitrary_digest() {
    let normal = summarize_logs(&query(), &[]).unwrap();
    let mut changed = normal.clone();
    changed.total = "10".into();
    assert!(changed.validate_for(&query()).is_err());
    let mut changed = normal;
    changed.logs_digest = fixed_hash(0xFF);
    assert!(changed.validate_for(&query()).is_err());
}

#[test]
fn threshold_zero_and_ambiguous_quorums_are_rejected() {
    let normal = summarize_logs(&query(), &[]).unwrap();
    let wrong = summarize_logs(&query(), &[log(10, 0, U256::from(1))]).unwrap();
    assert!(select_quorum(&query(), &[], 0).is_err());
    let votes = [observation("a", &normal), observation("b", &wrong)];
    assert!(select_quorum(&query(), &votes, 1).is_err());
    assert!(select_quorum(&query(), &votes, 3).unwrap().is_none());
}

#[test]
fn node_identity_must_be_nonempty_bounded_ascii_without_whitespace() {
    let normal = summarize_logs(&query(), &[]).unwrap();
    for node in ["", "node a", "node\na", "节点", &"a".repeat(129)] {
        assert!(select_quorum(&query(), &[observation(node, &normal)], 1).is_err());
    }
}

#[test]
fn same_transaction_reincluded_at_another_height_cannot_be_double_counted() {
    let original = log(10, 0, U256::from(10));
    let mut reincluded = original.clone();
    reincluded["blockNumber"] = json!("0xb");
    reincluded["blockHash"] = json!(fixed_hash(11));
    // logIndex can change when a transaction is included in a different block.
    reincluded["logIndex"] = json!("0x1");
    assert!(summarize_logs(&query(), &[original, reincluded]).is_err());
}

#[test]
fn same_transaction_may_have_multiple_distinct_logs_in_one_block() {
    let original = log(10, 0, U256::from(10));
    let mut another_event = original.clone();
    another_event["logIndex"] = json!("0x1");
    another_event["data"] = json!(format!("0x{:064x}", 20));
    let summary = summarize_logs(&query(), &[original, another_event]).unwrap();
    assert_eq!(summary.total, "30");
    assert_eq!(summary.transfer_count, 2);
}

#[test]
fn per_block_log_index_cannot_belong_to_two_different_transactions() {
    let original = log(10, 2, U256::from(10));
    let mut conflicting_position = original.clone();
    conflicting_position["transactionHash"] = json!(fixed_hash(0xFF));
    assert!(summarize_logs(&query(), &[original, conflicting_position]).is_err());
}
