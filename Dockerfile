FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libpq5 libssl3 && rm -rf /var/lib/apt/lists/*
COPY target/release/morphis /usr/local/bin/morphis
WORKDIR /app
EXPOSE 4000
CMD ["morphis"]
