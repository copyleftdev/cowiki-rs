import { useState, useRef, useEffect } from 'react'
import { startSimulation } from './api'

function HealthRing({ value, size = 80 }) {
  const r = size / 2 - 6, c = 2 * Math.PI * r
  const pct = Math.max(0, Math.min(1, value))
  const color = pct > 0.7 ? 'var(--green)' : pct > 0.3 ? 'var(--amber)' : 'var(--red)'
  return (
    <div style={{ width: size, height: size, position: 'relative', flexShrink: 0 }}>
      <svg width={size} height={size} style={{ transform: 'rotate(-90deg)' }}>
        <circle cx={size/2} cy={size/2} r={r} fill="none" stroke="var(--surface-2)" strokeWidth="5" />
        <circle cx={size/2} cy={size/2} r={r} fill="none" stroke={color} strokeWidth="5"
          strokeDasharray={c} strokeDashoffset={c * (1 - pct)} strokeLinecap="round"
          style={{ transition: 'stroke-dashoffset 0.3s ease' }} />
      </svg>
      <div style={{
        position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%,-50%)',
        fontSize: 14, fontWeight: 300, color, fontVariantNumeric: 'tabular-nums'
      }}>{(pct * 100).toFixed(0)}%</div>
    </div>
  )
}

function Metric({ label, value, unit, color = 'var(--cyan)' }) {
  return (
    <div style={{
      background: 'var(--surface-2)', borderRadius: 6, padding: '8px 10px',
      minWidth: 90,
    }}>
      <div style={{ fontSize: 9, textTransform: 'uppercase', letterSpacing: 1, color: 'var(--text-dim)' }}>{label}</div>
      <div style={{ fontSize: 18, fontWeight: 300, color, fontVariantNumeric: 'tabular-nums' }}>
        {value}<span style={{ fontSize: 10, color: 'var(--text-dim)', marginLeft: 2 }}>{unit}</span>
      </div>
    </div>
  )
}

function EventLine({ event }) {
  const colors = {
    seed: 'var(--purple)',
    query: 'var(--cyan)',
    maintain: 'var(--green)',
    create: 'var(--amber)',
    done: 'var(--text)',
  }
  const color = colors[event.type] || 'var(--text-dim)'

  let detail = ''
  if (event.type === 'seed') detail = `${event.page_count} pages, ${event.edge_count} edges, ${event.elapsed_us}us`
  if (event.type === 'query') detail = `"${event.query}" -> ${event.results} pages, ${event.score.toFixed(3)}, ${event.iterations}iter, ${event.elapsed_us}us`
  if (event.type === 'maintain') detail = `health=${(event.health*100).toFixed(0)}% pruned=${event.pruned} dreamed=${event.dreamed} ${event.elapsed_us}us`
  if (event.type === 'create') detail = `${event.id} (${event.tokens}t, ${event.links}lnk) -> ${event.page_count}pg ${event.edge_count}edg ${event.elapsed_us}us`
  if (event.type === 'done') detail = `${event.total_ops} ops, ${event.final_pages}pg, p50=${event.query_p50_us}us p99=${event.query_p99_us}us`

  return (
    <div style={{
      fontSize: 11, fontFamily: "'JetBrains Mono', monospace",
      padding: '2px 0', borderBottom: '1px solid var(--border)',
      lineHeight: 1.6,
    }}>
      <span style={{ color, fontWeight: 600, width: 60, display: 'inline-block' }}>
        {event.type.toUpperCase()}
      </span>
      <span style={{ color: 'var(--text-dim)' }}>{detail}</span>
    </div>
  )
}

