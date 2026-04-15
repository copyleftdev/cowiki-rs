use std::path::PathBuf;
use std::sync::Mutex;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use spread::SpreadConfig;
use temporal_graph::RemConfig;
use wiki_backend::types::PageId;
use wiki_backend::WikiBackend;

type AppState = std::sync::Arc<Mutex<WikiBackend>>;

// ─── Request / Response types ────────────────────────────────────────────────

#[derive(Deserialize)]
struct QueryRequest {
    query: String,
    #[serde(default = "default_budget")]
    budget: u64,
}

fn default_budget() -> u64 { 4000 }

#[derive(Serialize)]
struct PageSummary {
    id: String,
    title: String,
    token_cost: u64,
    link_count: usize,
}

#[derive(Serialize)]
struct PageDetail {
    id: String,
    title: String,
    content: String,
    links_to: Vec<String>,
    token_cost: u64,
}

#[derive(Serialize)]
struct QueryResponse {
    pages: Vec<QueryHit>,
    total_score: f64,
    total_cost: u64,
    converged: bool,
    iterations: usize,
}

#[derive(Serialize)]
struct QueryHit {
    id: String,
    title: String,
    token_cost: u64,
}

#[derive(Deserialize)]
struct CreatePageRequest {
    id: String,
    title: String,
    content: String,
}

#[derive(Deserialize)]
struct UpdatePageRequest {
    content: String,
}

#[derive(Serialize)]
struct MaintainResponse {
    health: f64,
    pruned_count: usize,
    dreamed_count: usize,
    dreamed_edges: Vec<[String; 2]>,
}

#[derive(Serialize)]
struct StatsResponse {
    page_count: usize,
    edge_count: usize,
    density: f64,
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn list_pages(State(state): State<AppState>) -> Json<Vec<PageSummary>> {
    let wiki = state.lock().unwrap();
    let pages: Vec<PageSummary> = wiki.all_pages().iter().map(|p| PageSummary {
        id: p.id.0.clone(),
        title: p.title.clone(),
        token_cost: p.token_cost,
        link_count: p.links_to.len(),
    }).collect();
    Json(pages)
}

async fn get_page(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PageDetail>, StatusCode> {
    let wiki = state.lock().unwrap();
    let meta = wiki.page(&PageId(id)).ok_or(StatusCode::NOT_FOUND)?;
    let content = std::fs::read_to_string(&meta.path).unwrap_or_default();

    Ok(Json(PageDetail {
        id: meta.id.0.clone(),
        title: meta.title.clone(),
        content,
        links_to: meta.links_to.iter().map(|l| l.0.clone()).collect(),
        token_cost: meta.token_cost,
    }))
}

async fn query_pages(
    State(state): State<AppState>,
    Json(req): Json<QueryRequest>,
) -> Json<QueryResponse> {
    let wiki = state.lock().unwrap();
    let result = wiki.retrieve(&req.query, req.budget, &SpreadConfig::default());

    Json(QueryResponse {
        pages: result.pages.iter().map(|p| QueryHit {
            id: p.id.0.clone(),
            title: p.title.clone(),
            token_cost: p.token_cost,
        }).collect(),
        total_score: result.total_score,
        total_cost: result.total_cost,
        converged: result.converged,
        iterations: result.iterations,
    })
}

async fn create_page_handler(
    State(state): State<AppState>,
    Json(req): Json<CreatePageRequest>,
) -> Result<StatusCode, StatusCode> {
    let mut wiki = state.lock().unwrap();
    wiki.create_page(&PageId(req.id), &req.title, &req.content)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    wiki.save().ok();
    Ok(StatusCode::CREATED)
}

async fn update_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePageRequest>,
) -> Result<StatusCode, StatusCode> {
    let mut wiki = state.lock().unwrap();
    wiki.update_page(&PageId(id), &req.content)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    wiki.save().ok();
    Ok(StatusCode::OK)
}

async fn maintain_handler(State(state): State<AppState>) -> Json<MaintainResponse> {
    let mut wiki = state.lock().unwrap();
    let report = wiki.maintain_with_dream(&RemConfig::default());

    // Resolve dreamed edge indices to page IDs.
    let dreamed_edges: Vec<[String; 2]> = report.dreamed_edges.iter()
        .filter_map(|&(src, dst)| {
            let pages = wiki.all_pages();
            let s = pages.get(src)?.id.0.clone();
            let d = pages.get(dst)?.id.0.clone();
            Some([s, d])
        })
        .collect();

    wiki.save().ok();

    Json(MaintainResponse {
        health: report.health,
        pruned_count: report.pruned.len(),
        dreamed_count: dreamed_edges.len(),
        dreamed_edges,
    })
}

async fn stats_handler(State(state): State<AppState>) -> Json<StatsResponse> {
    let wiki = state.lock().unwrap();
    let g = wiki.graph();
    let n = g.len();

    let edge_count = (0..n)
        .flat_map(|i| (0..n).map(move |j| (i, j)))
        .filter(|&(i, j)| g.raw_weight(i, j) > 0.0)
        .count();

    let max_edges = if n > 1 { n * (n - 1) } else { 1 };

    Json(StatsResponse {
        page_count: n,
        edge_count,
        density: edge_count as f64 / max_edges as f64,
    })
}

// ─── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let wiki_root = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: cowiki-server <wiki-directory>");
        std::process::exit(1);
    });

    let root = PathBuf::from(&wiki_root);
    if !root.exists() {
        eprintln!("Directory does not exist: {wiki_root}");
        std::process::exit(1);
    }

    eprintln!("Opening wiki at: {wiki_root}");
    let wiki = WikiBackend::open_or_rebuild(&root).unwrap_or_else(|e| {
        eprintln!("Failed to open wiki: {e}");
        std::process::exit(1);
    });
    eprintln!("Indexed {} pages", wiki.len());

    let state: AppState = std::sync::Arc::new(Mutex::new(wiki));

    let app = Router::new()
        .route("/api/pages", get(list_pages).post(create_page_handler))
        .route("/api/pages/{*id}", get(get_page).put(update_page_handler))
        .route("/api/query", post(query_pages))
        .route("/api/maintain", post(maintain_handler))
        .route("/api/stats", get(stats_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "0.0.0.0:3001";
    eprintln!("API ready at http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
