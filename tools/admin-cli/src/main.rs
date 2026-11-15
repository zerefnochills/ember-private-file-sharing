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
        #[arg(long)]
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

#[derive(Deserialize, Serialize)]
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

        Commands::Reports { server, token } => {
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

            match resp.json::<Vec<ReportItem>>().await {
                Ok(reports) => {
                    println!("Active Reports ({} total):", reports.len());
                    for (i, r) in reports.iter().enumerate() {
                        println!(
                            "[{}] ID: {} | Target: {} ({}) | Reason: {} | Status: {}",
                            i + 1,
                            r.id,
                            r.target_id,
                            r.target_type,
                            r.reason,
                            r.status
                        );
                        if let Some(ref notes) = r.notes {
                            println!("    Notes:   {notes}");
                        }
                        if let Some(ref content) = r.content {
                            println!("    Content: {content}");
                        }
                        if let Some(ref key) = r.link_key {
                            println!("    Key:     {key}");
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
