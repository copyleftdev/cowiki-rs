# Threat Modeling

Structured approach to identifying what can go wrong, what we're doing about it, and whether we've done a good enough job. STRIDE is the classic framework but I prefer attack trees for complex systems.

STRIDE categories:
- Spoofing identity
- Tampering with data
- Repudiation
- Information disclosure
- Denial of service
- Elevation of privilege

The process starts with [[security/attack-surface-mapping]] to identify entry points, then systematically asks "what could an attacker do here?" for each one.

Key insight: threat modeling is a graph problem. Threats connect through attack chains. A credential leak (information disclosure) enables spoofing, which enables tampering. The [[cognitive/spreading-activation]] model is actually useful here -- activate the initial vulnerability and see what becomes reachable.

Currently using this for [[projects/threat-model-review]]. The interesting finding is that [[security/supply-chain-security]] risks propagate through transitive dependencies in ways that look exactly like multi-hop spreading activation.

Related: [[distributed/fault-injection]] (testing the threats you identified), [[security/sbom-analysis]] (dependency-level threat surface).
