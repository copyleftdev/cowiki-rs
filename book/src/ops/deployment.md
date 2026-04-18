# Deployment

Two documented deployment patterns. Both work; they trade cost
against operational simplicity.

## Pattern A — DigitalOcean droplet (~$48/mo)

Single VM, systemd unit, Caddy in front for TLS.

```text
 Internet ─► Caddy :443 ─► cowiki-server :3002
            (auto Let's Encrypt)     (--read-only)
```

- `/opt/scotus/bin/cowiki-server` — binary deployed via rsync.
- `/opt/scotus/corpus/` — markdown files + `.cowiki/` sidecars.
- `/opt/scotus/ui/` — Vite build output.
- systemd unit with `MemoryMax=6G`, `ProtectSystem=strict`,
  `NoNewPrivileges`.
- Caddy enforces `Cache-Control` and blocks public POSTs to
  `/api/maintain`, `/api/pages`, `/api/corpora/select`.

See `deploy/scotus-explorer.service` and `deploy/scotus.Caddyfile`
in the repo (gitignored — contains operator-specific paths).

## Pattern B — DigitalOcean App Platform (~$98/mo)

Containerized. `scotus.cowiki.tech` currently runs on this path.

```text
 Internet ─► Cloudflare (DO's tenant) ─► App Platform container
            (auto TLS)                  (apps-d-2vcpu-8gb)
```

- Dockerfile in `deploy/Dockerfile.scotus` bakes server + UI +
  corpus + warmed `.cowiki/` cache.
- Image push to DOCR triggers auto-redeploy via spec
  `deploy_on_push: enabled: true`.
- TLS provisioned automatically by DO via their CF tenancy.
- No edge control — server-side `--read-only` and cache-control
  middleware are necessary.

## Sizing

| axis | measurement | implication |
|---|---|---|
| RSS at rest, 10k corpus | ~3.4 GB | need ≥8 GB plan (4 GB too tight) |
| Cold boot (markdown rescan) | 18 s | show a splash or pre-warm in CI |
| Warm boot (from `.cowiki/`) | 3 s | pre-warm is worth the extra rsync step |
| Per-query CPU | ~3 ms p50 | single-core serves ~300 qps; 2-vcpu plan fine |
| UI bundle gzipped | ~66 KB | browser-cached for 1 year via cache-control |
| Egress per page view | ~30 KB | 1 TB bandwidth ≈ 30M views |

## Security posture

- `--read-only` flag on the server process blocks write
  endpoints at the Rust layer. Edge-level block is belt-and-
  braces; server-level block is authoritative.
- Corpus data is public (SCOTUS opinions are public domain); no
  auth needed for the read surface.
- No user-supplied file uploads, no SQL injection surface (the
  SQLite use is all parameterized).
- CORS is permissive (`CorsLayer::permissive()`). Tighten if
  embedding in authenticated contexts.

## Redeploy loop

App Platform:

```sh
# rebuild + push
docker build --platform linux/amd64 \
  -f deploy/Dockerfile.scotus \
  -t registry.digitalocean.com/<registry>/scotus-explorer:latest .
docker push registry.digitalocean.com/<registry>/scotus-explorer:latest
# App Platform pulls + redeploys automatically (~3 minutes)
```

Droplet:

```sh
deploy/deploy-scotus.sh
```

Both typical round-trip time: ~5 minutes local build + ~3 minutes
deploy.

## Observability

- `/api/perf` — atomic counters; poll at 1-minute cadence for
  metrics.
- `/api/stats` — page/edge count; stable unless corpus changes.
- systemd journal (droplet) or `doctl apps logs` (App Platform) —
  process-level logs including startup banner and error traces.

See [Observability](observability.md) for thresholds and alert
targets.

<!-- TODO: Caddyfile reference, systemd unit reference, App
     Platform spec reference, the Cloudflare-in-front option
     (proxy scotus.cowiki.tech through a CF zone you control to
     get proper edge caching). -->
