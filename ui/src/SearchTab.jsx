import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { listPages, getPage, queryPages } from './api'
import NeighborhoodGraph from './NeighborhoodGraph'

const Icon = ({ name, style }) => (
  <span className="material-symbols-outlined" style={style}>{name}</span>
)

// ── Snippet extraction ───────────────────────────────────────────────────────
// The query API returns metadata only; content (and therefore a readable
// snippet) lives behind /api/pages/:id. We load snippets lazily for the top
// results after the query returns and fill them in optimistically.

function snippetOf(content) {
  if (!content) return ''
  const lines = content.split('\n').filter(Boolean)
  // Drop the title line (# Heading) and any section header lines.
  const body = lines.filter(l => !l.trim().startsWith('#')).join(' ')
  // Strip wiki formatting for the teaser.
  const stripped = body
    .replace(/\[\[([^\]|]+\|)?([^\]]+)\]\]/g, '$2')
    .replace(/\*\*(.+?)\*\*/g, '$1')
    .replace(/\*(.+?)\*/g, '$1')
    .replace(/<math[^>]*>[\s\S]*?<\/math>/g, '')
    .replace(/\s+/g, ' ')
    .trim()
  return stripped.slice(0, 220) + (stripped.length > 220 ? '…' : '')
}

// ── Lightweight wikitext renderer for the drawer ─────────────────────────────

function renderInline(text, onLink) {
  const tokens = []
  const re = /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]|\*\*([^*]+)\*\*|\*([^*\n]+)\*/g
  let last = 0
  let match
  let i = 0
  while ((match = re.exec(text)) !== null) {
    if (match.index > last) tokens.push(text.slice(last, match.index))
    if (match[1] !== undefined) {
      const target = match[1]
      const display = match[2] || target
      tokens.push(
        <button
          key={`l${i++}`}
          className="drawer-inline-link"
          onClick={e => { e.stopPropagation(); onLink(target) }}
        >{display}</button>
      )
    } else if (match[3] !== undefined) {
      tokens.push(<strong key={`b${i++}`}>{match[3]}</strong>)
    } else if (match[4] !== undefined) {
      tokens.push(<em key={`i${i++}`}>{match[4]}</em>)
    }
    last = re.lastIndex
  }
  if (last < text.length) tokens.push(text.slice(last))
  return tokens
}

