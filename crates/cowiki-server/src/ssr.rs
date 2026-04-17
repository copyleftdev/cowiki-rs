//! Server-side HTML rendering for crawler-friendly surfaces.
//!
//! The React SPA at `/` is the interactive experience. These routes are
//! what Googlebot, OpenGraph scrapers, and direct link-shares see. Every
//! article gets a dedicated URL with proper `<title>`, meta description,
//! canonical link, Open Graph / Twitter cards, and Schema.org JSON-LD
//! (Article + BreadcrumbList + WebSite with SearchAction + mentions).
//! Internal links are normal `<a>` tags so PageRank-style signals flow.
//!
//! Output is self-contained HTML with inline CSS — no external network
//! calls, no JS required. A "Search in Co-Wiki" CTA links users who land
//! here from a search result back to the SPA.

use std::collections::BTreeMap;
use std::sync::Mutex;

use wiki_backend::types::PageId;
use wiki_backend::WikiBackend;

// ── Small primitives ────────────────────────────────────────────────────────

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// JSON-safe string escape (minimal — content strings only, not structural).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            _ => out.push(c),
        }
    }
    out
}

fn enc(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

/// Strip wikitext/markdown down to a plain readable teaser.
pub fn make_description(content: &str, cap: usize) -> String {
    let mut s = String::with_capacity(cap + 8);
    for line in content.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') { continue; }
        if !s.is_empty() { s.push(' '); }
        s.push_str(l);
        if s.len() >= cap * 2 { break; }
    }
    // Strip wiki/markdown formatting from the teaser.
    let mut cleaned = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '[' if chars.peek() == Some(&'[') => {
                chars.next();
                let mut inner = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc == ']' {
                        chars.next();
                        if chars.peek() == Some(&']') { chars.next(); break; }
                        inner.push(']');
                    } else {
                        inner.push(nc);
                        chars.next();
                    }
                }
                // Prefer display text after '|' if present.
                let display = inner.rsplit_once('|').map(|(_, d)| d).unwrap_or(&inner);
                cleaned.push_str(display);
            }
            '*' => { /* drop bold/italic markers */ }
            _ => cleaned.push(c),
        }
    }
    let cleaned: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() > cap {
        let truncated: String = cleaned.chars().take(cap).collect();
        format!("{truncated}…")
    } else {
        cleaned
    }
}

// ── Markdown → HTML renderer (scoped to our ingested format) ────────────────

fn render_inline(text: &str, corpus: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' && chars.peek() == Some(&'[') {
            chars.next();
            let mut inner = String::new();
            let mut closed = false;
            while let Some(&nc) = chars.peek() {
                if nc == ']' {
                    chars.next();
                    if chars.peek() == Some(&']') {
                        chars.next();
                        closed = true;
                        break;
                    }
                    inner.push(']');
                } else {
                    inner.push(nc);
                    chars.next();
                }
            }
            if !closed {
                out.push_str("[[");
                out.push_str(&html_escape(&inner));
                continue;
            }
            let (target, display) = match inner.split_once('|') {
                Some((t, d)) => (t.trim(), d.trim()),
                None => (inner.as_str(), inner.as_str()),
            };
            out.push_str(&format!(
                r#"<a href="/w/{corpus}/{t}">{d}</a>"#,
                corpus = enc(corpus),
                t = enc(target),
                d = html_escape(display),
            ));
        } else if c == '*' && chars.peek() == Some(&'*') {
            chars.next();
            let mut inner = String::new();
            let mut closed = false;
            while let Some(&nc) = chars.peek() {
                if nc == '*' {
                    chars.next();
                    if chars.peek() == Some(&'*') { chars.next(); closed = true; break; }
                    inner.push('*');
                } else { inner.push(nc); chars.next(); }
            }
            if closed {
                out.push_str("<strong>");
                out.push_str(&html_escape(&inner));
                out.push_str("</strong>");
            } else {
                out.push_str("**");
                out.push_str(&html_escape(&inner));
            }
        } else if c == '*' {
            // Italic
            let mut inner = String::new();
            let mut closed = false;
            while let Some(&nc) = chars.peek() {
                if nc == '*' { chars.next(); closed = true; break; }
                if nc == '\n' { break; }
                inner.push(nc);
                chars.next();
            }
            if closed && !inner.is_empty() {
                out.push_str("<em>");
                out.push_str(&html_escape(&inner));
                out.push_str("</em>");
            } else {
                out.push('*');
                out.push_str(&html_escape(&inner));
            }
        } else if c == '[' {
            // Handle [text](url) link
            let mut text_part = String::new();
            let mut url_part = String::new();
            let mut closed_bracket = false;
            while let Some(&nc) = chars.peek() {
                if nc == ']' { chars.next(); closed_bracket = true; break; }
                if nc == '\n' { break; }
                text_part.push(nc);
                chars.next();
            }
            if closed_bracket && chars.peek() == Some(&'(') {
                chars.next();
                let mut closed_paren = false;
                while let Some(&nc) = chars.peek() {
                    if nc == ')' { chars.next(); closed_paren = true; break; }
                    if nc == '\n' { break; }
                    url_part.push(nc);
                    chars.next();
                }
                if closed_paren {
                    out.push_str(&format!(
                        r#"<a href="{u}" rel="nofollow noopener" target="_blank">{t}</a>"#,
                        u = html_escape(&url_part),
                        t = html_escape(&text_part),
                    ));
                    continue;
                }
            }
            out.push('[');
            out.push_str(&html_escape(&text_part));
            if closed_bracket { out.push(']'); }
        } else {
            // HTML-escape a single character by passing through the escaper.
            out.push_str(&html_escape(&c.to_string()));
        }
    }
    out
}

