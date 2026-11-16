//! Ember Admin CLI for takedowns, report review, and transparency statistics.

use std::collections::HashMap;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "ember-admin")]
#[command(about = "Ember relay server administrative and takedown tool", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute an administrative takedown by target ID (blob or mailbox)
    Takedown {
        /// Base URL of the Ember relay server
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        server: String,

        /// Bearer admin/reviewer token (or set EMBER_ADMIN_TOKEN env var)
        #[arg(long)]
        token: Option<String>,

        /// Target blob ID or mailbox ID to remove
        #[arg(long, allow_hyphen_values = true)]
        target: String,

        /// Target type: 'blob' or 'mailbox'
        #[arg(long, default_value = "blob")]
        target_type: String,

        /// Legal order reference or grievance ticket ID
        #[arg(long)]
        order_ref: String,

        /// Name or identifier of the operator executing the takedown
        #[arg(long)]
        operator: String,

        /// Reason for takedown (e.g. court_order, ncii, illegal_content)
        #[arg(long)]
        reason: String,
    },

    /// List pending abuse and moderation reports (reviewer role only)
    Reports {
        /// Base URL of the Ember relay server
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        server: String,

        /// Bearer admin/reviewer token (or set EMBER_ADMIN_TOKEN env var)
        #[arg(long)]
        token: Option<String>,

        /// Optional reviewer 64-hex secret key to decrypt sealed reports inline
        #[arg(long)]
        secret_key: Option<String>,

        /// Optional reviewer 64-hex public key
        #[arg(long)]
        public_key: Option<String>,
    },

    /// Generate a new X25519 reviewer keypair for sealed-box report encryption
    GenerateReviewerKeys,

    /// Decrypt a report file, ID, or sealed field using the reviewer secret key
    DecryptReport {
        /// Reviewer 64-hex secret key (or set EMBER_REVIEWER_SECRET_KEY env var)
        #[arg(long)]
        secret_key: Option<String>,

        /// Reviewer 64-hex public key (or set EMBER_REVIEWER_PUBLIC_KEY env var)
        #[arg(long)]
        public_key: Option<String>,

        /// Path to report JSON file on disk
        #[arg(long)]
        file: Option<std::path::PathBuf>,

        /// Report ID to fetch and decrypt from relay server
        #[arg(long)]
        id: Option<String>,

        /// Base URL of the Ember relay server
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        server: String,

        /// Bearer admin/reviewer token (or set EMBER_ADMIN_TOKEN env var)
        #[arg(long)]
        token: Option<String>,

        /// Directly decrypt a single sealed string (e.g. "sealed:...")
        #[arg(long)]
        sealed: Option<String>,
    },

    /// Fetch and display public transparency statistics
    Transparency {
        /// Base URL of the Ember relay server
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        server: String,

        /// Output raw JSON instead of human-readable summary
        #[arg(long)]
        json: bool,
    },
}

#[derive(Serialize)]
struct AdminTakedownPayload {
    target_id: String,
    target_type: String,
    request_ref: String,
    operator: String,
    reason: String,
}

#[derive(Deserialize, Serialize)]
struct TakedownResponse {
    id: String,
    timestamp: u64,
    target_id: String,
    target_type: String,
    request_ref: String,
    operator: String,
    reason: String,
    action_taken: String,
}

#[derive(Deserialize, Serialize, Clone)]
struct ReportItem {
    id: String,
    created_at: u64,
    expires_at: u64,
    target_id: String,
    target_type: String,
    reason: String,
    notes: Option<String>,
    content: Option<String>,
    link_key: Option<String>,
    status: String,
    #[serde(default)]
    encrypted: bool,
}

fn resolve_reviewer_keys(
    secret_key_arg: Option<String>,
    public_key_arg: Option<String>,
) -> Result<([u8; 32], [u8; 32]), String> {
    let sk_str = secret_key_arg
        .or_else(|| std::env::var("EMBER_REVIEWER_SECRET_KEY").ok())
        .ok_or_else(|| {
            "Reviewer secret key required (via --secret-key or EMBER_REVIEWER_SECRET_KEY)"
                .to_string()
        })?;

    let pk_str = public_key_arg
        .or_else(|| std::env::var("EMBER_REVIEWER_PUBLIC_KEY").ok())
        .ok_or_else(|| {
            "Reviewer public key required (via --public-key or EMBER_REVIEWER_PUBLIC_KEY)"
                .to_string()
        })?;

    let sk = ember_core::parse_hex_key(&sk_str)
        .map_err(|e| format!("Invalid reviewer secret key hex: {e}"))?;
    let pk = ember_core::parse_hex_key(&pk_str)
        .map_err(|e| format!("Invalid reviewer public key hex: {e}"))?;

    Ok((pk, sk))
}

