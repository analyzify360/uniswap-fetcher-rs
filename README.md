# Uniswap Fetcher

Uniswap Fetcher is a Rust-based library that provides functionalities to fetch and decode Uniswap V3 pool events. It is designed to be used as a Python module using PyO3.

## Features

- Fetch pool events by token pairs within a specified block range.
- Fetch pool events by token pairs within a specified time range.
- Fetch pool created events between two timestamps.
- Get block number range for a given timestamp range.

## Prerequisites

- Rust and Cargo: Install from [rustup.rs](https://rustup.rs/)
- Python 3.10 or higher
- An Ethereum node RPC URL

## Installation

1. Clone the repository:

    ```sh
    git clone https://github.com/yourusername/uniswap-fetcher-rs.git
    cd uniswap-fetcher-rs
    ```

2. Build the Python module:

    ```sh
    maturin develop
    ```

    This will build the Rust code and install the Python module locally.

## Usage

### Python

```python
from uniswap_fetcher_rs import UniswapFetcher

# Initialize the fetcher with the RPC URL
fetcher = UniswapFetcher("http://localhost:8545")

# Define token pairs and block range
token_pairs = [("0xaea46a60368a7bd060eec7df8cba43b7ef41ad85", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", 3000)]
from_block = 12345678
to_block = 12345778

# Fetch pool events by token pairs
events = fetcher.get_pool_events_by_token_pairs(token_pairs, from_block, to_block)
print(events)

# Define time range
start_timestamp = 1633046400  # Example start timestamp
end_timestamp = 1633132800    # Example end timestamp

# Fetch pool data by token pairs within the specified time range
pool_data = fetcher.fetch_pool_data(token_pairs, start_timestamp, end_timestamp)
print(pool_data)

# Fetch pool created events between two timestamps
pool_created_events = fetcher.get_pool_created_events_between_two_timestamps(start_timestamp, end_timestamp)
print(pool_created_events)
```
To use the library directly in Rust, you can call the asynchronous functions provided in the `lib.rs` file.
## Testing
To run the tests, use the following command:
```sh
cargo test
```
## License
This project is licensed under the MIT License. See the [MIT License](LICENSE) file for details.
