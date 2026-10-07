FROM rust:1-slim AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY config ./config
COPY docs.html docs.html
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12
COPY --from=build /build/target/release/xbrowsersync-api-rs /app/xbrowsersync-api-rs
WORKDIR /data
ENV XBSAPI_DB_PATH=/data/db/xbrowsersync.sqlite3
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/app/xbrowsersync-api-rs"]
