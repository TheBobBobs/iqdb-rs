use std::sync::Arc;

use axum::{
    routing::{get, post},
    Extension, Router,
};
use clap::Parser;
use iqdb_rs::{ImageData, DB};
use tokio::{
    signal,
    sync::{Mutex, RwLock},
};

mod response;
pub use response::{ApiError, ApiResponse};
mod routes;
pub mod utils;

#[derive(Parser)]
#[clap(disable_help_flag = true)]
struct Args {
    /// The address to bind to
    #[arg(short = 'h', long = "host", default_value = "0.0.0.0")]
    host: String,
    /// The port to listen on
    #[arg(short = 'p', long = "port", default_value_t = 5588)]
    port: u16,
    /// The path to the sqlite db
    #[arg(short = 'd', long = "database", default_value = "iqdb.sqlite")]
    db_path: std::path::PathBuf,

    /// Print help
    #[clap(long, action = clap::ArgAction::HelpLong)]
    help: Option<bool>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let sql_db = sqlite::open(args.db_path).unwrap();
    let db = {
        let create = "
        CREATE TABLE IF NOT EXISTS 'images'
        (
            'id' INTEGER PRIMARY KEY NOT NULL ,
            'avglf1' REAL NOT NULL , 'avglf2' REAL NOT NULL , 'avglf3' REAL NOT NULL ,
            'sig' BLOB NOT NULL
        )";
        sql_db.execute(create).unwrap();
        let query = "SELECT * FROM images";
        let parsed = sql_db.prepare(query).unwrap().into_iter().map(|row| {
            let values: Vec<sqlite::Value> = row.unwrap().into();
            ImageData::try_from(values).unwrap()
        });
        DB::new(parsed)
    };
    let db = Arc::new(RwLock::new(db));
    let sql_db = Arc::new(Mutex::new(sql_db));

    let app = Router::new()
        .route("/query", get(routes::query::get).post(routes::query::get))
        .route(
            "/images/:id",
            post(routes::images::post).delete(routes::images::delete),
        )
        .route("/status", get(routes::status::get))
        .layer(Extension(db))
        .layer(Extension(sql_db));
    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
