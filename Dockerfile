FROM rust:1.90 as builder

WORKDIR /usr/src/app

# Install latest protoc
RUN PROTOC_VERSION="29.3" && \
    curl -LO "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-x86_64.zip" && \
    unzip "protoc-${PROTOC_VERSION}-linux-x86_64.zip" -d /usr/local && \
    rm "protoc-${PROTOC_VERSION}-linux-x86_64.zip"

RUN rustup target add x86_64-unknown-linux-musl

COPY Cargo.toml Cargo.lock build.rs ./
COPY src src
COPY proto proto

RUN wget https://nyars.moe/static/export/parser.csv
RUN wget https://github.com/daac-tools/vibrato/releases/download/v0.5.0/ipadic-mecab-2_7_0.tar.xz && \
    tar xvf ipadic-mecab-2_7_0.tar.xz --strip-components=1

RUN cargo build --target x86_64-unknown-linux-musl --release

FROM scratch

COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-musl/release/nyars-tokenizer /usr/local/bin/nyars-tokenizer

CMD ["nyars-tokenizer"]