mod admin;
mod auth;
mod bands;
mod cloud;
mod jam;
mod models;
mod projects;
mod social;
mod tracks;

use axum::{extract::Request, middleware, response::Response, Router};
use sqlx::postgres::PgPoolOptions;
use std::env;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

/// Logging middleware — prints method, path, and response status for every request.
async fn log_request(req: Request, next: axum::middleware::Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    println!("[REQ] {method} {uri}");
    let response = next.run(req).await;
    println!("[RES] {method} {uri} -> {}", response.status());
    response
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/jamhub".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await?;

    println!("Connected to database");

    // Run schema migration on startup (idempotent thanks to IF NOT EXISTS)
    let schema = include_str!("../schema.sql");
    // Split on semicolons and run each statement (sqlx doesn't support multi-statement)
    for statement in schema.split(';') {
        let stmt = statement.trim();
        if stmt.is_empty() {
            continue;
        }
        // Ignore index creation errors (they fail if index already exists)
        let _ = sqlx::query(stmt).execute(&pool).await;
    }
    println!("Schema applied");

    // Ensure uploads directory exists
    tokio::fs::create_dir_all("./uploads").await?;

    let api = Router::new()
        .merge(auth::router())
        .merge(tracks::router())
        .merge(projects::router())
        .merge(social::router())
        .merge(cloud::router())
        .merge(bands::router())
        .merge(admin::router())
        .route_layer(middleware::from_fn(auth::jwt_auth));

    let app = Router::new()
        .nest("/api", api)
        .nest("/api", jam::router())
        .nest_service("/uploads", ServeDir::new("./uploads"))
        .nest_service("/downloads", ServeDir::new("./dist"))
        .fallback_service(ServeDir::new("./crates/server/web"))
        .layer(middleware::from_fn(log_request))
        .layer(CorsLayer::permissive())
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("ThroughWaves web server listening on http://0.0.0.0:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
