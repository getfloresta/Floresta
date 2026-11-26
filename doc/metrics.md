# Metrics

This project uses [`Prometheus`](https://prometheus.io/) as a monitoring system. To enable it you must build the project with the `metrics` feature enabled:

```sh
# Build florestad and floresta-cli separately
cargo build --release -p florestad --features metrics
cargo build --release -p floresta-cli
```

The easiest way to visualize those metrics is by using some observability graphic tool like [Grafana](https://grafana.com/). To make it easier, you can also straight away use the `contrib/docker/docker-compose.yml` file to spin up an infrastructure that will run the project with Prometheus and Grafana.

The metrics server listens on all interfaces using a network-specific default port:

| Network | Default port |
| --- | ---: |
| Bitcoin | 3333 |
| Testnet | 3334 |
| Testnet4 | 3335 |
| Signet | 3336 |
| Regtest | 3337 |

Use `--metrics-port` to override the default. For example, to listen on port `9400`:

```sh
florestad --metrics-port 9400
```

To run it, first make sure you have [Docker Compose](https://docs.docker.com/compose/) installed and then:

```sh
docker compose -f contrib/docker/docker-compose.yml up -d --build
```

Grafana should now be available to you at http://localhost:3000. To log in, please check the credentials defined in the `contrib/docker/docker-compose.yml` file.
