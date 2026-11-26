//! CLI for sigmaos-site.

use clap::{Parser, Subcommand};
use sigmaos_site::{build, default_config, load_config, save_config, SiteConfig};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sigmaos-site", about = "SigmaOS static site builder")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build a site from content dir to output dir.
    Build {
        content: PathBuf,
        output: PathBuf,
        /// Optional JSON config file.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print default config as JSON.
    DefaultConfig,
    /// Save default config to a file.
    SaveConfig { file: PathBuf },
    /// Preview rendered markdown for a file.
    Preview { file: PathBuf },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    match Cli::parse().command {
        Commands::Build {
            content,
            output,
            config,
        } => {
            let cfg: SiteConfig = match config {
                Some(p) => load_config(&p)?,
                None => default_config(),
            };
            let result = build(&content, &output, &cfg)?;
            println!(
                "built {} pages, {} assets -> {}",
                result.pages.len(),
                result.assets_copied,
                output.display()
            );
        }
        Commands::DefaultConfig => {
            println!("{}", serde_json::to_string_pretty(&default_config())?);
        }
        Commands::SaveConfig { file } => {
            save_config(&default_config(), &file)?;
            println!("saved: {}", file.display());
        }
        Commands::Preview { file } => {
            let md = std::fs::read_to_string(file)?;
            print!("{}", sigmaos_site::markdown_to_html(&md));
        }
    }
    Ok(())
}
