use anyhow::{Context, Result};

use irigasi_core::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!(
            "irigasi-core {} ({})",
            env!("CARGO_PKG_VERSION"),
            env!("IRIGASI_GIT_SHA")
        );
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config_path =
        std::env::var("IRIGASI_CONFIG_PATH").unwrap_or_else(|_| "config/default.toml".to_string());

    let config = Config::load(&config_path)
        .with_context(|| format!("gagal load config dari {config_path}"))?;

    irigasi_core::run(config).await
}
