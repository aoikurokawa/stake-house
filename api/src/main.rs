use anyhow::{Context, Result};
use clap::Parser;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use stake_house_api::{cli::Cli, routes};

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = dotenvy::dotenv() {
        if !e.not_found() {
            eprintln!("warning: failed to load .env: {e}");
        }
    }
    let cli = Cli::parse();

    let opts = PgConnectOptions::new()
        .host(&cli.db_host)
        .port(cli.db_port)
        .username(&cli.db_user)
        .password(&cli.db_password)
        .database(&cli.db_name);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_with(opts)
        .await
        .context("connect to postgres")?;

    let app = routes::router(pool);
    let listener = tokio::net::TcpListener::bind(&cli.listen_addr)
        .await
        .with_context(|| format!("bind {}", cli.listen_addr))?;
    println!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