function ArticleBody({ content, onLink }) {
  const blocks = useMemo(() => {
    const paragraphs = content.split(/\n{2,}/)
    return paragraphs.map(p => p.trim()).filter(Boolean)
  }, [content])

  return (
    <div className="drawer-article">
      {blocks.map((block, i) => {
        const m = block.match(/^(#{1,6})\s+(.+)$/)
        if (m) {
          const level = Math.min(6, m[1].length)
          const Tag = `h${level}`
          return <Tag key={i} className={`drawer-h drawer-h${level}`}>{m[2]}</Tag>
        }
        return <p key={i}>{renderInline(block, onLink)}</p>
      })}
    </div>
  )
}

// ── Page drawer ──────────────────────────────────────────────────────────────

function PageDrawer({ id, onClose, onNavigate }) {
  const [page, setPage] = useState(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    setLoading(true)
    setPage(null)
    let cancelled = false
    getPage(id).then(p => {
      if (!cancelled) {
        setPage(p)
        setLoading(false)
      }
    })
    return () => { cancelled = true }
  }, [id])

  useEffect(() => {
    const onKey = e => { if (e.key === 'Escape') onClose() }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    <>
      <div className="drawer-overlay" onClick={onClose} />
      <aside className="drawer" role="dialog" aria-modal="true">
        <header className="drawer-header">
          <div>
            {page && <div className="drawer-title">{page.title}</div>}
            {page && <div className="drawer-sub">
              <span>{page.id}</span>
              <span className="drawer-sub-dot">·</span>
              <span>{page.token_cost} tokens</span>
              <span className="drawer-sub-dot">·</span>
              <span>{page.links_to.length} outbound</span>
            </div>}
            {loading && <div className="drawer-title" style={{ color: 'var(--text-dim)' }}>Loading…</div>}
          </div>
          <button className="drawer-close" onClick={onClose} aria-label="Close">
            <Icon name="close" />
          </button>
        </header>
        <div className="drawer-body">
          <NeighborhoodGraph centerId={id} onNavigate={onNavigate} />
          {page && <ArticleBody content={page.content} onLink={onNavigate} />}
          {loading && <SkeletonLines count={6} />}
        </div>
      </aside>
    </>
  )
}

// ── Skeletons ────────────────────────────────────────────────────────────────

function SkeletonLines({ count = 3 }) {
  return (
    <div className="skel-stack">
      {Array.from({ length: count }).map((_, i) => (
        <div key={i} className="skel-line" style={{ width: `${70 + (i % 3) * 10}%` }} />
      ))}
    </div>
  )
}

function ResultSkeleton() {
  return (
    <div className="search-result search-result-skel">
      <div className="skel-line" style={{ width: '55%', height: 14 }} />
      <div className="skel-line" style={{ width: '30%', height: 10, marginTop: 8 }} />
      <div className="skel-line" style={{ width: '90%', height: 10, marginTop: 10 }} />
      <div className="skel-line" style={{ width: '80%', height: 10, marginTop: 6 }} />
    </div>
  )
}

// ── Result card ──────────────────────────────────────────────────────────────

function ResultCard({ page, rank, maxLinks, snippet, onOpen, index }) {
  const linkPct = maxLinks > 0 ? page.links_to.length / maxLinks : 0
  return (
    <article
      className="search-result"
      style={{ animationDelay: `${Math.min(index, 20) * 30}ms` }}
      onClick={() => onOpen(page.id)}
      onKeyDown={e => { if (e.key === 'Enter') onOpen(page.id) }}
      tabIndex={0}
    >
      <div className="search-result-header">
        <h3 className="search-result-title">{page.title}</h3>
        <span className="search-result-rank">#{rank}</span>
      </div>
      <div className="search-result-id">{page.id}</div>
      <div className={`search-result-snippet ${snippet ? '' : 'muted'}`}>
        {snippet || <span className="skel-inline">·········· ·········· ··········</span>}
      </div>
      <div className="search-result-meta">
        <span className="search-result-item">
          <Icon name="token" /> {page.token_cost}t
        </span>
        <span className="search-result-item" style={{ flex: 1 }}>
          <Icon name="hub" />
          <span className="search-result-bar">
            <span
              className="search-result-bar-fill"
              style={{ width: `${Math.max(4, linkPct * 100)}%` }}
            />
          </span>
          <span className="search-result-bar-num">{page.links_to.length}</span>
        </span>
      </div>
    </article>
  )
}

// ── Main tab ─────────────────────────────────────────────────────────────────

export default function SearchTab() {
  const [query, setQuery] = useState('')
  const [submitted, setSubmitted] = useState('')
  const [results, setResults] = useState(null)
  const [snippets, setSnippets] = useState({})
  const [loading, setLoading] = useState(false)
  const [hints, setHints] = useState([])
  const [openId, setOpenId] = useState(null)
  const inputRef = useRef(null)

  // Bootstrap: derive quick-start hints from the most-linked pages in the corpus.
  useEffect(() => {
    listPages().then(pages => {
      const sorted = [...pages].sort((a, b) => b.link_count - a.link_count)
      setHints(sorted.slice(0, 8).map(p => ({ id: p.id, title: p.title })))
    })
  }, [])

  // Global '/' focuses the search input, like classic search UIs.
  useEffect(() => {
    const onKey = e => {
      if (e.key === '/' && document.activeElement !== inputRef.current) {
        e.preventDefault()
        inputRef.current?.focus()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const runQuery = useCallback(async (q) => {
    const qTrim = q.trim()
    if (!qTrim) return
    setSubmitted(qTrim)
    setLoading(true)
    setSnippets({})
    const r = await queryPages(qTrim, 6000)
    setResults(r)
    setLoading(false)

    // Lazy-load snippets for the top results.
    const top = r.pages.slice(0, 10)
    top.forEach(p => {
      getPage(p.id).then(detail => {
        if (detail) {
          setSnippets(prev => ({ ...prev, [p.id]: snippetOf(detail.content) }))
        }
      })
    })
  }, [])

  const onSubmit = e => {
    e.preventDefault()
    runQuery(query)
  }

  const onHint = title => {
    setQuery(title)
    runQuery(title)
  }

  const maxLinks = results ? Math.max(1, ...results.pages.map(p => p.links_to.length)) : 1

  return (
    <div className="search-tab">
      <div className="search-tab-inner">

        {/* Hero search */}
        <form className="search-hero" onSubmit={onSubmit}>
          <div className="search-hero-row">
            <Icon name="search" />
            <input
              ref={inputRef}
              className="search-hero-input"
              placeholder="Query the knowledge graph"
              value={query}
              onChange={e => setQuery(e.target.value)}
              aria-label="Search query"
              autoFocus
            />
            {query && (
              <button
                type="button"
                className="search-hero-clear"
                onClick={() => { setQuery(''); inputRef.current?.focus() }}
                aria-label="Clear"
              ><Icon name="close" /></button>
            )}
            <button className="search-hero-submit" disabled={loading || !query.trim()}>
              <Icon name="bolt" />
              Spread
            </button>
          </div>
          <div className="search-shortcut-hint">
            Press <kbd>/</kbd> to focus · <kbd>Esc</kbd> to close an article
          </div>
        </form>

        {/* Hints (shown until first query) */}
        {!submitted && hints.length > 0 && (
          <section>
            <div className="search-section-title">
              <Icon name="trending_up" /> Hubs in this corpus
            </div>
            <div className="search-hints">
              {hints.map(h => (
                <button
                  key={h.id}
                  className="search-hint"
                  onClick={() => onHint(h.title)}
                >
                  <Icon name="north_east" />
                  {h.title}
                </button>
              ))}
            </div>
          </section>
        )}

        {/* Query metrics */}
        {results && (
          <div className="search-metrics">
            <span className="search-metric-item">
              <Icon name="search" />
              <strong>{results.pages.length}</strong> hits
            </span>
            <span className="search-metric-item">
              <Icon name="timer" />
              <strong>{results.elapsed_us}</strong>μs
            </span>
            <span className="search-metric-item">
              <Icon name="autorenew" />
              <strong>{results.iterations}</strong> iter
            </span>
            <span className="search-metric-item">
              <Icon name="sigma" />
              <strong>{results.total_score.toFixed(3)}</strong>
            </span>
            <span
              className={`search-metric-item ${results.converged ? 'search-metric-converged' : 'search-metric-unconverged'}`}
              style={{ marginLeft: 'auto' }}
            >
              <Icon name={results.converged ? 'check_circle' : 'cyclone'} />
              {results.converged ? 'converged' : 'max iter'}
            </span>
          </div>
        )}

        {/* Results */}
        {loading && (
          <div className="search-results">
            {Array.from({ length: 4 }).map((_, i) => <ResultSkeleton key={i} />)}
          </div>
        )}
        {!loading && results && results.pages.length === 0 && (
          <div className="search-empty">
            <Icon name="troubleshoot" />
            <div>No pages activated above threshold.</div>
            <div style={{ opacity: 0.6, marginTop: 4 }}>Try a broader query.</div>
          </div>
        )}
        {!loading && results && results.pages.length > 0 && (
          <div className="search-results">
            {results.pages.map((p, i) => (
              <ResultCard
                key={p.id}
                page={p}
                rank={i + 1}
                index={i}
                maxLinks={maxLinks}
                snippet={snippets[p.id]}
                onOpen={setOpenId}
              />
            ))}
          </div>
        )}
      </div>

      {/* Drawer */}
      {openId && (
        <PageDrawer
          id={openId}
          onClose={() => setOpenId(null)}
          onNavigate={setOpenId}
        />
      )}
    </div>
  )
}
