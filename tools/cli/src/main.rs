//! Ember CLI: command-line client for encrypting, decrypting, sending, and receiving drops.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use clap::{Parser, Subcommand};
use ember_core::thread::{
    derive_rendezvous_mailbox, HandshakeInitiator, HandshakeResponder, MailboxCapability,
    ThreadSession,
};
use ember_core::{
    build_link, decrypt_drop, decrypt_drop_parts, encrypt_drop, encrypt_drop_to_parts,
    generate_blob_id, parse_link, strip_metadata, unwrap_key, wrap_key, DropMetadata, LinkPayload,
    SecretKey, StripStatus,
};

#[derive(Parser)]
#[command(
    name = "ember",
    version,
    about = "Ember: ephemeral encrypted drop client"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Encrypt a file into a local ciphertext drop
    Encrypt {
        /// Path to the file to encrypt
        file: PathBuf,

        /// Output path for ciphertext (defaults to <file>.ember)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Optional passphrase to wrap the link key
        #[arg(short, long)]
        passphrase: Option<String>,

        /// Sender intent: burn after first completed view
        #[arg(long, default_value_t = false)]
        burn: bool,

        /// Hostname to include in the share link
        #[arg(long, default_value = "ember.example.com")]
        host: String,

        /// Disable automatic location and author metadata stripping
        #[arg(long, default_value_t = false)]
        no_strip: bool,
    },

    /// Decrypt a local ciphertext drop
    Decrypt {
        /// Ciphertext file or link
        input: Option<String>,

        /// Path to ciphertext file
        #[arg(short, long)]
        ciphertext: Option<PathBuf>,

        /// Share link or link payload
        #[arg(short, long)]
        link: Option<String>,

        /// Destination path for decrypted output
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Passphrase if the drop is passphrase-protected
        #[arg(short, long)]
        passphrase: Option<String>,
    },

    /// Send an encrypted file through a relay server
    Send {
        /// Path to the file to send
        file: PathBuf,

        /// Relay server URL
        #[arg(short, long, default_value = "http://127.0.0.1:8080")]
        relay: String,

        /// Optional passphrase to protect the drop
        #[arg(short, long)]
        passphrase: Option<String>,

        /// Burn after first completed view
        #[arg(long, default_value_t = false)]
        burn: bool,

        /// Time-to-live in seconds (max 43,200)
        #[arg(long, default_value_t = 43200)]
        ttl: u64,

        /// Disable automatic location and author metadata stripping
        #[arg(long, default_value_t = false)]
        no_strip: bool,
    },

    /// Receive and decrypt a drop from a relay server
    Receive {
        /// Share link (e.g. https://<host>/d/<blob_id>#<payload>)
        link: String,

        /// Relay server URL override (defaults to host from link)
        #[arg(short, long)]
        relay: Option<String>,

        /// Destination path for downloaded file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Passphrase if the drop is passphrase-protected
        #[arg(short, long)]
        passphrase: Option<String>,
    },

    /// Start or join an ephemeral 1:1 encrypted thread for a drop
    Thread {
        /// Share link for the drop (e.g. https://<host>/d/<blob_id>#<payload>)
        link: String,

        /// Relay server URL override (defaults to host from link)
        #[arg(short, long)]
        relay: Option<String>,

        /// Passphrase if the drop is passphrase-protected
        #[arg(short, long)]
        passphrase: Option<String>,

        /// Custom local handle (defaults to random name like amber-fox-4821)
        #[arg(long)]
        handle: Option<String>,
    },
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Encrypt {
            file,
            output,
            passphrase,
            burn,
            host,
            no_strip,
        } => {
            if !file.exists() {
                eprintln!("Error: file not found: {}", file.display());
                std::process::exit(1);
            }

            let raw = fs::read(&file)?;
            let file_name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("drop.bin");

            let content = if no_strip {
                raw
            } else {
                let res = strip_metadata(&raw, Some(file_name))?;
                match res.status {
                    StripStatus::Stripped {
                        ref fields_removed, ..
                    } => {
                        println!(
                            "Stripped metadata before encryption: {}",
                            fields_removed.join(", ")
                        );
                        res.data
                    }
                    StripStatus::NoMetadataFound { .. } => raw,
                    StripStatus::UnsupportedFormat { ref reason, .. } => {
                        println!("Note: {reason}");
                        raw
                    }
                }
            };

            let created = now_secs();
            let expires = created + 43200;
            let metadata = DropMetadata::new_file(
                file_name,
                "application/octet-stream",
                content.len() as u64,
                burn,
                created,
                expires,
            );

            let master_key = SecretKey::generate();
            let ciphertext = encrypt_drop(&master_key, &metadata, &content)?;

            let out_path = output.unwrap_or_else(|| {
                let mut p = file.clone();
                p.set_extension("ember");
                p
            });

            fs::write(&out_path, &ciphertext)?;

            let payload = if let Some(pass) = passphrase {
                let wrapped = wrap_key(&master_key, &pass, None)?;
                LinkPayload::Protected(wrapped)
            } else {
                LinkPayload::Unprotected(master_key)
            };

            let blob_id = generate_blob_id();
            let share_link = build_link(&host, &blob_id, &payload)?;

            println!("Encrypted: {} -> {}", file.display(), out_path.display());
            println!("Ciphertext size: {} bytes", ciphertext.len());
            println!("Share link: {}", share_link);
        }

        Commands::Decrypt {
            input,
            ciphertext,
            link,
            output,
            passphrase,
        } => {
            let mut ct_path: Option<PathBuf> = ciphertext;
            let mut link_str: Option<String> = link;

            if let Some(pos_arg) = input {
                if pos_arg.starts_with("http://") || pos_arg.starts_with("https://") {
                    if link_str.is_none() {
                        link_str = Some(pos_arg);
                    }
                } else if Path::new(&pos_arg).exists() {
                    if ct_path.is_none() {
                        ct_path = Some(PathBuf::from(pos_arg));
                    }
                } else if link_str.is_none() {
                    link_str = Some(pos_arg);
                }
            }

            let ct_file = match ct_path {
                Some(p) => p,
                None => {
                    eprintln!("Error: ciphertext file required. Specify with --ciphertext <path>");
                    std::process::exit(1);
                }
            };

            let link_val = match link_str {
                Some(l) => l,
                None => {
                    eprintln!("Error: share link required. Specify with --link <url-or-payload>");
                    std::process::exit(1);
                }
            };

            let payload = if link_val.contains('#') || link_val.starts_with("http") {
                let parsed = parse_link(&link_val)?;
                parsed.payload
            } else {
                ember_core::decode_payload(&link_val)?
            };

            let master_key = match payload {
                LinkPayload::Unprotected(k) => {
                    if passphrase.is_some() {
                        eprintln!("Error: passphrase provided for an unprotected drop");
                        std::process::exit(1);
                    }
                    k
                }
                LinkPayload::Protected(wrapped_bytes) => {
                    let pass = match passphrase {
                        Some(p) => p,
                        None => {
                            eprintln!("Error: drop is passphrase-protected. Provide --passphrase <passphrase>");
                            std::process::exit(1);
                        }
                    };
                    unwrap_key(&wrapped_bytes, &pass)?
                }
            };

            let ct_bytes = fs::read(&ct_file)?;
            let (metadata, decrypted_content) = decrypt_drop(&master_key, &ct_bytes)?;

            let out_file = output.unwrap_or_else(|| PathBuf::from(metadata.sanitized_file_name()));
            fs::write(&out_file, &decrypted_content)?;

            println!("Decrypted successfully!");
            println!("Kind: {}", metadata.kind);
            println!("Original name: {}", metadata.name);
            println!("Plaintext size: {} bytes", decrypted_content.len());
            println!("Saved to: {}", out_file.display());
        }

        Commands::Send {
            file,
            relay,
            passphrase,
            burn,
            ttl,
            no_strip,
        } => {
            if !file.exists() {
                eprintln!("Error: file not found: {}", file.display());
                std::process::exit(1);
            }

            let raw = fs::read(&file)?;
            let file_name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("drop.bin");

            let content = if no_strip {
                raw
            } else {
                let res = strip_metadata(&raw, Some(file_name))?;
                match res.status {
                    StripStatus::Stripped {
                        ref fields_removed, ..
                    } => {
                        println!(
                            "Stripped metadata before sending: {}",
                            fields_removed.join(", ")
                        );
                        res.data
                    }
                    StripStatus::NoMetadataFound { .. } => raw,
                    StripStatus::UnsupportedFormat { ref reason, .. } => {
                        println!("Note: {reason}");
                        raw
                    }
                }
            };

            let created = now_secs();
            let expires = created + ttl.min(43200);
            let metadata = DropMetadata::new_file(
                file_name,
                "application/octet-stream",
                content.len() as u64,
                burn,
                created,
                expires,
            );

            let master_key = SecretKey::generate();
            let parts = encrypt_drop_to_parts(&master_key, &metadata, &content)?;
            let total_size: u64 = parts.iter().map(|p| p.len() as u64).sum();

            let relay_base = relay.trim_end_matches('/');
            let client = reqwest::Client::new();

            // 0. Request PoW challenge and solve puzzle
            let pow_resp = client
                .get(format!("{relay_base}/v1/pow/challenge"))
                .send()
                .await?;
            if !pow_resp.status().is_success() {
                eprintln!("Failed to get PoW challenge: HTTP {}", pow_resp.status());
                std::process::exit(1);
            }
            let pow_data: serde_json::Value = pow_resp.json().await?;
            let challenge = pow_data["challenge"]
                .as_str()
                .ok_or("Missing challenge field in PoW response")?;
            let difficulty = pow_data["difficulty"]
                .as_u64()
                .ok_or("Missing difficulty field in PoW response")?
                as u32;

            let nonce = ember_core::solve_pow(challenge.as_bytes(), difficulty, None)?;

            // 1. Create blob
            let create_body = serde_json::json!({
                "size": total_size,
                "parts": parts.len(),
                "burn": burn,
                "ttl_secs": ttl,
            });

            let resp = client
                .post(format!("{relay_base}/v1/blobs"))
                .header("X-Pow-Challenge", challenge)
                .header("X-Pow-Nonce", nonce.to_string())
                .json(&create_body)
                .send()
                .await?;

            if !resp.status().is_success() {
                eprintln!("Failed to create blob: HTTP {}", resp.status());
                std::process::exit(1);
            }

            let create_res: serde_json::Value = resp.json().await?;
            let blob_id = create_res["id"]
                .as_str()
                .ok_or("Missing id in response")?
                .to_string();
            let delete_token = create_res["delete_token"]
                .as_str()
                .ok_or("Missing delete_token in response")?
                .to_string();

            // 2. Upload parts
            for (idx, part) in parts.iter().enumerate() {
                let put_resp = client
                    .put(format!("{relay_base}/v1/blobs/{blob_id}/parts/{idx}"))
                    .header("Authorization", format!("Bearer {delete_token}"))
                    .body(part.clone())
                    .send()
                    .await?;

                if !put_resp.status().is_success() {
                    eprintln!("Failed to upload part {idx}: HTTP {}", put_resp.status());
                    std::process::exit(1);
                }
            }

            // 3. Complete blob
            let comp_resp = client
                .post(format!("{relay_base}/v1/blobs/{blob_id}/complete"))
                .header("Authorization", format!("Bearer {delete_token}"))
                .send()
                .await?;

            if !comp_resp.status().is_success() {
                eprintln!("Failed to complete upload: HTTP {}", comp_resp.status());
                std::process::exit(1);
            }

            // 4. Build share link
            let payload = if let Some(pass) = passphrase {
                let wrapped = wrap_key(&master_key, &pass, None)?;
                LinkPayload::Protected(wrapped)
            } else {
                LinkPayload::Unprotected(master_key)
            };

            let host = relay_base
                .strip_prefix("https://")
                .or_else(|| relay_base.strip_prefix("http://"))
                .unwrap_or(relay_base);

            let share_link = build_link(host, &blob_id, &payload)?;

            println!(
                "Uploaded: {} ({} parts, {} bytes)",
                file.display(),
                parts.len(),
                total_size
            );
            println!("Share link: {}", share_link);
        }

        Commands::Receive {
            link,
            relay,
            output,
            passphrase,
        } => {
            let parsed = parse_link(&link)?;
            let host = parsed.host;
            let blob_id = parsed.blob_id;
            let payload = parsed.payload;

            let relay_base = relay.unwrap_or_else(|| {
                if host.starts_with("127.0.0.1") || host.starts_with("localhost") {
                    format!("http://{host}")
                } else {
                    format!("https://{host}")
                }
            });
            let relay_base = relay_base.trim_end_matches('/');

            let client = reqwest::Client::new();

            // 1. Fetch metadata
            let meta_resp = client
                .get(format!("{relay_base}/v1/blobs/{blob_id}"))
                .send()
                .await?;

            if meta_resp.status() == reqwest::StatusCode::GONE {
                eprintln!("Error: This drop has expired or was already burned.");
                std::process::exit(1);
            }
            if !meta_resp.status().is_success() {
                eprintln!("Error fetching drop metadata: HTTP {}", meta_resp.status());
                std::process::exit(1);
            }

            let meta_json: serde_json::Value = meta_resp.json().await?;
            let part_count = meta_json["parts"].as_u64().unwrap_or(1) as u32;
            let burn = meta_json["burn"].as_bool().unwrap_or(false);

            if burn {
                println!("Note: Burn-after-view enabled (this drop can only be opened once).");
            }

            // 2. Claim download
            let claim_resp = client
                .post(format!("{relay_base}/v1/blobs/{blob_id}/claim"))
                .send()
                .await?;

            if claim_resp.status() == reqwest::StatusCode::CONFLICT {
                eprintln!("Error: This drop is currently being opened by another recipient.");
                std::process::exit(1);
            }
            if !claim_resp.status().is_success() {
                eprintln!("Error claiming drop: HTTP {}", claim_resp.status());
                std::process::exit(1);
            }

            let claim_json: serde_json::Value = claim_resp.json().await?;
            let claim_token = claim_json["claim"].as_str().map(|s| s.to_string());

            // 3. Download parts
            let mut parts = Vec::with_capacity(part_count as usize);
            for idx in 0..part_count {
                let mut req = client.get(format!("{relay_base}/v1/blobs/{blob_id}/parts/{idx}"));
                if let Some(ref ct) = claim_token {
                    req = req.header("X-Claim", ct);
                }
                let part_resp = req.send().await?;
                if !part_resp.status().is_success() {
                    eprintln!("Error downloading part {idx}: HTTP {}", part_resp.status());
                    std::process::exit(1);
                }
                let part_bytes = part_resp.bytes().await?.to_vec();
                parts.push(part_bytes);
            }

            // 4. Decrypt
            let master_key = match payload {
                LinkPayload::Unprotected(k) => {
                    if passphrase.is_some() {
                        eprintln!("Error: Passphrase provided for an unprotected drop.");
                        std::process::exit(1);
                    }
                    k
                }
                LinkPayload::Protected(wrapped_bytes) => {
                    let pass = match passphrase {
                        Some(p) => p,
                        None => {
                            eprintln!("Error: Drop is passphrase-protected. Provide --passphrase <passphrase>");
                            std::process::exit(1);
                        }
                    };
                    unwrap_key(&wrapped_bytes, &pass)?
                }
            };

            let (metadata, decrypted_content) = decrypt_drop_parts(&master_key, &parts)?;

            // 5. Finish burning
            let mut finish_req = client.post(format!("{relay_base}/v1/blobs/{blob_id}/finish"));
            if let Some(ref ct) = claim_token {
                finish_req = finish_req.header("X-Claim", ct);
            }
            let _ = finish_req.send().await;

            // 6. Save decrypted file
            let out_file = output.unwrap_or_else(|| PathBuf::from(metadata.sanitized_file_name()));
            fs::write(&out_file, &decrypted_content)?;

            println!("Received successfully!");
            println!("Original name: {}", metadata.name);
            println!("Plaintext size: {} bytes", decrypted_content.len());
            println!("Saved to: {}", out_file.display());
        }

        Commands::Thread {
            link,
            relay,
            passphrase,
            handle,
        } => {
            let parsed = parse_link(&link).map_err(|e| format!("Invalid drop link: {e}"))?;
            let server_url = relay.unwrap_or_else(|| format!("https://{}", parsed.host));
            let client = reqwest::Client::new();

            // Extract drop key
            let drop_key = match parsed.payload {
                LinkPayload::Unprotected(k) => {
                    if passphrase.is_some() {
                        eprintln!("Error: Passphrase provided for an unprotected drop.");
                        std::process::exit(1);
                    }
                    k
                }
                LinkPayload::Protected(wrapped_bytes) => {
                    let pass = match passphrase {
                        Some(p) => p,
                        None => {
                            eprintln!("Error: Drop is passphrase-protected. Provide --passphrase <passphrase>");
                            std::process::exit(1);
                        }
                    };
                    unwrap_key(&wrapped_bytes, &pass)?
                }
            };

            let local_handle = handle.unwrap_or_else(ember_core::thread::generate_handle);
            println!("Initializing ephemeral thread...");
            println!("Your handle: {local_handle}");

            let mb_rendezvous = derive_rendezvous_mailbox(&drop_key)
                .map_err(|e| format!("Rendezvous derivation failed: {e}"))?;

            // 1. Register rendezvous mailbox with PoW (idempotent)
            let _ = register_mailbox_with_pow(&client, &server_url, &mb_rendezvous, 3600).await;

            // 2. Check rendezvous mailbox: did peer already post their ephemeral public key?
            let check_res = client
                .get(format!(
                    "{server_url}/v1/mailboxes/{}/messages?poll_timeout_secs=0&drain=true",
                    mb_rendezvous.id
                ))
                .header(
                    "Authorization",
                    format!("Bearer {}", mb_rendezvous.pull_token),
                )
                .send()
                .await;

            let session = match check_res {
                Ok(resp) if resp.status().is_success() => {
                    let data: serde_json::Value = resp.json().await.unwrap_or_default();
                    let msgs = data.get("messages").and_then(|m| m.as_array());
                    if let Some(messages) = msgs {
                        if !messages.is_empty() {
                            // Responder path! Peer (initiator) is already waiting!
                            let peer_payload_b64 =
                                messages[0]["payload"].as_str().ok_or("Invalid payload")?;
                            let peer_msg_bytes = URL_SAFE_NO_PAD.decode(peer_payload_b64)?;
                            if peer_msg_bytes.len() != 49 {
                                return Err("Invalid peer handshake message length".into());
                            }

                            let (resp_msg, resp_keys) =
                                HandshakeResponder::respond(&drop_key, &peer_msg_bytes)
                                    .map_err(|e| format!("Handshake failed: {e}"))?;

                            // Post our handshake message to rendezvous mailbox
                            let post_req = serde_json::json!({
                                "payload": URL_SAFE_NO_PAD.encode(&resp_msg),
                            });
                            client
                                .post(format!(
                                    "{server_url}/v1/mailboxes/{}/messages",
                                    mb_rendezvous.id
                                ))
                                .header(
                                    "Authorization",
                                    format!("Bearer {}", mb_rendezvous.push_token),
                                )
                                .json(&post_req)
                                .send()
                                .await?;

                            // Register directional mailboxes on the server with PoW
                            for mb in [&resp_keys.mailbox_out, &resp_keys.mailbox_in] {
                                let _ = register_mailbox_with_pow(&client, &server_url, mb, 43200)
                                    .await;
                            }

                            ThreadSession::new(local_handle.clone(), resp_keys)
                        } else {
                            run_initiator_handshake(
                                &client,
                                &server_url,
                                &drop_key,
                                &mb_rendezvous,
                                local_handle.clone(),
                            )
                            .await?
                        }
                    } else {
                        run_initiator_handshake(
                            &client,
                            &server_url,
                            &drop_key,
                            &mb_rendezvous,
                            local_handle.clone(),
                        )
                        .await?
                    }
                }
                _ => {
                    run_initiator_handshake(
                        &client,
                        &server_url,
                        &drop_key,
                        &mb_rendezvous,
                        local_handle.clone(),
                    )
                    .await?
                }
            };

            println!("Thread connected! You can now send and receive messages.");
            println!("Type a message and press Enter. Type /quit to leave.\n");

            run_chat_loop(&client, &server_url, session).await?;
        }
    }

    Ok(())
}

