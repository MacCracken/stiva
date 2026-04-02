//! Stiva CLI — OCI container runtime for AGNOS.

use clap::{Parser, Subcommand};
use stiva::{Stiva, StivaConfig, StivaError};

#[derive(Parser)]
#[command(name = "stiva", about = "OCI container runtime for AGNOS")]
#[command(version)]
struct Cli {
    /// Root directory for container data.
    #[arg(long, default_value = "/var/lib/agnos/containers")]
    root: std::path::PathBuf,

    /// Image storage directory.
    #[arg(long, default_value = "/var/lib/agnos/images")]
    images: std::path::PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Pull an image from a registry.
    Pull {
        /// Image reference (e.g., nginx:latest).
        image: String,
    },
    /// Build an image from a Stivafile.
    Build {
        /// Path to Stivafile.
        #[arg(short, long, default_value = "Stivafile")]
        file: std::path::PathBuf,
        /// Build context directory.
        #[arg(short, long, default_value = ".")]
        context: std::path::PathBuf,
    },
    /// Push a local image to a registry.
    Push {
        /// Image ID or reference.
        image: String,
        /// Target registry reference (optional).
        target: Option<String>,
    },
    /// Run a container from an image.
    Run {
        /// Image reference.
        image: String,
        /// Container name.
        #[arg(long)]
        name: Option<String>,
        /// Run as daemon (detach).
        #[arg(short, long)]
        detach: bool,
        /// Port mapping (host:container).
        #[arg(short, long)]
        port: Vec<String>,
        /// Environment variable (KEY=VALUE).
        #[arg(short, long)]
        env: Vec<String>,
        /// Inject a secret (KEY=VALUE, delivered as env var).
        #[arg(short, long)]
        secret: Vec<String>,
        /// Command to run.
        #[arg(trailing_var_arg = true)]
        command: Vec<String>,
    },
    /// List containers.
    Ps,
    /// Stop a container.
    Stop {
        /// Container ID or name.
        id: String,
    },
    /// Remove a container.
    Rm {
        /// Container ID or name.
        id: String,
    },
    /// Execute a command in a running container.
    Exec {
        /// Container ID.
        id: String,
        /// Command and arguments.
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
    },
    /// List processes in a running container.
    Top {
        /// Container ID.
        id: String,
    },
    /// Inspect a container or image.
    Inspect {
        /// Container or image ID.
        id: String,
    },
    /// List local images.
    Images,
    /// Remove a local image.
    Rmi {
        /// Image ID or reference.
        image: String,
    },
    /// Tag a local image.
    Tag {
        /// Source image ID or reference.
        source: String,
        /// New reference (e.g., myregistry/myapp:v2).
        target: String,
    },
    /// Pause a running container.
    Pause {
        /// Container ID.
        id: String,
    },
    /// Unpause a paused container.
    Unpause {
        /// Container ID.
        id: String,
    },
    /// Show container stats.
    Stats {
        /// Container ID.
        id: String,
    },
    /// Show container logs.
    Logs {
        /// Container ID.
        id: String,
        /// Number of lines to show (from end).
        #[arg(short = 'n', long, default_value = "50")]
        tail: usize,
    },
    /// Send a signal to a container.
    Kill {
        /// Container ID.
        id: String,
        /// Signal number (default: 15 / SIGTERM).
        #[arg(short, long, default_value = "15")]
        signal: i32,
    },
    /// Export container rootfs as tar.
    Export {
        /// Container ID.
        id: String,
        /// Output tar path.
        #[arg(short, long)]
        output: std::path::PathBuf,
    },
    /// Import a tar as a local image.
    Import {
        /// Input tar path.
        file: std::path::PathBuf,
        /// Image name.
        #[arg(long)]
        name: String,
        /// Image tag.
        #[arg(long, default_value = "latest")]
        tag: String,
    },
    /// Copy files in/out of a container.
    Cp {
        /// Source (host path or container:path).
        source: String,
        /// Destination (host path or container:path).
        dest: String,
    },
    /// Remove stopped containers and unused images.
    Prune,
    /// Wait for a container to exit.
    Wait {
        /// Container ID.
        id: String,
    },
    /// Checkpoint a running container (CRIU).
    Checkpoint {
        /// Container ID.
        id: String,
        /// Leave container running after checkpoint.
        #[arg(long)]
        leave_running: bool,
    },
    /// Restore a container from checkpoint.
    Restore {
        /// Container ID.
        id: String,
        /// Checkpoint directory.
        checkpoint_dir: std::path::PathBuf,
    },
    /// Restart a stopped container.
    Restart {
        /// Container ID.
        id: String,
    },
    /// Show system information.
    Info,
    /// Stream container lifecycle events.
    Events,
    /// Show filesystem changes in a container vs its image.
    Diff {
        /// Container ID.
        id: String,
    },
    /// Generate shell completions.
    Completions {
        /// Shell type (bash, zsh, fish).
        shell: String,
    },
    /// Rename a container.
    Rename {
        /// Container ID.
        id: String,
        /// New name.
        name: String,
    },
    /// Garbage-collect unreferenced image blobs.
    Gc,
    /// Convert a YAML file (docker-compose, Dockerfile) to TOML equivalent.
    Convert {
        /// Input YAML file path.
        input: std::path::PathBuf,
        /// Output TOML file path (default: stdout).
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
        /// Input format.
        #[arg(short, long, default_value = "compose")]
        format: String,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    // Load config file defaults if present.
    if let Some(file_config) = load_config_file() {
        // Config file sets defaults; CLI args override.
        tracing::debug!("loaded config from ~/.stiva/config.toml");
        let _ = file_config; // Used below in run() via CLI defaults.
    }

    if let Err(e) = run(cli).await {
        // User-friendly error formatting.
        match &e {
            StivaError::ContainerNotFound(id) => eprintln!("Error: no such container: {id}"),
            StivaError::ImageNotFound(id) => eprintln!("Error: no such image: {id}"),
            StivaError::AlreadyRunning(id) => eprintln!("Error: container {id} is already running"),
            StivaError::InvalidState(msg) => eprintln!("Error: {msg}"),
            StivaError::InvalidReference(msg) => eprintln!("Error: invalid image reference: {msg}"),
            StivaError::AuthFailed(msg) => eprintln!("Error: authentication failed: {msg}"),
            _ => eprintln!("Error: {e}"),
        }
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), StivaError> {
    let cli_config = StivaConfig {
        root_path: cli.root,
        image_path: cli.images,
        ..Default::default()
    };
    let stiva = Stiva::new(cli_config.clone()).await?;

    match cli.command {
        Commands::Pull { image } => {
            let img = stiva.pull(&image).await?;
            println!("{}", img.id);
        }
        Commands::Build { file, context } => {
            let content = std::fs::read_to_string(&file).map_err(StivaError::Io)?;
            let img = stiva.build(&content, &context).await?;
            println!("{}", img.id);
        }
        Commands::Push { image, target } => {
            stiva.push(&image, target.as_deref()).await?;
            println!("pushed");
        }
        Commands::Run {
            image,
            name,
            detach,
            port,
            env,
            secret,
            command,
        } => {
            let env_map: std::collections::HashMap<String, String> = env
                .iter()
                .filter_map(|e| {
                    e.split_once('=')
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                })
                .collect();
            let secrets: Vec<kavach::SecretRef> = secret
                .iter()
                .filter_map(|s| {
                    s.split_once('=').map(|(k, _v)| kavach::SecretRef {
                        name: k.to_string(),
                        inject_via: kavach::credential::InjectionMethod::EnvVar {
                            var_name: k.to_string(),
                        },
                    })
                })
                .collect();
            let cfg = stiva::container::ContainerConfig {
                name,
                command,
                env: env_map,
                ports: port,
                detach,
                secrets,
                ..Default::default()
            };
            let c = stiva.run(&image, cfg).await?;
            println!("{}", c.id);
        }
        Commands::Ps => {
            let containers = stiva.ps().await?;
            println!(
                "{:<14} {:<15} {:<10} {:<30}",
                "CONTAINER ID", "NAME", "STATE", "IMAGE"
            );
            for c in containers {
                println!(
                    "{:<14} {:<15} {:<10} {:<30}",
                    &c.id[..12],
                    c.name.as_deref().unwrap_or("-"),
                    format!("{:?}", c.state),
                    c.image_ref,
                );
            }
        }
        Commands::Stop { id } => {
            stiva.stop(&id).await?;
            println!("{id}");
        }
        Commands::Rm { id } => {
            stiva.rm(&id).await?;
            println!("{id}");
        }
        Commands::Exec { id, command } => {
            let result = stiva.exec(&id, &command).await?;
            print!("{}", result.stdout);
            if !result.stderr.is_empty() {
                eprint!("{}", result.stderr);
            }
            std::process::exit(result.exit_code);
        }
        Commands::Top { id } => {
            let procs = stiva.top(&id).await?;
            println!(
                "{:<8} {:<8} {:<5} {:<16} CMDLINE",
                "PID", "PPID", "STATE", "COMMAND"
            );
            for p in procs {
                println!(
                    "{:<8} {:<8} {:<5} {:<16} {}",
                    p.pid, p.ppid, p.state, p.comm, p.cmdline
                );
            }
        }
        Commands::Inspect { id } => {
            // Try container first, then image.
            if let Ok(c) = stiva.inspect(&id).await {
                let score = stiva.container_security_score(&id).await.ok();
                let mut json = serde_json::to_value(&c)?;
                if let Some(s) = score {
                    json["security_score"] = serde_json::json!({
                        "value": s.value(),
                        "label": s.label(),
                    });
                }
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else if let Ok(img) = stiva.inspect_image(&id) {
                println!("{}", serde_json::to_string_pretty(&img)?);
            } else {
                return Err(StivaError::ContainerNotFound(id));
            }
        }
        Commands::Images => {
            let images = stiva.images().await?;
            println!(
                "{:<20} {:<10} {:<72} {:<10}",
                "REPOSITORY", "TAG", "IMAGE ID", "SIZE"
            );
            for img in images {
                println!(
                    "{:<20} {:<10} {:<72} {:<10}",
                    img.reference.repository,
                    img.reference.tag,
                    img.id,
                    format_size(img.size_bytes),
                );
            }
        }
        Commands::Rmi { image } => {
            stiva.rmi(&image)?;
            println!("{image}");
        }
        Commands::Tag { source, target } => {
            stiva.tag(&source, &target)?;
            println!("{target}");
        }
        Commands::Pause { id } => {
            stiva.pause(&id).await?;
            println!("{id}");
        }
        Commands::Unpause { id } => {
            stiva.unpause(&id).await?;
            println!("{id}");
        }
        Commands::Stats { id } => {
            let s = stiva.stats(&id).await?;
            println!(
                "memory:   {}/{}",
                format_size(s.memory_bytes),
                format_size(s.memory_limit_bytes)
            );
            println!("cpu:      {}us", s.cpu_usage_us);
            println!(
                "pids:     {}/{}",
                s.pids_current,
                if s.pids_limit == 0 {
                    "unlimited".to_string()
                } else {
                    s.pids_limit.to_string()
                }
            );
        }
        Commands::Logs { id, tail } => {
            let logs = stiva.log_tail(&id, tail).await?;
            print!("{logs}");
        }
        Commands::Kill { id, signal } => {
            stiva.signal(&id, signal).await?;
            println!("{id}");
        }
        Commands::Export { id, output } => {
            stiva.export(&id, &output).await?;
            println!("{}", output.display());
        }
        Commands::Import { file, name, tag } => {
            let img = stiva.import(&file, &name, &tag)?;
            println!("{}", img.id);
        }
        Commands::Cp { source, dest } => {
            // Format: container_id:/path or /host/path
            if let Some((id, container_path)) = source.split_once(':') {
                // Copy from container to host.
                stiva
                    .cp_from(
                        id,
                        std::path::Path::new(container_path),
                        std::path::Path::new(&dest),
                    )
                    .await?;
            } else if let Some((id, container_path)) = dest.split_once(':') {
                // Copy from host to container.
                stiva
                    .cp_into(
                        id,
                        std::path::Path::new(&source),
                        std::path::Path::new(container_path),
                    )
                    .await?;
            } else {
                return Err(StivaError::InvalidState(
                    "cp requires container:path format for source or destination".into(),
                ));
            }
            println!("ok");
        }
        Commands::Prune => {
            let (containers, images) = stiva.prune().await?;
            println!("removed {containers} containers, {images} images");
        }
        Commands::Wait { id } => {
            let result = stiva.wait(&id).await?;
            println!("exit_code: {}", result.exit_code);
            std::process::exit(result.exit_code);
        }
        Commands::Checkpoint { id, leave_running } => {
            let dir = stiva.checkpoint(&id, leave_running).await?;
            println!("{}", dir.display());
        }
        Commands::Restore { id, checkpoint_dir } => {
            stiva.restore(&id, &checkpoint_dir).await?;
            println!("{id}");
        }
        Commands::Restart { id } => {
            stiva.restart(&id).await?;
            println!("{id}");
        }
        Commands::Convert {
            input,
            output,
            format,
        } => {
            let content = std::fs::read_to_string(&input).map_err(StivaError::Io)?;
            let toml_output = match format.as_str() {
                "compose" => stiva::convert::compose_yaml_to_toml(&content)?,
                "dockerfile" => stiva::convert::dockerfile_to_toml(&content)?,
                other => {
                    return Err(StivaError::InvalidState(format!(
                        "unknown format '{other}', expected 'compose' or 'dockerfile'"
                    )));
                }
            };
            if let Some(out_path) = output {
                std::fs::write(&out_path, &toml_output).map_err(StivaError::Io)?;
                println!("{}", out_path.display());
            } else {
                print!("{toml_output}");
            }
        }
        Commands::Events => {
            println!("Streaming lifecycle events (Ctrl+C to stop)...");
            let bus = stiva.event_bus();
            let mut rx = bus.subscribe("container.lifecycle");
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        let ts = chrono::Utc::now().format("%H:%M:%S%.3f");
                        println!("[{ts}] {}", msg.payload);
                    }
                    Err(e) => {
                        eprintln!("event stream error: {e}");
                        break;
                    }
                }
            }
        }
        Commands::Diff { id } => {
            let rootfs = stiva.get_rootfs(&id).await?;
            let upper = rootfs.parent().map(|p| p.join("upper")).unwrap_or_default();
            if !upper.exists() {
                println!("(no changes)");
            } else {
                // Walk the upper layer to find changed files.
                fn walk_diff(dir: &std::path::Path, prefix: &str) -> Result<(), std::io::Error> {
                    for entry in std::fs::read_dir(dir)? {
                        let entry = entry?;
                        let name = entry.file_name();
                        let name_str = name.to_string_lossy();
                        let path = if prefix.is_empty() {
                            format!("/{name_str}")
                        } else {
                            format!("{prefix}/{name_str}")
                        };
                        let ft = entry.file_type()?;
                        if ft.is_dir() {
                            walk_diff(&entry.path(), &path)?;
                        } else {
                            // Whiteout files indicate deletion.
                            if name_str.starts_with(".wh.") {
                                let deleted = name_str.strip_prefix(".wh.").unwrap_or(&name_str);
                                println!("D  {}/{}", prefix, deleted);
                            } else {
                                println!("C  {path}");
                            }
                        }
                    }
                    Ok(())
                }
                walk_diff(&upper, "")
                    .map_err(|e| StivaError::Storage(format!("diff walk failed: {e}")))?;
            }
        }
        Commands::Completions { shell } => {
            use clap::CommandFactory;
            let mut cmd = Cli::command();
            let shell = match shell.as_str() {
                "bash" => clap_complete::Shell::Bash,
                "zsh" => clap_complete::Shell::Zsh,
                "fish" => clap_complete::Shell::Fish,
                other => {
                    return Err(StivaError::InvalidState(format!(
                        "unknown shell '{other}', expected bash/zsh/fish"
                    )));
                }
            };
            clap_complete::generate(shell, &mut cmd, "stiva", &mut std::io::stdout());
        }
        Commands::Rename { id, name } => {
            stiva.rename(&id, &name).await?;
            println!("{id}");
        }
        Commands::Gc => {
            let (blobs, layers) = stiva.gc()?;
            println!("removed {blobs} blobs, {layers} layers");
        }
        Commands::Info => {
            println!("stiva {}", env!("CARGO_PKG_VERSION"));
            println!("root:     {}", cli_config.root_path.display());
            println!("images:   {}", cli_config.image_path.display());
            let containers = stiva.ps().await.map(|c| c.len()).unwrap_or(0);
            let images = stiva.images().await.map(|i| i.len()).unwrap_or(0);
            println!("containers: {containers}");
            println!("images:     {images}");
            println!(
                "criu:     {}",
                if stiva::runtime::criu_available() {
                    "yes"
                } else {
                    "no"
                }
            );
            let score = stiva.security_score();
            println!("security: {score}");
        }
    }

    Ok(())
}

/// User configuration file (`~/.stiva/config.toml`).
#[derive(Debug, serde::Deserialize)]
struct FileConfig {
    /// Default registry (e.g., "ghcr.io").
    #[allow(dead_code)]
    default_registry: Option<String>,
    /// Root path for container data.
    #[allow(dead_code)]
    root_path: Option<String>,
    /// Image storage path.
    #[allow(dead_code)]
    image_path: Option<String>,
    /// Log level (e.g., "info", "debug", "warn").
    #[allow(dead_code)]
    log_level: Option<String>,
}

/// Load config file from `~/.stiva/config.toml` if it exists.
fn load_config_file() -> Option<FileConfig> {
    let home = std::env::var("HOME").ok()?;
    let config_path = std::path::PathBuf::from(home)
        .join(".stiva")
        .join("config.toml");
    if !config_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&config_path).ok()?;
    toml::from_str(&content).ok()
}

/// Format byte count as human-readable size.
fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < units.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{size:.0}{}", units[unit_idx])
    } else {
        format!("{size:.1}{}", units[unit_idx])
    }
}
