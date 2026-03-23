FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY . .
COPY open-ontologies /app/open-ontologies
RUN sed -i 's|path = "../open-ontologies"|path = "open-ontologies"|' Cargo.toml
RUN cargo build --release
FROM debian:bookworm-slim
COPY --from=builder /app/target/release/brain-in-the-fish /usr/local/bin/
ENTRYPOINT ["brain-in-the-fish"]
