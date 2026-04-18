import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { getPage, queryPages } from './api'
import NeighborhoodGraph from './NeighborhoodGraph'

const Icon = ({ name, style }) => (
  <span className="material-symbols-outlined" style={style}>{name}</span>
)

// ── Curated landing cards ───────────────────────────────────────────────────
//
// Hand-picked famous cases present in scotus-top10k. Cluster ids (and the
// slug suffix they produce) are stable — the slug base comes from
// CourtListener's `case_name_short`.
//
// Eras are advisory; used for the card subtitle to help users orient.
const LANDMARKS = [
  {
    id: 'brown-105221',
    title: 'Brown v. Board of Education',
    era: '1954 · Civil Rights',
    blurb: 'Separate educational facilities are inherently unequal. Overruled Plessy v. Ferguson.',
  },
  {
    id: 'miranda-107252',
    title: 'Miranda v. Arizona',
    era: '1966 · Criminal Procedure',
    blurb: 'Custodial interrogation requires warnings of the right to remain silent and counsel.',
  },
  {
    id: 'roe-108713',
    title: 'Roe v. Wade',
    era: '1973 · Privacy',
    blurb: 'Recognized a constitutional right to abortion under the Fourteenth Amendment.',
  },
  {
    id: 'gideon-106545',
    title: 'Gideon v. Wainwright',
    era: '1963 · Right to Counsel',
    blurb: 'Indigent defendants in state criminal trials are entitled to appointed counsel.',
  },
  {
    id: 'wickard-103716',
    title: 'Wickard v. Filburn',
    era: '1942 · Commerce Clause',
    blurb: 'Congress can regulate wholly intrastate activity that, in aggregate, affects interstate commerce.',
  },
  {
    id: 'gibbons-85412',
    title: 'Gibbons v. Ogden',
    era: '1824 · Commerce Clause',
    blurb: 'Established the federal government’s broad power over interstate commerce.',
  },
  {
    id: 'mcculloch-1320585',
    title: 'McCulloch v. Maryland',
    era: '1819 · Federalism',
    blurb: 'Necessary-and-Proper Clause authorizes federal action beyond enumerated powers.',
  },
  {
    id: 'plessy-94508',
    title: 'Plessy v. Ferguson',
    era: '1896 · Equal Protection',
    blurb: '“Separate but equal.” Later overturned by Brown v. Board of Education.',
  },
  {
    id: 'marsh-104216',
    title: 'Marsh v. Alabama',
    era: '1946 · First Amendment',
    blurb: 'State-action doctrine extended to company-owned towns; upheld First Amendment protections.',
  },
  {
    id: 'lochner-96276',
    title: 'Lochner v. New York',
    era: '1905 · Substantive Due Process',
    blurb: 'Struck down a maximum-hours law; gave the name to the Lochner era.',
  },
]

// Doctrine-seeded search suggestions.
const DOCTRINE_HINTS = [
  'commerce clause',
  'equal protection',
  'miranda warning',
  'due process',
  'first amendment free speech',
  'fourth amendment search and seizure',
  'cruel and unusual punishment',
  'establishment clause',
  'habeas corpus',
  'stare decisis',
]

// ── Snippet extraction (shared with the result card) ────────────────────────

function snippetOf(content) {
  if (!content) return ''
  const lines = content.split('\n').filter(Boolean)
  const body = lines.filter(l => !l.trim().startsWith('#')).join(' ')
  const stripped = body
    .replace(/\[\[([^\]|]+\|)?([^\]]+)\]\]/g, '$2')
    .replace(/\*\*(.+?)\*\*/g, '$1')
    .replace(/\*(.+?)\*/g, '$1')
    .replace(/\s+/g, ' ')
    .trim()
  return stripped.slice(0, 220) + (stripped.length > 220 ? '…' : '')
}

// ── Inline wikitext renderer for article bodies ─────────────────────────────

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
    return content.split(/\n{2,}/).map(p => p.trim()).filter(Boolean)
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

// ── Case drawer ─────────────────────────────────────────────────────────────

function CaseDrawer({ id, onClose, onNavigate }) {
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

  // Extract the "*filed DATE, ..." meta line if present so we can show it
  // in the header rather than inside the article body.
  const metaLine = page && page.content
    ? (page.content.split('\n').find(l => l.trim().startsWith('*filed ')) || '').replace(/^\*|\*$/g, '').trim()
    : ''

  return (
    <>
      <div className="drawer-overlay" onClick={onClose} />
      <aside className="drawer" role="dialog" aria-modal="true">
        <header className="drawer-header">
          <div>
            {page && <div className="drawer-title">{page.title}</div>}
            {page && metaLine && <div className="drawer-sub">{metaLine}</div>}
            {page && (
              <div className="drawer-sub">
                <span>{page.links_to.length} outgoing citation{page.links_to.length === 1 ? '' : 's'}</span>
                <span className="drawer-sub-dot">·</span>
                <span>{page.token_cost.toLocaleString()} tokens</span>
              </div>
            )}
            {loading && <div className="drawer-title" style={{ color: 'var(--text-dim)' }}>Loading…</div>}
          </div>
          <button className="drawer-close" onClick={onClose} aria-label="Close">
            <Icon name="close" />
          </button>
        </header>
        <div className="drawer-body">
          <NeighborhoodGraph centerId={id} onNavigate={onNavigate} />
          {page && (
            <ArticleBody
              content={page.content.replace(/^\*filed [^\n]*\*\n?/m, '')}
              onLink={onNavigate}
            />
          )}
          {loading && <SkeletonLines count={8} />}
        </div>
      </aside>
    </>
  )
}

