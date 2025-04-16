use anyhow::Result;
use chrono::Utc;
use dotenv::dotenv;
use ethers::{
    middleware::SignerMiddleware,
    prelude::*,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{transaction::eip2718::TypedTransaction, TransactionReceipt, H256, U256},
    utils::keccak256,
};
use std::{env, sync::Arc, time::Instant};
use tokio::time::{sleep, Duration};

async fn send_transaction(
    client: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>, 
    nonce_add: u64
) -> Result<(u64, H256)> {
    // Clone the Arc to avoid lifetime issues
    let client = client.clone();
    let address = client.address();
    
    // Get current nonce and adjust if needed
    let nonce = client.get_transaction_count(address, None).await?;
    let nonce = nonce.as_u64() + nonce_add;
    
    // Populate transaction
    let mut tx = TypedTransaction::default();
    tx.set_to(address);
    tx.set_value(U256::zero());
    tx.set_nonce(nonce);
    
    // Get the gas price and set it (2x the default)
    let gas_price = client.get_gas_price().await?;
    let gas_price = gas_price * 2;
    tx.set_gas_price(gas_price);
    
    // Estimate gas
    let gas = client.estimate_gas(&tx, None).await?;
    tx.set_gas(gas);
    
    // Get the current block number
    let start_get_block = Instant::now();
    let block_before = client.get_block_number().await?;
    println!("getBlock: {:?}", start_get_block.elapsed());
    println!("Block before: {}", block_before);
    
    // Start timing for the whole process
    let start_time = Instant::now();
    
    // Send transaction and get the transaction hash
    println!("Sending transaction...");
    let tx_hash = client.send_transaction(tx.clone(), None).await?.tx_hash();
    println!("Transaction hash: {}", tx_hash);
    // Wait for receipt
    loop {
        if let Some(receipt) = client.get_transaction_receipt(tx_hash).await? {
            let block_now = client.get_block_number().await?;
            println!("Block now:     {}", block_now);
            
            // Print the transaction status
            if let Some(status) = receipt.status {
                println!("Transaction Status: {}", if status.low_u32() == 1 { "SUCCESS" } else { "FAILED" });
            } else {
                println!("Transaction Status: UNKNOWN");
            }
            
            // Print the full receipt in a more readable format
            println!("\n====== TRANSACTION RECEIPT ======");
            println!("Transaction Hash: {}", receipt.transaction_hash);
            println!("Block Hash: {:?}", receipt.block_hash);
            println!("Block Number: {:?}", receipt.block_number);
            println!("Transaction Index: {:?}", receipt.transaction_index);
            println!("From: {:?}", receipt.from);
            println!("To: {:?}", receipt.to);
            println!("Contract Address: {:?}", receipt.contract_address);
            println!("Gas Used: {:?}", receipt.gas_used);
            println!("Cumulative Gas Used: {:?}", receipt.cumulative_gas_used);
            println!("Status: {:?}", receipt.status);
            println!("Effective Gas Price: {:?}", receipt.effective_gas_price);
            
            if !receipt.logs.is_empty() {
                println!("\nLogs:");
                for (i, log) in receipt.logs.iter().enumerate() {
                    println!("  Log #{}", i);
                    println!("    Address: {:?}", log.address);
                    println!("    Topics: {:?}", log.topics);
                    println!("    Data: {:?}", log.data);
                }
            }
            println!("================================\n");
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    
    let elapsed = start_time.elapsed();
    println!("Total transaction time: {:?}", elapsed);
    
    // Get final receipt
    let receipt = client.get_transaction_receipt(tx_hash).await?;
    let block_diff = receipt.as_ref()
        .and_then(|r| r.block_number)
        .map(|bn| bn.as_u64() - block_before.as_u64())
        .unwrap_or(0);
    
    println!("[tx] complete - block diff: {}", block_diff);
    
    // Wait before next transaction
    sleep(Duration::from_secs(5)).await;
    
    Ok((elapsed.as_millis() as u64, tx_hash))
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    let rpc_url = env::var("RPC_PROVIDER").expect("RPC_PROVIDER must be set");
    let private_key = env::var("PRIVATE_KEY_1").expect("PRIVATE_KEY_1 must be set");
    
    // Clone rpc_url before it's moved
    let rpc_url_display = rpc_url.clone();
    let provider = Provider::<Http>::try_from(rpc_url)?;
    let wallet: LocalWallet = private_key.parse()?;
    let wallet_address = wallet.address();
    let chain_id = provider.get_chainid().await?;
    let wallet = wallet.with_chain_id(chain_id.as_u64());
    
    let client = Arc::new(SignerMiddleware::new(provider, wallet));
    
    let block = client.get_block_number().await?;
    print!("Address: {}", wallet_address);
    println!("RPC URL: {}", rpc_url_display);
    println!("Chain ID: {}", chain_id);
    println!("Current block: {}", block);
    println!("Wallet address fuck: {}", wallet_address);
    
    // for i in 0..10 {
    let i= 1;
        println!("\n========== TEST #{} ==========", i);
        match send_transaction(client.clone(), i).await {
            Ok((time, hash)) => {
                println!("[TX] e2e time: {}ms, hash: {}", time, hash);
            }
            Err(e) => {
                println!("Transaction error: {}", e);
            }
        }
        println!("============================\n");
    // }
    
    Ok(())
}
