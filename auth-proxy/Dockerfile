FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev gcc g++ cmake make
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY auth-proxy/Cargo.toml auth-proxy/Cargo.toml
RUN mkdir -p src auth-proxy/src && \
    echo 'fn main() {}' > src/main.rs && \
    echo 'fn main() {}' > auth-proxy/src/main.rs && \
    cargo build --release -p auth-proxy

COPY auth-proxy/src/ auth-proxy/src/
RUN touch auth-proxy/src/main.rs && cargo build --release -p auth-proxy

FROM alpine:3.20
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/auth-proxy /usr/local/bin/auth-proxy
WORKDIR /app
EXPOSE 9080
CMD ["auth-proxy"]
