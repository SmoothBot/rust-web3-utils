use anyhow::Result;
use dotenv::dotenv;
use ethers::{
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{
        transaction::{eip2718::TypedTransaction, eip1559::Eip1559TransactionRequest}, 
        U256
    },
};
use std::{env, sync::Arc, time::Instant};

// Import our custom middlewares
mod middleware;
use middleware::sync_transaction::SyncTransactionMiddleware;
use middleware::realtime_transaction::RealtimeTransactionMiddleware;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    // Check for arguments from command line
    let args: Vec<String> = std::env::args().collect();
    
    // Default method is rise (eth_sendRawTransactionSync)
    let method = if args.len() > 1 {
        match args[1].as_str() {
            "rise" => "rise",   // use eth_sendRawTransactionSync
            "mega" => "mega",   // use realtime_sendRawTransaction
            _ => "rise"         // default to rise if unrecognized
        }
    } else {
        "rise"  // default to rise if no argument provided
    };
    
    // Optional test name is the second argument if provided
    let test_name = if args.len() > 2 { &args[2] } else { "" };
    
    // Setup connection
    let rpc_url = env::var("RPC_PROVIDER").expect("RPC_PROVIDER must be set");
    let private_key = env::var("PRIVATE_KEY_1").expect("PRIVATE_KEY_1 must be set");
    
    let rpc_url_display = rpc_url.clone();
    let provider = Provider::<Http>::try_from(rpc_url)?;
    let wallet: LocalWallet = private_key.parse()?;
    let wallet_address = wallet.address();
    let chain_id = provider.get_chainid().await?;
    let wallet = wallet.with_chain_id(chain_id.as_u64());
    
    // Create standard ethers middleware
    let client = Arc::new(SignerMiddleware::new(provider, wallet));
    
    // Create both middlewares
    let sync_client = SyncTransactionMiddleware::new(client.clone());
    let realtime_client = RealtimeTransactionMiddleware::new(client.clone());
    
    // Make necessary RPC calls before the transaction
    let default_gas_price = client.get_gas_price().await?;
    let gas_price: U256 = default_gas_price * 3; // Use 3x the default gas price
    
    // Display info
    println!("RPC URL: {}", rpc_url_display);
    println!("Chain ID: {}", chain_id);
    println!("Wallet address: {}", wallet_address);
    println!("Default gas price: {} gwei", default_gas_price.as_u64() / 1_000_000_000);
    println!("Using gas price (3x): {} gwei", gas_price.as_u64() / 1_000_000_000);
    if !test_name.is_empty() {
        println!("Test name: {}", test_name);
    }
    
    if method == "rise" {
        println!("\nSending a single transaction using eth_sendRawTransactionSync...");
    } else {
        println!("\nSending a single transaction using realtime_sendRawTransaction...");
    }
    
    // Get the nonce
    let nonce = client.get_transaction_count(wallet_address, None).await?;
    
    // Create an EIP-1559 transaction
    let max_priority_fee_per_gas = U256::from(1_000_000_000); // 1 gwei
    // Make sure max_fee_per_gas is at least as large as max_priority_fee_per_gas
    let max_fee_per_gas = if gas_price > max_priority_fee_per_gas {
        gas_price
    } else {
        // If gas_price is too low, make max_fee at least 2x the priority fee
        max_priority_fee_per_gas * 2
    };
    
    // Create transaction request with EIP-1559 parameters
    let tx_request = Eip1559TransactionRequest::new()
        .from(wallet_address)
        .to(wallet_address)
        .value(U256::zero())
        .chain_id(chain_id.as_u64())
        .nonce(nonce)
        .gas(21000)
        .max_fee_per_gas(max_fee_per_gas)
        .max_priority_fee_per_gas(max_priority_fee_per_gas);
    
    // Convert to TypedTransaction
    let tx = TypedTransaction::Eip1559(tx_request);
    
    // Sign the transaction
    let signature = client.signer().sign_transaction(&tx).await?;
    
    // Get the properly encoded transaction according to EIP-2718
    let raw_tx = tx.rlp_signed(&signature);
    
    let start = Instant::now();
    
    // Send the raw transaction using the selected method
    let receipt = if method == "rise" {
        // Use sync method (eth_sendRawTransactionSync)
        sync_client.send_raw_transaction_sync(raw_tx).await?
    } else {
        // Use realtime method (realtime_sendRawTransaction)
        realtime_client.send_raw_transaction_realtime(raw_tx).await?
    };
    
    // Measure transaction time
    let tx_duration = start.elapsed();
    println!("TX sent and confirmed in {:?}", tx_duration);
    
    // Print receipt information
    let status_str = if let Some(status) = receipt.status {
        if status.low_u32() == 1 { "SUCCESS" } else { "FAILED" }
    } else {
        "UNKNOWN"
    };
    
    // Get the actual transaction hash from the receipt
    let tx_hash = receipt.transaction_hash;
    
    println!("\n====== TRANSACTION RECEIPT ======");
    println!("Transaction Hash: {}", tx_hash);
    println!("Transaction Status: {}", status_str);
    println!("Block Number: {:?}", receipt.block_number);
    println!("Gas Used: {:?}", receipt.gas_used);
    println!("================================");
    
    // Get block information
    if let Some(block_number) = receipt.block_number {
        println!("Included in block: {}", block_number);
    }
    
    println!("\n===== SUMMARY =====");
    println!("TX hash: {}", tx_hash);
    println!("Transaction sent and confirmed in a single call in {:?}", tx_duration);
    
    if method == "rise" {
        println!("eth_sendRawTransactionSync worked successfully!");
    } else {
        println!("realtime_sendRawTransaction worked successfully!");
    }
    
    Ok(())
}