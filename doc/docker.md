# Floresta Docker Setup Guide

You can find a [Dockerfile](../Dockerfile) in the root directory of the project, which you can use to build a
docker image for Floresta. We also keep the docker image [dlsz/floresta](https://hub.docker.com/r/dlsz/floresta)
on Docker Hub, which you can pull and run directly.

If you want to run using compose, you may use a simple `docker-compose.yml` file like this:

```yaml
services:
  floresta:
    image: dlsz/floresta:latest
    container_name: Floresta
    command: florestad -c /data/config.toml --data-dir /data/.floresta
    ports:
      - 50001:50001
      - 8332:8332
    volumes:
      - /path/config/floresta.toml:/data/config.toml
      - /path/utreexo:/data/.floresta
    restart: unless-stopped
```

Here's a breakdown of the configuration:
- For the command, there are a couple of options that are worth noticing:
  - `--data-dir /data/.floresta` specifies `florestad`'s data directory. Here, `florestad` will store its blockchain data, wallet files, and other necessary data.

  - `-c /data/config.toml` specifies the path to the configuration file inside the container. By default, Floresta looks for a configuration file at the datadir if no configuration file is specified.
  You should mount a volume to at each path to persist data outside the container.

  - `-n <network>` specifies the Bitcoin network to connect to (mainnet, testnet, testnet4, signet, regtest). Make sure this matches your configuration file.
- The `ports` section maps the container's ports to your host machine. Adjust these as necessary.
  - `50001` is used for Electrum server connections. It may change depending on the network you are using or your configuration.

  - `8332` is used for RPC connections. Adjust this if you have changed the RPC port in your configuration file.

This setup will run Floresta in a Docker container and expose the RPC and Electrum ports, so you can connect to them. After the container is running, you can connect to it using an Electrum wallet or any other compatible client.

To use the RPC via CLI, you can use a command like this:

```bash
docker exec -it Floresta floresta-cli getblockchaininfo
```

## Monitoring

Floresta also (optionally) provides [Prometheus](https://prometheus.io/) metrics endpoint, which you can enable at compile time. If you want a quick setup with Grafana, we provide a [docker-compose.yml](../docker-compose.yml) for that as well. Just use:

```bash
docker compose up -d
```

This will start Floresta on Bitcoin mainnet by default. All blockchain data, metrics, and Grafana configurations are persisted in Docker volumes.

### Running on Different Networks

The provided docker-compose.yml supports running Floresta on different Bitcoin networks using the `NETWORK` environment variable:

```bash
# Run on Signet
NETWORK=signet docker compose up -d

# Run on Testnet
NETWORK=testnet docker compose up -d

# Run on Testnet4
NETWORK=testnet4 docker compose up -d

# Run on Regtest
NETWORK=regtest docker compose up -d
```

The compose setup automatically configures Floresta to listen on fixed ports (`50001` for Electrum, `8332` for RPC, `3333` for metrics) regardless of the network, so you don't need to adjust port mappings when switching networks.

You can also create a `.env` file in the same directory as `docker-compose.yml`:

```bash
NETWORK=signet
```

Then simply run:

```bash
docker compose up -d
```

### Using Local Floresta Data

If you already have Floresta data on your machine (e.g., from running it natively), you can reuse that data instead of starting from scratch:

```bash
# Use your existing ~/.floresta directory
FLORESTA_DATA=$HOME/.floresta docker compose up -d

# Or specify any other path
FLORESTA_DATA=/mnt/ssd/floresta-data docker compose up -d
```

You can also add this to your `.env` file:

```bash
NETWORK=signet
FLORESTA_DATA=/home/youruser/.floresta
```