fn render_body(content: &str, corpus: &str) -> String {
    let mut out = String::new();
    for block in content.split("\n\n") {
        let block = block.trim();
        if block.is_empty() { continue; }

        // Heading
        if let Some(rest) = block.strip_prefix('#') {
            let mut depth = 1;
            let mut rem = rest;
            while let Some(r) = rem.strip_prefix('#') { depth += 1; rem = r; if depth >= 6 { break; } }
            let text = rem.trim();
            let level = depth.clamp(2, 6);  // reserve h1 for page title
            out.push_str(&format!(
                "<h{level}>{}</h{level}>\n",
                render_inline(text, corpus)
            ));
            continue;
        }

        // Bullet list (lines starting with '*')
        if block.lines().all(|l| l.trim_start().starts_with('*')) {
            out.push_str("<ul>\n");
            for line in block.lines() {
                let item = line.trim_start().trim_start_matches('*').trim();
                out.push_str(&format!("  <li>{}</li>\n", render_inline(item, corpus)));
            }
            out.push_str("</ul>\n");
            continue;
        }

        // Paragraph (preserve single \n as <br>).
        let paragraph = block.replace('\n', " ");
        out.push_str(&format!("<p>{}</p>\n", render_inline(&paragraph, corpus)));
    }
    out
}

// ── Shared page chrome ──────────────────────────────────────────────────────

