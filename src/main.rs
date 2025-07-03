mod block_loader;
mod formatter;
use crate::formatter::get_standard_format_output;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <transaction_hash>", args[0]);
        eprintln!(
            "Example: {} 0xe29d7508fe37d756d83e672be53843d10d084f9c69fca1b7e9a34ea8eb96f918",
            args[0]
        );
        std::process::exit(1);
    }

    let tx_hash = &args[1];

    match get_standard_format_output(tx_hash).await {
        Some((json, block_number)) => {
            if block_number >= 20000 {
                println!(
                    "this transaction is not found in the first 20000 blocks, it was found in block number: {}",
                    block_number
                );
                println!("{}", serde_json::to_string_pretty(&json).unwrap());
            } else {
                println!("{}", serde_json::to_string_pretty(&json).unwrap());
            }
        }
        None => {
            println!("Transaction not found.");
        }
    }
}
