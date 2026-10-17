use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use clap::{Parser, Subcommand};

use chittr_cartridge_cli::new::{self, NewProject, Sdk};
use chittr_cartridge_cli::{pack, sync, validate, watch};

#[derive(Parser)]
#[command(
    name = "chittr-cartridge",
    version,
    about = "Check, pack, scaffold and watch Chittr Cartridges"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validates a Cartridge directory (or a .cartridge file) with the server's rules. Prints "file: rule" on failure.
    Check {
        /// The Cartridge directory: the one holding manifest.json, server.wasm, templates/ and assets/.
        path: PathBuf,
    },
    /// Re-packs and re-checks a Cartridge directory on every save. Nothing is uploaded.
    Watch {
        /// The Cartridge directory to watch.
        dir: PathBuf,
    },
    /// Creates a Cartridge project in ./<name>: a Server Guest, the card template, and a build script.
    New {
        /// The project's name: the directory, the crate, and the Cartridge's name.
        name: String,
        /// Also create a Canvas Guest (canvas/), and place its <lc-canvas> in the card.
        #[arg(long)]
        canvas: bool,
        /// Use a local checkout of the SDK (its sdk/chittr-cartridge directory) instead of the GitHub repository.
        #[arg(long, value_name = "DIR")]
        sdk_path: Option<PathBuf>,
    },
    /// Copies an `xtp plugin init` scaffold's src/pdk.rs into a project, pruned to the Hooks its src/lib.rs defines.
    Sync {
        /// The scaffold directory `xtp plugin init` wrote.
        scaffold: PathBuf,
        /// The Cartridge's Rust project, the directory holding its Cargo.toml and src/lib.rs.
        project: PathBuf,
    },
    /// Packs a Cartridge directory into a .cartridge file, validating it first.
    Pack {
        /// The Cartridge directory.
        dir: PathBuf,
        /// Where to write the archive. Defaults to <project>/<project>.cartridge, next to the directory.
        out: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    match cli.command {
        Command::Check { path } => {
            let bytes = pack::archive_bytes(&path)?;
            match validate::validate(&bytes) {
                Ok(cartridge) => {
                    println!("{}", summary(&cartridge));
                    for warning in &cartridge.warnings {
                        eprintln!("warning: {warning}");
                    }
                    Ok(ExitCode::SUCCESS)
                }
                Err(error) => {
                    eprintln!("{error}");
                    Ok(ExitCode::from(1))
                }
            }
        }
        Command::Watch { dir } => {
            watch::run(&dir)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::New {
            name,
            canvas,
            sdk_path,
        } => {
            let sdk = sdk_path.map(Sdk::Path).unwrap_or(Sdk::Git);
            let project = NewProject {
                name: name.clone(),
                canvas,
                sdk,
            };
            let root = new::create(&std::env::current_dir()?, &project)?;
            println!("created {}", root.display());
            println!("next: cd {name} && ./build.sh && chittr-cartridge check cartridge");
            Ok(ExitCode::SUCCESS)
        }
        Command::Sync { scaffold, project } => {
            let result = sync::sync(&scaffold, &project)?;
            println!("{}: exports {}", result.kind, result.kept.join(", "));
            let removed = if result.removed.is_empty() {
                "none".to_string()
            } else {
                result.removed.join(", ")
            };
            println!("removed unused exports: {removed}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Pack { dir, out } => {
            let dir = dir
                .canonicalize()
                .with_context(|| format!("{}", dir.display()))?;
            let files = pack::collect(&dir)?;
            let bytes = pack::zip(&files)?;
            match validate::validate(&bytes) {
                Ok(cartridge) => {
                    let out = out.unwrap_or_else(|| pack::default_out(&dir));
                    std::fs::write(&out, &bytes)
                        .with_context(|| format!("writing {}", out.display()))?;
                    println!("wrote {} ({})", out.display(), summary(&cartridge));
                    for warning in &cartridge.warnings {
                        eprintln!("warning: {warning}");
                    }
                    Ok(ExitCode::SUCCESS)
                }
                Err(error) => {
                    eprintln!("{error}");
                    Ok(ExitCode::from(1))
                }
            }
        }
    }
}

fn summary(cartridge: &validate::Cartridge) -> String {
    format!(
        "ok: {} {} (hooks: {}; canvas: {}; templates: {}; assets: {})",
        cartridge.manifest.name,
        cartridge.manifest.version,
        cartridge.hooks.join(", "),
        if cartridge.canvas_wasm.is_some() {
            "yes"
        } else {
            "no"
        },
        cartridge.templates.len(),
        cartridge.assets.len()
    )
}
