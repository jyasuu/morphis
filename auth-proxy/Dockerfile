FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY target/release/auth-proxy /usr/local/bin/auth-proxy
WORKDIR /app
EXPOSE 9080
CMD ["auth-proxy"]
