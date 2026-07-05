use clap::Parser;

#[derive(Parser)]
#[command(about = "HTTP API over LST holder snapshots")]
pub struct Cli {
    /// Address to listen on
    #[arg(long, env = "API_LISTEN_ADDR", default_value = "127.0.0.1:3000")]
    pub listen_addr: String,

    /// Postgres host
    #[arg(long, env = "DB_HOST")]
    pub db_host: String,

    /// Postgres port
    #[arg(long, env = "DB_PORT")]
    pub db_port: u16,

    /// Postgres user
    #[arg(long, env = "DB_USER")]
    pub db_user: String,

    /// Postgres password
    #[arg(long, env = "DB_PASSWORD")]
    pub db_password: String,

    /// Postgres database name
    #[arg(long, env = "DB_NAME")]
    pub db_name: String,
}
