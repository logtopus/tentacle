FROM alpine

RUN mkdir /tentacle

WORKDIR /tentacle

ADD ./target/x86_64-unknown-linux-musl/release/tentacle /tentacle/tentacle
ADD conf/docker.yml /tentacle/config.yml

EXPOSE 8081

ENTRYPOINT ["./tentacle", "-c", "config.yml", "-vv"]

CMD []
