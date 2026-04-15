import { useState, useRef, useEffect } from 'react'
import { startSimulation } from './api'

function Ring({ value, size = 90, label }) {
  const r = size / 2 - 6, c = 2 * Math.PI * r
  const pct = Math.max(0, Math.min(1, value))
  const color = pct > 0.7 ? 'var(--green)' : pct > 0.3 ? 'var(--amber)' : 'var(--red)'
  return (
    <div style={{ textAlign: 'center' }}>
      <div style={{ width: size, height: size, position: 'relative', margin: '0 auto' }}>
        <svg width={size} height={size} style={{ transform: 'rotate(-90deg)' }}>
          <circle cx={size/2} cy={size/2} r={r} fill="none" stroke="var(--surface-2)" strokeWidth="4" />
          <circle cx={size/2} cy={size/2} r={r} fill="none" stroke={color} strokeWidth="4"
            strokeDasharray={c} strokeDashoffset={c * (1 - pct)} strokeLinecap="round"
            style={{ transition: 'stroke-dashoffset 0.2s ease' }} />
        </svg>
        <div style={{
          position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%,-50%)',
          fontSize: 16, fontWeight: 300, color, fontVariantNumeric: 'tabular-nums'
        }}>{(pct * 100).toFixed(0)}%</div>
      </div>
      {label && <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1, color: 'var(--text-dim)', marginTop: 4 }}>{label}</div>}
    </div>
  )
}

function Big({ value, unit, label, color = 'var(--cyan)' }) {
  return (
    <div style={{ background: 'var(--surface)', border: '1px solid var(--border)', borderRadius: 8, padding: '12px 16px', textAlign: 'center' }}>
      <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1.5, color: 'var(--text-dim)', fontWeight: 600 }}>{label}</div>
      <div style={{ fontSize: 28, fontWeight: 300, color, fontVariantNumeric: 'tabular-nums', marginTop: 2 }}>
        {value}<span style={{ fontSize: 12, color: 'var(--text-dim)', marginLeft: 3 }}>{unit}</span>
      </div>
    </div>
  )
}

function Sparkline({ data, max, height = 50, color = 'var(--cyan)' }) {
  const m = max || Math.max(...data, 1)
  return (
    <div style={{
      height, background: 'var(--surface)', borderRadius: 6,
      display: 'flex', alignItems: 'flex-end', gap: 1, overflow: 'hidden',
      border: '1px solid var(--border)', padding: '2px 1px',
    }}>
      {data.map((v, i) => (
        <div key={i} style={{
          flex: 1, minWidth: 1.5,
          height: `${Math.max(2, (v / m) * 100)}%`,
          background: v > m * 0.85 ? 'var(--red)' : v > m * 0.6 ? 'var(--amber)' : color,
          borderRadius: '1px 1px 0 0',
        }} />
      ))}
    </div>
  )
}

function EventFeed({ events }) {
  const ref = useRef(null)
  useEffect(() => { if (ref.current) ref.current.scrollTop = ref.current.scrollHeight }, [events])

  const colors = { seed: 'var(--purple)', query: 'var(--cyan)', maintain: 'var(--green)', create: 'var(--amber)', done: 'var(--text)' }

  const fmt = (e) => {
    if (e.type === 'seed') return `seeded ${e.page_count} pages, ${e.edge_count} edges`
    if (e.type === 'query') return `"${e.query}" ${e.results}pg ${e.score.toFixed(3)} ${e.iterations}i ${e.elapsed_us}us`
    if (e.type === 'maintain') return `health=${(e.health*100).toFixed(0)}% -${e.pruned} +${e.dreamed}dream ${e.elapsed_us}us`
    if (e.type === 'create') return `${e.id} ${e.page_count}pg ${e.edge_count}edg ${e.elapsed_us}us`
    if (e.type === 'done') return `done: ${e.total_ops}ops ${e.final_pages}pg p50=${e.query_p50_us}us p99=${e.query_p99_us}us`
    return ''
  }

  return (
    <div ref={ref} style={{
      flex: 1, overflow: 'auto', background: 'var(--surface)',
      borderRadius: 6, border: '1px solid var(--border)', padding: '4px 8px',
      fontFamily: "'JetBrains Mono', monospace", fontSize: 10, lineHeight: 1.7,
    }}>
      {events.map((e, i) => (
        <div key={i} style={{ borderBottom: '1px solid var(--border)', padding: '1px 0' }}>
          <span style={{ color: colors[e.type], fontWeight: 600, display: 'inline-block', width: 52 }}>{e.type.toUpperCase()}</span>
          <span style={{ color: 'var(--text-dim)' }}>{fmt(e)}</span>
        </div>
      ))}
    </div>
  )
}

