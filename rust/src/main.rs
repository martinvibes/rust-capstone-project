#![allow(unused)]
// Import necessary Bitcoin and serialization libraries
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;

// Bitcoin Core RPC connection parameters for regtest network
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// Custom RPC function to send Bitcoin using the 'send' RPC call
// This demonstrates how to use RPC calls not directly exposed by the bitcoincore_rpc library
// The 'send' call is used for sending to multiple addresses with automatic fee calculation
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    // Prepare arguments for the 'send' RPC call
    // Format: send [{"address": amount}, ...] [conf_target] [estimate_mode] [fee_rate] [options]
    let args = [
        json!([{addr : 100 }]), // recipient address and amount (100 BTC)
        json!(null),            // conf target (null = use default)
        json!(null),            // estimate mode (null = use default)
        json!(null),            // fee rate in sats/vb (null = use default)
        json!(null),            // Empty option object
    ];

    // Define structure to deserialize the RPC response
    #[derive(Deserialize)]
    struct SendResult {
        complete: bool, // Whether the transaction was successfully created
        txid: String,   // Transaction ID of the created transaction
    }

    // Make the RPC call and deserialize the response
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete); // Ensure transaction was created successfully
    Ok(send_result.txid)
}

// Static array for empty address list (used as a placeholder)
static EMPTY_ADDRESS: [bitcoincore_rpc::bitcoin::Address<
    bitcoincore_rpc::bitcoin::address::NetworkUnchecked,
>; 0] = [];

