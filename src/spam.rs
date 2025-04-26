use anyhow::Result;
use chrono::Utc;
use dotenv::dotenv;
use ethers::{
    core::types::Bytes,
    middleware::SignerMiddleware,
    providers::{Http, JsonRpcClient, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{transaction::eip2718::TypedTransaction, TransactionReceipt, H256, U256},
};
use std::{env, fs, io::Write, path::Path, sync::Arc, time::Instant};
use tokio::time::sleep;
use std::time::Duration;

// Import our custom middlewares
mod middleware;
use middleware::sync_transaction::SyncTransactionMiddleware;
use middleware::realtime_transaction::RealtimeTransactionMiddleware;

/// Sends a transaction and waits for the receipt
/// This version removes unnecessary await calls to minimize RPC requests
async fn send_and_confirm_transaction(
    client: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
    nonce: u64,
    gas_price: U256,
) -> Result<(H256, Duration, Duration)> {
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
                // Short sleep to avoid hammering the RPC - slow chain problem, don't use for rise and mega
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

/// Generates a markdown report of test results
fn generate_report(
    test_name: &str,
    method: &str,
    rpc_url: &str,
    chain_id: U256,
    wallet_address: &str,
    gas_price: U256,
    total_duration: Duration,
    results: &[(H256, Duration, Duration, Duration)],
) -> Result<String> {
    let timestamp = Utc::now().format("%Y-%m-%d-%H%M%S");
    let filename = if test_name.is_empty() {
        format!("rpc-test-{}.md", timestamp)
    } else {
        format!("{}-{}.md", test_name, timestamp)
    };
    
    let path = Path::new("results").join(&filename);
    
    // Create statistics
    let (min_send, max_send, avg_send, 
         min_confirm, max_confirm, avg_confirm,
         min_total, max_total, avg_total) = if !results.is_empty() {
        // Send time stats
        let send_times = results.iter().map(|(_, s, _, _)| s.as_millis() as u128).collect::<Vec<_>>();
        let min_send = send_times.iter().min().unwrap_or(&0);
        let max_send = send_times.iter().max().unwrap_or(&0);
        let avg_send = send_times.iter().sum::<u128>() / send_times.len() as u128;

        // Confirm time stats
        let confirm_times = results.iter().map(|(_, _, c, _)| c.as_millis() as u128).collect::<Vec<_>>();
        let min_confirm = confirm_times.iter().min().unwrap_or(&0);
        let max_confirm = confirm_times.iter().max().unwrap_or(&0);
        let avg_confirm = confirm_times.iter().sum::<u128>() / confirm_times.len() as u128;

        // Total time stats
        let total_times = results.iter().map(|(_, _, _, t)| t.as_millis() as u128).collect::<Vec<_>>();
        let min_total = total_times.iter().min().unwrap_or(&0);
        let max_total = total_times.iter().max().unwrap_or(&0);
        let avg_total = total_times.iter().sum::<u128>() / total_times.len() as u128;
        
        (*min_send, *max_send, avg_send,
         *min_confirm, *max_confirm, avg_confirm,
         *min_total, *max_total, avg_total)
    } else {
        (0, 0, 0, 0, 0, 0, 0, 0, 0)
    };
    
    // Create markdown content
    let mut md_content = String::new();
    
    // Title and testing information
    md_content.push_str(&format!("# RPC Latency Test Results: {}\n\n", 
        if test_name.is_empty() { "Default" } else { test_name }));
    
    md_content.push_str("## Test Information\n\n");
    md_content.push_str(&format!("- **Date and Time**: {}\n", Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
    md_content.push_str(&format!("- **RPC URL**: {}\n", rpc_url));
    md_content.push_str(&format!("- **Chain ID**: {}\n", chain_id));
    md_content.push_str(&format!("- **Wallet**: {}\n", wallet_address));
    md_content.push_str(&format!("- **Gas Price**: {} gwei\n", gas_price.as_u64() / 1_000_000_000));
    md_content.push_str(&format!("- **Transaction Method**: {}\n", method));
    md_content.push_str(&format!("- **Total Test Duration**: {} ms\n", total_duration.as_millis()));
    md_content.push_str(&format!("- **Number of Transactions**: {}\n\n", results.len()));
    
    // Summary statistics
    md_content.push_str("## Summary Statistics\n\n");
    md_content.push_str("| Metric | Min (ms) | Max (ms) | Avg (ms) |\n");
    md_content.push_str("|--------|----------|----------|----------|\n");
    md_content.push_str(&format!("| Send Time | {} | {} | {} |\n", min_send, max_send, avg_send));
    md_content.push_str(&format!("| Confirm Time | {} | {} | {} |\n", min_confirm, max_confirm, avg_confirm));
    md_content.push_str(&format!("| Total Time | {} | {} | {} |\n\n", min_total, max_total, avg_total));
    
    // Individual transactions
    md_content.push_str("## Individual Transaction Results\n\n");
    md_content.push_str("| TX# | Send (ms) | Confirm (ms) | Total (ms) | Hash |\n");
    md_content.push_str("|-----|-----------|--------------|------------|--------------|\n");
    
    for (i, (hash, send_time, confirm_time, total_time)) in results.iter().enumerate() {
        md_content.push_str(&format!("| {} | {} | {} | {} | `0x{}` |\n", 
            i + 1,
            send_time.as_millis(),
            confirm_time.as_millis(),
            total_time.as_millis(),
            // Convert the full hash to a hex string without truncation
            hex::encode(hash.as_bytes())
        ));
    }
    
    // Create directory if it doesn't exist
    if !Path::new("results").exists() {
        fs::create_dir("results")?;
    }
    
    // Write to file
    let mut file = fs::File::create(&path)?;
    file.write_all(md_content.as_bytes())?;
    
    println!("\nReport saved to: {}", path.display());
    
    Ok(filename)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    // Check for command line args
    let args: Vec<String> = std::env::args().collect();
    
    // Default method is async
    let method = if args.len() > 1 {
        match args[1].as_str() {
            "async" => "async", // default method using regular sendTransaction + waitForReceipt
            "rise" => "rise",   // use eth_sendRawTransactionSync
            "mega" => "mega",   // use realtime_sendRawTransaction
            _ => "async"        // treat any other value as test name with async method
        }
    } else {
        "async"  // default to async if no argument provided
    };
    
    // If first arg is a method type, test name is the second arg, otherwise test name is first arg
    let test_name = if method == "async" && args.len() > 1 && args[1] != "async" {
        &args[1]  // first arg is the test name
    } else if args.len() > 2 {
        &args[2]  // second arg is the test name
    } else {
        ""  // no test name provided
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
    
    // Create standard ethers middleware
    let client = Arc::new(SignerMiddleware::new(provider, wallet));
    
    // Create our custom middlewares
    let sync_client = SyncTransactionMiddleware::new(client.clone());
    let realtime_client = RealtimeTransactionMiddleware::new(client.clone());
    
    // Make necessary RPC calls before the transaction loop
    let starting_nonce = client.get_transaction_count(wallet_address, None).await?.as_u64();
    let default_gas_price = client.get_gas_price().await?;
    // Use 3x the default gas price, or 1 gwei if the gas price is zero
    let gas_price: U256 = if default_gas_price.is_zero() {
        println!("Warning: RPC returned zero gas price, using 1 gwei as default");
        U256::from(1_000_000_000) // 1 gwei
    } else {
        default_gas_price * 3
    };
    
    // Display info
    println!("RPC URL: {}", rpc_url_display);
    println!("Chain ID: {}", chain_id);
    println!("Wallet address fuck: {}", wallet_address);
    println!("Starting nonce: {}", starting_nonce);
    println!("Default gas price: {} gwei", default_gas_price.as_u64() / 1_000_000_000);
    println!("Using gas price (3x): {} gwei", gas_price.as_u64() / 1_000_000_000);
    // Display test name and transaction method
    if !test_name.is_empty() {
        println!("Test name: {}", test_name);
    }
    println!("Transaction method: {}", method);
    
    // Start timer for entire batch
    let batch_start_time = Instant::now();
    
    // Get number of transactions from args or use default
    let tx_count_arg_index = if method == "async" && args.len() > 1 && args[1] != "async" {
        2  // If first arg is test name, tx count is arg[2]
    } else {
        3  // If first arg is method and second is test name, tx count is arg[3]
    };
    
    let num_transactions = if args.len() > tx_count_arg_index {
        args[tx_count_arg_index].parse::<u64>().unwrap_or(10)
    } else {
        10 // Default to 10 transactions
    };
    
    println!("\nSending {} transactions sequentially, waiting for confirmation after each...", num_transactions);
    
    let mut results = Vec::with_capacity(num_transactions as usize);
    
    for i in 0..num_transactions {
        let nonce = starting_nonce + i;
        
        println!("\n--- Transaction #{} (nonce: {}) ---", i + 1, nonce);
        
        // Start timing total transaction time
        let tx_start = Instant::now();
        
        if method == "async" {
            // Use regular async transaction method
            match send_and_confirm_transaction(client.clone(), nonce, gas_price).await {
                Ok((hash, send_time, confirm_time)) => {
                    let total_time = tx_start.elapsed();
                    println!("TX #{}: total time: {:?} (send: {:?}, confirm: {:?})", 
                             i + 1, total_time, send_time, confirm_time);
                    
                    results.push((hash, send_time, confirm_time, total_time));
                },
                Err(e) => {
                    println!("TX #{}: error: {}", i + 1, e);
                }
            }
        } else {
            // Create transaction with explicit nonce and hardcoded gas values
            // Use EIP-1559 transaction type for compatibility with the sync methods
            
            // Ensure we have a non-zero gas price
            // Gas price is already set to at least 1 gwei in the main function
            
            // Set priority fee to 1 gwei
            let max_priority_fee_per_gas = U256::from(1_000_000_000); // 1 gwei
            
            // Make sure max_fee_per_gas is at least as large as max_priority_fee_per_gas
            let max_fee_per_gas = if gas_price > max_priority_fee_per_gas {
                gas_price
            } else {
                // If gas_price is too low, make max_fee at least 2x the priority fee
                max_priority_fee_per_gas * 2
            };
            
            // Create EIP-1559 transaction request
            let tx_request = ethers::types::transaction::eip1559::Eip1559TransactionRequest::new()
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
            
            // Start measuring send time
            let send_start = Instant::now();
            
            // Sign the transaction
            let signature = client.signer().sign_transaction(&tx).await?;
            
            // Get the properly encoded transaction according to EIP-2718
            let raw_tx = tx.rlp_signed(&signature);
            
            let send_time;
            let confirm_time = Duration::default();  // Not applicable for sync methods
            let hash: H256;
            let receipt: TransactionReceipt;
            
            if method == "rise" {
                // Use eth_sendRawTransactionSync
                println!("Sending TX #{} with eth_sendRawTransactionSync...", i + 1);
                receipt = sync_client.send_raw_transaction_sync(raw_tx).await?;
                send_time = send_start.elapsed();
                hash = receipt.transaction_hash;
            } else {
                // Use realtime_sendRawTransaction
                println!("Sending TX #{} with realtime_sendRawTransaction...", i + 1);
                receipt = realtime_client.send_raw_transaction_realtime(raw_tx).await?;
                send_time = send_start.elapsed();
                hash = receipt.transaction_hash;
            }
            
            let total_time = tx_start.elapsed();
            
            // Print the transaction status
            let status_str = if let Some(status) = receipt.status {
                if status.low_u32() == 1 { "SUCCESS" } else { "FAILED" }
            } else {
                "UNKNOWN"
            };
            
            println!("\n====== TRANSACTION RECEIPT ======");
            println!("Transaction Hash: {}", hash);
            println!("Transaction Status: {}", status_str);
            println!("Block Number: {:?}", receipt.block_number);
            println!("Gas Used: {:?}", receipt.gas_used);
            println!("================================");
            
            // Print block information
            if let Some(block_number) = receipt.block_number {
                println!("Included in block: {}", block_number);
            }
            
            println!("TX #{}: total time: {:?} (send: {:?})", 
                   i + 1, total_time, send_time);
            
            // For sync methods, send time is the total time (confirm time is 0)
            results.push((hash, send_time, confirm_time, total_time));
        }
        
        println!("--- End Transaction #{} ---\n", i + 1);
    }
    
    let batch_elapsed = batch_start_time.elapsed();
    
    // Print summary
    println!("\n===== SUMMARY =====");
    println!("Total time for all transactions: {:?}", batch_elapsed);
    println!();
    
    println!("Individual Transaction Results:");
    println!("{:<5} {:<12} {:<12} {:<12} {:<64}", 
             "TX#", "SEND (ms)", "CONFIRM (ms)", "TOTAL (ms)", "HASH");
    println!("{}", "-".repeat(120));
    
    for (i, (hash, send_time, confirm_time, total_time)) in results.iter().enumerate() {
        println!("{:<5} {:<12} {:<12} {:<12} {:<64}", 
                 i + 1,
                 send_time.as_millis(),
                 confirm_time.as_millis(),
                 total_time.as_millis(),
                 hash);
    }
    
    // Calculate min, max, and averages
    if !results.is_empty() {
        // Send time stats
        let send_times = results.iter().map(|(_, s, _, _)| s.as_millis() as u128).collect::<Vec<_>>();
        let min_send = send_times.iter().min().unwrap_or(&0);
        let max_send = send_times.iter().max().unwrap_or(&0);
        let avg_send = send_times.iter().sum::<u128>() / send_times.len() as u128;

        // Confirm time stats
        let confirm_times = results.iter().map(|(_, _, c, _)| c.as_millis() as u128).collect::<Vec<_>>();
        let min_confirm = confirm_times.iter().min().unwrap_or(&0);
        let max_confirm = confirm_times.iter().max().unwrap_or(&0);
        let avg_confirm = confirm_times.iter().sum::<u128>() / confirm_times.len() as u128;

        // Total time stats
        let total_times = results.iter().map(|(_, _, _, t)| t.as_millis() as u128).collect::<Vec<_>>();
        let min_total = total_times.iter().min().unwrap_or(&0);
        let max_total = total_times.iter().max().unwrap_or(&0);
        let avg_total = total_times.iter().sum::<u128>() / total_times.len() as u128;
        
        println!("\nLATENCY STATISTICS:");
        println!("{:<13} {:<10} {:<10} {:<10}", "", "MIN (ms)", "MAX (ms)", "AVG (ms)");
        println!("{}", "-".repeat(45));
        println!("{:<13} {:<10} {:<10} {:<10}", "Send time:", min_send, max_send, avg_send);
        println!("{:<13} {:<10} {:<10} {:<10}", "Confirm time:", min_confirm, max_confirm, avg_confirm);
        println!("{:<13} {:<10} {:<10} {:<10}", "Total time:", min_total, max_total, avg_total);
        
        println!("\nSUMMARY: {} transactions sent and confirmed sequentially in {} ms (min: {} ms, max: {} ms, avg: {} ms)",
            results.len(), batch_elapsed.as_millis(), min_total, max_total, avg_total);
        
        // Generate markdown report
        match generate_report(
            test_name,
            method,
            &rpc_url_display,
            chain_id, 
            &wallet_address.to_string(), 
            gas_price, 
            batch_elapsed, 
            &results
        ) {
            Ok(filename) => println!("Report generated: results/{}", filename),
            Err(e) => println!("Failed to generate report: {}", e),
        }
    }
    
    Ok(())
}