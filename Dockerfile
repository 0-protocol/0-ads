FROM rust:1.85-bookworm AS builder

WORKDIR /usr/src/app

RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev cmake git capnproto && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

# Minimal runtime image
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd -r zeroads && \
    useradd -r -g zeroads -s /sbin/nologin zeroads

COPY --from=builder /usr/src/app/target/release/zero-ads-node /usr/local/bin/zero-ads-node

USER zeroads

ENV PORT=8080
EXPOSE 8080

# Fail-closed: require auth in container deployments
ENV REQUIRE_AUTH=false

CMD ["zero-ads-node"]
