from ubuntu:20.04

ENV TZ=America/Toronto
RUN ln -snf /usr/share/zoneinfo/$TZ /etc/localtime && echo $TZ > /etc/timezone

# Install rustup
RUN apt-get update && apt-get install -y curl

ENV RUSTUP_HOME /usr/local/rustup
ENV CARGO_HOME /usr/local/cargo
ENV PATH /usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Install dpkg dependencies
RUN apt-get update && apt-get install -y dpkg dpkg-dev
