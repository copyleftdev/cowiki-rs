import { useState, useEffect, useCallback, useRef } from 'react'
import { listPages, getPage, queryPages, createPage, runMaintain, getStats, getPerf, runStress, getCorpora, selectCorpus } from './api'
import SimulationTab from './SimulationTab'
import SearchTab from './SearchTab'
import CorpusSelector, { topicStyle } from './CorpusSelector'
import './index.css'

function HealthRing({ value }) {
  const r = 42, c = 2 * Math.PI * r
  const pct = Math.max(0, Math.min(1, value))
  const color = pct > 0.7 ? 'var(--green)' : pct > 0.3 ? 'var(--amber)' : 'var(--red)'
  return (
    <div className="health-ring">
      <svg width="100" height="100">
        <circle cx="50" cy="50" r={r} fill="none" stroke="var(--surface-2)" strokeWidth="6" />
        <circle cx="50" cy="50" r={r} fill="none" stroke={color} strokeWidth="6"
          strokeDasharray={c} strokeDashoffset={c * (1 - pct)}
          strokeLinecap="round" style={{ transition: 'stroke-dashoffset 0.6s ease' }} />
      </svg>
      <div className="center" style={{ color }}>{(pct * 100).toFixed(0)}%</div>
    </div>
  )
}

function LatencyBars({ stress }) {
  if (!stress) return null
  const max = stress.p99_us || 1
  const bars = [
    { label: 'min', value: stress.min_us, color: 'var(--green)' },
    { label: 'p50', value: stress.p50_us, color: 'var(--cyan)' },
    { label: 'p95', value: stress.p95_us, color: 'var(--amber)' },
    { label: 'p99', value: stress.p99_us, color: 'var(--red)' },
    { label: 'max', value: stress.max_us, color: 'var(--red)' },
  ]
  return (
    <div>
      {bars.map(b => (
        <div className="latency-bar" key={b.label}>
          <span className="lbl">{b.label}</span>
          <div className="bar-bg">
            <div className="bar-fill" style={{
              width: `${(b.value / max) * 100}%`,
              background: b.color,
            }} />
          </div>
          <span className="val" style={{ color: b.color }}>{b.value}us</span>
        </div>
      ))}
    </div>
  )
}

