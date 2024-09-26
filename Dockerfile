# Build image

FROM rust:latest as build

COPY . .
RUN cargo build --release

# Output image

FROM ubuntu:24.04 as binary

RUN apt-get update && apt-get install -y ca-certificates
COPY --from=build /target/release/monitorbot /usr/local/bin/
ENV MONITORBOT_PORT=80
ENTRYPOINT ["monitorbot"]
