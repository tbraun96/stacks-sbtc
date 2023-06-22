# Stacks Signer API CLI

This is a CLI application for the Stacks Signer API. It provides an API server for handling transactions and votes. It can generate dummy data for testing,  generate and serve API documentation, and run a Swagger UI server.

## Requirements

- Rust
- SQLite

## Dev/Compilation setup

To make use of `sqlx` and verify the sql queries on your own, you should follow the following steps:

1. remove `sqlx-data.json`
2. install `sqlx-cli` version `0.5.13`. So `cargo install sqlx-cli --version=0.5.13`
3. make sure you have sqlite installed
4. create a `.env` file in the `stacks-signer-api` root folder with the env variable `DATABASE_URL`
5. The url for sqlite is in the format `DATABASE_URL=sqlite://$(pwd)/stacks-signer-api/dev-signer-api.sqlite`
6. generate the test db using `sqlx database create`
7. run the `init` migration `sqlx migrate run`
8. prepare the `offline` static check cache `cargo sqlx prepare -- --lib`

## Installation

1. Clone the repository:

```bash
git clone git@github.com:Trust-Machines/core-eng.git
```

2. Navigate to the project folder:

```bash
cd core-eng/stacks-signer-api
```

3. Build the CLI:

```bash
cargo build --release
```

4. Navigate to the output folder:

```bash
cd target/release
```

## Usage

- To run the API server:

```bash
./stacks-signer-api run --address 0.0.0.0 --port 3030
```

- To serve Swagger UI:

```bash
./stacks-signer-api swagger --address 0.0.0.0 --port 8080
```

- To generate API documentation:

```bash
./stacks-signer-api docs --output api-doc.json
```

- To run the API server with dummy data:

```bash
./stacks-signer-api dummy --address 0.0.0.0 --port 3030
```

## Command-line Arguments

### Run

- `--address` - Address to run the API server on (Default: `0.0.0.0`)
- `--port` - Port to run the API server on (Default: `3030`)

Example:

```bash
./stacks-signer-api run --address 127.0.0.1 --port 8000
```

### Swagger

- `--address` - Address to run the Swagger UI server on (Default: `0.0.0.0`)
- `--port` - Port to run the Swagger UI server on (Default: `3030`)
- `--path` - Path of hosted open api doc file (Default: `/api-docs.json`)

Example:

```bash
./stacks-signer-api swagger --address 127.0.0.1 --port 8000
```

### Docs

- `--output` - Output file to save the API documentation to. If not provided, it prints to stdout.

Example:

```bash
./stacks-signer-api docs --output api-doc.json
```

### Simulator

- `--address` - Address to run the API server with simulated data on (Default:`0.0.0.0`)
- `--port` - Port to run the API server with simulated data on (Default: `3030`)

Example:

```bash
./stacks-signer-api simulator --address 127.0.0.1 --port 8000
```

## License

This project is licensed under [GPLv3](../LICENSE).