async fn register_mailbox_with_pow(
    client: &reqwest::Client,
    server_url: &str,
    mb: &MailboxCapability,
    ttl_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let pow_resp = client
        .get(format!("{server_url}/v1/pow/challenge"))
        .send()
        .await?;
    if !pow_resp.status().is_success() {
        return Err(format!("Failed to get PoW challenge: HTTP {}", pow_resp.status()).into());
    }
    let pow_data: serde_json::Value = pow_resp.json().await?;
    let challenge = pow_data["challenge"]
        .as_str()
        .ok_or("Missing challenge in response")?;
    let difficulty = pow_data["difficulty"]
        .as_u64()
        .ok_or("Missing difficulty in response")? as u32;
    let nonce = ember_core::solve_pow(challenge.as_bytes(), difficulty, None)?;

    let mb_reg = serde_json::json!({
        "id": mb.id,
        "push_token_hash": URL_SAFE_NO_PAD.encode(mb.push_token_hash()),
        "pull_token_hash": URL_SAFE_NO_PAD.encode(mb.pull_token_hash()),
        "ttl_secs": ttl_secs,
        "pow": {
            "challenge": challenge,
            "nonce": nonce,
        }
    });

    let reg_resp = client
        .post(format!("{server_url}/v1/mailboxes"))
        .json(&mb_reg)
        .send()
        .await?;

    if !reg_resp.status().is_success() && reg_resp.status() != reqwest::StatusCode::CONFLICT {
        eprintln!(
            "Warning: mailbox registration returned HTTP {}",
            reg_resp.status()
        );
    }

    Ok(())
}

