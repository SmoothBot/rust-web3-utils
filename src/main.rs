use anyhow::Result;
use dotenv::dotenv;
use ethers::{
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{transaction::eip2718::TypedTransaction, TransactionReceipt, H256, U256},
};
use std::{env, sync::Arc, time::Instant};
use tokio::time::sleep;
use std::time::Duration;

/// Sends a transaction and waits for the receipt
async fn send_and_confirm_transaction(
    client: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    gas_price: U256,
) -> Result<(H256, Duration, Duration)> {
    let address = client.address();
    
    // Populate transaction
    let mut tx = TypedTransaction::default();
    tx.set_to(address);
    tx.set_value(U256::zero());
    
    // Set fixed gas limit - 21000 is the cost of a simple ETH transfer
    tx.set_gas(21000);
    
    // Use the gas price passed from the main function
    tx.set_gas_price(gas_price);
    
    // Start measuring send time
    let send_start = Instant::now();
    
    // Send transaction
    let pending_tx = client.send_transaction(tx, None).await?;
    let tx_hash = pending_tx.tx_hash();
    
    // Measure send time
    let send_duration = send_start.elapsed();
    println!("TX sent in {:?}, hash: {}", send_duration, tx_hash);
    
    // Start measuring confirmation time
    let confirm_start = Instant::now();
    
    // Wait for receipt
    println!("Waiting for confirmation...");
    let mut receipt: Option<TransactionReceipt> = None;
    
    while receipt.is_none() {
        match client.get_transaction_receipt(tx_hash).await? {
            Some(r) => {
                receipt = Some(r.clone());
                
                // Print the transaction status in a more readable format
                let status_str = if let Some(status) = r.status {
                    if status.low_u32() == 1 { "SUCCESS" } else { "FAILED" }
                } else {
                    "UNKNOWN"
                };
                
                println!("\n====== TRANSACTION RECEIPT ======");
                println!("Transaction Hash: {:?}", r.transaction_hash);
                println!("Transaction Status: {}", status_str);
                println!("Block Number: {:?}", r.block_number);
                println!("Gas Used: {:?}", r.gas_used);
                println!("================================");
                break;
            }
            None => {
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
    
    // Measure confirmation time
    let confirm_duration = confirm_start.elapsed();
    println!("TX confirmed in {:?}", confirm_duration);
    
    // Get block information
    if let Some(r) = receipt {
        if let Some(block_number) = r.block_number {
            println!("Included in block: {}", block_number);
        }
    }
    
    Ok((tx_hash, send_duration, confirm_duration))
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    // Check for test name from command line args
    let args: Vec<String> = std::env::args().collect();
    let test_name = if args.len() > 1 { &args[1] } else { "" };
    
    // Setup connection
    let rpc_url = env::var("RPC_PROVIDER").expect("RPC_PROVIDER must be set");
    let private_key = env::var("PRIVATE_KEY_1").expect("PRIVATE_KEY_1 must be set");
    
    let rpc_url_display = rpc_url.clone();
    let provider = Provider::<Http>::try_from(rpc_url)?;
    let wallet: LocalWallet = private_key.parse()?;
    let wallet_address = wallet.address();
    let chain_id = provider.get_chainid().await?;
    let wallet = wallet.with_chain_id(chain_id.as_u64());
    
    let client = Arc::new(SignerMiddleware::new(provider, wallet));
    
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
    
    println!("\nSending a single transaction and measuring latency...");
    
    // Start timing total transaction time
    let tx_start = Instant::now();
    
    match send_and_confirm_transaction(client.clone(), gas_price).await {
        Ok((tx_hash, send_time, confirm_time)) => {
            let total_time = tx_start.elapsed();
            println!("\n===== SUMMARY =====");
            println!("TX hash: {}", tx_hash);
            println!("Transaction sent and confirmed in {:?} (send: {:?}, confirm: {:?})", 
                    total_time, send_time, confirm_time);
        },
        Err(e) => {
            println!("Error: {}", e);
        }
    }
    
    Ok(())
}