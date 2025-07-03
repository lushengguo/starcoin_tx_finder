use bcs::from_bytes;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use starcoin_types::transaction::TransactionPayload;
use tokio_tungstenite::connect_async;
use url::Url;

pub fn decode_payload(hex_payload: &str) -> Option<serde_json::Value> {
    let payload_bytes = hex::decode(hex_payload.strip_prefix("0x")?).ok()?;
    let payload: TransactionPayload = from_bytes(&payload_bytes).ok()?;

    match payload {
        TransactionPayload::ScriptFunction(sf) => {
            let module = sf.module().to_string();
            let function = sf.function().to_string();
            let ty_args = sf
                .ty_args()
                .iter()
                .map(|ty| {
                    let ty_str = ty.to_string();
                    // Try to parse Struct type: 0x...::Module::Name
                    if let Some((address, rest)) = ty_str.split_once("::") {
                        if let Some((module, name)) = rest.split_once("::") {
                            json!({
                                "Struct": {
                                    "address": address,
                                    "module": module,
                                    "name": name,
                                    "type_params": []
                                }
                            })
                        } else {
                            json!(ty_str)
                        }
                    } else {
                        json!(ty_str)
                    }
                })
                .collect::<Vec<_>>();
            let args = sf
                .args()
                .iter()
                .map(|arg| {
                    if arg.len() == 16 {
                        let mut buf = [0u8; 16];
                        buf.copy_from_slice(arg);
                        let value = u128::from_le_bytes(buf);
                        let formatted = format!("u128: {}", thousands_separator(value));
                        serde_json::Value::String(formatted)
                    } else {
                        serde_json::Value::String(format!("0x{}", hex::encode(arg)))
                    }
                })
                .collect::<Vec<_>>();

            // Extract address from module string (format: 0x...::ModuleName)
            let (address, module_name) = if let Some(idx) = module.find("::") {
                (module[..idx].to_string(), module[idx + 2..].to_string())
            } else {
                (module.clone(), String::new())
            };

            Some(json!({
                "ScriptFunction": {
                    "func": {
                        "address": address,
                        "module": module_name,
                        "functionName": function
                    },
                    "ty_args": ty_args,
                    "args": args
                }
            }))
        }
        _ => None,
    }
}

fn thousands_separator(n: u128) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;
    for c in s.chars().rev() {
        if count != 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }
    result.chars().rev().collect()
}

pub async fn get_timestamp_from_block_header(ws_url: &str, block_hash: &str) -> Option<i64> {
    let url = Url::parse(ws_url).ok()?;
    let (mut ws_stream, _) = connect_async(url).await.ok()?;

    let request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "chain.get_block_by_hash",
        "params": [block_hash]
    });

    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Text(
            request.to_string(),
        ))
        .await
        .ok()?;

    while let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(response))) =
        ws_stream.next().await
    {
        let json: serde_json::Value = serde_json::from_str(&response).ok()?;
        
        if let Some(result) = json.get("result") {
            if let Some(header) = result.get("header") {
                if let Some(timestamp) = header.get("timestamp") {
                    return timestamp.as_str().and_then(|s| s.parse::<i64>().ok());
                }
            }
        }
    }

    None
}

pub async fn get_transaction_events_ws(
    ws_url: &str,
    transaction_hash: &str,
) -> Option<serde_json::Value> {
    let url = Url::parse(ws_url).ok()?;

    let request_payload = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "chain.get_events_by_txn_hash",
        "params": [transaction_hash]
    });

    let (mut ws_stream, _) = connect_async(url).await.ok()?;
    let msg = tokio_tungstenite::tungstenite::Message::Text(request_payload.to_string());
    ws_stream.send(msg).await.ok()?;

    while let Some(Ok(msg)) = ws_stream.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            return serde_json::from_str(&text).ok();
        }
    }

    None
}

pub async fn get_transaction_by_hash_ws(
    ws_url: &str,
    transaction_hash: &str,
) -> Option<serde_json::Value> {
    let url = Url::parse(ws_url).ok()?;

    let request_payload = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "chain.get_transaction",
        "params": [transaction_hash]
    });

    let (mut ws_stream, _) = connect_async(url).await.ok()?;
    let msg = tokio_tungstenite::tungstenite::Message::Text(request_payload.to_string());
    ws_stream.send(msg).await.ok()?;

    while let Some(Ok(msg)) = ws_stream.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            return serde_json::from_str(&text).ok();
        }
    }

    None
}

pub async fn get_transaction_info_ws(
    ws_url: &str,
    transaction_hash: &str,
) -> Option<serde_json::Value> {
    let url = Url::parse(ws_url).ok()?;

    let request_payload = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "chain.get_transaction_info",
        "params": [transaction_hash]
    });

    let (mut ws_stream, _) = connect_async(url).await.ok()?;
    let msg = tokio_tungstenite::tungstenite::Message::Text(request_payload.to_string());
    ws_stream.send(msg).await.ok()?;

    while let Some(Ok(msg)) = ws_stream.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            return serde_json::from_str(&text).ok();
        }
    }

    None
}

pub fn parse_event_data(event: &serde_json::Value) -> serde_json::Value {
    let mut parsed_event = serde_json::json!({});

    if let Some(data) = event.get("data") {
        parsed_event["Data"] = data.clone();
    }

    if let Some(type_tag) = event.get("type_tag") {
        if let Some(type_str) = type_tag.as_str() {
            if let Some(module_start) = type_str.find("::") {
                let after_address = &type_str[module_start + 2..];
                if let Some(module_end) = after_address.find("::") {
                    let module_name = &after_address[..module_end];
                    parsed_event["Module"] = serde_json::Value::String(module_name.to_string());

                    let event_name_with_generics = &after_address[module_end + 2..];
                    parsed_event["Name"] =
                        serde_json::Value::String(event_name_with_generics.to_string());
                }
            }
        }
    }

    if let Some(event_key) = event.get("event_key") {
        if let Some(key_str) = event_key.as_str() {
            if key_str.len() >= 42 {
                let salt_part = &key_str[2..18];
                let address_part = &key_str[18..];

                if let Ok(salt_bytes) = hex::decode(salt_part) {
                    if salt_bytes.len() >= 4 {
                        let salt = u32::from_le_bytes([
                            salt_bytes[0],
                            salt_bytes[1],
                            salt_bytes[2],
                            salt_bytes[3],
                        ]);
                        parsed_event["Key"] = serde_json::json!({
                            "address": format!("0x{}", address_part),
                            "salt": salt.to_string()
                        });
                    }
                }
            }
        }
    }

    if let Some(seq) = event.get("event_seq_number") {
        if let Some(seq_str) = seq.as_str() {
            if let Ok(seq_num) = seq_str.parse::<u64>() {
                parsed_event["Seq"] =
                    serde_json::Value::String(thousands_separator(seq_num as u128));
            }
        }
    }

    parsed_event
}

pub fn get_decoded_payload(tx_response: &serde_json::Value) -> Option<serde_json::Value> {
    tx_response
        .get("result")
        .and_then(|r| r.get("user_transaction"))
        .and_then(|u| u.get("raw_txn"))
        .and_then(|t| t.get("payload"))
        .and_then(|p| p.as_str())
        .and_then(decode_payload)
}
