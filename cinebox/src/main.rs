use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("cinebox=info")),
        )
        .init();

    cinebox::run().map_err(|error| anyhow::anyhow!("running eframe application: {error}"))
}
