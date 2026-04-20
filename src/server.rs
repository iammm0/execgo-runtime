use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};

use crate::{
    error::AppError,
    runtime::RuntimeService,
    types::{HealthResponse, SubmitTaskRequest},
};

pub fn build_router(service: RuntimeService) -> Router {
    Router::new()
        .route("/api/v1/tasks", post(create_task))
        .route("/api/v1/tasks/:id", get(get_task))
        .route("/api/v1/tasks/:id/kill", post(kill_task))
        .route("/api/v1/tasks/:id/events", get(get_events))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .with_state(service)
}

async fn create_task(
    State(service): State<RuntimeService>,
    Json(payload): Json<SubmitTaskRequest>,
) -> Result<impl IntoResponse, AppError> {
    let response = service.submit_task(payload).await?;
    Ok(Json(response))
}

async fn get_task(
    State(service): State<RuntimeService>,
    Path(task_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let response = service.get_task_status(&task_id).await?;
    Ok(Json(response))
}

async fn kill_task(
    State(service): State<RuntimeService>,
    Path(task_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let response = service.kill_task(&task_id).await?;
    Ok(Json(response))
}

async fn get_events(
    State(service): State<RuntimeService>,
    Path(task_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let response = service.get_events(&task_id).await?;
    Ok(Json(response))
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn readyz(State(service): State<RuntimeService>) -> Result<impl IntoResponse, AppError> {
    service.ready().await?;
    Ok(Json(HealthResponse {
        status: "ready",
        version: env!("CARGO_PKG_VERSION"),
    }))
}

async fn metrics(State(service): State<RuntimeService>) -> Result<impl IntoResponse, AppError> {
    Ok(service.metrics().await)
}
