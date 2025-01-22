FROM python:3.12-alpine3.20 AS builder

ENV VIRTUAL_ENV="/opt/swirlc"
ENV PATH="${VIRTUAL_ENV}/bin:${PATH}"

COPY ./pyproject.toml ./MANIFEST.in ./LICENSE ./README.md /build/
COPY ./requirements.txt           \
     ./bandit-requirements.txt    \
     ./lint-requirements.txt      \
     ./test-requirements.txt      \
     /build/
COPY swirlc /build/swirlc

RUN cd build \
    && python -m venv ${VIRTUAL_ENV} \
    && pip install .

FROM python:3.12-alpine3.20

# Install Rust and compilation dependencies
RUN apk add --no-cache \
    cargo \
    rust \
    gcc \
    musl-dev \
    openssl-dev \
    pkgconfig \
    git \
    make \
    && rm -rf /var/cache/apk/*

LABEL maintainer="Iacopo Colonnelli <iacopo.colonnelli@unito.it>"
LABEL maintainer="Doriana Medić <doriana.medic@unito.it>"
LABEL maintainer="Alberto Mulone <alberto.mulone@unito.it>"

ENV VIRTUAL_ENV="/opt/swirlc"
ENV PATH="${VIRTUAL_ENV}/bin:${PATH}"
# Set Rust env variables for better container compatibility
ENV RUSTFLAGS="-C target-feature=-crt-static"
ENV CARGO_HOME="/usr/local/cargo"
ENV RUST_BACKTRACE=1
ENV PATH="${CARGO_HOME}/bin:${PATH}"

RUN mkdir -p "${CARGO_HOME}" \
    && chmod -R 777 "${CARGO_HOME}"

COPY --from=builder ${VIRTUAL_ENV} ${VIRTUAL_ENV}
COPY --from=builder /build/swirlc/compiler/rust/src /opt/swirlc/lib/python3.12/site-packages/swirlc/compiler/rust/src

CMD ["/bin/sh"]