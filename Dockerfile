FROM rust:1.85-bookworm AS rust-builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release -p cowiki-server

FROM node:22-slim AS ui-builder

WORKDIR /build/ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci
COPY ui/ ./
RUN npx vite build

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /build/target/release/cowiki-server /usr/local/bin/
COPY --from=ui-builder /build/ui/dist /srv/ui
COPY demo-wiki/ /srv/wiki

EXPOSE 3001

CMD ["cowiki-server", "/srv/wiki", "--ui", "/srv/ui"]
