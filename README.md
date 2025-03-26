# rust-web3-utils

## EVM RPC Latency Test

A small Rust utility to test the latency of EVM RPC providers, focusing on transaction submission and receipt time.

### Setup

1. Copy `.env.example` to `.env` and fill in your RPC endpoint and private key:
   ```
   cp .env.example .env
   ```

2. Edit the `.env` file with your values:
   ```
   RPC_PROVIDER=https://your-rpc-endpoint
   PRIVATE_KEY_1=your_private_key_without_0x_prefix
   ```

### Running the Test

```
cargo run
```

The application will:
1. Connect to the specified RPC provider
2. Send 10 zero-value transactions to self
3. Measure and report the time from transaction submission to receipt
4. Show block progression data

### Note

This tool sends actual on-chain transactions that require gas. Make sure your wallet has enough funds for gas fees.