fn main() -> bitcoincore_rpc::Result<()> {
    // Step 1: Establish connection to Bitcoin Core RPC server
    // This creates a client that can communicate with the Bitcoin Core node
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Step 2: Get basic blockchain information to verify connection
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Infomation: {blockchain_info:?}");

    // Step 3: Create or load wallets for mining and trading operations
    // We create two wallets: 'Miner' for mining rewards and 'Trader' for receiving transactions
    for wallet_name in ["Miner", "Trader"] {
        // Attempt to create each wallet
        let res = rpc.create_wallet(wallet_name, None, None, None, None);
        match res {
            Ok(_) => println!("Wallet '{wallet_name}' created."),
            Err(e) => {
                // Handle the case where wallet already exists
                // Bitcoin Core returns an error if wallet already exists, but this is not a real error
                let msg = format!("{e}");
                if msg.contains("already exists") {
                    println!("Wallet '{wallet_name}' has been created already");
                } else {
                    // If it's a different error, propagate it up
                    return Err(e);
                }
            }
        }
    }

    // Step 4: Create wallet-specific RPC clients
    // Each wallet needs its own client instance to perform wallet-specific operations
    let miner_wallet = Client::new(
        &format!("{}/wallet/{}", RPC_URL, "Miner"),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;
    let trader_wallet = Client::new(
        &format!("{}/wallet/{}", RPC_URL, "Trader"),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Step 5: Generate spendable balance in the Miner wallet
    // In Bitcoin, mining rewards require 100 confirmations to become spendable
    // We need to mine enough blocks to have a positive balance

    // Generate a new address for receiving mining rewards
    let mining_address = miner_wallet
        .get_new_address(Some("Mining Reward"), None)?
        .assume_checked(); // Assume the address is valid (safe for regtest)
    println!("Miner's mining address: {mining_address}");

    // Mine blocks until the wallet has a positive balance
    // Each block reward is 50 BTC in regtest mode, but requires 100 confirmations
    let mut balance = miner_wallet.get_balance(None, None)?.to_btc();
    let mut blocks_mined = 0;
    while balance <= 0.0 {
        // Generate 1 block and send the reward to the mining address
        miner_wallet.generate_to_address(1, &mining_address)?;
        blocks_mined += 1;
        // Check the updated balance
        balance = miner_wallet.get_balance(None, None)?.to_btc();
    }
    println!("Blocks mined until positive balance: {blocks_mined}");
    println!("Miner balance: {balance} BTC");

    // Step 6: Set up the Trader wallet and perform a transaction
    // Generate a new address for the Trader to receive Bitcoin
    let trader_address = trader_wallet
        .get_new_address(Some("Received"), None)?
        .assume_checked();
    println!("Trader's receiving address: {trader_address}");

    // Send 20 BTC from Miner to Trader
    // This creates a transaction and returns the transaction ID
    let txid = miner_wallet.send_to_address(
        &trader_address,
        Amount::from_btc(20.0)?, // Convert 20.0 BTC to Amount type
        None,                    // comment (optional)
        None,                    // subtract fee from amount (optional)
        None,                    // replaceable (optional)
        None,                    // conf target (optional)
        None,                    // estimate mode (optional)
        None,                    // avoid reuse (optional)
    )?;
    println!("Sent 20 BTC from Miner to Trader. Tx ID: {txid}");

    // Step 7: Monitor the transaction in the mempool
    // Get detailed information about the unconfirmed transaction
    let mempool_entry = miner_wallet.get_mempool_entry(&txid)?;
    println!("Mempool entry for txid {txid}: {mempool_entry:#?}");

    // Mine 1 block to confirm the transaction
    // This moves the transaction from mempool to a confirmed block
    miner_wallet.generate_to_address(1, &mining_address)?;
    println!("Mined 1 block to confirm the transaction.");

    // Step 8: Extract comprehensive transaction details for analysis
    // Import additional types needed for transaction analysis
    use bitcoincore_rpc::bitcoin::Txid;
    use std::path::Path;

    // Get the confirmed transaction information
    let tx_info = miner_wallet.get_transaction(&txid, None)?;
    let block_hash = tx_info
        .info
        .blockhash
        .expect("Transaction should be confirmed in a block");

    // Get block information to determine block height
    let block = miner_wallet.get_block_info(&block_hash)?;
    let block_height = block.height;

    // Get the raw transaction data and decode it for detailed analysis
    let raw_tx = miner_wallet.get_raw_transaction(&txid, Some(&block_hash))?;
    let decoded_tx = miner_wallet.decode_raw_transaction(&raw_tx, None)?;

    // Step 9: Analyze transaction inputs (where the Bitcoin came from)
    // Get the first input (assuming single input transaction for simplicity)
    let input = &decoded_tx.vin[0];
    let prev_txid = input.txid.expect("Input should have txid");
    let prev_vout = input.vout.expect("Input should have vout") as usize;

    // Get the previous transaction that this input is spending from
    let prev_tx = miner_wallet.get_raw_transaction(&prev_txid, None)?;
    let prev_decoded = miner_wallet.decode_raw_transaction(&prev_tx, None)?;

    // Extract the address and amount from the previous output being spent
    let prev_output = &prev_decoded.vout[prev_vout];
    let input_addresses = &prev_output.script_pub_key.addresses;
    let miner_input_address: String = input_addresses
        .first()
        .map(|a| format!("{}", a.clone().assume_checked()))
        .unwrap_or_default();
    let miner_input_amount: f64 = prev_output.value.to_btc();

    // Step 10: Analyze transaction outputs (where the Bitcoin is going)
    // Initialize variables to store output details
    let mut trader_output_address: String = String::new();
    let mut trader_output_amount: f64 = 0.0;
    let mut miner_change_address: String = String::new();
    let mut miner_change_amount: f64 = 0.0;

    // Iterate through all outputs to identify trader's output and miner's change
    for vout in &decoded_tx.vout {
        if let Some(addr) = &vout.script_pub_key.address {
            let addr_str = addr.clone().assume_checked().to_string();
            println!("  Address: {addr_str}, Value: {:.8}", vout.value.to_btc());

            // Check if this output is going to the trader
            if addr_str == trader_address.to_string() {
                trader_output_address = addr_str.clone();
                trader_output_amount = vout.value.to_btc();
            } else {
                // Check if this output belongs to the miner (change address)
                let info = miner_wallet.get_address_info(&addr.clone().assume_checked());
                if let Ok(address_info) = info {
                    if address_info.is_mine.unwrap_or(false) {
                        miner_change_address = addr_str.clone();
                        miner_change_amount = vout.value.to_btc();
                    }
                }
            }
        }
    }

    // Print extracted output information
    println!("miner_change_address: {miner_change_address}");
    println!("trader_output_amount: {trader_output_amount:.8}");
    println!("miner_change_amount: {miner_change_amount:.8}");
    println!("trader_output_address: {trader_output_address}");

    // Step 11: Calculate transaction fee
    // Fee = Input amount - (Output1 amount + Output2 amount)
    // This represents the difference between what was spent and what was received
    let tx_fee = miner_input_amount - (trader_output_amount + miner_change_amount);

    // Step 12: Write transaction details to output file
    // Create the output file in the parent directory
    let out_path = Path::new("../out.txt");
    let mut out_file = File::create(out_path)?;

    // Write each transaction detail on a separate line in the required format:
    writeln!(out_file, "{txid}")?; // Transaction ID
    writeln!(out_file, "{miner_input_address}")?; // Input address (miner's address)
    writeln!(out_file, "{miner_input_amount:.8}")?; // Input amount (8 decimal places for BTC)
    writeln!(out_file, "{trader_output_address}")?; // Output address (trader's address)
    writeln!(out_file, "{trader_output_amount:.8}")?; // Output amount sent to trader
    writeln!(out_file, "{miner_change_address}")?; // Change address (miner's change)
    writeln!(out_file, "{miner_change_amount:.8}")?; // Change amount returned to miner
    writeln!(out_file, "{:.8}", tx_fee.abs())?; // Transaction fee (absolute value)
    writeln!(out_file, "{block_height}")?; // Block height where transaction was confirmed
    writeln!(out_file, "{block_hash}")?; // Block hash where transaction was confirmed

    println!("Transaction details written to ../out.txt");

    Ok(())
}
