# Metrics

This project uses [`Prometheus`](https://prometheus.io/) as a monitoring system. To enable it you must build the project with the `metrics` feature enabled:

```sh
# Build florestad and floresta-cli separately
cargo build --release -p florestad --features metrics
cargo build --release -p floresta-cli
```

The metrics endpoint listens on `0.0.0.0` by default, on a port that depends on the network, so nodes running on different networks don't conflict:

| Network  | Default port |
|----------|--------------|
| bitcoin  | 3333         |
| testnet  | 3334         |
| testnet4 | 3335         |
| signet   | 3336         |
| regtest  | 3337         |

You can change the address and port with `--metrics-address <address>[:<port>]`. If you omit the port, the network's default is used:

```sh
florestad --network signet --metrics-address 127.0.0.1:9333
```

The easiest way to visualize those metrics is by using some observability graphic tool like [Grafana](https://grafana.com/). To make it easier, you can also straight away use the `contrib/docker/docker-compose.yml` file to spin up an infrastructure that will run the project with Prometheus and Grafana.

To run it, first make sure you have [Docker Compose](https://docs.docker.com/compose/) installed and then:

```sh
docker compose -f contrib/docker/docker-compose.yml up -d --build
```

Grafana should now be available to you at http://localhost:3000. To log in, please check the credentials defined in the `contrib/docker/docker-compose.yml` file.
