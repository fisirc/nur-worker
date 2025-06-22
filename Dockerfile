FROM alpine:3.14

RUN ls
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

ENV PATH=/root/.cargo/bin:$PATH

RUN rustup install nightly
