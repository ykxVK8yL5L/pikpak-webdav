FROM rust:alpine3.14 as builder
WORKDIR app
COPY . .
RUN cargo build --release --bin app

FROM alpine:3.14 as runtime
COPY --from=builder /app/target/release/app /usr/local/bin
ENTRYPOINT ["./usr/local/bin/app"]