export default function App() {
  const [pages, setPages] = useState([])
  const [selected, setSelected] = useState(null)
  const [query, setQuery] = useState('')
  const [results, setResults] = useState(null)
  const [stats, setStats] = useState(null)
  const [perf, setPerf] = useState(null)
  const [maintainResult, setMaintainResult] = useState(null)
  const [stressResult, setStressResult] = useState(null)
  const [busy, setBusy] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [newId, setNewId] = useState('')
  const [newTitle, setNewTitle] = useState('')
  const [newContent, setNewContent] = useState('')
  const [tab, setTab] = useState('search')
  const [theme, setTheme] = useState(() => document.documentElement.dataset.theme || 'dark')
  const [corpora, setCorpora] = useState([])
  const [activeCorpus, setActiveCorpus] = useState(null)
  const perfInterval = useRef(null)

  useEffect(() => {
    getCorpora().then(cs => {
      setCorpora(cs)
      const active = cs.find(c => c.active) || cs[0]
      if (active) setActiveCorpus(active.name)
    })
  }, [])

  const handleCorpusSwitch = useCallback(async (name) => {
    const ok = await selectCorpus(name)
    if (!ok) return
    setActiveCorpus(name)
    setSelected(null)
    setResults(null)
    setMaintainResult(null)
    setStressResult(null)
    const cs = await getCorpora()
    setCorpora(cs)
    const [p, s, pf] = await Promise.all([listPages({ limit: 200 }), getStats(), getPerf()])
    setPages(p)
    setStats(s)
    setPerf(pf)
  }, [])

  useEffect(() => {
    document.documentElement.dataset.theme = theme
    try { localStorage.setItem('cowiki-theme', theme) } catch (e) { /* ignore */ }
  }, [theme])

  const toggleTheme = () => setTheme(t => t === 'dark' ? 'light' : 'dark')

  const exampleQueries = [
    { label: 'memory + sleep', query: 'memory sleep consolidation' },
    { label: 'attack vectors', query: 'vulnerability dependency attack' },
    { label: 'graph trust', query: 'graph traversal trust' },
    { label: 'cognitive priming', query: 'priming associative activation' },
    { label: 'consensus', query: 'distributed consensus partition' },
    { label: 'chunk retrieval', query: 'chunking knapsack budget density' },
    { label: 'threat model', query: 'threat model attack surface' },
    { label: 'REM dreaming', query: 'rem decay prune dream maintenance' },
    { label: 'Kahneman', query: 'system fast slow thinking' },
    { label: 'DDIA', query: 'replication consistency data' },
  ]

  const refresh = useCallback(async () => {
    const [p, s, pf] = await Promise.all([listPages({ limit: 200 }), getStats(), getPerf()])
    setPages(p)
    setStats(s)
    setPerf(pf)
  }, [])

  useEffect(() => {
    refresh()
    perfInterval.current = setInterval(async () => {
      const [s, pf] = await Promise.all([getStats(), getPerf()])
      setStats(s)
      setPerf(pf)
    }, 2000)
    return () => clearInterval(perfInterval.current)
  }, [refresh])

  const handleQuery = async () => {
    if (!query.trim()) return
    setBusy(true)
    const r = await queryPages(query)
    setResults(r)
    setBusy(false)
    const pf = await getPerf()
    setPerf(pf)
  }

  const handleSelect = async (id) => {
    const p = await getPage(id)
    setSelected(p)
  }

  const handleMaintain = async () => {
    setBusy(true)
    const r = await runMaintain()
    setMaintainResult(r)
    setBusy(false)
    await refresh()
  }

  const handleStress = async () => {
    setBusy(true)
    const r = await runStress(200, query || 'spreading activation')
    setStressResult(r)
    setBusy(false)
    await refresh()
  }

  const handleCreate = async () => {
    if (!newId.trim() || !newTitle.trim()) return
    setBusy(true)
    await createPage(newId, newTitle, newContent)
    setCreateOpen(false)
    setNewId('')
    setNewTitle('')
    setNewContent('')
    setBusy(false)
    await refresh()
  }

  return (
    <div className="app">
      {/* ── Header ────────────────────────────────────────────── */}
      <header className="header">
        <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
          <div style={{ display: 'flex', alignItems: 'baseline' }}>
            <h1>Co-Wiki</h1>
            <span className="sub">Spreading Activation Engine</span>
          </div>
          {corpora.length > 0 && activeCorpus && (
            <CorpusSelector
              corpora={corpora}
              activeName={activeCorpus}
              onSelect={handleCorpusSwitch}
            />
          )}
          <div style={{ display: 'flex', gap: 2, marginLeft: 16 }}>
            <button
              className={`tab-btn ${tab === 'search' ? 'active' : ''}`}
              onClick={() => setTab('search')}
            >Search</button>
            <button
              className={`tab-btn ${tab === 'wiki' ? 'active' : ''}`}
              onClick={() => setTab('wiki')}
            >Instrument</button>
            <button
              className={`tab-btn ${tab === 'simulate' ? 'active' : ''}`}
              onClick={() => setTab('simulate')}
            >Simulation</button>
          </div>
        </div>
        <div className="header-stats">
          {stats && <>
            <span className="stat-pill cyan">{stats.page_count} pages</span>
            <span className="stat-pill amber">{stats.edge_count} edges</span>
            <span className="stat-pill purple">{(stats.density * 100).toFixed(1)}% density</span>
          </>}
          {perf && perf.queries > 0 &&
            <span className="stat-pill green">{perf.query_avg_us.toFixed(0)}us avg</span>
          }
          <div className="mutex">
            <div className={`mutex-dot ${busy ? 'busy' : ''}`} />
            {busy ? 'LOCKED' : 'IDLE'}
          </div>
          <button
            className="theme-toggle"
            onClick={toggleTheme}
            aria-label={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`}
            title={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`}
          >
            <span className="material-symbols-outlined">
              {theme === 'dark' ? 'light_mode' : 'dark_mode'}
            </span>
          </button>
        </div>
      </header>

      {tab === 'simulate' ? (
        <div style={{ gridColumn: '1 / -1' }}>
          <SimulationTab />
        </div>
      ) : tab === 'search' ? (
        <SearchTab key={activeCorpus || 'none'} />
      ) : <>

      {/* ── Left panel: Query + Pages ─────────────────────────── */}
      <div className="panel">
        <div className="panel-title">Retrieval</div>
        <div className="search-box">
          <input
            placeholder="Query the knowledge graph..."
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleQuery()}
          />
          <button className="btn btn-primary" onClick={handleQuery} disabled={busy}>
            Spread
          </button>
        </div>

        <div className="example-queries">
          {exampleQueries.map(eq => (
            <button
              key={eq.label}
              className="example-pill"
              onClick={() => {
                setQuery(eq.query)
                setBusy(true)
                queryPages(eq.query).then(r => {
                  setResults(r)
                  setBusy(false)
                  getPerf().then(setPerf)
                })
              }}
            >
              {eq.label}
            </button>
          ))}
        </div>

        {results && (
          <div className="query-meta">
            <span>score <strong>{results.total_score.toFixed(4)}</strong></span>
            <span>cost <strong>{results.total_cost}</strong>t</span>
            <span>iter <strong>{results.iterations}</strong></span>
            <span>time <strong>{results.elapsed_us}</strong>us</span>
            <span>{results.converged ? 'converged' : 'max iter'}</span>
          </div>
        )}

        {results && results.pages.map(p => (
          <div
            key={p.id}
            className={`result-card ${selected?.id === p.id ? 'active' : ''}`}
            onClick={() => handleSelect(p.id)}
          >
            <div className="title">{p.title}</div>
            <div className="id">{p.id}</div>
            <div className="meta-row">
              <span className="stat-pill cyan" style={{ fontSize: 10 }}>{p.token_cost}t</span>
              {p.links_to.length > 0 &&
                <span className="stat-pill amber" style={{ fontSize: 10 }}>
                  {p.links_to.length} links
                </span>
              }
            </div>
          </div>
        ))}

        <div className="divider" />

        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
          <div className="panel-title" style={{ margin: 0 }}>All Pages</div>
          <button className="btn btn-ghost btn-sm" onClick={() => setCreateOpen(true)}>+ New</button>
        </div>

        {stats && stats.page_count > pages.length && (
          <div style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 8 }}>
            Showing {pages.length.toLocaleString()} of {stats.page_count.toLocaleString()} · use search to find more
          </div>
        )}

        {pages.map(p => (
          <div
            key={p.id}
            className={`result-card ${selected?.id === p.id ? 'active' : ''}`}
            onClick={() => handleSelect(p.id)}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between' }}>
              <span className="title">{p.title}</span>
              <span style={{ fontSize: 10, color: 'var(--text-dim)' }}>{p.token_cost}t</span>
            </div>
            <div className="id">{p.id}</div>
          </div>
        ))}
      </div>

      {/* ── Center: Page viewer ───────────────────────────────── */}
      <div className="panel">
        {selected ? (
          <div className="page-viewer">
            <h1>{selected.title}</h1>
            <div className="page-id">{selected.id} / {selected.token_cost} tokens</div>

            {selected.links_to.length > 0 && (
              <div className="backlinks">
                {selected.links_to.map(link => (
                  <button key={link} className="backlink" onClick={() => handleSelect(link)}>
                    {link}
                  </button>
                ))}
              </div>
            )}

            <div className="divider" />
            <div className="page-content">{selected.content}</div>
          </div>
        ) : (
          <div className="empty-state">
            <div className="big">G</div>
            Search or select a page
          </div>
        )}
      </div>

      {/* ── Right panel: Performance + REM ─────────────────────── */}
      <div className="panel">
        <div className="panel-title">Performance Counters</div>

        {perf && (
          <div className="perf-grid">
            <div className="perf-card cyan">
              <div className="label">Queries</div>
              <div className="value">{perf.queries}</div>
            </div>
            <div className="perf-card green">
              <div className="label">Avg Latency</div>
              <div className="value">{perf.query_avg_us.toFixed(0)}<span className="unit">us</span></div>
            </div>
            <div className="perf-card amber">
              <div className="label">Min / Max</div>
              <div className="value" style={{ fontSize: 14 }}>
                {perf.query_min_us}<span className="unit">us</span> / {perf.query_max_us}<span className="unit">us</span>
              </div>
            </div>
            <div className="perf-card purple">
              <div className="label">Lock Wait</div>
              <div className="value">{perf.lock_avg_ns.toFixed(0)}<span className="unit">ns</span></div>
            </div>
            <div className="perf-card cyan">
              <div className="label">Maintains</div>
              <div className="value">{perf.maintains}</div>
            </div>
            <div className="perf-card amber">
              <div className="label">Creates</div>
              <div className="value">{perf.creates}</div>
            </div>
          </div>
        )}

        <div className="divider" />

        {/* Stress test */}
        <div className="panel-title">Stress Test</div>
        <button className="btn btn-primary" onClick={handleStress} disabled={busy}
          style={{ width: '100%', marginBottom: 12 }}>
          Fire 200 Queries
        </button>

        {stressResult && (
          <>
            <div className="perf-grid">
              <div className="perf-card green">
                <div className="label">Throughput</div>
                <div className="value">{stressResult.throughput_qps.toFixed(0)}<span className="unit">qps</span></div>
              </div>
              <div className="perf-card cyan">
                <div className="label">Avg</div>
                <div className="value">{stressResult.avg_us.toFixed(0)}<span className="unit">us</span></div>
              </div>
            </div>
            <LatencyBars stress={stressResult} />
          </>
        )}

        <div className="divider" />

        {/* REM Agent */}
        <div className="panel-title">REM Agent</div>

        {maintainResult && <HealthRing value={maintainResult.health} />}

        <button className="btn btn-purple" onClick={handleMaintain} disabled={busy}
          style={{ width: '100%', marginBottom: 12 }}>
          Run Maintenance Cycle
        </button>

        {maintainResult && (
          <div>
            <div className="perf-grid">
              <div className="perf-card green">
                <div className="label">Health</div>
                <div className="value">{(maintainResult.health * 100).toFixed(0)}<span className="unit">%</span></div>
              </div>
              <div className="perf-card amber">
                <div className="label">Time</div>
                <div className="value">{(maintainResult.elapsed_us / 1000).toFixed(1)}<span className="unit">ms</span></div>
              </div>
              <div className="perf-card red">
                <div className="label">Pruned</div>
                <div className="value">{maintainResult.pruned_count}</div>
              </div>
              <div className="perf-card purple">
                <div className="label">Dreamed</div>
                <div className="value">{maintainResult.dreamed_count}</div>
              </div>
            </div>

            {maintainResult.dreamed_edges.length > 0 && (
              <div>
                <div className="panel-title">Discovered Backlinks</div>
                {maintainResult.dreamed_edges.map(([src, dst], i) => (
                  <div key={i} className="dream-edge">
                    {src} <span>-&gt;</span> {dst}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      </>}

      {/* ── Create dialog ──────────────────────────────────────── */}
      {createOpen && (
        <>
          <div className="dialog-overlay" onClick={() => setCreateOpen(false)} />
          <div className="dialog-content">
            <h2>Create Page</h2>
            <input placeholder="page-id (e.g. ai/transformers)" value={newId} onChange={e => setNewId(e.target.value)} />
            <input placeholder="Page Title" value={newTitle} onChange={e => setNewTitle(e.target.value)} />
            <textarea placeholder="Content with [[backlinks]]..." value={newContent} onChange={e => setNewContent(e.target.value)} />
            <div className="dialog-buttons">
              <button className="btn btn-ghost" onClick={() => setCreateOpen(false)}>Cancel</button>
              <button className="btn btn-primary" onClick={handleCreate} disabled={busy}>Create</button>
            </div>
          </div>
        </>
      )}
    </div>
  )
}
