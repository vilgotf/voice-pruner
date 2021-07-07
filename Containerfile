FROM rust:alpine as BUILDER

WORKDIR /usr/src/discord-bot
RUN apk --no-cache add \
	musl-dev
COPY Cargo.* ./
COPY src src
RUN cargo install --path .
RUN strip /usr/local/cargo/bin/voice-pruner

FROM scratch

COPY --from=BUILDER /usr/local/cargo/bin/voice-pruner /usr/local/bin/voice-pruner

ENTRYPOINT ["/usr/local/bin/voice-pruner"]
