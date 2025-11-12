# Compound Liquidity Monitor

A Rust-based monitoring tool that tracks available liquidity for Compound Finance markets and sends HTTP notifications when liquidity falls below a configured threshold.

## Features

- Monitor available liquidity (cash) for any Compound cToken market
- Track total borrows and reserves
- Configurable liquidity threshold alerts
- HTTP webhook notifications with detailed market data
- Configurable polling intervals
- JSON-based configuration
- Pre-configured for USDC on Ethereum Mainnet

## Prerequisites

- Rust 1.70+ (install from https://rustup.rs)
- Ethereum RPC endpoint (Alchemy, Infura, or local node)
- A webhook endpoint to receive alerts (optional for testing)

## Installation

```bash
git clone <repository-url>
cd compound-monitor
cargo build --release
```

## Configuration

The monitor uses a `config.json` file for configuration. It supports both **Compound V2** and **Compound V3** protocols.

### Important: Compound V2 vs V3

**Compound V3** (also called Compound III or Comet) is the current version used by https://app.compound.finance.

- **V3 USDC Market**: `0xc3d688B66703497DAA19211EEdff47f25384cdc3` (use this for app.compound.finance)
- **V2 USDC Market**: `0x39AA39c021dfbaE8faC545936693aC917d5E7563` (legacy)

### Quick Start for Compound V3 (Recommended)

1. Copy the example configuration:
```bash
cp config.example.json config.json
```

2. Edit `config.json` with your settings:
```json
{
  "compound_version": "v3",
  "rpc_url": "https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY",
  "market_address": "0xc3d688B66703497DAA19211EEdff47f25384cdc3",
  "market_name": "USDC",
  "webhook_url": "https://your-webhook-endpoint.com/notify",
  "poll_interval_secs": 60,
  "liquidity_threshold": "1000000000000",
  "notification_enabled": true
}
```

### Configuration Parameters

- **compound_version**: Protocol version - `"v2"` or `"v3"` (default: `"v2"`)
  - Use `"v3"` for https://app.compound.finance markets
  - Use `"v2"` for legacy Compound V2 markets
- **rpc_url**: Ethereum RPC endpoint URL (required)
  - Get a free API key from [Alchemy](https://www.alchemy.com/) or [Infura](https://infura.io/)
- **market_address**: Compound contract address to monitor (required)
  - **V3 Markets** (Compound III - Current):
    - USDC: `0xc3d688B66703497DAA19211EEdff47f25384cdc3`
  - **V2 Markets** (Legacy):
    - cUSDC: `0x39AA39c021dfbaE8faC545936693aC917d5E7563`
    - cDAI: `0x5d3a536E4D6DbD6114cc1Ead35777bAB948E3643`
    - cETH: `0x4Ddc2D193948926D02f9B1fE9e1daa0718270ED5`
- **market_name**: Human-readable name for the market (optional)
- **webhook_url**: HTTP endpoint to receive JSON alerts (required)
- **poll_interval_secs**: Seconds between liquidity checks (default: 60)
- **liquidity_threshold**: Minimum liquidity threshold in token base units
  - For USDC (6 decimals): `1000000000000` = 1,000,000 USDC
  - For DAI (18 decimals): `1000000000000000000000000` = 1,000,000 DAI
- **notification_enabled**: Enable/disable webhook notifications (default: true)

## Usage

The tool supports multiple commands:

### 1. Monitor Liquidity (Default)

Monitor available liquidity and receive alerts:

```bash
cargo run --release
# or
cargo run --release -- monitor
```

Enable debug logging:

```bash
RUST_LOG=debug cargo run --release
```

### 2. Supply (Deposit) USDC

Deposit USDC to Compound V3:

```bash
# Supply 10 USDC (10000000 with 6 decimals)
cargo run --release -- supply --amount 10000000 --private-key YOUR_PRIVATE_KEY
```

Or add your private key to `config.json`:

```json
{
  "compound_version": "v3",
  "rpc_url": "...",
  "market_address": "...",
  "private_key": "0xYOUR_PRIVATE_KEY_HERE",
  ...
}
```

Then you can omit the `--private-key` flag:

```bash
cargo run --release -- supply --amount 10000000
```

### 3. Withdraw USDC

Withdraw USDC from Compound V3:

```bash
# Withdraw 5 USDC (5000000 with 6 decimals)
cargo run --release -- withdraw --amount 5000000 --private-key YOUR_PRIVATE_KEY
```

### 4. Check Balance

#### Single Address

Check a specific wallet's balances:

```bash
cargo run --release -- balance --address 0xYourWalletAddress
```

#### Batch Check (Monitor Multiple Addresses)

Monitor multiple addresses at once using `monitor_address.json`:

1. Create `monitor_address.json` (copy from example):
```bash
cp monitor_address.example.json monitor_address.json
```

2. Edit `monitor_address.json` with your addresses:
```json
{
  "addresses": [
    {
      "name": "Main Wallet",
      "address": "0xYourAddress1"
    },
    {
      "name": "Trading Wallet",
      "address": "0xYourAddress2"
    },
    {
      "name": "DeFi Wallet",
      "address": "0xYourAddress3"
    }
  ]
}
```

3. Run balance check without `--address` flag:
```bash
cargo run --release -- balance
```

This will check all addresses in the list and display:
- Name (from your config)
- Address
- Token information
- Wallet balance
- Compound balance

### Important Notes

- **USDC uses 6 decimals**: 1 USDC = 1,000,000 (1 million base units)
- **Keep private keys secure**: Never commit `config.json` with your private key to version control
- **Gas fees**: All transactions require ETH for gas fees
- **Approval**: First supply will require approval transaction (happens automatically)

## Webhook Alert Format

When liquidity falls below the threshold, a POST request is sent to your webhook URL with the following JSON payload:

```json
{
  "market_address": "0x39AA39c021dfbaE8faC545936693aC917d5E7563",
  "market_symbol": "cUSDC",
  "available_liquidity": "950000000000000000000000",
  "total_borrows": "5000000000000000000000000",
  "total_reserves": "100000000000000000000000",
  "threshold": "1000000000000000000000000",
  "timestamp": 1699564800,
  "message": "Available liquidity (950000000000000000000000) is below threshold (1000000000000000000000000)"
}
```

## Example Webhook Server

For testing, you can use a simple webhook server:

```bash
# Using webhook.site (online service)
# Visit https://webhook.site and copy your unique URL

# Or use a local test server (Python)
python3 -m http.server 8080
# Then use webhook_url = "http://localhost:8080"
```

## Common Market Addresses (Ethereum Mainnet)

| Token | Address |
|-------|---------|
| cUSDC | 0x39AA39c021dfbaE8faC545936693aC917d5E7563 |
| cDAI  | 0x5d3a536E4D6DbD6114cc1Ead35777bAB948E3643 |
| cETH  | 0x4Ddc2D193948926D02f9B1fE9e1daa0718270ED5 |
| cUSDT | 0xf650C3d88D12dB855b8bf7D11Be6C55A4e07dCC9 |
| cWBTC | 0xC11b1268C1A384e55C48c2391d8d480264A3A7F4 |

## Troubleshooting

### Invalid RPC URL
Make sure your RPC endpoint is valid and has sufficient rate limits.

### Invalid Market Address
Verify the cToken contract address for your desired market at https://compound.finance/markets

### Liquidity Threshold Format
The threshold should be in the token's base units (with decimals):
- For 6-decimal tokens (USDC, USDT): `1000000000000` = 1,000,000 USDC
- For 18-decimal tokens (DAI, ETH): `1000000000000000000000000` = 1,000,000 DAI

**Default Configuration Note**: The default config is set to monitor USDC with a 1M USDC threshold.

### Webhook Not Receiving Alerts
- Verify your webhook URL is accessible
- Check firewall settings
- Test with a public webhook service like webhook.site

## License

MIT