const INLINE_CSS: &str = r#"
:root { --fg:#141722; --fg-dim:#5c6472; --bg:#ffffff; --surface:#f3f5f9;
  --border:#dbe0ea; --accent:#006b91; --accent-bg:rgba(0,107,145,0.08); }
@media (prefers-color-scheme: dark) {
  :root { --fg:#e0e0f0; --fg-dim:#8a8ea8; --bg:#0a0a0f; --surface:#12121a;
    --border:#2a2a3a; --accent:#00d4ff; --accent-bg:rgba(0,212,255,0.1); }
}
* { box-sizing:border-box; }
html,body { margin:0; padding:0; }
body { background:var(--bg); color:var(--fg);
  font-family:'Inter',-apple-system,system-ui,sans-serif;
  line-height:1.65; -webkit-font-smoothing:antialiased; }
.wrap { max-width:780px; margin:0 auto; padding:24px 20px 80px; }
header.site { display:flex; gap:12px; align-items:baseline; justify-content:space-between;
  padding-bottom:12px; border-bottom:1px solid var(--border); margin-bottom:20px; font-size:13px; }
header.site a { color:var(--fg); text-decoration:none; }
header.site nav { color:var(--fg-dim); }
header.site nav a { color:var(--fg); }
header.site nav a:hover { color:var(--accent); }
.cta { background:var(--accent); color:var(--bg); padding:6px 14px; border-radius:16px;
  text-decoration:none; font-size:12px; font-weight:500; }
.cta:hover { filter:brightness(1.1); }
article h1 { font-size:32px; line-height:1.2; margin:0 0 4px; font-weight:600; letter-spacing:-0.01em; }
article .meta { font-size:12px; color:var(--fg-dim); margin-bottom:28px;
  font-family:'JetBrains Mono',ui-monospace,monospace; }
article .meta span + span::before { content:" · "; opacity:0.5; margin:0 2px; }
article h2 { font-size:22px; margin:32px 0 10px; font-weight:600; }
article h3 { font-size:17px; margin:24px 0 8px; font-weight:600; color:var(--accent); }
article h4, article h5, article h6 { font-size:14px; margin:20px 0 6px; text-transform:uppercase;
  letter-spacing:1px; color:var(--fg-dim); }
article p { margin:0 0 14px; font-size:16px; }
article a { color:var(--accent); text-decoration:none; border-bottom:1px dashed transparent; }
article a:hover { border-bottom-color:var(--accent); background:var(--accent-bg); }
article ul { padding-left:22px; }
article ul li { margin-bottom:4px; }
aside.related { margin-top:48px; padding-top:24px; border-top:1px solid var(--border); }
aside.related h2 { font-size:14px; text-transform:uppercase; letter-spacing:1.5px; color:var(--fg-dim); }
aside.related ul { list-style:none; padding:0; display:grid;
  grid-template-columns:repeat(auto-fill,minmax(220px,1fr)); gap:6px 16px; }
aside.related li a { display:block; padding:4px 8px; border-radius:6px;
  font-size:13px; font-family:'JetBrains Mono',ui-monospace,monospace; color:var(--fg); }
aside.related li a:hover { background:var(--accent-bg); color:var(--accent); }
footer.site { margin-top:48px; padding-top:16px; border-top:1px solid var(--border);
  font-size:11px; color:var(--fg-dim); text-align:center; }
.corpus-hero { padding:32px 0; text-align:left; }
.corpus-hero h1 { font-size:40px; margin:0 0 6px; }
.corpus-hero p { font-size:16px; color:var(--fg-dim); }
.corpus-stats { display:flex; gap:24px; margin:24px 0; font-size:13px;
  font-variant-numeric:tabular-nums; font-family:'JetBrains Mono',ui-monospace,monospace; }
.corpus-stats dt { color:var(--fg-dim); font-size:10px; text-transform:uppercase; letter-spacing:1px; }
.corpus-stats dd { margin:2px 0; font-size:18px; font-weight:500; }
.page-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(260px,1fr)); gap:8px; }
.page-grid a { display:block; padding:10px 12px; background:var(--surface); border:1px solid var(--border);
  border-radius:8px; text-decoration:none; color:var(--fg); }
.page-grid a:hover { border-color:var(--accent); }
.page-grid .title { font-size:14px; font-weight:500; color:var(--accent); }
.page-grid .sub { font-size:11px; color:var(--fg-dim); font-family:'JetBrains Mono',ui-monospace,monospace; margin-top:2px; }
"#;

fn site_header_html() -> String {
    r#"<header class="site">
<nav><a href="/">Co-Wiki</a></nav>
<a class="cta" href="/">Search in Co-Wiki</a>
</header>"#.to_string()
}

// ── Article rendering ───────────────────────────────────────────────────────

