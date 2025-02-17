# syntax=docker/dockerfile:1.6

FROM gitea.contc/controlplane/rust-builder:0.2.0 as source
ARG GITVERSION=
WORKDIR /root/source/jaeger-anomaly-detection-engine
COPY --link . /root/source/jaeger-anomaly-detection-engine

FROM source as test
RUN --mount=type=ssh,required=true \
    --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/source/jaeger-anomaly-detection-engine/target \
    RUST_BACKTRACE=full /root/.cargo/bin/cargo test

FROM source as audit
RUN --mount=type=ssh,required=true \
    --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/source/jaeger-anomaly-detection-engine/target \
    /root/.cargo/bin/cargo audit --color=always

FROM source as build-dev
RUN --mount=type=ssh,required=true \
    --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/source/jaeger-anomaly-detection-engine/target \
    /root/.cargo/bin/cargo build --bin jaeger-anomaly-detection-engine \
    && cp target/debug/jaeger-anomaly-detection-engine .

FROM source as build-release
RUN --mount=type=ssh,required=true \
    --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/source/jaeger-anomaly-detection-engine/target \
    /root/.cargo/bin/cargo build --release --bin jaeger-anomaly-detection-engine \
    && cp target/release/jaeger-anomaly-detection-engine .

FROM ubuntu:24.04 as image-dev
COPY --from=build-dev \
    /root/source/jaeger-anomaly-detection-engine/jaeger-anomaly-detection-engine \
    /usr/bin/
EXPOSE 9999
CMD /usr/bin/jaeger-anomaly-detection-engine

FROM ubuntu:24.04 as image-release
COPY --from=build-release \
    /root/source/jaeger-anomaly-detection-engine/jaeger-anomaly-detection-engine \
    /usr/bin/
EXPOSE 9999
CMD /usr/bin/jaeger-anomaly-detection-engine
