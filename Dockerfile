# Build image

FROM rust:1.47 as build

COPY . .
RUN cargo test --release --all
RUN cargo build --release

# Output image

FROM ubuntu:focal as binary

RUN apt-get update && apt-get install -y ca-certificates
COPY --from=build /target/release/monitorbot /usr/local/bin/
ENV PORT=3001
ENTRYPOINT ["monitorbot"]