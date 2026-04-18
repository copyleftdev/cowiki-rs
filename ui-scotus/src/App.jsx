import { useEffect, useState } from 'react'
import ExplorerView from './ExplorerView'
import { selectCorpus } from './api'

const Icon = ({ name, style }) => (
  <span className="material-symbols-outlined" style={style}>{name}</span>
)

function storedTheme() {
  try {
    const saved = localStorage.getItem('scotus-explorer-theme')
    if (saved === 'light' || saved === 'dark') return saved
  } catch (_e) { /* ignore */ }
  if (typeof window !== 'undefined'
      && window.matchMedia('(prefers-color-scheme: light)').matches) return 'light'
  return 'dark'
}

export default function App() {
  const [theme, setTheme] = useState(storedTheme)

  // Pin the backend's active corpus to scotus-top10k. No-ops if the server
  // only hosts that one corpus.
  useEffect(() => {
    selectCorpus('scotus-top10k').catch(() => {})
  }, [])

  useEffect(() => {
    document.documentElement.dataset.theme = theme
    try { localStorage.setItem('scotus-explorer-theme', theme) } catch (_e) { /* ignore */ }
  }, [theme])

  const toggleTheme = () => setTheme(t => t === 'dark' ? 'light' : 'dark')

  return (
    <div className="explorer-app">
      <header className="explorer-header">
        <div className="explorer-brand">
          <Icon name="gavel" style={{ fontSize: 22, color: 'var(--cyan)' }} />
          <div className="explorer-brand-text">
            <div className="explorer-brand-title">SCOTUS Explorer</div>
            <div className="explorer-brand-sub">
              10,000 most-cited Supreme Court opinions · powered by{' '}
              <a
                href="https://github.com/copyleftdev/cowiki-rs"
                target="_blank"
                rel="noreferrer"
                className="explorer-brand-link"
              >
                cowiki
              </a>
            </div>
          </div>
        </div>
        <div className="explorer-header-actions">
          <button
            className="btn btn-ghost btn-sm"
            onClick={toggleTheme}
            aria-label={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`}
            title={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`}
          >
            <Icon name={theme === 'dark' ? 'light_mode' : 'dark_mode'} />
          </button>
        </div>
      </header>

      <main className="explorer-main">
        <ExplorerView />
      </main>

      <footer className="explorer-footer">
        <span>
          Opinion text and citation graph derived from{' '}
          <a href="https://www.courtlistener.com" target="_blank" rel="noreferrer">CourtListener</a>{' '}
          bulk data. Search is spreading activation over a cluster-level citation graph.
        </span>
      </footer>
    </div>
  )
}
