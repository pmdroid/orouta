use std::path::PathBuf;
use std::sync::Arc;

fn config_path() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
            eprintln!("--config requires a path");
            std::process::exit(2);
        }
        if let Some(path) = arg.strip_prefix("--config=") {
            return PathBuf::from(path);
        }
    }
    PathBuf::from("orouta.toml")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let path = config_path();
    let config = match orouta::Config::load(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let bind = config.listen_addr();
    let client = reqwest::Client::new();
    let tailscale = Arc::new(orouta::Tailscale::new());
    tailscale.spawn_refresh(&client);
    let app = orouta::app_with_tailscale(Arc::new(config), client, Some(path), tailscale);
    let listener = match tokio::net::TcpListener::bind(&bind).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("bind {bind}: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!("listening on {bind}");
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