pub fn render_article(
    backend: &WikiBackend,
    corpus: &str,
    id: &str,
    base_url: &str,
) -> Option<String> {
    let meta = backend.page(&PageId(id.to_string()))?;
    let root = backend.root();
    let content = std::fs::read_to_string(root.join(&meta.path)).ok()?;

    let title_esc = html_escape(&meta.title);
    let corpus_esc = html_escape(corpus);
    let description = make_description(&content, 180);
    let desc_esc = html_escape(&description);
    let canonical = format!("{base_url}/w/{}/{}", enc(corpus), enc(id));
    let search_url = format!("{base_url}/?q={}", enc(&meta.title));

    // JSON-LD: Article + BreadcrumbList + WebSite
    let mentions_json: Vec<String> = meta.links_to.iter().filter_map(|l| {
        let linked = backend.page(l)?;
        Some(format!(
            r#"{{"@type":"Thing","name":"{name}","url":"{url}"}}"#,
            name = json_escape(&linked.title),
            url = format!("{base_url}/w/{}/{}", enc(corpus), enc(&l.0)),
        ))
    }).collect();

    let json_ld = format!(r#"<script type="application/ld+json">{{
  "@context":"https://schema.org",
  "@graph":[
    {{"@type":"WebSite","@id":"{base}/#website","name":"Co-Wiki",
     "description":"Spreading-activation search over curated knowledge graphs",
     "url":"{base}/",
     "potentialAction":{{"@type":"SearchAction",
       "target":{{"@type":"EntryPoint","urlTemplate":"{base}/?q={{search_term_string}}"}},
       "query-input":"required name=search_term_string"}}}},
    {{"@type":"Article","@id":"{canon}#article","headline":"{title}",
     "description":"{desc}","articleSection":"{section}","inLanguage":"en",
     "isPartOf":{{"@id":"{base}/#website"}},
     "mainEntityOfPage":{{"@type":"WebPage","@id":"{canon}"}},
     "url":"{canon}","mentions":[{mentions}]}},
    {{"@type":"BreadcrumbList","itemListElement":[
       {{"@type":"ListItem","position":1,"name":"Co-Wiki","item":"{base}/"}},
       {{"@type":"ListItem","position":2,"name":"{section}","item":"{base}/c/{section_enc}"}},
       {{"@type":"ListItem","position":3,"name":"{title}","item":"{canon}"}}
    ]}}
  ]
}}</script>"#,
        base = base_url,
        canon = canonical,
        title = json_escape(&meta.title),
        desc = json_escape(&description),
        section = json_escape(corpus),
        section_enc = enc(corpus),
        mentions = mentions_json.join(","),
    );

    let body_html = render_body(&content, corpus);
    let related = if meta.links_to.is_empty() { String::new() } else {
        let items: Vec<String> = meta.links_to.iter().filter_map(|l| {
            let p = backend.page(l)?;
            Some(format!(
                r#"<li><a href="/w/{c}/{id}">{t}</a></li>"#,
                c = enc(corpus),
                id = enc(&l.0),
                t = html_escape(&p.title),
            ))
        }).collect();
        format!(r#"<aside class="related">
<h2>Linked from this article</h2>
<ul>{items}</ul>
</aside>"#, items = items.join("\n"))
    };

    Some(format!(r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title_esc} · {corpus_esc} · Co-Wiki</title>
<meta name="description" content="{desc_esc}">
<link rel="canonical" href="{canonical}">
<meta name="robots" content="index, follow, max-image-preview:large, max-snippet:-1">
<meta property="og:type" content="article">
<meta property="og:title" content="{title_esc}">
<meta property="og:description" content="{desc_esc}">
<meta property="og:url" content="{canonical}">
<meta property="og:site_name" content="Co-Wiki">
<meta property="og:locale" content="en_US">
<meta property="article:section" content="{corpus_esc}">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="{title_esc}">
<meta name="twitter:description" content="{desc_esc}">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap">
<style>{css}</style>
{json_ld}
</head>
<body>
<div class="wrap">
{header}
<nav aria-label="Breadcrumb" style="font-size:12px;color:var(--fg-dim);margin-bottom:16px;font-family:'JetBrains Mono',ui-monospace,monospace;">
<a href="/">Home</a> / <a href="/c/{corpus_enc}">{corpus_esc}</a> / {title_esc}
</nav>
<article>
<h1>{title_esc}</h1>
<div class="meta">
<span>{corpus_esc}</span>
<span>{tokens} tokens</span>
<span>{links} outbound links</span>
</div>
{body_html}
{related}
</article>
<footer class="site">
<a href="{search_url}">Search for “{title_esc}” in Co-Wiki →</a>
</footer>
</div>
</body>
</html>"#,
        css = INLINE_CSS,
        json_ld = json_ld,
        header = site_header_html(),
        corpus_enc = enc(corpus),
        tokens = meta.token_cost,
        links = meta.links_to.len(),
    ))
}

// ── Corpus landing page ─────────────────────────────────────────────────────

