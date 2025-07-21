use anyhow::Result;
use dotenv::dotenv;
use ethers::{
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{transaction::eip2718::TypedTransaction, U256},
};
use std::{env, sync::Arc, time::Instant};
use tokio::time::{sleep, Duration};

// Include the middleware module from parent directory
#[path = "../middleware/sync_transaction.rs"]  
mod sync_transaction;

use sync_transaction::SyncTransactionMiddleware;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    // Setup connection
    let rpc_url = env::var("RPC_PROVIDER").expect("RPC_PROVIDER must be set");
    let private_key = env::var("PRIVATE_KEY_1").expect("PRIVATE_KEY_1 must be set");
    
    let provider = Provider::<Http>::try_from(rpc_url.clone())?;
    let wallet: LocalWallet = private_key.parse()?;
    let wallet_address = wallet.address();
    let chain_id = provider.get_chainid().await?;
    let wallet = wallet.with_chain_id(chain_id.as_u64());
    
    // Create standard ethers middleware
    let client = Arc::new(SignerMiddleware::new(provider, wallet));
    
    // Create sync transaction middleware
    let sync_client = SyncTransactionMiddleware::new(client.clone());
    
    // Get initial gas price and nonce
    let gas_price: U256 = client.get_gas_price().await? * 3; // 3x gas price for faster inclusion
    let initial_nonce = client.get_transaction_count(wallet_address, None).await?.as_u64();
    
    println!("=== Nonce Real-time Update Test ===");
    println!("RPC URL: {}", rpc_url);
    println!("Chain ID: {}", chain_id);
    println!("Wallet address: {}", wallet_address);
    println!("Initial nonce: {}", initial_nonce);
    println!("Gas price: {} gwei", gas_price.as_u64() / 1_000_000_000);
    println!();
    
    let num_loops = 10;
    let mut failed_tests = 0;
    let mut successful_tests = 0;
    
    println!("Running {} synchronous tests...\n", num_loops);
    
    for i in 0..num_loops {
        println!("--- Test #{} ---", i + 1);
        
        // Get current nonce before transaction
        let nonce_before = client.get_transaction_count(wallet_address, None).await?.as_u64();
        println!("Nonce before TX: {}", nonce_before);
        
        // Create transaction
        let max_priority_fee_per_gas = U256::from(1_000_000_000); // 1 gwei
        let max_fee_per_gas = gas_price.max(max_priority_fee_per_gas * 2);
        
        let tx_request = ethers::types::transaction::eip1559::Eip1559TransactionRequest::new()
            .from(wallet_address)
            .to(wallet_address)
            .value(U256::zero())
            .chain_id(chain_id.as_u64())
            .nonce(nonce_before)
            .gas(21000)
            .max_fee_per_gas(max_fee_per_gas)
            .max_priority_fee_per_gas(max_priority_fee_per_gas);
        
        let tx = TypedTransaction::Eip1559(tx_request);
        
        // Sign and send transaction
        let start_time = Instant::now();
        let signature = client.signer().sign_transaction(&tx).await?;
        let raw_tx = tx.rlp_signed(&signature);
        
        // Send transaction synchronously
        let receipt = match sync_client.send_raw_transaction_sync(raw_tx).await {
            Ok(r) => r,
            Err(e) => {
                println!("Error sending transaction: {}", e);
                failed_tests += 1;
                continue;
            }
        };
        let send_time = start_time.elapsed();
        
        println!("TX Hash: {}", receipt.transaction_hash);
        println!("TX Status: {}", 
            if receipt.status.map(|s| s.low_u32() == 1).unwrap_or(false) { 
                "SUCCESS" 
            } else { 
                "FAILED" 
            }
        );
        println!("Send time: {:?}", send_time);
        
        // Immediately fetch nonce after transaction
        let nonce_after = client.get_transaction_count(wallet_address, None).await?.as_u64();
        println!("Nonce after TX: {}", nonce_after);
        
        // Check if nonce was updated correctly
        let expected_nonce = nonce_before + 1;
        if nonce_after == expected_nonce {
            println!("✓ PASS: Nonce updated correctly (expected: {}, got: {})", expected_nonce, nonce_after);
            successful_tests += 1;
        } else {
            println!("✗ FAIL: Nonce NOT updated correctly (expected: {}, got: {})", expected_nonce, nonce_after);
            failed_tests += 1;
        }
        
        // Add small delay to avoid rapid-fire nonce conflicts
        if i < num_loops - 1 {
            sleep(Duration::from_millis(500)).await;
        }
        
        println!();
    }
    
    // Print summary
    println!("=== Test Summary ===");
    println!("Total tests: {}", num_loops);
    println!("Successful: {} ({}%)", successful_tests, (successful_tests * 100) / num_loops);
    println!("Failed: {} ({}%)", failed_tests, (failed_tests * 100) / num_loops);
    
    if failed_tests > 0 {
        println!("\n⚠️  WARNING: {} tests failed - nonce is NOT being updated in real-time!", failed_tests);
        std::process::exit(1);
    } else {
        println!("\n✅ All tests passed - nonce is being updated in real-time!");
    }
    
    Ok(())
}