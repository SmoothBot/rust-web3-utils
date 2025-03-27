use anyhow::Result;
use dotenv::dotenv;
use ethers::{
    middleware::SignerMiddleware,
    prelude::*,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{transaction::eip2718::TypedTransaction, H256, U256},
};
use std::{env, sync::Arc, time::Instant};
use tokio::time::sleep;
use std::time::Duration;

async fn send_transaction(
    client: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    nonce: u64,
) -> Result<H256> {
    let address = client.address();
    
    // Populate transaction with explicit nonce
    let mut tx = TypedTransaction::default();
    tx.set_to(address);
    tx.set_value(U256::zero());
    tx.set_nonce(nonce);
    
    // Set gas params
    let gas_price = client.get_gas_price().await?;
    tx.set_gas_price(gas_price);
    let gas = client.estimate_gas(&tx, None).await?;
    tx.set_gas(gas);
    
    // Send transaction and return hash immediately without waiting for receipt
    let pending_tx = client.send_transaction(tx, None).await?;
    Ok(pending_tx.tx_hash())
}

async fn wait_for_receipts(
    client: &SignerMiddleware<Provider<Http>, LocalWallet>,
    tx_hashes: &[H256],
) -> Result<()> {
    let mut pending_hashes: Vec<H256> = tx_hashes.to_vec();
    
    while !pending_hashes.is_empty() {
        println!("Waiting for {} transactions to be mined...", pending_hashes.len());
        
        // Create a temporary vector to store hashes that are still pending
        let mut still_pending = Vec::new();
        
        // Check each hash
        for hash in pending_hashes.iter() {
            // Try to get the receipt (this is fully async and awaits the future)
            match client.get_transaction_receipt(*hash).await {
                Ok(Some(_receipt)) => {
                    println!("Transaction {} confirmed", hash);
                    // Don't add to still_pending
                },
                _ => {
                    // Still pending, add back to our tracking list
                    still_pending.push(*hash);
                }
            }
        }
        
        // Replace our list with the list of txs that are still pending
        pending_hashes = still_pending;
        
        // Short sleep to avoid hammering the RPC
        sleep(Duration::from_millis(500)).await;
    }
    
    println!("All transactions confirmed!");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
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
    
    // Get starting nonce
    let starting_nonce = client.get_transaction_count(wallet_address, None).await?.as_u64();
    
    // Display info
    println!("RPC URL: {}", rpc_url_display);
    println!("Chain ID: {}", chain_id);
    println!("Wallet address: {}", wallet_address);
    println!("Starting nonce: {}", starting_nonce);
    
    // Start timer for entire batch
    let batch_start_time = Instant::now();
    
    // Send 10 transactions as fast as possible
    println!("\nSending 10 transactions as fast as possible...");
    let mut tx_hashes = Vec::with_capacity(10);
    
    for i in 0..10 {
        let nonce = starting_nonce + i;
        let start = Instant::now();
        
        match send_transaction(client.clone(), nonce).await {
            Ok(hash) => {
                let elapsed = start.elapsed();
                println!("TX #{}: sent in {:?}, hash: {}", i + 1, elapsed, hash);
                tx_hashes.push(hash);
            },
            Err(e) => {
                println!("TX #{}: error: {}", i + 1, e);
            }
        }
    }
    
    let batch_elapsed = batch_start_time.elapsed();
    println!("All transactions sent in {:?}", batch_elapsed);
    
    // Now wait for all receipts
    wait_for_receipts(&client, &tx_hashes).await?;
    
    println!("Total execution time: {:?}", batch_start_time.elapsed());
    
    Ok(())
}