fn decrypt_sealed_field(val: &Option<String>, keys: &([u8; 32], [u8; 32])) -> String {
    if let Some(ref s) = val {
        if s.starts_with(ember_core::SEALED_PREFIX) {
            match ember_core::unseal_report_field(s, &keys.0, &keys.1) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(e) => format!("[Decryption failed: {e}]"),
            }
        } else {
            s.clone()
        }
    } else {
        "(none)".to_string()
    }
}

#[derive(Deserialize, Serialize)]
struct TransparencyResponse {
    total_takedowns: usize,
    total_reports: usize,
    takedowns_by_reason: HashMap<String, usize>,
    reports_by_reason: HashMap<String, usize>,
}

fn resolve_token(token_arg: Option<String>) -> Result<String, String> {
    if let Some(t) = token_arg {
        if !t.trim().is_empty() {
            return Ok(t);
        }
    }
    std::env::var("EMBER_ADMIN_TOKEN").map_err(|_| {
        "Admin token must be provided via --token or EMBER_ADMIN_TOKEN environment variable"
            .to_string()
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let client = reqwest::Client::new();

    match cli.command {
        Commands::Takedown {
            server,
            token,
            target,
            target_type,
            order_ref,
            operator,
            reason,
        } => {
            let admin_token = match resolve_token(token) {
                Ok(t) => t,
                Err(err) => {
                    eprintln!("Error: {err}");
                    return ExitCode::FAILURE;
                }
            };

            let payload = AdminTakedownPayload {
                target_id: target,
                target_type,
                request_ref: order_ref,
                operator,
                reason,
            };

            let url = format!("{}/v1/admin/takedown", server.trim_end_matches('/'));
            let resp = match client
                .post(&url)
                .bearer_auth(admin_token)
                .json(&payload)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Network request failed: {e}");
                    return ExitCode::FAILURE;
                }
            };

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                eprintln!("Takedown failed with HTTP {status}: {body}");
                return ExitCode::FAILURE;
            }

            match resp.json::<TakedownResponse>().await {
                Ok(record) => {
                    println!("Takedown successful.");
                    println!("Record ID:     {}", record.id);
                    println!("Target ID:     {}", record.target_id);
                    println!("Target Type:   {}", record.target_type);
                    println!("Action Taken:  {}", record.action_taken);
                    println!("Reference:     {}", record.request_ref);
                    println!("Operator:      {}", record.operator);
                    println!("Timestamp:     {}", record.timestamp);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("Failed to parse response: {e}");
                    ExitCode::FAILURE
                }
            }
        }

        Commands::Reports {
            server,
            token,
            secret_key,
            public_key,
        } => {
            let admin_token = match resolve_token(token) {
                Ok(t) => t,
                Err(err) => {
                    eprintln!("Error: {err}");
                    return ExitCode::FAILURE;
                }
            };

            let reviewer_keys = if secret_key.is_some() || public_key.is_some() {
                match resolve_reviewer_keys(secret_key, public_key) {
                    Ok(k) => Some(k),
                    Err(err) => {
                        eprintln!("Key error: {err}");
                        return ExitCode::FAILURE;
                    }
                }
            } else {
                None
            };

            let url = format!("{}/v1/admin/reports", server.trim_end_matches('/'));
            let resp = match client.get(&url).bearer_auth(admin_token).send().await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Network request failed: {e}");
                    return ExitCode::FAILURE;
                }
            };

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                eprintln!("Fetch reports failed with HTTP {status}: {body}");
                return ExitCode::FAILURE;
            }

            match resp.json::<Vec<ReportItem>>().await {
                Ok(reports) => {
                    println!("Active Reports ({} total):", reports.len());
                    for (i, r) in reports.iter().enumerate() {
                        println!(
                            "[{}] ID: {} | Target: {} ({}) | Reason: {} | Status: {} | Encrypted: {}",
                            i + 1,
                            r.id,
                            r.target_id,
                            r.target_type,
                            r.reason,
                            r.status,
                            r.encrypted
                        );
                        if let Some(ref notes) = r.notes {
                            println!("    Notes:   {notes}");
                        }
                        if let Some(ref content) = r.content {
                            if let Some(ref keys) = reviewer_keys {
                                let dec = decrypt_sealed_field(&r.content, keys);
                                println!("    Content: {dec} (decrypted)");
                            } else {
                                println!("    Content: {content}");
                            }
                        }
                        if let Some(ref key) = r.link_key {
                            if let Some(ref keys) = reviewer_keys {
                                let dec = decrypt_sealed_field(&r.link_key, keys);
                                println!("    Key:     {dec} (decrypted)");
                            } else {
                                println!("    Key:     {key}");
                            }
                        }
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("Failed to parse response: {e}");
                    ExitCode::FAILURE
                }
            }
        }

        Commands::GenerateReviewerKeys => {
            let (pk, sk) = ember_core::generate_reviewer_keypair();
            let pk_hex = ember_core::key_to_hex(&pk);
            let sk_hex = ember_core::key_to_hex(&sk);
            println!("=== Reviewer Keypair Generated ===");
            println!("Public Key (hex): {pk_hex}");
            println!("Secret Key (hex): {sk_hex}");
            println!();
            println!("To configure ember-server to encrypt reports at rest:");
            println!("  ember-server --reviewer-public-key {pk_hex} ...");
            println!();
            println!("To decrypt reports with ember-admin:");
            println!("  ember-admin decrypt-report --secret-key {sk_hex} --public-key {pk_hex} --file <path>");
            ExitCode::SUCCESS
        }

        Commands::DecryptReport {
            secret_key,
            public_key,
            file,
            id,
            server,
            token,
            sealed,
        } => {
            let keys = match resolve_reviewer_keys(secret_key, public_key) {
                Ok(k) => k,
                Err(err) => {
                    eprintln!("Error resolving reviewer keys: {err}");
                    return ExitCode::FAILURE;
                }
            };

            // Option 1: Decrypt direct sealed string
            if let Some(sealed_val) = sealed {
                let dec = decrypt_sealed_field(&Some(sealed_val), &keys);
                println!("Decrypted Payload: {dec}");
                return ExitCode::SUCCESS;
            }

            // Option 2: Decrypt report file on disk
            if let Some(file_path) = file {
                let raw = match std::fs::read_to_string(&file_path) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to read file {}: {e}", file_path.display());
                        return ExitCode::FAILURE;
                    }
                };
                let report: ReportItem = match serde_json::from_str(&raw) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to parse report JSON: {e}");
                        return ExitCode::FAILURE;
                    }
                };

                println!(
                    "=== Report Details (from file: {}) ===",
                    file_path.display()
                );
                println!("ID:          {}", report.id);
                println!("Target:      {} ({})", report.target_id, report.target_type);
                println!("Reason:      {}", report.reason);
                println!("Status:      {}", report.status);
                if let Some(ref notes) = report.notes {
                    println!("Notes:       {notes}");
                }
                println!(
                    "Content:     {}",
                    decrypt_sealed_field(&report.content, &keys)
                );
                println!(
                    "Link Key:    {}",
                    decrypt_sealed_field(&report.link_key, &keys)
                );
                return ExitCode::SUCCESS;
            }

            // Option 3: Fetch report by ID from server
            if let Some(report_id) = id {
                let admin_token = match resolve_token(token) {
                    Ok(t) => t,
                    Err(err) => {
                        eprintln!("Error: {err}");
                        return ExitCode::FAILURE;
                    }
                };

                let url = format!("{}/v1/admin/reports", server.trim_end_matches('/'));
                let resp = match client.get(&url).bearer_auth(admin_token).send().await {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Network request failed: {e}");
                        return ExitCode::FAILURE;
                    }
                };

                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    eprintln!("Fetch reports failed with HTTP {status}: {body}");
                    return ExitCode::FAILURE;
                }

                let reports: Vec<ReportItem> = match resp.json().await {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to parse response: {e}");
                        return ExitCode::FAILURE;
                    }
                };

                let found = reports.into_iter().find(|r| r.id == report_id);
                match found {
                    Some(report) => {
                        println!("=== Report Details (ID: {}) ===", report.id);
                        println!("Target:      {} ({})", report.target_id, report.target_type);
                        println!("Reason:      {}", report.reason);
                        println!("Status:      {}", report.status);
                        if let Some(ref notes) = report.notes {
                            println!("Notes:       {notes}");
                        }
                        println!(
                            "Content:     {}",
                            decrypt_sealed_field(&report.content, &keys)
                        );
                        println!(
                            "Link Key:    {}",
                            decrypt_sealed_field(&report.link_key, &keys)
                        );
                        ExitCode::SUCCESS
                    }
                    None => {
                        eprintln!("Report ID '{report_id}' not found on server");
                        ExitCode::FAILURE
                    }
                }
            } else {
                eprintln!("Error: Must provide either --file <PATH>, --id <REPORT_ID>, or --sealed <STRING>");
                ExitCode::FAILURE
            }
        }

        Commands::Transparency { server, json } => {
            let url = format!("{}/v1/transparency", server.trim_end_matches('/'));
            let resp = match client.get(&url).send().await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Network request failed: {e}");
                    return ExitCode::FAILURE;
                }
            };

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                eprintln!("Failed with HTTP {status}: {body}");
                return ExitCode::FAILURE;
            }

            if json {
                let body = resp.text().await.unwrap_or_default();
                println!("{body}");
                ExitCode::SUCCESS
            } else {
                match resp.json::<TransparencyResponse>().await {
                    Ok(stats) => {
                        println!("=== Ember Relay Transparency Report ===");
                        println!("Total Takedowns Processed: {}", stats.total_takedowns);
                        for (reason, count) in &stats.takedowns_by_reason {
                            println!("  - {reason}: {count}");
                        }
                        println!("Total Reports Received:    {}", stats.total_reports);
                        for (reason, count) in &stats.reports_by_reason {
                            println!("  - {reason}: {count}");
                        }
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Failed to parse transparency data: {e}");
                        ExitCode::FAILURE
                    }
                }
            }
        }
    }
}
