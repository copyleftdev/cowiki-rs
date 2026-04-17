import { useState, useRef, useEffect } from 'react'

// Map topic name → accent token + Material Symbol icon. Unknown topics fall
// through to the default cyan/folder. Keeps the design system in charge of
// the actual color values; we just semantic-tag each corpus.
export const TOPIC_STYLE = {
  'ai':                 { accent: 'cyan',   icon: 'smart_toy',   blurb: 'machine intelligence' },
  'cult-of-done':       { accent: 'amber',  icon: 'task_alt',    blurb: 'shipping discipline' },
  'dark-gamification':  { accent: 'purple', icon: 'casino',      blurb: 'coercive design' },
  'game-theory':        { accent: 'green',  icon: 'extension',   blurb: 'strategic interaction' },
}

const DEFAULT_STYLE = { accent: 'cyan', icon: 'folder', blurb: '' }

export function topicStyle(name) {
  return TOPIC_STYLE[name] || DEFAULT_STYLE
}

function abbr(n) {
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k`
  return `${n}`
}

export default function CorpusSelector({ corpora, activeName, onSelect }) {
  const [open, setOpen] = useState(false)
  const rootRef = useRef(null)

  useEffect(() => {
    if (!open) return
    const close = (e) => {
      if (rootRef.current && !rootRef.current.contains(e.target)) setOpen(false)
    }
    const esc = (e) => { if (e.key === 'Escape') setOpen(false) }
    window.addEventListener('mousedown', close)
    window.addEventListener('keydown', esc)
    return () => {
      window.removeEventListener('mousedown', close)
      window.removeEventListener('keydown', esc)
    }
  }, [open])

  const active = corpora.find(c => c.name === activeName) || corpora[0]
  if (!active) return null
  const activeStyle = topicStyle(active.name)

  const pickOne = (name) => {
    setOpen(false)
    if (name !== activeName) onSelect(name)
  }

  return (
    <div className="corpus-selector" ref={rootRef}>
      <button
        className={`corpus-trigger corpus-accent-${activeStyle.accent}`}
        onClick={() => setOpen(o => !o)}
        aria-expanded={open}
        aria-haspopup="listbox"
      >
        <span className="material-symbols-outlined corpus-trigger-icon">
          {activeStyle.icon}
        </span>
        <span className="corpus-trigger-name">{active.name}</span>
        <span className="corpus-trigger-count">{abbr(active.page_count)}</span>
        <span className="material-symbols-outlined corpus-trigger-chevron">
          {open ? 'expand_less' : 'expand_more'}
        </span>
      </button>

      {open && (
        <div className="corpus-menu" role="listbox">
          <div className="corpus-menu-title">
            Corpora · {corpora.length}
          </div>
          {corpora.map(c => {
            const s = topicStyle(c.name)
            const isActive = c.name === activeName
            return (
              <button
                key={c.name}
                className={`corpus-option corpus-accent-${s.accent} ${isActive ? 'is-active' : ''}`}
                onClick={() => pickOne(c.name)}
                role="option"
                aria-selected={isActive}
              >
                <span className="material-symbols-outlined corpus-option-icon">
                  {s.icon}
                </span>
                <div className="corpus-option-body">
                  <div className="corpus-option-header">
                    <span className="corpus-option-name">{c.name}</span>
                    {isActive && (
                      <span className="material-symbols-outlined corpus-option-check">
                        check_circle
                      </span>
                    )}
                  </div>
                  {s.blurb && <div className="corpus-option-blurb">{s.blurb}</div>}
                  <div className="corpus-option-stats">
                    <span><strong>{abbr(c.page_count)}</strong> pages</span>
                    <span><strong>{abbr(c.edge_count)}</strong> edges</span>
                    <span><strong>{(c.density * 100).toFixed(2)}%</strong> density</span>
                  </div>
                </div>
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
