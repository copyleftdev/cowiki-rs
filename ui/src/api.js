const API = 'http://localhost:3001/api'

export async function listPages() {
  const res = await fetch(`${API}/pages`)
  return res.json()
}

export async function getPage(id) {
  const res = await fetch(`${API}/pages/${id}`)
  if (!res.ok) return null
  return res.json()
}

export async function queryPages(query, budget = 4000) {
  const res = await fetch(`${API}/query`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, budget }),
  })
  return res.json()
}

export async function createPage(id, title, content) {
  const res = await fetch(`${API}/pages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, title, content }),
  })
  return res.ok
}

export async function updatePage(id, content) {
  const res = await fetch(`${API}/pages/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content }),
  })
  return res.ok
}

export async function runMaintain() {
  const res = await fetch(`${API}/maintain`, { method: 'POST' })
  return res.json()
}

export async function getStats() {
  const res = await fetch(`${API}/stats`)
  return res.json()
}