async fn run_initiator_handshake(
    client: &reqwest::Client,
    server_url: &str,
    drop_key: &SecretKey,
    mb_rendezvous: &MailboxCapability,
    local_handle: String,
) -> Result<ThreadSession, Box<dyn std::error::Error>> {
    let init = HandshakeInitiator::new(drop_key).map_err(|e| format!("Initiator error: {e}"))?;
    let init_msg = init.handshake_message();

    let post_req = serde_json::json!({
        "payload": URL_SAFE_NO_PAD.encode(init_msg),
    });
    client
        .post(format!(
            "{server_url}/v1/mailboxes/{}/messages",
            mb_rendezvous.id
        ))
        .header(
            "Authorization",
            format!("Bearer {}", mb_rendezvous.push_token),
        )
        .json(&post_req)
        .send()
        .await?;

    println!("Waiting for peer to join thread...");

    let mut resp_msg_opt = None;
    for _ in 0..60 {
        let resp = client
            .get(format!(
                "{server_url}/v1/mailboxes/{}/messages?poll_timeout_secs=5&drain=true",
                mb_rendezvous.id
            ))
            .header(
                "Authorization",
                format!("Bearer {}", mb_rendezvous.pull_token),
            )
            .send()
            .await?;

        if resp.status().is_success() {
            let data: serde_json::Value = resp.json().await.unwrap_or_default();
            if let Some(messages) = data.get("messages").and_then(|m| m.as_array()) {
                if !messages.is_empty() {
                    let payload_b64 = messages[0]["payload"].as_str().ok_or("Invalid payload")?;
                    let bytes = URL_SAFE_NO_PAD.decode(payload_b64)?;
                    if bytes.len() == 49 {
                        resp_msg_opt = Some(bytes);
                        break;
                    }
                }
            }
        }
    }

    let resp_msg = resp_msg_opt.ok_or("Timed out waiting for peer to join thread")?;
    let init_keys = init
        .complete(&resp_msg)
        .map_err(|e| format!("Handshake completion failed: {e}"))?;

    // Cleanup rendezvous mailbox
    let _ = client
        .delete(format!("{server_url}/v1/mailboxes/{}", mb_rendezvous.id))
        .header(
            "Authorization",
            format!("Bearer {}", mb_rendezvous.pull_token),
        )
        .send()
        .await;

    // Register directional mailboxes with PoW
    for mb in [&init_keys.mailbox_out, &init_keys.mailbox_in] {
        let _ = register_mailbox_with_pow(client, server_url, mb, 43200).await;
    }

    Ok(ThreadSession::new(local_handle, init_keys))
}

