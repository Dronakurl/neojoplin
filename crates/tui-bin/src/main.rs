// NeoJoplin TUI - Terminal User Interface

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Run the TUI application
    neojoplin_tui::run_app().await?;

    Ok(())
}
