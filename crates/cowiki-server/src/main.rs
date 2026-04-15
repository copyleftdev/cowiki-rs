use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

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

// ─── Shared state ────────────────────────────────────────────────────────────

struct Inner {
    wiki: Mutex<WikiBackend>,
    counters: Counters,
}

/// Live performance counters.
struct Counters {
    queries: AtomicU64,
    query_us_total: AtomicU64,
    query_us_min: AtomicU64,
    query_us_max: AtomicU64,
    maintains: AtomicU64,
    maintain_us_total: AtomicU64,
    creates: AtomicU64,
    lock_acquisitions: AtomicU64,
    lock_wait_ns_total: AtomicU64,
}

impl Counters {
    fn new() -> Self {
        Self {
            queries: AtomicU64::new(0),
            query_us_total: AtomicU64::new(0),
            query_us_min: AtomicU64::new(u64::MAX),
            query_us_max: AtomicU64::new(0),
            maintains: AtomicU64::new(0),
            maintain_us_total: AtomicU64::new(0),
            creates: AtomicU64::new(0),
            lock_acquisitions: AtomicU64::new(0),
            lock_wait_ns_total: AtomicU64::new(0),
        }
    }

    fn record_query(&self, us: u64) {
        self.queries.fetch_add(1, Ordering::Relaxed);
        self.query_us_total.fetch_add(us, Ordering::Relaxed);
        self.query_us_min.fetch_min(us, Ordering::Relaxed);
        self.query_us_max.fetch_max(us, Ordering::Relaxed);
    }

    fn record_maintain(&self, us: u64) {
        self.maintains.fetch_add(1, Ordering::Relaxed);
        self.maintain_us_total.fetch_add(us, Ordering::Relaxed);
    }

    fn record_lock(&self, wait_ns: u64) {
        self.lock_acquisitions.fetch_add(1, Ordering::Relaxed);
        self.lock_wait_ns_total.fetch_add(wait_ns, Ordering::Relaxed);
    }
}

type AppState = Arc<Inner>;

/// Acquire the wiki lock and record lock-wait time.
fn acquire_wiki(state: &Inner) -> std::sync::MutexGuard<'_, WikiBackend> {
    let t = Instant::now();
    let guard = state.wiki.lock().unwrap();
    state.counters.record_lock(t.elapsed().as_nanos() as u64);
    guard
}

// ─── Types ───────────────────────────────────────────────────────────────────

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
    links_to: Vec<String>,
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
    elapsed_us: u64,
}

#[derive(Serialize)]
struct QueryHit {
    id: String,
    title: String,
    token_cost: u64,
    links_to: Vec<String>,
}

#[derive(Deserialize)]
struct CreatePageRequest {
    id: String,
    title: String,
    content: String,
}

#[derive(Serialize)]
struct MaintainResponse {
    health: f64,
    pruned_count: usize,
    dreamed_count: usize,
    dreamed_edges: Vec<[String; 2]>,
    elapsed_us: u64,
}

#[derive(Serialize)]
struct StatsResponse {
    page_count: usize,
    edge_count: usize,
    density: f64,
}

#[derive(Serialize)]
struct PerfResponse {
    queries: u64,
    query_avg_us: f64,
    query_min_us: u64,
    query_max_us: u64,
    maintains: u64,
    maintain_avg_us: f64,
    creates: u64,
    lock_acquisitions: u64,
    lock_avg_ns: f64,
}

#[derive(Deserialize)]
struct StressRequest {
    #[serde(default = "default_n")]
    n: usize,
    #[serde(default = "default_query_str")]
    query: String,
}

fn default_n() -> usize { 100 }
fn default_query_str() -> String { "spreading activation".into() }

