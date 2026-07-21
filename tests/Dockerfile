FROM ghcr.io/orange-opensource/hurl:latest
USER root
RUN apk add --no-cache curl bash jq python3 postgresql-client
COPY tests/ /tests/
COPY seed_es.sh /tests/seed_es.sh
COPY benchmark/ /benchmark/
RUN chmod +x /tests/docker-entrypoint.sh /benchmark/run.sh
ENTRYPOINT ["/tests/docker-entrypoint.sh"]
