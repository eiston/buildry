# syntax=docker/dockerfile:1
FROM rust:1-trixie AS build
ARG TARGETARCH
ARG DIOXUS_VERSION=0.7.10
RUN apt-get update && apt-get install -y --no-install-recommends clang lld \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add wasm32-unknown-unknown
RUN case "$TARGETARCH" in \
      arm64) arch=aarch64 ;; \
      amd64) arch=x86_64 ;; \
      *) echo "Unsupported architecture: $TARGETARCH"; exit 1 ;; \
    esac \
    && curl -fsSL "https://github.com/DioxusLabs/dioxus/releases/download/v${DIOXUS_VERSION}/dx-${arch}-unknown-linux-gnu.tar.gz" \
       | tar -xz -C /usr/local/bin dx
WORKDIR /app
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    if [ -d target/dx/buildry-property-catalog/release/web/public ]; then find target/dx/buildry-property-catalog/release/web/public -mindepth 1 -delete; fi \
    && dx build --web --release --locked && mkdir -p /out && cp -a target/dx/buildry-property-catalog/release/web/public /out/public
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/server/target \
    cargo build --manifest-path server/Cargo.toml --release --locked && cp server/target/release/buildry-property-api /out/buildry-property-api

FROM build AS test
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/server/target \
    cargo test --manifest-path server/Cargo.toml --release --locked

FROM debian:trixie-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 app
WORKDIR /app/server
COPY --from=build /out/buildry-property-api /usr/local/bin/buildry-property-api
COPY --from=build /out/public /app/public
COPY --from=build /app/assets /app/assets
ENV APP_BIND_ADDR=0.0.0.0:8080 APP_DIST=/app/public
USER app
EXPOSE 8080
ENTRYPOINT ["buildry-property-api"]
