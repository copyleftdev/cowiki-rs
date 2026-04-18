import { useState, useEffect, useMemo, useRef } from 'react'
import { getNeighborhood } from './api'

// ── Layout ──────────────────────────────────────────────────────────────────
// Radial layout with two concentric rings. Positions are committed to the SVG
// `transform` attribute on the OUTER <g>; a nested inner <g> owns the entrance
// animation so the CSS `transform` from @keyframes cannot clobber the position
// (which is what makes nodes "snap" to origin mid-animate).

const W = 580
const H = 460
const CX = W / 2
const CY = H / 2

const DIRECTION_ORDER = { out: 0, both: 1, in: 2, indirect: 3, center: -1 }

const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v))

function ringRadii(oneHopCount, hasTwoHop) {
  // Grow the inner ring with crowd, cap so it stays in viewBox.
  const r1 = clamp(110 + oneHopCount * 1.2, 110, 172)
  const r2 = Math.min(r1 + 58, (Math.min(W, H) / 2) - 24)
  return { r1, r2: hasTwoHop ? r2 : r1 }
}

function nodeRadius(cost, hops) {
  if (hops === 0) return 22
  const base = Math.log2(Math.max(2, cost)) * 1.1
  const cap = hops === 1 ? 11 : 6.5
  const floor = hops === 1 ? 5 : 3
  return clamp(base, floor, cap)
}

function truncateLabel(s, n = 16) {
  return s.length > n ? s.slice(0, n - 1) + '…' : s
}

function computeLayout(nodes) {
  const oneHop = [...nodes.filter(n => n.hops === 1)]
    .sort((a, b) => (DIRECTION_ORDER[a.direction] - DIRECTION_ORDER[b.direction])
      || (b.token_cost - a.token_cost))
  const twoHop = [...nodes.filter(n => n.hops === 2)]
    .sort((a, b) => b.token_cost - a.token_cost)

  const { r1, r2 } = ringRadii(oneHop.length, twoHop.length > 0)
  const positions = new Map()

  if (oneHop.length === 1) {
    positions.set(oneHop[0].id, { x: CX, y: CY - r1, ring: 1 })
  } else {
    oneHop.forEach((n, i) => {
      const angle = -Math.PI / 2 + (i / oneHop.length) * 2 * Math.PI
      positions.set(n.id, {
        x: CX + r1 * Math.cos(angle),
        y: CY + r1 * Math.sin(angle),
        ring: 1,
      })
    })
  }

  // 2-hop: offset a half-step so outer nodes don't line up radially with inner.
  const offset = oneHop.length > 0 ? Math.PI / oneHop.length : 0
  twoHop.forEach((n, i) => {
    const angle = -Math.PI / 2 + offset + (i / Math.max(1, twoHop.length)) * 2 * Math.PI
    positions.set(n.id, {
      x: CX + r2 * Math.cos(angle),
      y: CY + r2 * Math.sin(angle),
      ring: 2,
    })
  })

  return { positions, oneHop, twoHop }
}

// Curved edge path: quadratic bezier bowed toward the center for inner-inner
// edges, away from center for edges that touch the center node.
function edgePath(a, b, touchesCenter) {
  const dx = b.x - a.x
  const dy = b.y - a.y
  const len = Math.hypot(dx, dy) || 1
  const nx = -dy / len
  const ny = dx / len
  const bow = touchesCenter ? len * 0.06 : len * 0.18
  // Bow away from center for non-center edges; straight-ish for center edges.
  const mx = (a.x + b.x) / 2 + nx * bow
  const my = (a.y + b.y) / 2 + ny * bow
  return `M ${a.x} ${a.y} Q ${mx} ${my} ${b.x} ${b.y}`
}

// ── Component ────────────────────────────────────────────────────────────────

