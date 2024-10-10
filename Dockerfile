FROM rust:1.81.0 as builder
WORKDIR /app
COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm src/main.rs
COPY src src
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
COPY --from=builder /app/target/release/masterserver /usr/local/bin/masterserver
EXPOSE 8080
EXPOSE 8081
CMD ["masterserver"]