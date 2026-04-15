import { useState, useRef, useEffect, useCallback } from 'react'
import { startSimulation } from './api'

// ─── Fixed-size ring buffer (no allocations after init) ──────────────────────

class RingBuffer {
  constructor(capacity) {
    this.buf = new Float64Array(capacity)
    this.cap = capacity
    this.len = 0
    this.head = 0
  }
  push(v) {
    this.buf[this.head] = v
    this.head = (this.head + 1) % this.cap
    if (this.len < this.cap) this.len++
  }
  toArray() {
    if (this.len < this.cap) return Array.from(this.buf.subarray(0, this.len))
    return [
      ...Array.from(this.buf.subarray(this.head, this.cap)),
      ...Array.from(this.buf.subarray(0, this.head)),
    ]
  }
  max() {
    let m = 0
    const n = Math.min(this.len, this.cap)
    for (let i = 0; i < n; i++) if (this.buf[i] > m) m = this.buf[i]
    return m || 1
  }
  avg() {
    if (this.len === 0) return 0
    let s = 0
    const n = Math.min(this.len, this.cap)
    for (let i = 0; i < n; i++) s += this.buf[i]
    return s / n
  }
  clear() { this.len = 0; this.head = 0 }
}

// ─── Pure components ─────────────────────────────────────────────────────────

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
  const m = max || 1
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

// ─── Event feed with virtualized cap ─────────────────────────────────────────

const MAX_LOG_LINES = 200
const EVENT_COLORS = { seed: 'var(--purple)', query: 'var(--cyan)', maintain: 'var(--green)', create: 'var(--amber)', done: 'var(--text)' }

function fmtEvent(e) {
  switch (e.type) {
    case 'seed': return `seeded ${e.page_count} pages, ${e.edge_count} edges`
    case 'query': return `"${e.query}" ${e.results}pg ${e.score.toFixed(3)} ${e.iterations}i ${e.elapsed_us}us`
    case 'maintain': return `health=${(e.health*100).toFixed(0)}% -${e.pruned} +${e.dreamed}dream ${e.elapsed_us}us`
    case 'create': return `${e.id} ${e.page_count}pg ${e.edge_count}edg ${e.elapsed_us}us`
    case 'done': return `${e.total_ops}ops ${e.final_pages}pg p50=${e.query_p50_us}us p99=${e.query_p99_us}us`
    default: return ''
  }
}

function EventFeed({ events }) {
  const ref = useRef(null)
  useEffect(() => { if (ref.current) ref.current.scrollTop = ref.current.scrollHeight }, [events.length])

  return (
    <div ref={ref} style={{
      flex: 1, overflow: 'auto', background: 'var(--surface)',
      borderRadius: 6, border: '1px solid var(--border)', padding: '4px 8px',
      fontFamily: "'JetBrains Mono', monospace", fontSize: 10, lineHeight: 1.7,
    }}>
      {events.map((e, i) => (
        <div key={i} style={{ borderBottom: '1px solid var(--border)', padding: '1px 0' }}>
          <span style={{ color: EVENT_COLORS[e.type], fontWeight: 600, display: 'inline-block', width: 52 }}>
            {e.type.toUpperCase()}
          </span>
          <span style={{ color: 'var(--text-dim)' }}>{fmtEvent(e)}</span>
        </div>
      ))}
    </div>
  )
}

// ─── Main component ──────────────────────────────────────────────────────────

const MAX_PAGES = 500
const MAX_OPS = 5000

