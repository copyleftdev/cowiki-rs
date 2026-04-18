# SSR routes

Server-side-rendered HTML for crawlers, link-sharers, and direct
URL visitors. The SPA at `/` is the interactive surface; the SSR
routes are the canonical-URL surface.

*Skeleton reference — full HTML structure and JSON-LD schemas
planned for the next documentation slice.*

## `GET /w/{corpus}/{*id}`

Article page. Semantic HTML with full OG / Twitter meta,
canonical URL, and a JSON-LD `@graph` containing `WebSite` (with
`SearchAction`), `Article` (with `mentions` for every linked
page), and `BreadcrumbList`.

Body: `<article>` with the page's markdown rendered, including
`[[link]]` references as real `<a href>` so PageRank-style signals
flow between articles.

Used by: search engines indexing the corpus, social-media link
previews, direct URL shares.

## `GET /c/{corpus}`

Corpus landing page. `CollectionPage` + `Dataset` JSON-LD, page
stats, and the top ~60 hub articles as a grid of `<a>` tags.

## `GET /sitemap.xml`

Generated from all loaded corpora. Article priority scales with
outbound link count (hubs get more weight).

## `GET /robots.txt`

Allows all user agents. Disallows `/api/`. Points at
`/sitemap.xml`.

## Base URL derivation

Per-request from `Host` header + `X-Forwarded-Proto`. Canonical
URLs and JSON-LD entries use this so a deployment behind a
proxy gets the public URL, not the internal one.

All path components go through `urlencoding::encode` before
interpolation — case-name slugs containing parens or apostrophes
are common in legal corpora.

## Route ordering

SSR routes are registered *before* the `fallback_service`
(ServeDir) so they take precedence over the SPA at the same
path. Without this ordering, `ServeDir` would intercept
`/w/scotus/...` and return a 404.

<!-- TODO: full HTML template reference, JSON-LD schemas with
     all fields, image/OG metadata, accessibility notes. -->
