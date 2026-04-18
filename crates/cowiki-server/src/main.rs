mod simulate;
mod ssr;

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use axum::extract::{Path, Query as AxumQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use spread::SpreadConfig;
use temporal_graph::RemConfig;
use wiki_backend::types::PageId;
use wiki_backend::WikiBackend;

// ─── Shared state ────────────────────────────────────────────────────────────
//
// The server can host multiple corpora simultaneously. One is marked active;
// all operational endpoints (query, pages, neighborhood, maintain, stress,
// simulate) route to it. The UI switches corpus via POST /api/corpora/select
// which is effectively a global toggle — fine for a single-user workstation
// deployment; a multi-tenant variant would push corpus into the request path.

struct Inner {
    corpora: BTreeMap<String, RwLock<WikiBackend>>,
    active: RwLock<String>,
    counters: Counters,
    /// In read-only mode all mutating endpoints (create_page, maintain,
    /// corpus-select) return 403. Enabled via `--read-only` when the
    /// server is deployed publicly without an edge (Caddy/CF) to enforce
    /// the same guarantee. Read endpoints (query, get_page, neighborhood,
    /// stats, perf, stress) are unaffected.
    read_only: bool,
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

/// Resolve the currently-active corpus to its RwLock. The BTreeMap only grows
/// (corpora are registered at startup; `select_corpus` cannot remove entries),
/// so a cloned name is always findable.
fn active_lock(state: &Inner) -> &RwLock<WikiBackend> {
    let active = state.active.read().clone();
    state
        .corpora
        .get(&active)
        .expect("active corpus must be registered")
}

/// Shared-read guard on the active corpus. Use from endpoints that only read
/// the graph / metadata / TF-IDF (query, list, stats, neighborhood, get_page).
fn acquire_wiki(state: &Inner) -> RwLockReadGuard<'_, WikiBackend> {
    let lock = active_lock(state);
    let t = Instant::now();
    let guard = lock.read();
    state.counters.record_lock(t.elapsed().as_nanos() as u64);
    guard
}

/// Exclusive-write guard on the active corpus. Use from endpoints that mutate
/// the wiki (create_page, maintain).
fn acquire_wiki_mut(state: &Inner) -> RwLockWriteGuard<'_, WikiBackend> {
    let lock = active_lock(state);
    let t = Instant::now();
    let guard = lock.write();
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
struct CorpusSummary {
    name: String,
    page_count: usize,
    edge_count: usize,
    density: f64,
    active: bool,
}

#[derive(Deserialize)]
struct SelectCorpusRequest {
    name: String,
}

#[derive(Serialize)]
struct NeighborNode {
    id: String,
    title: String,
    token_cost: u64,
    hops: u32,
    direction: &'static str,
}

#[derive(Serialize)]
struct NeighborEdge {
    from: String,
    to: String,
    weight: f64,
}

#[derive(Serialize)]
struct NeighborhoodResponse {
    center: String,
    nodes: Vec<NeighborNode>,
    edges: Vec<NeighborEdge>,
    truncated: bool,
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

#[derive(Deserialize, Default)]
#[serde(default)]
struct ListPagesParams {
    /// Cap the number of entries returned. Omit for no limit.
    /// The UI passes a cap because at 100k+ pages the full list is
    /// unrenderable in a browser; small corpora get the whole list as before.
    limit: Option<usize>,
    /// `id` (default, alphabetical) or `hubs` (outbound-link count desc).
    order: Option<String>,
}

async fn list_pages(
    State(state): State<AppState>,
    AxumQuery(params): AxumQuery<ListPagesParams>,
) -> Json<Vec<PageSummary>> {
    let wiki = acquire_wiki(&state);
    let all = wiki.all_pages();

    // If the client asked for hub-ordering, use a partial heap-style selection
    // instead of sorting all N — O(N log k) rather than O(N log N). At N=500k,
    // k=8 this is ~500k comparisons vs ~10M.
    let want_hubs = params.order.as_deref() == Some("hubs");
    let mut selected: Vec<&wiki_backend::types::PageMeta> = if want_hubs {
        let k = params.limit.unwrap_or(all.len()).min(all.len());
        let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<(usize, usize)>> =
            std::collections::BinaryHeap::with_capacity(k + 1);
        for (i, p) in all.iter().enumerate() {
            heap.push(std::cmp::Reverse((p.links_to.len(), i)));
            if heap.len() > k { heap.pop(); }
        }
        let mut out: Vec<(usize, usize)> = heap
            .into_iter()
            .map(|std::cmp::Reverse(pair)| pair)
            .collect();
        out.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        out.into_iter().map(|(_, i)| &all[i]).collect()
    } else {
        let take = params.limit.unwrap_or(all.len());
        all.iter().take(take).collect()
    };

    let pages = selected
        .drain(..)
        .map(|p| PageSummary {
            id: p.id.0.clone(),
            title: p.title.clone(),
            token_cost: p.token_cost,
            link_count: p.links_to.len(),
            links_to: p.links_to.iter().map(|l| l.0.clone()).collect(),
        })
        .collect();
    Json(pages)
}

async fn get_page(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PageDetail>, StatusCode> {
    // Take a snapshot of the metadata we need, then drop the lock before
    // touching the disk. Otherwise every reader queues behind file I/O.
    let (full_path, detail_template) = {
        let wiki = acquire_wiki(&state);
        let meta = wiki.page(&PageId(id)).ok_or(StatusCode::NOT_FOUND)?;
        let full = wiki.root().join(&meta.path);
        let tmpl = PageDetail {
            id: meta.id.0.clone(),
            title: meta.title.clone(),
            content: String::new(),
            links_to: meta.links_to.iter().map(|l| l.0.clone()).collect(),
            token_cost: meta.token_cost,
        };
        (full, tmpl)
    };

    // Content file absent while metadata exists means the index and disk have
    // diverged — surface it rather than returning an empty string behind 200.
    let content = std::fs::read_to_string(&full_path).map_err(|e| {
        eprintln!(
            "get_page: content missing for {} at {}: {e}",
            detail_template.id,
            full_path.display(),
        );
        StatusCode::BAD_GATEWAY
    })?;

    Ok(Json(PageDetail { content, ..detail_template }))
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
    if state.read_only { return Err(StatusCode::FORBIDDEN); }
    let mut wiki = acquire_wiki_mut(&state);
    let id = req.id.clone();
    wiki.create_page(&PageId(req.id), &req.title, &req.content)
        .map_err(|e| {
            eprintln!("create_page: write failed for {id}: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    // Persistence failures are not recoverable from the client's perspective —
    // the in-memory graph advanced but disk state didn't. Surface it.
    wiki.save().map_err(|e| {
        eprintln!("create_page: save failed for {id}: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    state.counters.creates.fetch_add(1, Ordering::Relaxed);
    Ok(StatusCode::CREATED)
}

async fn maintain_handler(
    State(state): State<AppState>,
) -> Result<Json<MaintainResponse>, StatusCode> {
    if state.read_only { return Err(StatusCode::FORBIDDEN); }
    let t = Instant::now();
    let mut wiki = acquire_wiki_mut(&state);
    let report = wiki.maintain_with_dream(&RemConfig::default());

    let dreamed_edges: Vec<[String; 2]> = {
        let pages = wiki.all_pages();
        report.dreamed_edges.iter()
            .filter_map(|&(src, dst)| {
                Some([pages.get(src)?.id.0.clone(), pages.get(dst)?.id.0.clone()])
            })
            .collect()
    };

    wiki.save().map_err(|e| {
        eprintln!("maintain: save failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    drop(wiki);
    let elapsed_us = t.elapsed().as_micros() as u64;
    state.counters.record_maintain(elapsed_us);

    Ok(Json(MaintainResponse {
        health: report.health,
        pruned_count: report.pruned.len(),
        dreamed_count: dreamed_edges.len(),
        dreamed_edges,
        elapsed_us,
    }))
}

async fn neighborhood_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NeighborhoodResponse>, StatusCode> {
    const MAX_NODES: usize = 48;

    let wiki = acquire_wiki(&state);
    let pages = wiki.all_pages();
    let g = wiki.graph();
    let n = g.len();

    let center = wiki
        .page_index(&PageId(id.clone()))
        .ok_or(StatusCode::NOT_FOUND)?;

    // BFS out to depth 2 treating the graph as undirected (associative reach
    // is what we want to surface — direction is encoded per-node separately).
    // Walk the CSR adjacency directly via neighbors_out/in — O(Σ degrees of
    // frontier) instead of O(frontier × n). At n=495k the previous scan-all
    // path cost ~1s on a hub node; this is < 10ms.
    let mut hops: Vec<Option<u32>> = vec![None; n];
    hops[center] = Some(0);
    let mut frontier = vec![center];
    for depth in 1..=2u32 {
        let mut next = Vec::new();
        for &i in &frontier {
            for j in g.neighbors_out(i).into_iter().chain(g.neighbors_in(i)) {
                if hops[j].is_none() {
                    hops[j] = Some(depth);
                    next.push(j);
                }
            }
        }
        frontier = next;
    }

    // Classify direction relative to the center.
    let direction_of = |i: usize| -> &'static str {
        if i == center {
            return "center";
        }
        let out = g.raw_weight(center, i) > 0.0;
        let inb = g.raw_weight(i, center) > 0.0;
        match (out, inb) {
            (true, true) => "both",
            (true, false) => "out",
            (false, true) => "in",
            (false, false) => "indirect",
        }
    };

    // Build node list with min-hop tracking.
    let mut reached: Vec<(usize, u32)> = (0..n)
        .filter_map(|i| hops[i].map(|h| (i, h)))
        .collect();

    // Cap: keep center + all 1-hop + best 2-hop by edge weight to any 1-hop node.
    // We precompute score_2hop once per 2-hop node (O(reached × one_hop) but
    // once, not O(reached log reached × one_hop) the way a closure in sort_by
    // would have to recompute). At 495k with hub-class centers this is the
    // difference between ~150 ms and a few ms per call.
    let truncated = reached.len() > MAX_NODES;
    if truncated {
        let one_hop_idxs: Vec<usize> = reached
            .iter()
            .filter_map(|&(i, h)| if h == 1 { Some(i) } else { None })
            .collect();
        let mut score_cache: Vec<f64> = Vec::with_capacity(reached.len());
        for &(i, h) in &reached {
            let s = if h == 2 {
                one_hop_idxs
                    .iter()
                    .map(|&j| g.raw_weight(i, j).max(g.raw_weight(j, i)))
                    .fold(0.0_f64, f64::max)
            } else {
                0.0
            };
            score_cache.push(s);
        }
        let mut indexed: Vec<(usize, u32, f64)> = reached
            .iter()
            .zip(&score_cache)
            .map(|(&(i, h), &s)| (i, h, s))
            .collect();
        indexed.sort_by(|a, b| {
            a.1.cmp(&b.1).then_with(|| {
                b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        indexed.truncate(MAX_NODES);
        reached = indexed.into_iter().map(|(i, h, _)| (i, h)).collect();
    }

    // Keep indices alongside nodes so we don't need a second id→idx lookup pass.
    // Previously this handler did `pages.iter().position(|p| p.id.0 == node.id)`
    // for every kept node — 48 × N linear scans on a 495k corpus.
    let kept: Vec<(usize, u32)> = reached.iter().copied().collect();
    let nodes: Vec<NeighborNode> = kept
        .iter()
        .filter_map(|&(i, h)| {
            let p = pages.get(i)?;
            Some(NeighborNode {
                id: p.id.0.clone(),
                title: p.title.clone(),
                token_cost: p.token_cost,
                hops: h,
                direction: direction_of(i),
            })
        })
        .collect();

    // Edges between nodes that made the cut.
    let kept_idx: Vec<usize> = kept.iter().map(|&(i, _)| i).collect();

    let mut edges = Vec::new();
    for &i in &kept_idx {
        for &j in &kept_idx {
            if i == j {
                continue;
            }
            let w = g.raw_weight(i, j);
            if w > 0.0 {
                edges.push(NeighborEdge {
                    from: pages[i].id.0.clone(),
                    to: pages[j].id.0.clone(),
                    weight: w,
                });
            }
        }
    }

    Ok(Json(NeighborhoodResponse {
        center: id,
        nodes,
        edges,
        truncated,
    }))
}

// ─── SSR / SEO handlers ──────────────────────────────────────────────────────

fn base_url(headers: &HeaderMap) -> String {
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:3001");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| {
            if host.starts_with("localhost") || host.starts_with("127.") || host.starts_with("0.0.0.0") {
                "http"
            } else {
                "https"
            }
        });
    format!("{scheme}://{host}")
}

async fn ssr_article_handler(
    State(state): State<AppState>,
    Path((corpus, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let Some(lock) = state.corpora.get(&corpus) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let wiki = lock.read();
    let base = base_url(&headers);
    match ssr::render_article(&wiki, &corpus, &id, &base) {
        Some(html) => Html(html).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn ssr_corpus_handler(
    State(state): State<AppState>,
    Path(corpus): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(lock) = state.corpora.get(&corpus) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let wiki = lock.read();
    let base = base_url(&headers);
    Html(ssr::render_corpus(&wiki, &corpus, &base)).into_response()
}

async fn sitemap_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let base = base_url(&headers);
    let xml = ssr::render_sitemap(&state.corpora, &base);
    (
        [("content-type", "application/xml; charset=utf-8")],
        xml,
    ).into_response()
}

async fn robots_handler(headers: HeaderMap) -> Response {
    let base = base_url(&headers);
    (
        [("content-type", "text/plain; charset=utf-8")],
        ssr::render_robots(&base),
    ).into_response()
}

async fn list_corpora(State(state): State<AppState>) -> Json<Vec<CorpusSummary>> {
    let active = state.active.read().clone();
    let out: Vec<CorpusSummary> = state
        .corpora
        .iter()
        .map(|(name, lock)| {
            let wiki = lock.read();
            let g = wiki.graph();
            let n = g.len();
            let (_, _, values) = g.adj_transpose_csr();
            let edge_count = values.len();
            let max_edges = if n > 1 { n * (n - 1) } else { 1 };
            CorpusSummary {
                name: name.clone(),
                page_count: n,
                edge_count,
                density: edge_count as f64 / max_edges as f64,
                active: *name == active,
            }
        })
        .collect();
    Json(out)
}

async fn select_corpus(
    State(state): State<AppState>,
    Json(req): Json<SelectCorpusRequest>,
) -> Result<StatusCode, StatusCode> {
    if state.read_only { return Err(StatusCode::FORBIDDEN); }
    if !state.corpora.contains_key(&req.name) {
        return Err(StatusCode::NOT_FOUND);
    }
    *state.active.write() = req.name;
    Ok(StatusCode::NO_CONTENT)
}

async fn stats_handler(State(state): State<AppState>) -> Json<StatsResponse> {
    let wiki = acquire_wiki(&state);
    let g = wiki.graph();
    let n = g.len();
    // Edge count == CSR nnz. Avoids an O(n²) scan on every poll.
    let (_, _, values) = g.adj_transpose_csr();
    let edge_count = values.len();
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
        p50_us: percentile(&latencies, 50),
        p95_us: percentile(&latencies, 95),
        p99_us: percentile(&latencies, 99),
        throughput_qps: n as f64 / (total as f64 / 1_000_000.0),
    })
}

/// Nearest-rank percentile: index = ceil(p/100 · n) − 1, clamped.
/// Preconditions: `sorted` is non-empty and ascending; `p ≤ 100`.
pub(crate) fn percentile(sorted: &[u64], p: usize) -> u64 {
    let n = sorted.len();
    let idx = ((p * n + 99) / 100).saturating_sub(1).min(n - 1);
    sorted[idx]
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn percentile_nearest_rank() {
        // 100 sorted samples: p99 is the 99th ordinal (index 98), not the max.
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&v, 50), 50);
        assert_eq!(percentile(&v, 95), 95);
        assert_eq!(percentile(&v, 99), 99);
        assert_eq!(percentile(&v, 100), 100);
    }

    #[test]
    fn percentile_small_n() {
        assert_eq!(percentile(&[42], 50), 42);
        assert_eq!(percentile(&[42], 99), 42);
        // n=2 ascending: p50 is the lower sample, p99 the upper — not both the max.
        assert_eq!(percentile(&[10, 20], 50), 10);
        assert_eq!(percentile(&[10, 20], 99), 20);
    }
}

// ─── Simulation (SSE) ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SimulateParams {
    #[serde(default = "default_seed_pages")]
    pages: usize,
    #[serde(default = "default_ops")]
    ops: usize,
}

fn default_seed_pages() -> usize { 150 }
fn default_ops() -> usize { 300 }

async fn simulate_handler(
    AxumQuery(params): AxumQuery<SimulateParams>,
) -> Sse<impl futures::Stream<Item = Result<SseEvent, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<simulate::Event>(512);

    tokio::task::spawn_blocking(move || {
        simulate::run_simulation(params.pages, params.ops, |event| {
            let _ = tx.blocking_send(event);
        });
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|event| {
        let json = serde_json::to_string(&event).unwrap_or_default();
        Ok(SseEvent::default().data(json))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ─── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // CLI:  cowiki-server <wiki-dir> [<wiki-dir> ...] [--ui <dist>] [--port <N>] [--read-only]
    // Every non-flag argument is a corpus root; its directory basename is
    // the corpus name shown in the UI selector.
    let argv: Vec<String> = std::env::args().collect();

    let mut ui_dir: Option<String> = None;
    let mut port: u16 = 3001;
    let mut read_only: bool = false;
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut i = 1;
    while i < argv.len() {
        let a = &argv[i];
        if a == "--ui" {
            ui_dir = argv.get(i + 1).cloned();
            i += 2;
        } else if a == "--port" {
            port = argv.get(i + 1)
                .and_then(|s| s.parse().ok())
                .expect("--port requires a u16");
            i += 2;
        } else if a == "--read-only" {
            read_only = true;
            i += 1;
        } else {
            roots.push(PathBuf::from(a));
            i += 1;
        }
    }
    // Env var overrides so ops can configure without touching the CLI string.
    if let Some(env_port) = std::env::var("COWIKI_PORT").ok().and_then(|s| s.parse().ok()) {
        port = env_port;
    }
    if std::env::var("COWIKI_READ_ONLY").ok().as_deref() == Some("1") {
        read_only = true;
    }

    if roots.is_empty() {
        eprintln!("Usage: cowiki-server <wiki-dir> [<wiki-dir> ...] [--ui <dist-dir>] [--port <N>] [--read-only]");
        std::process::exit(1);
    }

    let mut corpora: BTreeMap<String, RwLock<WikiBackend>> = BTreeMap::new();
    for root in &roots {
        if !root.exists() {
            eprintln!("Directory does not exist: {}", root.display());
            std::process::exit(1);
        }
        let name = root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("corpus")
            .to_string();
        eprintln!("Opening corpus '{name}' at {}", root.display());
        let wiki = WikiBackend::open_or_rebuild(root).unwrap_or_else(|e| {
            eprintln!("  failed: {e}");
            std::process::exit(1);
        });
        eprintln!("  indexed {} pages", wiki.len());
        if corpora.contains_key(&name) {
            eprintln!("  duplicate corpus name '{name}' — skipping");
            continue;
        }
        corpora.insert(name, RwLock::new(wiki));
    }

    // Default active: the first corpus in alphabetical order (BTreeMap's natural order).
    let first = corpora.keys().next().cloned().unwrap();
    let state: AppState = Arc::new(Inner {
        corpora,
        active: RwLock::new(first.clone()),
        counters: Counters::new(),
        read_only,
    });

    let mut app = Router::new()
        .route("/api/corpora", get(list_corpora))
        .route("/api/corpora/select", post(select_corpus))
        .route("/api/pages", get(list_pages).post(create_page_handler))
        .route("/api/pages/{*id}", get(get_page))
        .route("/api/query", post(query_pages))
        .route("/api/maintain", post(maintain_handler))
        .route("/api/stats", get(stats_handler))
        .route("/api/neighborhood/{*id}", get(neighborhood_handler))
        .route("/api/perf", get(perf_handler))
        .route("/api/stress", post(stress_handler))
        .route("/api/simulate", get(simulate_handler))
        // SEO / SSR surfaces — crawlers and link shares land here.
        .route("/w/{corpus}/{*id}", get(ssr_article_handler))
        .route("/c/{corpus}", get(ssr_corpus_handler))
        .route("/sitemap.xml", get(sitemap_handler))
        .route("/robots.txt", get(robots_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    if let Some(ref dir) = ui_dir {
        eprintln!("Serving UI from: {dir}");
        // Cache headers on static assets. DO App Platform's proxy injects
        // `cache-control: private` by default, which blocks both edge and
        // browser caching and forces every visitor to re-download the full
        // bundle on every page load. Setting explicit headers here wins
        // against their default because the browser sees both values and
        // honors the closest-to-origin one.
        //
        // Vite emits hashed filenames under /assets/ — safe to cache for a
        // year and mark immutable. The HTML shell at / is short-cached so
        // a UI deploy propagates in minutes.
        let static_router: Router = Router::new()
            .fallback_service(
                tower_http::services::ServeDir::new(dir)
                    .fallback(tower_http::services::ServeFile::new(
                        PathBuf::from(dir).join("index.html"),
                    )),
            )
            .layer(axum::middleware::from_fn(
                |req: axum::extract::Request, next: axum::middleware::Next| async move {
                    let path = req.uri().path().to_owned();
                    let mut resp = next.run(req).await;
                    let cc = if path.starts_with("/assets/") {
                        "public, max-age=31536000, immutable"
                    } else {
                        "public, max-age=300"
                    };
                    resp.headers_mut().insert(
                        axum::http::header::CACHE_CONTROL,
                        axum::http::HeaderValue::from_static(cc),
                    );
                    resp
                },
            ));
        app = app.fallback_service(static_router);
    }

    let addr = format!("0.0.0.0:{port}");
    let mode = if read_only { " [read-only]" } else { "" };
    if ui_dir.is_some() {
        eprintln!("Co-Wiki ready at http://{addr}  (default corpus: {first}){mode}");
    } else {
        eprintln!("API ready at http://{addr}  (default corpus: {first}){mode}");
    }

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
