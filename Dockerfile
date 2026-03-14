FROM rust:latest AS builder

WORKDIR /usr/src/app

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev cmake git capnproto capnproto

COPY Cargo.toml ./
COPY src ./src

RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/zero-ads-node /usr/local/bin/zero-ads-node

# Railway provides $PORT
ENV PORT=8080
EXPOSE 8080

CMD ["zero-ads-node"]
