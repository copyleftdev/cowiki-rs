# Supply Chain Security

Your software is only as secure as its weakest dependency. The SolarWinds attack (2020) proved this at scale -- compromise the build pipeline and you compromise every customer.

Attack vectors:
- Typosquatting (malicious packages with similar names)
- Maintainer account compromise
- Build system injection
- Dependency confusion (private vs public registry)

Mitigation layers:
1. Pin exact versions, verify checksums
2. SBOM generation and monitoring (see [[security/sbom-analysis]])
3. Minimal dependency policy
4. Vendoring critical dependencies
5. Reproducible builds

This connects to [[security/attack-surface-mapping]] in a non-obvious way: every dependency is an extension of your attack surface. A library with 200 transitive deps has a much larger surface than one with 3.

For the Co-Wiki project specifically, the Rust crate ecosystem is relatively safe (cargo's checksum verification, crates.io policies), but the SQLite bundled build in rusqlite is a 250,000-line C codebase compiled into our binary. That's a real trust boundary.

Applied to [[projects/threat-model-review]] -- the most interesting finding was about the npm frontend dependencies. See [[distributed/consensus-protocols]] for an unrelated but structurally similar trust problem.
