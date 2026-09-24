# Quickstart

First make sure you have a running version of the Jaeger instance you want to send data to:

```shell
docker run -d -e COLLECTOR_OTLP_ENABLED=true -p16686:16686 -p4317:4317 jaegertracing/all-in-one:latest
```

Launch the servers:

```shell
cargo run --bin example-otel-server1
```

```shell
cargo run --bin example-otel-server2
```

Send a request from the client:

```shell
cargo run --bin example-otel-client
```

Open `http://localhost:16686/` in the browser, you will see the following picture.

![jaeger](jaeger.png)