#[derive(Serialize)]
struct StressResponse {
    n: usize,
    total_us: u64,
    avg_us: f64,
    min_us: u64,
    max_us: u64,
    p50_us: u64,
    p95_us: u64,
    p99_us: u64,
    throughput_qps: f64,
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn list_pages(State(state): State<AppState>) -> Json<Vec<PageSummary>> {
    let wiki = acquire_wiki(&state);
    let pages = wiki.all_pages().iter().map(|p| PageSummary {
        id: p.id.0.clone(),
        title: p.title.clone(),
        token_cost: p.token_cost,
        link_count: p.links_to.len(),
        links_to: p.links_to.iter().map(|l| l.0.clone()).collect(),
    }).collect();
    Json(pages)
}

async fn get_page(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PageDetail>, StatusCode> {
    let wiki = acquire_wiki(&state);
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
    let t = Instant::now();
    let wiki = acquire_wiki(&state);
    let result = wiki.retrieve(&req.query, req.budget, &SpreadConfig::default());
    drop(wiki);
    let elapsed_us = t.elapsed().as_micros() as u64;

    state.counters.record_query(elapsed_us);

    Json(QueryResponse {
        pages: result.pages.iter().map(|p| QueryHit {
            id: p.id.0.clone(),
            title: p.title.clone(),
            token_cost: p.token_cost,
            links_to: p.links_to.iter().map(|l| l.0.clone()).collect(),
        }).collect(),
        total_score: result.total_score,
        total_cost: result.total_cost,
        converged: result.converged,
        iterations: result.iterations,
        elapsed_us,
    })
}

async fn create_page_handler(
    State(state): State<AppState>,
    Json(req): Json<CreatePageRequest>,
) -> Result<StatusCode, StatusCode> {
    let mut wiki = acquire_wiki(&state);
    wiki.create_page(&PageId(req.id), &req.title, &req.content)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    wiki.save().ok();
    state.counters.creates.fetch_add(1, Ordering::Relaxed);
    Ok(StatusCode::CREATED)
}

async fn maintain_handler(State(state): State<AppState>) -> Json<MaintainResponse> {
    let t = Instant::now();
    let mut wiki = acquire_wiki(&state);
    let report = wiki.maintain_with_dream(&RemConfig::default());

    let dreamed_edges: Vec<[String; 2]> = report.dreamed_edges.iter()
        .filter_map(|&(src, dst)| {
            let pages = wiki.all_pages();
            Some([pages.get(src)?.id.0.clone(), pages.get(dst)?.id.0.clone()])
        })
        .collect();

    wiki.save().ok();
    drop(wiki);
    let elapsed_us = t.elapsed().as_micros() as u64;
    state.counters.record_maintain(elapsed_us);

    Json(MaintainResponse {
        health: report.health,
        pruned_count: report.pruned.len(),
        dreamed_count: dreamed_edges.len(),
        dreamed_edges,
        elapsed_us,
    })
}

async fn stats_handler(State(state): State<AppState>) -> Json<StatsResponse> {
    let wiki = acquire_wiki(&state);
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

async fn perf_handler(State(state): State<AppState>) -> Json<PerfResponse> {
    let c = &state.counters;
    let queries = c.queries.load(Ordering::Relaxed);
    let q_total = c.query_us_total.load(Ordering::Relaxed);
    let maintains = c.maintains.load(Ordering::Relaxed);
    let m_total = c.maintain_us_total.load(Ordering::Relaxed);
    let locks = c.lock_acquisitions.load(Ordering::Relaxed);
    let lock_total = c.lock_wait_ns_total.load(Ordering::Relaxed);

    let q_min = c.query_us_min.load(Ordering::Relaxed);

    Json(PerfResponse {
        queries,
        query_avg_us: if queries > 0 { q_total as f64 / queries as f64 } else { 0.0 },
        query_min_us: if q_min == u64::MAX { 0 } else { q_min },
        query_max_us: c.query_us_max.load(Ordering::Relaxed),
        maintains,
        maintain_avg_us: if maintains > 0 { m_total as f64 / maintains as f64 } else { 0.0 },
        creates: c.creates.load(Ordering::Relaxed),
        lock_acquisitions: locks,
        lock_avg_ns: if locks > 0 { lock_total as f64 / locks as f64 } else { 0.0 },
    })
}

async fn stress_handler(
    State(state): State<AppState>,
    Json(req): Json<StressRequest>,
) -> Json<StressResponse> {
    let cfg = SpreadConfig::default();
    let mut latencies: Vec<u64> = Vec::with_capacity(req.n);

    for _ in 0..req.n {
        let t = Instant::now();
        let wiki = acquire_wiki(&state);
        let result = wiki.retrieve(&req.query, 2000, &cfg);
        drop(wiki);
        let us = t.elapsed().as_micros() as u64;
        state.counters.record_query(us);
        latencies.push(us);
        std::hint::black_box(&result);
    }

    latencies.sort();
    let total: u64 = latencies.iter().sum();
    let n = latencies.len();

    Json(StressResponse {
        n,
        total_us: total,
        avg_us: total as f64 / n as f64,
        min_us: latencies[0],
        max_us: latencies[n - 1],
        p50_us: latencies[n / 2],
        p95_us: latencies[n * 95 / 100],
        p99_us: latencies[n * 99 / 100],
        throughput_qps: n as f64 / (total as f64 / 1_000_000.0),
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

    let state: AppState = Arc::new(Inner {
        wiki: Mutex::new(wiki),
        counters: Counters::new(),
    });

    let mut app = Router::new()
        .route("/api/pages", get(list_pages).post(create_page_handler))
        .route("/api/pages/{*id}", get(get_page))
        .route("/api/query", post(query_pages))
        .route("/api/maintain", post(maintain_handler))
        .route("/api/stats", get(stats_handler))
        .route("/api/perf", get(perf_handler))
        .route("/api/stress", post(stress_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Serve static UI files if --ui <path> is provided.
    let ui_dir = std::env::args().nth(2).and_then(|flag| {
        if flag == "--ui" { std::env::args().nth(3) } else { None }
    });
    if let Some(ref dir) = ui_dir {
        eprintln!("Serving UI from: {dir}");
        app = app.fallback_service(
            tower_http::services::ServeDir::new(dir)
                .fallback(tower_http::services::ServeFile::new(
                    PathBuf::from(dir).join("index.html"),
                )),
        );
    }

    let addr = "0.0.0.0:3001";
    if ui_dir.is_some() {
        eprintln!("Co-Wiki ready at http://{addr}");
    } else {
        eprintln!("API ready at http://{addr}");
        eprintln!("  (add --ui <path> to serve the frontend)");
    }

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