export default function SimulationTab() {
  const [running, setRunning] = useState(false)
  const [events, setEvents] = useState([])
  const [summary, setSummary] = useState(null)
  const [pages, setPages] = useState(150)
  const [ops, setOps] = useState(500)
  const [tick, setTick] = useState(0) // render trigger for mutable state
  const esRef = useRef(null)

  // Mutable accumulators (no React state per event)
  const live = useRef({
    pageCount: 0, edgeCount: 0, density: 0,
    health: 1,
    queries: 0, queryTotal: 0,
    maintains: 0, creates: 0,
    pruned: 0, dreamed: 0,
  })
  const latBuf = useRef(new RingBuffer(120))
  const iterBuf = useRef(new RingBuffer(120))
  const healthBuf = useRef(new RingBuffer(50))
  const eventBuf = useRef([])
  const rafId = useRef(null)
  const dirty = useRef(false)

  // Batched render: flush accumulated state to React at most once per frame
  const scheduleRender = useCallback(() => {
    if (dirty.current) return
    dirty.current = true
    rafId.current = requestAnimationFrame(() => {
      dirty.current = false
      setEvents([...eventBuf.current])
      setTick(t => t + 1)
    })
  }, [])

  // Cleanup on unmount or tab switch
  useEffect(() => {
    return () => {
      if (esRef.current) esRef.current.close()
      if (rafId.current) cancelAnimationFrame(rafId.current)
    }
  }, [])

  const handleStart = () => {
    const clampedPages = Math.min(Math.max(pages, 5), MAX_PAGES)
    const clampedOps = Math.min(Math.max(ops, 10), MAX_OPS)
    setPages(clampedPages)
    setOps(clampedOps)

    // Reset all mutable state
    live.current = {
      pageCount: 0, edgeCount: 0, density: 0, health: 1,
      queries: 0, queryTotal: 0, maintains: 0, creates: 0,
      pruned: 0, dreamed: 0,
    }
    latBuf.current.clear()
    iterBuf.current.clear()
    healthBuf.current.clear()
    eventBuf.current = []

    setRunning(true)
    setEvents([])
    setSummary(null)
    setTick(0)

    esRef.current = startSimulation(clampedPages, clampedOps, (event) => {
      // Accumulate into mutable refs (no React state update per event)
      const L = live.current
      switch (event.type) {
        case 'seed':
          L.pageCount = event.page_count
          L.edgeCount = event.edge_count
          L.density = event.density
          break
        case 'query':
          L.queries++
          L.queryTotal += event.elapsed_us
          latBuf.current.push(event.elapsed_us)
          iterBuf.current.push(event.iterations)
          break
        case 'maintain':
          L.maintains++
          L.health = event.health
          L.pruned += event.pruned
          L.dreamed += event.dreamed
          healthBuf.current.push(event.health)
          break
        case 'create':
          L.creates++
          L.pageCount = event.page_count
          L.edgeCount = event.edge_count
          if (L.pageCount > 1) L.density = L.edgeCount / (L.pageCount * (L.pageCount - 1))
          break
        case 'done':
          setSummary(event)
          setRunning(false)
          break
      }

      // Cap the event log
      if (eventBuf.current.length >= MAX_LOG_LINES) {
        eventBuf.current = eventBuf.current.slice(-MAX_LOG_LINES / 2)
      }
      eventBuf.current.push(event)

      scheduleRender()
    })
  }

  const handleStop = () => {
    if (esRef.current) { esRef.current.close(); esRef.current = null }
    setRunning(false)
  }

  // Read from mutable refs for render
  const L = live.current
  const latData = latBuf.current.toArray()
  const iterData = iterBuf.current.toArray()
  const healthData = healthBuf.current.toArray()
  const latMax = latBuf.current.max()
  const iterMax = iterBuf.current.max()
  const queryAvg = L.queries > 0 ? L.queryTotal / L.queries : 0

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
          <input type="number" value={pages} min={5} max={MAX_PAGES}
            onChange={e => setPages(+e.target.value)} style={inputStyle} disabled={running} />
          <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>Operations</span>
          <input type="number" value={ops} min={10} max={MAX_OPS}
            onChange={e => setOps(+e.target.value)} style={inputStyle} disabled={running} />
          <span style={{ fontSize: 10, color: 'var(--text-dim)' }}>max {MAX_PAGES}pg / {MAX_OPS}ops</span>
        </div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          {running && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <div className="mutex-dot busy" />
              <span style={{ fontSize: 11, color: 'var(--amber)', fontVariantNumeric: 'tabular-nums' }}>
                {L.queries + L.maintains + L.creates} / {ops}
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
        <Big label="Pages" value={L.pageCount} unit="" color="var(--cyan)" />
        <Big label="Edges" value={L.edgeCount} unit="" color="var(--amber)" />
        <Big label="Density" value={(L.density * 100).toFixed(1)} unit="%" color="var(--purple)" />
        <Big label="Queries" value={L.queries} unit="" color="var(--green)" />
        <Big label="Avg Latency" value={queryAvg.toFixed(0)} unit="us" color="var(--cyan)" />
        <Big label="Maintains" value={L.maintains} unit="" color="var(--purple)" />
        <Big label="Pruned" value={L.pruned} unit="" color="var(--red)" />
        <Big label="Dreamed" value={L.dreamed} unit="" color="var(--purple)" />
      </div>

      {/* ── Middle: charts + REM ─────────────────────────────── */}
      <div style={{
        display: 'grid', gridTemplateColumns: '1fr 1fr 200px', gap: 1, background: 'var(--border)', flex: '0 0 auto',
      }}>
        <div style={{ background: 'var(--surface)', padding: 12 }}>
          <div className="panel-title">Query Latency</div>
          <Sparkline data={latData} max={latMax} height={70} color="var(--cyan)" />
          {summary && (
            <div style={{ display: 'flex', gap: 16, marginTop: 8, fontSize: 11, fontVariantNumeric: 'tabular-nums' }}>
              <span>p50 <strong style={{ color: 'var(--green)' }}>{summary.query_p50_us}us</strong></span>
              <span>p95 <strong style={{ color: 'var(--amber)' }}>{summary.query_p95_us}us</strong></span>
              <span>p99 <strong style={{ color: 'var(--red)' }}>{summary.query_p99_us}us</strong></span>
              <span>avg <strong style={{ color: 'var(--cyan)' }}>{summary.query_avg_us.toFixed(0)}us</strong></span>
            </div>
          )}
        </div>

        <div style={{ background: 'var(--surface)', padding: 12 }}>
          <div className="panel-title">Convergence (iterations per query)</div>
          <Sparkline data={iterData} max={iterMax} height={70} color="var(--green)" />
          {iterData.length > 0 && (
            <div style={{ display: 'flex', gap: 16, marginTop: 8, fontSize: 11 }}>
              <span>avg <strong style={{ color: 'var(--green)' }}>{iterBuf.current.avg().toFixed(1)}</strong> iter</span>
              <span>max <strong style={{ color: 'var(--amber)' }}>{iterBuf.current.max()}</strong></span>
            </div>
          )}
        </div>

        <div style={{ background: 'var(--surface)', padding: 12, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center' }}>
          <Ring value={L.health} size={80} label="Graph Health" />
          {healthData.length > 1 && (
            <div style={{ width: '100%', marginTop: 8 }}>
              <Sparkline data={healthData} max={1} height={24} color="var(--green)" />
            </div>
          )}
        </div>
      </div>

      {/* ── Bottom: event feed ───────────────────────────────── */}
      <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
        {events.length === 0 && !running ? (
          <div style={{
            flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center',
            color: 'var(--text-dim)', fontSize: 13, background: 'var(--surface)',
          }}>
            Hit "Run Simulation" to generate an ephemeral wiki and stress the full stack
          </div>
        ) : (
          <EventFeed events={events} />
        )}
      </div>
    </div>
  )
}
