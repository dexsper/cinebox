# syntax=docker/dockerfile:1

# Builds cinebox-tmdb-proxy from source into a static musl binary on an empty image.
# The context is the repository root; compose.yaml next to this file sets it:
#
#   docker build -f packaging/tmdb-proxy/Dockerfile -t cinebox-tmdb-proxy .

FROM rust:1.95-alpine AS builder

# aws-lc (TLS) builds with cmake and perl, mimalloc with the C toolchain.
RUN apk add --no-cache build-base cmake perl ca-certificates

WORKDIR /src
COPY . .

# The registry and target caches only speed up rebuilds; the binary is copied out of them.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -p cinebox-tmdb-proxy \
    && install -D -m 0755 target/release/cinebox-tmdb-proxy /out/cinebox-tmdb-proxy \
    && mkdir -p /out/cache

FROM scratch

# reqwest checks certificates against the system store; the DoH and system-proxy paths need it too.
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /out/cinebox-tmdb-proxy /cinebox-tmdb-proxy
# A new named volume takes this directory's owner, so the non-root user can write the cache.
COPY --from=builder --chown=65532:65532 /out/cache /cache

USER 65532:65532
ENV LISTEN=0.0.0.0:8080 \
    CACHE_DIR=/cache \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
VOLUME /cache
EXPOSE 8080

# No HEALTHCHECK: scratch has no curl. Probe GET /healthz from outside instead.
ENTRYPOINT ["/cinebox-tmdb-proxy"]
