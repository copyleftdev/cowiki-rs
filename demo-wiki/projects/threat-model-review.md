# Threat Model Review (Q2 2026)

Current project: reviewing the threat model for the internal platform team. Due by end of April.

Status: in progress. Completed the [[security/attack-surface-mapping]], working through [[security/threat-modeling]] for each entry point.

Findings so far:
1. The internal API surface is larger than documented. 14 endpoints missing from the API spec.
2. Three services have no auth on internal-to-internal calls. They rely on network segmentation which is not a security boundary.
3. The npm dependency tree for the admin dashboard includes 847 transitive packages. [[security/sbom-analysis]] flagged 3 with known CVEs, 1 critical.
4. The [[security/supply-chain-security]] posture is weak for the CI/CD pipeline. Build secrets are in environment variables, not a vault.

Next steps:
- Complete STRIDE analysis for the 14 undocumented endpoints
- Review the auth bypass risk with the platform team
- Run [[distributed/fault-injection]] on the auth service to verify it fails closed
- Draft remediation recommendations

This project is teaching me that [[security/attack-surface-mapping]] and [[cognitive/spreading-activation]] are structurally the same problem. You start at an entry point and propagate through connections. Threats spread like activation.