pub fn render_corpus(backend: &WikiBackend, corpus: &str, base_url: &str) -> String {
    let pages = backend.all_pages();
    let n = pages.len();
    let g = backend.graph();
    let (_, _, values) = g.adj_transpose_csr();
    let edge_count = values.len();
    let density = if n > 1 { edge_count as f64 / (n * (n - 1)) as f64 } else { 0.0 };

    let canonical = format!("{base_url}/c/{}", enc(corpus));
    let corpus_esc = html_escape(corpus);
    let description = format!(
        "{n} articles, {edge_count} backlink edges — spreading-activation search over the {corpus} knowledge slice.",
        n = n, edge_count = edge_count, corpus = corpus,
    );

    // Top pages by outbound link count (hubs first — most navigationally useful)
    let mut ranked: Vec<&_> = pages.iter().collect();
    ranked.sort_by(|a, b| b.links_to.len().cmp(&a.links_to.len()));
    let top: Vec<&_> = ranked.iter().take(60).copied().collect();

    let grid: String = top.iter().map(|p| format!(
        r#"<a href="/w/{c}/{id}">
<div class="title">{t}</div>
<div class="sub">{tokens}t · {links} links</div>
</a>"#,
        c = enc(corpus),
        id = enc(&p.id.0),
        t = html_escape(&p.title),
        tokens = p.token_cost,
        links = p.links_to.len(),
    )).collect::<Vec<_>>().join("\n");

    let json_ld = format!(r#"<script type="application/ld+json">{{
  "@context":"https://schema.org",
  "@graph":[
    {{"@type":"CollectionPage","@id":"{canon}","name":"{name}",
     "description":"{desc}","url":"{canon}","inLanguage":"en",
     "isPartOf":{{"@id":"{base}/#website"}}}},
    {{"@type":"Dataset","name":"{name}",
     "description":"{desc}","url":"{canon}","variableMeasured":[
       {{"@type":"PropertyValue","name":"articles","value":"{n}"}},
       {{"@type":"PropertyValue","name":"backlink-edges","value":"{edges}"}}
     ]}},
    {{"@type":"BreadcrumbList","itemListElement":[
       {{"@type":"ListItem","position":1,"name":"Co-Wiki","item":"{base}/"}},
       {{"@type":"ListItem","position":2,"name":"{name}","item":"{canon}"}}
    ]}}
  ]
}}</script>"#,
        base = base_url,
        canon = canonical,
        name = json_escape(corpus),
        desc = json_escape(&description),
        n = n,
        edges = edge_count,
    );

    format!(r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{corpus_esc} · Co-Wiki</title>
<meta name="description" content="{desc}">
<link rel="canonical" href="{canonical}">
<meta name="robots" content="index, follow">
<meta property="og:type" content="website">
<meta property="og:title" content="{corpus_esc} · Co-Wiki">
<meta property="og:description" content="{desc}">
<meta property="og:url" content="{canonical}">
<meta name="twitter:card" content="summary">
<meta name="twitter:title" content="{corpus_esc} · Co-Wiki">
<meta name="twitter:description" content="{desc}">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap">
<style>{css}</style>
{json_ld}
</head>
<body>
<div class="wrap">
{header}
<section class="corpus-hero">
<h1>{corpus_esc}</h1>
<p>{desc}</p>
<dl class="corpus-stats">
<div><dt>Articles</dt><dd>{n}</dd></div>
<div><dt>Backlink edges</dt><dd>{edge_count}</dd></div>
<div><dt>Density</dt><dd>{density:.3}%</dd></div>
</dl>
</section>
<h2 style="font-size:14px;text-transform:uppercase;letter-spacing:1.5px;color:var(--fg-dim);margin-bottom:12px;">Hubs</h2>
<div class="page-grid">
{grid}
</div>
<footer class="site">
<a href="/?corpus={corpus_enc}">Search {corpus_esc} in Co-Wiki →</a>
</footer>
</div>
</body>
</html>"#,
        desc = html_escape(&description),
        css = INLINE_CSS,
        json_ld = json_ld,
        header = site_header_html(),
        corpus_enc = enc(corpus),
        density = density * 100.0,
    )
}

// ── Sitemap.xml ─────────────────────────────────────────────────────────────

pub fn render_sitemap(
    corpora: &BTreeMap<String, Mutex<WikiBackend>>,
    base_url: &str,
) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n"
    );
    // Root
    out.push_str(&format!(
        "  <url><loc>{base}/</loc><changefreq>weekly</changefreq><priority>1.0</priority></url>\n",
        base = base_url
    ));
    // Corpus landing pages + articles
    for (name, mutex) in corpora {
        let wiki = mutex.lock().unwrap();
        let corpus_enc = enc(name);
        out.push_str(&format!(
            "  <url><loc>{base}/c/{c}</loc><changefreq>weekly</changefreq><priority>0.9</priority></url>\n",
            base = base_url,
            c = corpus_enc,
        ));
        for page in wiki.all_pages() {
            // Articles with many outbound links are hubs — slightly higher priority.
            let prio = (0.5 + (page.links_to.len() as f64 * 0.01).min(0.3)).min(0.9);
            out.push_str(&format!(
                "  <url><loc>{base}/w/{c}/{id}</loc><changefreq>monthly</changefreq><priority>{prio:.2}</priority></url>\n",
                base = base_url,
                c = corpus_enc,
                id = enc(&page.id.0),
            ));
        }
    }
    out.push_str("</urlset>\n");
    out
}

pub fn render_robots(base_url: &str) -> String {
    format!(
        "User-agent: *\n\
         Allow: /\n\
         Disallow: /api/\n\
         \n\
         Sitemap: {base}/sitemap.xml\n",
        base = base_url,
    )
}