export default function SimulationTab() {
  const [running, setRunning] = useState(false)
  const [events, setEvents] = useState([])
  const [summary, setSummary] = useState(null)
  const [liveStats, setLiveStats] = useState({ pages: 0, edges: 0, health: 1, queries: 0, maintains: 0, creates: 0 })
  const [latencies, setLatencies] = useState([])
  const [pages, setPages] = useState(150)
  const [ops, setOps] = useState(300)
  const logRef = useRef(null)
  const esRef = useRef(null)

  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight
    }
  }, [events])

  const handleStart = () => {
    setRunning(true)
    setEvents([])
    setSummary(null)
    setLiveStats({ pages: 0, edges: 0, health: 1, queries: 0, maintains: 0, creates: 0 })
    setLatencies([])

    esRef.current = startSimulation(pages, ops, (event) => {
      setEvents(prev => [...prev.slice(-500), event])

      if (event.type === 'seed') {
        setLiveStats(s => ({ ...s, pages: event.page_count, edges: event.edge_count }))
      }
      if (event.type === 'query') {
        setLiveStats(s => ({ ...s, queries: s.queries + 1 }))
        setLatencies(prev => [...prev.slice(-200), event.elapsed_us])
      }
      if (event.type === 'maintain') {
        setLiveStats(s => ({ ...s, maintains: s.maintains + 1, health: event.health }))
      }
      if (event.type === 'create') {
        setLiveStats(s => ({ ...s, creates: s.creates + 1, pages: event.page_count, edges: event.edge_count }))
      }
      if (event.type === 'done') {
        setSummary(event)
        setRunning(false)
      }
    })
  }

  const handleStop = () => {
    if (esRef.current) esRef.current.close()
    setRunning(false)
  }

  const avgLatency = latencies.length > 0
    ? (latencies.reduce((a, b) => a + b, 0) / latencies.length).toFixed(0)
    : 0
  const maxLatency = latencies.length > 0 ? Math.max(...latencies) : 1

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 320px', gap: 1, height: '100%', background: 'var(--border)' }}>

      {/* Left: Event log */}
      <div style={{ background: 'var(--surface)', padding: 16, display: 'flex', flexDirection: 'column' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
          <div className="panel-title" style={{ margin: 0 }}>Simulation Log</div>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <label style={{ fontSize: 11, color: 'var(--text-dim)' }}>
              Pages: <input type="number" value={pages} onChange={e => setPages(+e.target.value)}
                style={{ width: 50, background: 'var(--surface-2)', border: '1px solid var(--border)',
                  borderRadius: 4, color: 'var(--text)', padding: '2px 6px', fontSize: 11 }} />
            </label>
            <label style={{ fontSize: 11, color: 'var(--text-dim)' }}>
              Ops: <input type="number" value={ops} onChange={e => setOps(+e.target.value)}
                style={{ width: 50, background: 'var(--surface-2)', border: '1px solid var(--border)',
                  borderRadius: 4, color: 'var(--text)', padding: '2px 6px', fontSize: 11 }} />
            </label>
            {running
              ? <button className="btn" style={{ background: 'var(--red)', color: '#fff', padding: '4px 12px' }} onClick={handleStop}>Stop</button>
              : <button className="btn btn-primary" style={{ padding: '4px 12px' }} onClick={handleStart}>Run Simulation</button>
            }
          </div>
        </div>

        <div ref={logRef} style={{
          flex: 1, overflow: 'auto', background: 'var(--surface-2)',
          borderRadius: 6, padding: 8, border: '1px solid var(--border)',
        }}>
          {events.length === 0 && !running && (
            <div style={{ color: 'var(--text-dim)', fontSize: 12, textAlign: 'center', padding: 40 }}>
              Click "Run Simulation" to generate an ephemeral wiki and watch<br />
              spreading activation, REM cycles, and page creation in real time.
            </div>
          )}
          {events.map((e, i) => <EventLine key={i} event={e} />)}
        </div>
      </div>

      {/* Right: Live telemetry */}
      <div style={{ background: 'var(--surface)', padding: 16, overflow: 'auto' }}>
        <div className="panel-title">Live Telemetry</div>

        {/* Health ring */}
        <div style={{ display: 'flex', justifyContent: 'center', marginBottom: 16 }}>
          <HealthRing value={liveStats.health} size={100} />
        </div>

        {/* Counters */}
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8, marginBottom: 16 }}>
          <Metric label="Pages" value={liveStats.pages} unit="" color="var(--cyan)" />
          <Metric label="Edges" value={liveStats.edges} unit="" color="var(--amber)" />
          <Metric label="Queries" value={liveStats.queries} unit="" color="var(--green)" />
          <Metric label="Avg Latency" value={avgLatency} unit="us" color="var(--cyan)" />
          <Metric label="Maintains" value={liveStats.maintains} unit="" color="var(--purple)" />
          <Metric label="Creates" value={liveStats.creates} unit="" color="var(--amber)" />
        </div>

        {/* Latency sparkline */}
        {latencies.length > 0 && (
          <>
            <div className="panel-title">Query Latency</div>
            <div style={{
              height: 60, background: 'var(--surface-2)', borderRadius: 6,
              padding: '4px 0', display: 'flex', alignItems: 'flex-end', gap: 1,
              overflow: 'hidden', marginBottom: 16,
            }}>
              {latencies.slice(-100).map((us, i) => (
                <div key={i} style={{
                  flex: 1, minWidth: 2,
                  height: `${Math.max(2, (us / maxLatency) * 100)}%`,
                  background: us > maxLatency * 0.8 ? 'var(--red)'
                    : us > maxLatency * 0.5 ? 'var(--amber)' : 'var(--cyan)',
                  borderRadius: '2px 2px 0 0',
                  transition: 'height 0.1s',
                }} />
              ))}
            </div>
          </>
        )}

        {/* Summary */}
        {summary && (
          <>
            <div className="panel-title">Final Results</div>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
              <Metric label="Total Ops" value={summary.total_ops} unit="" color="var(--text)" />
              <Metric label="Total Time" value={(summary.total_us / 1000).toFixed(1)} unit="ms" color="var(--text)" />
              <Metric label="Query p50" value={summary.query_p50_us} unit="us" color="var(--green)" />
              <Metric label="Query p99" value={summary.query_p99_us} unit="us" color="var(--red)" />
              <Metric label="Final Pages" value={summary.final_pages} unit="" color="var(--cyan)" />
              <Metric label="Final Edges" value={summary.final_edges} unit="" color="var(--amber)" />
            </div>
          </>
        )}
      </div>
    </div>
  )
}
