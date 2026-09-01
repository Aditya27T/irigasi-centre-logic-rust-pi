mod routes;
mod state;

pub use state::AppState;

pub async fn serve(state: AppState, bind_addr: &str) -> anyhow::Result<()> {
    let app = routes::build_router(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("Dashboard HTTP aktif di http://{bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
