FROM ghcr.io/orange-opensource/hurl:latest
USER root
RUN apk add --no-cache curl bash jq python3 postgresql-client
COPY . /tests/
RUN chmod +x /tests/docker-entrypoint.sh
ENTRYPOINT ["/tests/docker-entrypoint.sh"]