export default function NeighborhoodGraph({ centerId, onNavigate }) {
  const [data, setData] = useState(null)
  const [hoverId, setHoverId] = useState(null)
  const [tooltip, setTooltip] = useState(null)
  const wrapRef = useRef(null)
  const loadingRef = useRef(0)

  useEffect(() => {
    const mine = ++loadingRef.current
    setData(null)
    setHoverId(null)
    setTooltip(null)
    getNeighborhood(centerId).then(d => {
      if (loadingRef.current === mine) setData(d)
    })
  }, [centerId])

  const layout = useMemo(() => (data ? computeLayout(data.nodes) : null), [data])

  if (!data) {
    return (
      <div className="graph-panel graph-panel-loading">
        <div className="graph-panel-spinner" />
        <div className="graph-panel-msg">Mapping neighborhood…</div>
      </div>
    )
  }

  const center = data.nodes.find(n => n.hops === 0)
  const { positions, oneHop, twoHop } = layout

  const counts = {
    out: data.nodes.filter(n => n.direction === 'out').length,
    in: data.nodes.filter(n => n.direction === 'in').length,
    both: data.nodes.filter(n => n.direction === 'both').length,
    indirect: data.nodes.filter(n => n.direction === 'indirect').length,
  }

  const edgesToDraw = data.edges.filter(e =>
    positions.has(e.from) && positions.has(e.to))

  // Tooltip uses the node's real bounding rect so it works regardless of
  // viewBox aspect-ratio scaling.
  const handleEnter = (node, evt) => {
    setHoverId(node.id)
    const svgEl = evt.currentTarget.ownerSVGElement
    const nodeRect = evt.currentTarget.getBoundingClientRect()
    const wrapRect = wrapRef.current?.getBoundingClientRect()
    if (!svgEl || !wrapRect) return
    const x = nodeRect.left + nodeRect.width / 2 - wrapRect.left
    const y = nodeRect.top - wrapRect.top
    setTooltip({
      title: node.title, direction: node.direction,
      hops: node.hops, cost: node.token_cost,
      x, y,
    })
  }

  const handleLeave = () => {
    setHoverId(null)
    setTooltip(null)
  }

  return (
    <div className="graph-panel">
      <div className="graph-panel-header">
        <div className="search-section-title" style={{ marginBottom: 0 }}>
          <span className="material-symbols-outlined">hub</span>
          Neighborhood
        </div>
        <div className="graph-legend">
          <span className="graph-legend-item graph-dot-out">
            <span className="graph-legend-dot" /> links to · {counts.out}
          </span>
          <span className="graph-legend-item graph-dot-in">
            <span className="graph-legend-dot" /> links in · {counts.in}
          </span>
          <span className="graph-legend-item graph-dot-both">
            <span className="graph-legend-dot" /> reciprocal · {counts.both}
          </span>
          {counts.indirect > 0 && (
            <span className="graph-legend-item graph-dot-indirect">
              <span className="graph-legend-dot" /> 2-hop · {counts.indirect}
            </span>
          )}
          {data.truncated && (
            <span className="graph-legend-item graph-legend-truncated">
              <span className="material-symbols-outlined">more_horiz</span>
              truncated
            </span>
          )}
        </div>
      </div>

      <div className="graph-panel-svg-wrap" ref={wrapRef}>
        <svg viewBox={`0 0 ${W} ${H}`} className="graph-panel-svg"
             role="img" aria-label="Neighborhood graph"
             preserveAspectRatio="xMidYMid meet">

          {/* concentric reference rings */}
          <circle cx={CX} cy={CY} r={ringRadii(oneHop.length, twoHop.length > 0).r1}
                  className="graph-ring" />
          {twoHop.length > 0 && (
            <circle cx={CX} cy={CY} r={ringRadii(oneHop.length, true).r2}
                    className="graph-ring graph-ring-outer" />
          )}

          {/* edges (curved) */}
          <g className={`graph-edges ${hoverId ? 'graph-edges-dimmed' : ''}`}>
            {edgesToDraw.map((e, i) => {
              const a = positions.get(e.from)
              const b = positions.get(e.to)
              if (!a || !b) return null
              const touchesCenter = e.from === center.id || e.to === center.id
              const isHovered = hoverId && (e.from === hoverId || e.to === hoverId)
              return (
                <path
                  key={i}
                  d={edgePath(a, b, touchesCenter)}
                  className={`graph-edge ${touchesCenter ? 'graph-edge-center' : ''} ${isHovered ? 'graph-edge-active' : ''}`}
                  fill="none"
                />
              )
            })}
          </g>

          {/* nodes: outer <g> owns position, inner <g> owns the animation */}
          <g className="graph-nodes">
            {[...twoHop, ...oneHop].map((n, i) => {
              const p = positions.get(n.id)
              if (!p) return null
              const r = nodeRadius(n.token_cost, n.hops)
              const isHover = hoverId === n.id
              const dim = hoverId && !isHover && hoverId !== center.id
              return (
                <g
                  key={n.id}
                  className={`graph-node graph-node-${n.direction} ${isHover ? 'graph-node-hover' : ''} ${dim ? 'graph-node-dim' : ''}`}
                  transform={`translate(${p.x} ${p.y})`}
                  onMouseEnter={evt => handleEnter(n, evt)}
                  onMouseLeave={handleLeave}
                  onFocus={evt => handleEnter(n, evt)}
                  onBlur={handleLeave}
                  onClick={() => onNavigate(n.id)}
                  tabIndex={0}
                  role="button"
                  aria-label={`${n.title} · ${n.direction} · ${n.hops}-hop`}
                >
                  <g
                    className="graph-node-anim"
                    style={{ animationDelay: `${Math.min(i, 40) * 14}ms` }}
                  >
                    <circle className="graph-node-halo" r={r + 4} />
                    <circle className="graph-node-dot" r={r} />
                  </g>
                </g>
              )
            })}
          </g>

          {/* center */}
          <g className="graph-node graph-node-center"
             transform={`translate(${CX} ${CY})`}>
            <circle className="graph-node-halo" r={nodeRadius(center.token_cost, 0) + 8} />
            <circle className="graph-node-dot" r={nodeRadius(center.token_cost, 0)} />
            <text y={3.5} className="graph-node-center-label">
              {truncateLabel(center.title, 14)}
            </text>
          </g>
        </svg>

        {tooltip && (
          <div
            className="graph-tooltip"
            style={{ left: tooltip.x, top: tooltip.y }}
          >
            <div className="graph-tooltip-title">{tooltip.title}</div>
            <div className="graph-tooltip-meta">
              {tooltip.direction === 'out' && 'outbound →'}
              {tooltip.direction === 'in' && '← inbound'}
              {tooltip.direction === 'both' && 'reciprocal ↔'}
              {tooltip.direction === 'indirect' && '2-hop'}
              <span className="graph-tooltip-dot">·</span>
              {tooltip.cost}t
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
