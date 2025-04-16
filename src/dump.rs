use anyhow::Result;
use dotenv::dotenv;
use ethers::{
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{transaction::eip2718::TypedTransaction, H256, U256},
};
use futures::future::join_all;
use std::{env, sync::Arc, time::Instant};

/// Creates a transaction that can be sent
async fn create_transaction(
    client: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    nonce: u64,
    gas_price: U256,
) -> Result<TypedTransaction> {
    let address = client.address();
    
    // Populate transaction with explicit nonce and hardcoded gas values
    let mut tx = TypedTransaction::default();
    tx.set_to(address);
    tx.set_value(U256::zero());
    tx.set_nonce(nonce);
    
    // Set fixed gas limit - 21000 is the cost of a simple ETH transfer
    tx.set_gas(21000);
    
    // Use the gas price passed from the main function
    tx.set_gas_price(gas_price);
    
    Ok(tx)
}

/// Sends a transaction without waiting for confirmation or receipt
async fn send_transaction(
    client: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    tx: TypedTransaction,
) -> Result<H256> {
    // Start measuring send time
    let send_start = Instant::now();
    
    // Send transaction
    let pending_tx = client.send_transaction(tx, None).await?;
    let tx_hash = pending_tx.tx_hash();
    
    // Measure send time
    let send_duration = send_start.elapsed();
    println!("TX sent in {:?}, hash: {}", send_duration, tx_hash);
    
    Ok(tx_hash)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let num_transactions = if args.len() > 1 {
        args[1].parse::<u64>().unwrap_or(10)
    } else {
        10 // Default to 10 transactions
    };
    
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
    
    // Make necessary RPC calls before the transaction loop
    let starting_nonce = client.get_transaction_count(wallet_address, None).await?.as_u64();
    let default_gas_price = client.get_gas_price().await?;
    let gas_price: U256 = default_gas_price * 3; // Use 3x the default gas price
    
    // Display info
    println!("RPC URL: {}", rpc_url_display);
    println!("Chain ID: {}", chain_id);
    println!("Wallet address: {}", wallet_address);
    println!("Starting nonce: {}", starting_nonce);
    println!("Default gas price: {} gwei", default_gas_price.as_u64() / 1_000_000_000);
    println!("Using gas price (3x): {} gwei", gas_price.as_u64() / 1_000_000_000);
    
    // Start timer for entire batch
    let batch_start_time = Instant::now();
    
    println!("\nPreparing {} transactions...", num_transactions);
    
    let mut prepared_txs = Vec::with_capacity(num_transactions as usize);
    
    // First, create all transactions (without signing)
    let prep_start = Instant::now();
    for i in 0..num_transactions {
        let nonce = starting_nonce + i;
        
        match create_transaction(client.clone(), nonce, gas_price).await {
            Ok(tx) => {
                println!("TX #{} prepared with nonce: {}", i + 1, nonce);
                prepared_txs.push((i, nonce, tx));
            },
            Err(e) => {
                println!("Failed to prepare TX #{}: {}", i + 1, e);
            }
        }
    }
    let prep_duration = prep_start.elapsed();
    println!("All transactions prepared in {:?} ({:.2} tx/s)", 
             prep_duration, 
             prepared_txs.len() as f64 / prep_duration.as_secs_f64());
    
    // Now send all transactions in parallel without awaiting each one
    println!("\nSubmitting all transactions in parallel...");
    let mut futures = Vec::with_capacity(prepared_txs.len());
    let mut sent_txs = Vec::with_capacity(prepared_txs.len());
    
    // Create futures for all the transactions
    for (i, nonce, tx) in prepared_txs {
        let client_clone = client.clone();
        
        futures.push(async move {
            let result = send_transaction(client_clone, tx).await;
            (i, nonce, result)
        });
    }
    
    // Execute all sends in parallel
    let sending_start = Instant::now();
    let results = join_all(futures).await;
    let sending_duration = sending_start.elapsed();
    
    // Process results
    for (i, nonce, result) in results {
        match result {
            Ok(hash) => {
                println!("TX #{} (nonce: {}): hash {}", i + 1, nonce, hash);
                sent_txs.push(hash);
            },
            Err(e) => {
                println!("TX #{} (nonce: {}): error: {}", i + 1, nonce, e);
            }
        }
    }
    
    println!("All transactions submitted in {:?} ({:.2} tx/s)", 
             sending_duration, 
             sent_txs.len() as f64 / sending_duration.as_secs_f64());
    
    let batch_elapsed = batch_start_time.elapsed();
    
    // Print summary
    println!("\n===== SUMMARY =====");
    println!("Total time to send all transactions: {:?}", batch_elapsed);
    println!("Transactions per second: {:.2}", num_transactions as f64 / batch_elapsed.as_secs_f64());
    println!("Total transactions sent: {}", sent_txs.len());
    
    Ok(())
}