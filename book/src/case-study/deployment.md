# Deployment

*Skeleton chapter.*

The story of getting `scotus.cowiki.tech` live: build, push,
configure, verify, harden, cache. Four push/deploy cycles to get it
production-correct.

## Planned sections

- **Why App Platform over a droplet** — the trade-off, the cost
  delta, the operator convenience argument.
- **The image** — multi-stage Dockerfile, chown-at-COPY to avoid
  the 1.5 GB layer duplication, size after fixes (2.28 GB
  uncompressed, 630 MB on the wire).
- **DNS, CNAMEs, and the Cloudflare tenancy** — why
  `scotus-explorer-*.ondigitalocean.app` resolves to CF IPs, why
  that doesn't mean *we* control the CF cache, what "Cloudflare
  for SaaS" is doing under DO's hood.
- **Postmortem: the public-write hole** — shipped with
  `POST /api/maintain` returning 200 to unauthenticated callers.
  Detected during verification, fixed with a `--read-only` server
  flag, redeployed.
- **Postmortem: the cache-control `private`** — first-visit 3s on
  the JS bundle. DO's proxy injects `cache-control: private` by
  default; the browser honors our `public, max-age=31536000`
  override once the server emits it explicitly.
- **The deploy script** — local build → warm `.cowiki` →
  `docker push` → `doctl` auto-redeploy via spec's
  `deploy_on_push`.
- **Observability** — `/api/stats`, `/api/perf`, `journalctl`,
  `doctl apps logs`.

<!-- TODO(next slice): write this chapter in full. -->
