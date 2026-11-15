//! Offline evidence content verifier. Reads no chain state and verifies no
//! committee signatures; the caller separately anchors a summary to its source.
use std::io::{self, Read};
use treasurylens_core::{EvidenceBundle, verify_evidence};

const MAX_INPUT_BYTES: u64 = 1024 * 1024;

fn run() -> Result<(), String> {
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read evidence: {error}"))?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err("evidence input exceeds 1 MiB".into());
    }
    let bundle: EvidenceBundle = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid evidence JSON: {error}"))?;
    let audit = verify_evidence(&bundle).map_err(|error| error.to_string())?;
    let output = serde_json::json!({
        "status": "valid",
        "certificate_verified": false,
        "verification_scope": "Internal content consistency only: schema, query identity, snapshot binding, canonical events, count, exact uint256 total and logs digest. No network requests or committee-signature verification.",
        "audit": audit,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!(
            "{}",
            serde_json::json!({
                "status": "invalid",
                "certificate_verified": false,
                "error": error,
            })
        );
        std::process::exit(1);
    }
}
