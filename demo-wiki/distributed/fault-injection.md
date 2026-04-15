# Fault Injection

Deliberately introducing failures into a system to test its resilience. If you haven't tested it broken, you don't know if it works.

Approaches:
- Chaos engineering (Netflix Chaos Monkey): kill random processes in production
- Network partition simulation (toxiproxy, tc netem)
- Deterministic simulation (TigerBeetle's VOPR): replay exact failure sequences
- Fuzz testing: random malformed inputs

The VOPR approach is what we used for the Co-Wiki gauntlet tests. A seeded PRNG drives a state machine through random operations -- create pages, edit content, corrupt weights, run REM cycles -- checking invariants after every step. If it fails on seed 0xDEAD, you replay that exact sequence.

This connects beautifully to [[security/attack-surface-mapping]]. Fault injection is offense applied to your own system. The attack surface map tells you where to inject; the fault injection tells you what breaks.

Also related to [[distributed/eventual-consistency]] (what happens when a replica receives a stale write during a partition?) and [[distributed/consensus-protocols]] (Jepsen is essentially fault injection for consensus implementations).

The gauntlet ran 22,500 chaos operations on the spreading activation primitives. Every invariant held. That's not a proof -- but 22,500 failed attempts to break it is strong evidence.

See [[cognitive/memory-consolidation]] for why the REM agent needs to be tested under chaos (it modifies the graph while queries may be in flight).