// ── Skeletons ───────────────────────────────────────────────────────────────

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

// ── Result card ─────────────────────────────────────────────────────────────

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
          <Icon name="description" /> {page.token_cost.toLocaleString()} tokens
        </span>
        <span className="search-result-item" style={{ flex: 1 }}>
          <Icon name="link" />
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

// ── Landmark card (landing state) ───────────────────────────────────────────

function LandmarkCard({ landmark, onOpen, index }) {
  return (
    <button
      className="landmark-card"
      onClick={() => onOpen(landmark.id)}
      style={{ animationDelay: `${Math.min(index, 10) * 40}ms` }}
    >
      <div className="landmark-card-era">{landmark.era}</div>
      <div className="landmark-card-title">{landmark.title}</div>
      <div className="landmark-card-blurb">{landmark.blurb}</div>
      <div className="landmark-card-cta">
        Read opinion <Icon name="arrow_forward" />
      </div>
    </button>
  )
}

// ── Main view ───────────────────────────────────────────────────────────────

export default function ExplorerView() {
  const [query, setQuery] = useState('')
  const [submitted, setSubmitted] = useState('')
  const [results, setResults] = useState(null)
  const [snippets, setSnippets] = useState({})
  const [loading, setLoading] = useState(false)
  const [openId, setOpenId] = useState(null)
  const inputRef = useRef(null)

  // '/' focuses the search input (classic search-UI shortcut).
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
    r.pages.slice(0, 12).forEach(p => {
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

  const onHint = text => {
    setQuery(text)
    runQuery(text)
  }

  const clearResults = () => {
    setResults(null)
    setSubmitted('')
    setQuery('')
    setSnippets({})
    inputRef.current?.focus()
  }

  const maxLinks = results ? Math.max(1, ...results.pages.map(p => p.links_to.length)) : 1

  return (
    <div className="explorer-view">

      {/* Hero search */}
      <form className="search-hero" onSubmit={onSubmit}>
        <div className="search-hero-row">
          <Icon name="search" />
          <input
            ref={inputRef}
            className="search-hero-input"
            placeholder="Search the opinions — e.g. “commerce clause”, “search and seizure”"
            value={query}
            onChange={e => setQuery(e.target.value)}
            aria-label="Search query"
            autoFocus
          />
          {query && (
            <button
              type="button"
              className="search-hero-clear"
              onClick={clearResults}
              aria-label="Clear"
            ><Icon name="close" /></button>
          )}
          <button className="search-hero-submit" disabled={loading || !query.trim()}>
            <Icon name="bolt" />
            Search
          </button>
        </div>
        <div className="search-shortcut-hint">
          Press <kbd>/</kbd> to focus · <kbd>Esc</kbd> to close an opinion
        </div>
      </form>

      {/* Landing content: landmark cards + doctrine hints */}
      {!submitted && (
        <>
          <section className="landmark-section">
            <div className="search-section-title">
              <Icon name="star" /> Landmark cases to start with
            </div>
            <div className="landmark-grid">
              {LANDMARKS.map((l, i) => (
                <LandmarkCard
                  key={l.id}
                  landmark={l}
                  index={i}
                  onOpen={setOpenId}
                />
              ))}
            </div>
          </section>

          <section>
            <div className="search-section-title">
              <Icon name="auto_awesome" /> Or try a doctrine
            </div>
            <div className="search-hints">
              {DOCTRINE_HINTS.map(h => (
                <button
                  key={h}
                  className="search-hint"
                  onClick={() => onHint(h)}
                >
                  <Icon name="arrow_outward" />
                  {h}
                </button>
              ))}
            </div>
          </section>
        </>
      )}

      {/* Query metrics */}
      {results && (
        <div className="search-metrics">
          <span className="search-metric-item">
            <Icon name="search" />
            <strong>{results.pages.length}</strong> results for{' '}
            <em style={{ marginLeft: 4 }}>{submitted}</em>
          </span>
          <span className="search-metric-item">
            <Icon name="timer" />
            <strong>{(results.elapsed_us / 1000).toFixed(1)}</strong> ms
          </span>
          <span className="search-metric-item">
            <Icon name="autorenew" />
            <strong>{results.iterations}</strong> iter
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
          <div>No opinions activated above threshold.</div>
          <div style={{ opacity: 0.6, marginTop: 4 }}>Try a broader doctrine or add a term.</div>
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

      {/* Case drawer */}
      {openId && (
        <CaseDrawer
          id={openId}
          onClose={() => setOpenId(null)}
          onNavigate={setOpenId}
        />
      )}
    </div>
  )
}
