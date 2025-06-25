FROM rust:1.87-slim-bookworm AS builder

WORKDIR /docker
COPY . .
RUN cargo build --release
RUN cp ./target/release/cavalier-backend /

FROM debian:bookworm-slim AS final
COPY --from=builder /docker/target/release/cavalier-backend /usr/bin/
ENV RUST_LOG="cavalier_backend=debug,info"
ENTRYPOINT ["/usr/bin/cavalier-backend"]
EXPOSE 80/tcp
