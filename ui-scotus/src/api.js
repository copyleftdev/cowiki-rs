// SCOTUS Explorer — trimmed API surface.
// No stats/perf/maintain/stress/simulate — this is a read-only reader UI.
const API = '/api'

const safe = (promise, fallback) => promise.catch(() => fallback)

export const listPages = ({ limit, order } = {}) => {
  const qs = new URLSearchParams()
  if (limit != null) qs.set('limit', String(limit))
  if (order) qs.set('order', order)
  const q = qs.toString()
  return safe(fetch(`${API}/pages${q ? `?${q}` : ''}`).then(r => r.json()), [])
}

export const getPage = id =>
  safe(fetch(`${API}/pages/${encodeURI(id)}`).then(r => r.ok ? r.json() : null), null)

export const getNeighborhood = id =>
  safe(fetch(`${API}/neighborhood/${encodeURI(id)}`).then(r => r.ok ? r.json() : null), null)

export const queryPages = (query, budget = 6000) =>
  fetch(`${API}/query`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, budget }),
  }).then(r => r.json())

export const getCorpora = () => safe(fetch(`${API}/corpora`).then(r => r.json()), [])

// Defensive pin: when the backend hosts multiple corpora, the Explorer
// UI should always target scotus-top10k regardless of last-active state.
export const selectCorpus = name =>
  fetch(`${API}/corpora/select`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  }).then(r => r.ok)
