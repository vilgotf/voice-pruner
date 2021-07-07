FROM rust:alpine as BUILDER

WORKDIR /usr/src/discord-bot
COPY Cargo.* ./
COPY src src
RUN cargo install --path .
RUN strip /usr/local/cargo/bin/voice-pruner

FROM scratch

COPY --from=builder /usr/local/cargo/bin/voice-pruner /usr/local/bin

ENTRYPOINT ["/usr/local/bin/voice-pruner"]