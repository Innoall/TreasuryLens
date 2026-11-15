//! Independent on-network consumer of TreasuryLens certified records.
use lyquid::prelude::*;

state! {
    network source: LyquidID = LyquidID::default();
    network checks: HashMap<String, String> = new_hashmap();
}

#[method::network(export = eth)]
fn constructor(ctx: &mut _, source: LyquidID) {
    *ctx.network.source = source;
}

// Cross-Lyquid atomic calls run as sequenced network transactions, not eth_call
// instance aliases. A separate read-only method exposes the persisted decision.
#[method::network(export = eth)]
fn check_budget(ctx: &mut _, query_id: String, budget: U256) -> LyquidResult<bool> {
    let key = format!("{query_id}:{budget}");
    if ctx.network.checks.contains_key(&key) {
        return Ok(true);
    }
    let source = *ctx.network.source;
    lyquid::println!("budget read source={} query={}", source, query_id);
    let reply = call!((source).get_result_json(query_id: String = query_id.clone()) -> (record: LyquidResult<String>));
    let record_json = reply.record?;
    lyquid::println!("budget source response={}", record_json);
    let record: serde_json::Value = serde_json::from_str(&record_json)
        .map_err(|e| LyquidError::LyquorRuntime(e.to_string()))?;
    let result = if let Some(total) = record["summary"]["total"].as_str() {
        let amount: U256 = total.parse().map_err(|_| LyquidError::LyquorInput)?;
        serde_json::json!({"query_id":query_id,"status":"confirmed","total":total,
            "budget":budget.to_string(),"budget_exceeded":amount > budget})
    } else {
        serde_json::json!({"query_id":query_id,"status":"rejected","budget":budget.to_string(),
            "error":"no retained certified source result"})
    };
    // Bound local prototype state; old decisions are disposable and recomputable.
    if ctx.network.checks.len() >= 200 {
        ctx.network.checks.clear();
    }
    lyquid::println!("budget decision={}", result);
    ctx.network.checks.insert(key, result.to_string());
    Ok(true)
}

#[method::network(export = eth)]
fn get_check_json(ctx: &_, query_id: String, budget: U256) -> LyquidResult<String> {
    Ok(ctx
        .network
        .checks
        .get(&format!("{query_id}:{budget}"))
        .cloned()
        .unwrap_or_else(|| "null".into()))
}
