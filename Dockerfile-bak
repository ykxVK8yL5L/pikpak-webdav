FROM rust:alpine3.14 as builder
RUN apk add --no-cache musl-dev
WORKDIR app
COPY . .
RUN cargo build --release

FROM alpine:3.14 as runtime
COPY --from=builder /app/target/release/webdav /usr/local/bin
ENTRYPOINT ["./usr/local/bin/webdav"]
