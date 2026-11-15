//! Deterministic policy ablation over synthetic observations derived from a
//! supplied persisted record. This is not a devnet benchmark or a certificate verifier.
use alloy_primitives::{U256, keccak256};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, Read};
use treasurylens_core::{Observation, QuerySpec, Summary, select_quorum};

#[derive(Debug, Clone, Deserialize)]
struct SourceRecord {
    query: QuerySpec,
    summary: Summary,
}

#[derive(Debug, Serialize)]
struct Outcome {
    accepted: bool,
    amount_correct: bool,
    snapshot_safe: bool,
    incorrect_accept: bool,
    unsafe_accept: bool,
    total: Option<String>,
    transfer_count: Option<u64>,
    supporting_nodes: Vec<String>,
    current_record_supporters: usize,
    rejection_reason: Option<String>,
}

impl Outcome {
    fn rejected(reason: impl Into<String>) -> Self {
        Self {
            accepted: false,
            amount_correct: false,
            snapshot_safe: false,
            incorrect_accept: false,
            unsafe_accept: false,
            total: None,
            transfer_count: None,
            supporting_nodes: vec![],
            current_record_supporters: 0,
            rejection_reason: Some(reason.into()),
        }
    }

    fn accepted(
        source: &Summary,
        total: String,
        transfer_count: u64,
        supporters: Vec<&Observation>,
        safety_threshold: usize,
    ) -> Self {
        // Equality binds all source summary fields, not merely its amount or
        // snapshot text. The source query ID transitively binds all query fields.
        let current_record_supporters = supporters
            .iter()
            .filter(|observation| observation.summary == *source)
            .count();
        let amount_correct = total == source.total;
        let snapshot_safe = current_record_supporters >= safety_threshold;
        let mut supporting_nodes: Vec<_> = supporters
            .into_iter()
            .map(|observation| observation.node_id.clone())
            .collect();
        supporting_nodes.sort();
        Self {
            accepted: true,
            amount_correct,
            snapshot_safe,
            incorrect_accept: !amount_correct,
            unsafe_accept: !snapshot_safe,
            total: Some(total),
            transfer_count: Some(transfer_count),
            supporting_nodes,
            current_record_supporters,
            rejection_reason: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct Variants {
    single_node: Outcome,
    totals_only_majority3: Outcome,
    hardened_quorum3: Outcome,
}

#[derive(Debug, Serialize)]
struct Scenario {
    name: String,
    synthetic: bool,
    observations: Vec<Observation>,
    variants: Variants,
}

fn evaluate_variants(source: &SourceRecord, observations: &[Observation]) -> Variants {
    let single_node = observations
        .iter()
        .find(|observation| observation.node_id == "node0")
        .map(|observation| {
            Outcome::accepted(
                &source.summary,
                observation.summary.total.clone(),
                observation.summary.transfer_count,
                vec![observation],
                1,
            )
        })
        .unwrap_or_else(|| Outcome::rejected("node0 did not respond"));

    // Deliberate ablation: this policy does not compare query ID, snapshot or
    // logs digest. Distinct identity counting is retained to isolate that omission.
    let mut seen = std::collections::BTreeSet::new();
    let mut groups: BTreeMap<(String, u64), Vec<&Observation>> = BTreeMap::new();
    for observation in observations {
        if seen.insert(&observation.node_id) {
            groups
                .entry((
                    observation.summary.total.clone(),
                    observation.summary.transfer_count,
                ))
                .or_default()
                .push(observation);
        }
    }
    let winners: Vec<_> = groups
        .into_iter()
        .filter(|(_, supporters)| supporters.len() >= 3)
        .collect();
    let totals_only_majority3 = match winners.as_slice() {
        [((total, count), supporters)] => Outcome::accepted(
            &source.summary,
            total.clone(),
            *count,
            supporters.clone(),
            3,
        ),
        [] => Outcome::rejected("fewer than three distinct matching total/count responses"),
        _ => Outcome::rejected("ambiguous total/count majority"),
    };

    let hardened_quorum3 = match select_quorum(&source.query, observations, 3) {
        Ok(Some(quorum)) => {
            let supporters: Vec<_> = observations
                .iter()
                .filter(|observation| {
                    quorum.supporting_nodes.contains(&observation.node_id)
                        && observation.summary == quorum.summary
                })
                .collect();
            Outcome::accepted(
                &source.summary,
                quorum.summary.total,
                quorum.summary.transfer_count,
                supporters,
                3,
            )
        }
        Ok(None) => Outcome::rejected("fewer than three distinct matching query-bound summaries"),
        Err(error) => Outcome::rejected(error.to_string()),
    };
    Variants {
        single_node,
        totals_only_majority3,
        hardened_quorum3,
    }
}

fn scenarios(source: &SourceRecord) -> Result<Vec<Scenario>, String> {
    source
        .summary
        .validate_for(&source.query)
        .map_err(|error| error.to_string())?;
    let amount =
        U256::from_str_radix(&source.summary.total, 10).map_err(|error| error.to_string())?;
    let wrong_amount = amount
        .checked_add(U256::from(1))
        .ok_or_else(|| "cannot model total + 1 when source amount is uint256 max".to_string())?;
    let mut wrong = source.summary.clone();
    wrong.total = wrong_amount.to_string();
    let mut stale_query = source.query.clone();
    stale_query.snapshot_hash = format!(
        "{:#x}",
        keccak256(format!(
            "TreasuryLens/synthetic-stale-snapshot/v1/{}",
            source.summary.query_id
        ))
    );
    if stale_query.snapshot_hash == source.query.snapshot_hash {
        return Err("synthetic snapshot unexpectedly collides with source snapshot".into());
    }
    let mut stale = source.summary.clone();
    stale.snapshot_hash = stale_query.snapshot_hash.clone();
    stale.query_id = stale_query.query_id().map_err(|error| error.to_string())?;
    // Keeping the numerical data and event digest unchanged isolates snapshot
    // binding. This is a coherent alternate endpoint identity, not real RPC data.
    let honest: Vec<_> = (0..4)
        .map(|index| Observation {
            node_id: format!("node{index}"),
            summary: source.summary.clone(),
        })
        .collect();
    let mut wrong_one = honest.clone();
    wrong_one[0].summary = wrong.clone();
    let mut wrong_two = wrong_one.clone();
    wrong_two[1].summary = wrong;
    let mut stale_one = honest.clone();
    stale_one[0].summary = stale.clone();
    let mut stale_two = stale_one.clone();
    stale_two[1].summary = stale;
    let cases = [
        ("honest4", honest.clone()),
        ("proposer_wrong_total", wrong_one),
        ("2wrong_total", wrong_two),
        ("one_stale_snapshot", stale_one),
        ("two_stale_snapshots", stale_two),
        ("two_missing", honest[..2].to_vec()),
    ];
    Ok(cases
        .into_iter()
        .map(|(name, observations)| Scenario {
            name: name.into(),
            synthetic: true,
            variants: evaluate_variants(source, &observations),
            observations,
        })
        .collect())
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let record: SourceRecord = serde_json::from_str(&input)
        .map_err(|error| format!("expected a persisted record with query and summary: {error}"))?;
    let record = SourceRecord {
        query: record
            .query
            .normalized()
            .map_err(|error| error.to_string())?,
        summary: record.summary,
    };
    let cases = scenarios(&record)?;
    let output = serde_json::json!({
        "kind": "deterministic-fault-model-ablation",
        "source": {
            "query_id": record.summary.query_id,
            "query": record.query,
            "summary": record.summary,
        },
        "threshold": 3,
        "definitions": {
            "evidence_boundary": "Input is a caller-supplied persisted record; this binary checks schema/bindings, not its committee certificate. All node observations below are synthetic policy inputs, not live nodes or signatures.",
            "single_node": "Node0 accepts its own observation without a quorum or source binding check.",
            "totals_only_majority3": "Three distinct identities agree on total and transfer count; query ID, snapshot and log digest are ignored by the acceptance policy.",
            "hardened_quorum3": "The existing select_quorum policy requires three distinct matching complete summaries bound to the requested query and snapshot.",
            "amount_correct": "An accepted amount equals the supplied source amount; false on rejection does not mean an incorrect amount was returned.",
            "snapshot_safe": "An accepted output has variant-required support matching every source summary field: one for single_node and three for each quorum variant. Numerical agreement alone is insufficient. False on rejection is not an unsafe acceptance.",
            "incorrect_accept": "Accepted and amount_correct is false.",
            "unsafe_accept": "Accepted and snapshot_safe is false.",
            "stale_model": "One or two observations use a different snapshot and its coherently recomputed query ID while retaining identical totals/count/event digest to isolate snapshot binding.",
            "scope": "Deterministic utility/safety comparison only; no network timing, throughput, empirical failure frequency, real Byzantine-node behavior or global novelty is claimed."
        },
        "scenarios": cases,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("evaluate: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceRecord {
        // A unit-test fixture, not evidence of a real devnet record.
        let query = QuerySpec {
            chain_id: 31337,
            token: format!("0x{}", "11".repeat(20)),
            treasury: format!("0x{}", "22".repeat(20)),
            from_block: 1,
            to_block: 10,
            snapshot_hash: format!("0x{}", "33".repeat(32)),
            rule_version: 1,
        };
        SourceRecord {
            summary: Summary {
                query_id: query.query_id().unwrap(),
                snapshot_hash: query.snapshot_hash.clone(),
                total: "100".into(),
                transfer_count: 1,
                logs_digest: format!("0x{}", "44".repeat(32)),
            },
            query,
        }
    }

    #[test]
    fn honest_scenario_is_useful_for_all_variants() {
        let cases = scenarios(&source()).unwrap();
        let case = cases.iter().find(|case| case.name == "honest4").unwrap();
        for variant in [
            &case.variants.single_node,
            &case.variants.totals_only_majority3,
            &case.variants.hardened_quorum3,
        ] {
            assert!(variant.accepted && variant.amount_correct && variant.snapshot_safe);
            assert!(!variant.incorrect_accept && !variant.unsafe_accept);
        }
    }

    #[test]
    fn one_stale_response_does_not_make_a_three_current_supporter_majority_unsafe() {
        let cases = scenarios(&source()).unwrap();
        let case = cases
            .iter()
            .find(|case| case.name == "one_stale_snapshot")
            .unwrap();
        assert!(case.variants.single_node.unsafe_accept);
        assert!(case.variants.single_node.amount_correct);
        assert!(case.variants.totals_only_majority3.accepted);
        assert!(case.variants.totals_only_majority3.snapshot_safe);
        assert_eq!(
            case.variants
                .totals_only_majority3
                .current_record_supporters,
            3
        );
        assert!(case.variants.hardened_quorum3.snapshot_safe);
    }

    #[test]
    fn two_stale_responses_expose_totals_only_binding_failure() {
        let cases = scenarios(&source()).unwrap();
        let case = cases
            .iter()
            .find(|case| case.name == "two_stale_snapshots")
            .unwrap();
        assert!(case.variants.totals_only_majority3.accepted);
        assert!(case.variants.totals_only_majority3.amount_correct);
        assert!(case.variants.totals_only_majority3.unsafe_accept);
        assert_eq!(
            case.variants
                .totals_only_majority3
                .current_record_supporters,
            2
        );
        assert!(!case.variants.hardened_quorum3.accepted);
        assert!(!case.variants.hardened_quorum3.unsafe_accept);
    }

    #[test]
    fn wrong_totals_and_missing_inputs_trade_utility_for_safety() {
        let cases = scenarios(&source()).unwrap();
        let one_wrong = cases
            .iter()
            .find(|case| case.name == "proposer_wrong_total")
            .unwrap();
        assert!(one_wrong.variants.single_node.incorrect_accept);
        assert!(one_wrong.variants.hardened_quorum3.amount_correct);
        for name in ["2wrong_total", "two_missing"] {
            let case = cases.iter().find(|case| case.name == name).unwrap();
            assert!(!case.variants.totals_only_majority3.accepted);
            assert!(!case.variants.hardened_quorum3.accepted);
            assert!(!case.variants.hardened_quorum3.unsafe_accept);
        }
    }

    #[test]
    fn source_identity_mismatch_and_uint256_overflow_are_explicit_errors() {
        let mut record = source();
        record.summary.query_id = format!("0x{}", "ff".repeat(32));
        assert!(scenarios(&record).is_err());
        record = source();
        record.summary.total = U256::MAX.to_string();
        assert!(scenarios(&record).unwrap_err().contains("uint256 max"));
    }
}