async fn run_chat_loop(
    client: &reqwest::Client,
    server_url: &str,
    mut session: ThreadSession,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::AsyncBufReadExt;
    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin).lines();

    let client_clone = client.clone();
    let url = server_url.to_string();
    let in_mb_id = session.mailbox_in.id.clone();
    let in_pull_token = session.mailbox_in.pull_token.clone();
    let out_mb_id = session.mailbox_out.id.clone();
    let out_push_token = session.mailbox_out.push_token.clone();
    let out_pull_token = session.mailbox_out.pull_token.clone();

    loop {
        tokio::select! {
            line_res = reader.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        let trimmed = line.trim();
                        if trimmed == "/quit" {
                            println!("Closing thread...");
                            let _ = client_clone
                                .delete(format!("{url}/v1/mailboxes/{out_mb_id}"))
                                .header("Authorization", format!("Bearer {out_pull_token}"))
                                .send()
                                .await;
                            println!("Thread closed.");
                            break;
                        }
                        if trimmed.is_empty() {
                            continue;
                        }

                        let enc = session.encrypt_text(trimmed)?;
                        let ct_bytes = serde_json::to_vec(&enc)?;
                        let post_body = serde_json::json!({
                            "payload": URL_SAFE_NO_PAD.encode(&ct_bytes),
                        });

                        let res = client_clone
                            .post(format!("{url}/v1/mailboxes/{out_mb_id}/messages"))
                            .header("Authorization", format!("Bearer {out_push_token}"))
                            .json(&post_body)
                            .send()
                            .await?;

                        if !res.status().is_success() {
                            eprintln!("Failed to send message: {}", res.status());
                        }
                    }
                    _ => break,
                }
            }
            recv_res = client_clone
                .get(format!("{url}/v1/mailboxes/{in_mb_id}/messages?poll_timeout_secs=2&drain=true"))
                .header("Authorization", format!("Bearer {in_pull_token}"))
                .send() => {
                if let Ok(resp) = recv_res {
                    if resp.status() == reqwest::StatusCode::NOT_FOUND {
                        println!("\nPeer closed the thread.");
                        break;
                    }
                    if resp.status().is_success() {
                        if let Ok(data) = resp.json::<serde_json::Value>().await {
                            if let Some(msgs) = data.get("messages").and_then(|m| m.as_array()) {
                                for m in msgs {
                                    if let Some(payload_str) = m.get("payload").and_then(|p| p.as_str()) {
                                        if let Ok(ct_bytes) = URL_SAFE_NO_PAD.decode(payload_str) {
                                            if let Ok(payload) = serde_json::from_slice(&ct_bytes) {
                                                if let Ok(decrypted) = session.decrypt_message(&payload) {
                                                    println!("<{}>: {}", decrypted.sender_handle, decrypted.text);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
