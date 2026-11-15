//! Ember CLI: command-line client for encrypting and decrypting drops locally.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use ember_core::{
    build_link, decrypt_drop, encrypt_drop, generate_blob_id, parse_link, unwrap_key, wrap_key,
    DropMetadata, LinkPayload, SecretKey,
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
    /// Encrypt a file into an Ember ciphertext drop
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
    },

    /// Decrypt an Ember ciphertext drop
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
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Encrypt {
            file,
            output,
            passphrase,
            burn,
            host,
        } => {
            if !file.exists() {
                eprintln!("Error: file not found: {}", file.display());
                std::process::exit(1);
            }

            let content = fs::read(&file)?;
            let file_name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("drop.bin");

            let created = now_secs();
            let expires = created + 43200; // 12 hours max TTL
            let metadata = DropMetadata::new_file(
                file_name,
                "application/octet-stream",
                content.len() as u64,
                burn,
                created,
                expires,
            );

            let master_key = SecretKey::generate();
            let ciphertext = encrypt_drop(&master_key, &metadata, &content, None)?;

            let out_path = output.unwrap_or_else(|| {
                let mut p = file.clone();
                p.set_extension("ember");
                p
            });

            fs::write(&out_path, &ciphertext)?;

            let payload = if let Some(pass) = passphrase {
                let wrapped = wrap_key(&master_key, &pass, None, None, None)?;
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
            // Resolve ciphertext path and link string from arguments
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
                    // Could be raw base64url payload
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
    }

    Ok(())
}
