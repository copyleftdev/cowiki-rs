# SBOM Analysis

Software Bill of Materials: a complete inventory of every component in your software, including transitive dependencies, their versions, licenses, and known vulnerabilities.

Formats: SPDX, CycloneDX. Tools: syft, trivy, grype for vulnerability matching.

The analysis process:
1. Generate SBOM from build artifacts (not source -- runtime deps may differ)
2. Cross-reference against vulnerability databases (NVD, OSV, GitHub Advisory)
3. Score risk by reachability -- is the vulnerable function actually called?
4. Track license compliance (GPL contamination in proprietary codebases)

SBOM is the structured input to [[security/supply-chain-security]]. Without it you're guessing about your dependency surface. With it you can do [[security/attack-surface-mapping]] at the component level.

The reachability analysis is particularly interesting. A vulnerability in a library you depend on but never call the affected function is lower risk than one in your hot path. This is a graph traversal problem -- similar to [[cognitive/spreading-activation]] but on a call graph instead of a knowledge graph.

Applied to: [[projects/threat-model-review]].