export default function SimulationTab() {
  const [running, setRunning] = useState(false)
  const [events, setEvents] = useState([])
  const [summary, setSummary] = useState(null)
  const [pages, setPages] = useState(150)
  const [ops, setOps] = useState(500)
  const esRef = useRef(null)

  // Live accumulators
  const [live, setLive] = useState({
    pageCount: 0, edgeCount: 0, density: 0,
    health: 1, healthHistory: [],
    queries: 0, queryAvg: 0, queryTotal: 0,
    maintains: 0, creates: 0,
    pruned: 0, dreamed: 0,
    latencies: [], iterations: [],
    convergenceRate: 0,
  })

  const handleStart = () => {
    setRunning(true)
    setEvents([])
    setSummary(null)
    setLive({
      pageCount: 0, edgeCount: 0, density: 0,
      health: 1, healthHistory: [],
      queries: 0, queryAvg: 0, queryTotal: 0,
      maintains: 0, creates: 0,
      pruned: 0, dreamed: 0,
      latencies: [], iterations: [],
      convergenceRate: 0,
    })

    esRef.current = startSimulation(pages, ops, (event) => {
      setEvents(prev => [...prev.slice(-300), event])

      setLive(s => {
        const next = { ...s }
        if (event.type === 'seed') {
          next.pageCount = event.page_count
          next.edgeCount = event.edge_count
          next.density = event.density
        }
        if (event.type === 'query') {
          next.queries++
          next.queryTotal += event.elapsed_us
          next.queryAvg = next.queryTotal / next.queries
          next.latencies = [...s.latencies.slice(-150), event.elapsed_us]
          next.iterations = [...s.iterations.slice(-150), event.iterations]
          next.convergenceRate = event.converged ? s.convergenceRate * 0.95 + 0.05 : s.convergenceRate * 0.95
        }
        if (event.type === 'maintain') {
          next.maintains++
          next.health = event.health
          next.healthHistory = [...s.healthHistory.slice(-50), event.health]
          next.pruned += event.pruned
          next.dreamed += event.dreamed
        }
        if (event.type === 'create') {
          next.creates++
          next.pageCount = event.page_count
          next.edgeCount = event.edge_count
          if (next.pageCount > 1) next.density = next.edgeCount / (next.pageCount * (next.pageCount - 1))
        }
        if (event.type === 'done') {
          setSummary(event)
          setRunning(false)
        }
        return next
      })
    })
  }

  const handleStop = () => {
    if (esRef.current) esRef.current.close()
    setRunning(false)
  }

  const maxLat = live.latencies.length > 0 ? Math.max(...live.latencies) : 1
  const maxIter = live.iterations.length > 0 ? Math.max(...live.iterations) : 1

  const inputStyle = {
    width: 60, background: 'var(--surface-2)', border: '1px solid var(--border)',
    borderRadius: 4, color: 'var(--text)', padding: '4px 8px', fontSize: 12, fontFamily: 'inherit',
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--bg)', gap: 1 }}>

      {/* ── Control bar ──────────────────────────────────────── */}
      <div style={{
        background: 'var(--surface)', padding: '8px 16px',
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      }}>
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>Seed pages</span>
          <input type="number" value={pages} onChange={e => setPages(+e.target.value)} style={inputStyle} />
          <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>Operations</span>
          <input type="number" value={ops} onChange={e => setOps(+e.target.value)} style={inputStyle} />
        </div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          {running && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <div className="mutex-dot busy" />
              <span style={{ fontSize: 11, color: 'var(--amber)' }}>
                {live.queries + live.maintains + live.creates} / {ops}
              </span>
            </div>
          )}
          {running
            ? <button className="btn" style={{ background: 'var(--red)', color: '#fff' }} onClick={handleStop}>Stop</button>
            : <button className="btn btn-primary" onClick={handleStart}>Run Simulation</button>
          }
        </div>
      </div>

      {/* ── Top counters ─────────────────────────────────────── */}
      <div style={{
        display: 'grid', gridTemplateColumns: 'repeat(8, 1fr)', gap: 1, background: 'var(--border)',
      }}>
        <Big label="Pages" value={live.pageCount} unit="" color="var(--cyan)" />
        <Big label="Edges" value={live.edgeCount} unit="" color="var(--amber)" />
        <Big label="Density" value={(live.density * 100).toFixed(1)} unit="%" color="var(--purple)" />
        <Big label="Queries" value={live.queries} unit="" color="var(--green)" />
        <Big label="Avg Latency" value={live.queryAvg.toFixed(0)} unit="us" color="var(--cyan)" />
        <Big label="Maintains" value={live.maintains} unit="" color="var(--purple)" />
        <Big label="Pruned" value={live.pruned} unit="" color="var(--red)" />
        <Big label="Dreamed" value={live.dreamed} unit="" color="var(--purple)" />
      </div>

      {/* ── Middle: charts + REM ─────────────────────────────── */}
      <div style={{
        display: 'grid', gridTemplateColumns: '1fr 1fr 200px', gap: 1, background: 'var(--border)', flex: '0 0 auto',
      }}>
        {/* Latency chart */}
        <div style={{ background: 'var(--surface)', padding: 12 }}>
          <div className="panel-title">Query Latency</div>
          <Sparkline data={live.latencies.slice(-120)} max={maxLat} height={70} color="var(--cyan)" />
          {summary && (
            <div style={{ display: 'flex', gap: 16, marginTop: 8, fontSize: 11, fontVariantNumeric: 'tabular-nums' }}>
              <span>p50 <strong style={{ color: 'var(--green)' }}>{summary.query_p50_us}us</strong></span>
              <span>p95 <strong style={{ color: 'var(--amber)' }}>{summary.query_p95_us}us</strong></span>
              <span>p99 <strong style={{ color: 'var(--red)' }}>{summary.query_p99_us}us</strong></span>
              <span>avg <strong style={{ color: 'var(--cyan)' }}>{summary.query_avg_us.toFixed(0)}us</strong></span>
            </div>
          )}
        </div>

        {/* Convergence chart */}
        <div style={{ background: 'var(--surface)', padding: 12 }}>
          <div className="panel-title">Convergence (iterations per query)</div>
          <Sparkline data={live.iterations.slice(-120)} max={maxIter} height={70} color="var(--green)" />
          {live.iterations.length > 0 && (
            <div style={{ display: 'flex', gap: 16, marginTop: 8, fontSize: 11 }}>
              <span>avg <strong style={{ color: 'var(--green)' }}>
                {(live.iterations.reduce((a,b)=>a+b,0)/live.iterations.length).toFixed(1)}
              </strong> iter</span>
              <span>max <strong style={{ color: 'var(--amber)' }}>{Math.max(...live.iterations)}</strong></span>
            </div>
          )}
        </div>

        {/* REM Agent */}
        <div style={{ background: 'var(--surface)', padding: 12, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center' }}>
          <Ring value={live.health} size={80} label="Graph Health" />
          {live.healthHistory.length > 1 && (
            <div style={{ width: '100%', marginTop: 8 }}>
              <Sparkline data={live.healthHistory} max={1} height={24} color="var(--green)" />
            </div>
          )}
        </div>
      </div>

      {/* ── Bottom: event feed ───────────────────────────────── */}
      <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', padding: '0 0 0 0' }}>
        <EventFeed events={events} />
      </div>
    </div>
  )
}
