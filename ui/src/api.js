const API = 'http://localhost:3001/api'

export const listPages = () => fetch(`${API}/pages`).then(r => r.json())
export const getPage = id => fetch(`${API}/pages/${id}`).then(r => r.ok ? r.json() : null)
export const getStats = () => fetch(`${API}/stats`).then(r => r.json())
export const getPerf = () => fetch(`${API}/perf`).then(r => r.json())

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
