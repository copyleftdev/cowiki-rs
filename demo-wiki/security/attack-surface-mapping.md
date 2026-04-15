# Attack Surface Mapping

Systematic enumeration of all points where an adversary can interact with a system. Not just open ports -- includes APIs, file uploads, auth flows, third-party integrations, build pipelines.

The process is essentially graph traversal. You start at the entry points and spread outward through dependencies, trust boundaries, and data flows. Sound familiar? It's [[cognitive/spreading-activation]] applied to security.

Methodology I use:
1. Enumerate external interfaces (HTTP, gRPC, WebSocket, file drop)
2. Map authentication and authorization boundaries
3. Trace data flows through the system (where does user input go?)
4. Identify trust boundaries (what crosses from untrusted to trusted?)
5. Document third-party dependencies and their update cadence

This feeds directly into [[security/threat-modeling]]. The attack surface is the "where," threat modeling is the "what could go wrong."

Currently applying this to [[projects/threat-model-review]]. The internal API surface is larger than expected -- see [[security/supply-chain-security]] for the dependency angle.

Tools: Burp Suite for web, nmap for network, custom scripts for API enumeration. For the dependency graph, [[security/sbom-analysis]] is the structured approach.
