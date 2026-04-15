const API = '/api'

const safe = (promise, fallback) => promise.catch(() => fallback)

export const listPages = () => safe(fetch(`${API}/pages`).then(r => r.json()), [])
export const getPage = id => safe(fetch(`${API}/pages/${id}`).then(r => r.ok ? r.json() : null), null)
export const getStats = () => safe(fetch(`${API}/stats`).then(r => r.json()), null)
export const getPerf = () => safe(fetch(`${API}/perf`).then(r => r.json()), null)

export const queryPages = (query, budget = 4000) =>
  fetch(`${API}/query`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, budget }),
  }).then(r => r.json())

export const createPage = (id, title, content) =>
  fetch(`${API}/pages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, title, content }),
  }).then(r => r.ok)

export const runMaintain = () =>
  fetch(`${API}/maintain`, { method: 'POST' }).then(r => r.json())

export const runStress = (n = 100, query = 'spreading activation') =>
  fetch(`${API}/stress`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ n, query }),
  }).then(r => r.json())

export function startSimulation(pages = 150, ops = 300, onEvent) {
  const es = new EventSource(`${API}/simulate?pages=${pages}&ops=${ops}`)
  es.onmessage = (e) => {
    const event = JSON.parse(e.data)
    onEvent(event)
    if (event.type === 'done') es.close()
  }
  es.onerror = () => es.close()
  return es
}
