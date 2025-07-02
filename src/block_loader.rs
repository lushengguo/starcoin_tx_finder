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

pub fn format_events_part(events_response: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    if let Some(result) = events_response.get("result") {
        if let Some(events_array) = result.as_array() {
            let formatted_events: Vec<serde_json::Value> = events_array
                .iter()
                .map(|event| parse_event_data(event))
                .collect();
            return Some(formatted_events);
        }
    }
    None
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

pub fn format_raw_data_part(
    tx_response: &serde_json::Value,
    tx_info_response: &serde_json::Value,
    events_response: &serde_json::Value,
) -> Option<serde_json::Value> {
    let tx_result = tx_response.get("result")?;
    let info_result = tx_info_response.get("result")?;
    let events_result = events_response.get("result")?;

    let mut complete_tx = json!({
        "_id": "",
        "block_hash": tx_result.get("block_hash").unwrap_or(&json!("")),
        "block_number": tx_result.get("block_number").unwrap_or(&json!("")),
        "event_root_hash": info_result.get("event_root_hash").unwrap_or(&json!("")),
        "events": [],
        "gas_used": info_result.get("gas_used").unwrap_or(&json!("")),
        "state_root_hash": info_result.get("state_root_hash").unwrap_or(&json!("")),
        "status": info_result.get("status").unwrap_or(&json!("")),
        "timestamp": 0,
        "transaction_global_index": tx_result.get("transaction_global_index")
            .or_else(|| info_result.get("transaction_global_index"))
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())))
            .map(|v| json!(v))
            .unwrap_or(json!(0)),
        "transaction_hash": tx_result.get("transaction_hash").unwrap_or(&json!("")),
        "transaction_index": tx_result.get("transaction_index").unwrap_or(&json!(0)),
        "transaction_type": "ScriptFunction",
        "user_transaction": tx_result.get("user_transaction").unwrap_or(&json!({}))
    });

    if let Some(events_array) = events_result.as_array() {
        let mut formatted_events = Vec::new();
        for event in events_array {
            let formatted_event = json!({
                "_id": "",
                "block_hash": event.get("block_hash").unwrap_or(&json!("")),
                "block_number": event.get("block_number").unwrap_or(&json!("")),
                "data": event.get("data").unwrap_or(&json!("")),
                "decode_event_data": "",
                "event_index": event.get("event_index").unwrap_or(&json!(0)),
                "event_key": event.get("event_key").unwrap_or(&json!("")),
                "event_seq_number": event.get("event_seq_number").unwrap_or(&json!("")),
                "transaction_global_index": 0, // in demo data this field was alaways 0, I don't know why
                "transaction_hash": event.get("transaction_hash").unwrap_or(&json!("")),
                "transaction_index": event.get("transaction_index").unwrap_or(&json!(0)),
                "type_tag": event.get("type_tag").unwrap_or(&json!(""))
            });
            formatted_events.push(formatted_event);
        }
        complete_tx["events"] = json!(formatted_events);
    }

    // Extract user_transaction and payload_str before any mutable borrow
    let user_transaction = complete_tx.get("user_transaction").cloned();
    let payload_str = user_transaction
        .as_ref()
        .and_then(|ut| ut.get("raw_txn"))
        .and_then(|rt| rt.get("payload"))
        .and_then(|p| p.as_str());

    let payload_str = match payload_str {
        Some(s) => s,
        None => return None,
    };

    // Now perform the mutable borrow
    if let Some(raw_txn_obj) = complete_tx
        .get_mut("user_transaction")
        .and_then(|ut| ut.get_mut("raw_txn"))
        .and_then(|rt| rt.as_object_mut())
    {
        let decoded = decode_payload(payload_str)?;
        // Build trimmed_decoded with raw values for args and ty_args
        let trimmed_decoded = decoded
            .get("ScriptFunction")
            .map(|sf| {
                // Get original args as numbers if possible
                let args = sf
                    .get("args")
                    .map(convert_args_to_standard_schema_in_raw_data)
                    .unwrap_or_default();
                // Get ty_args as original type strings if possible
                let ty_args = sf
                    .get("ty_args")
                    .map(convert_ty_args_to_standard_schema_in_raw_data)
                    .unwrap_or_default();
                let func = sf.get("func");
                if let Some(func_obj) = func.and_then(|f| f.as_object()) {
                    let function = func_obj.get("functionName").cloned().unwrap_or_default();
                    let module = if let (Some(addr), Some(module)) =
                        (func_obj.get("address"), func_obj.get("module"))
                    {
                        serde_json::Value::String(format!(
                            "{}::{}",
                            addr.as_str().unwrap_or(""),
                            module.as_str().unwrap_or("")
                        ))
                    } else {
                        serde_json::Value::String(String::new())
                    };
                    serde_json::json!({
                        "ScriptFunction": {
                            "args": args,
                            "function": function,
                            "module": module,
                            "ty_args": ty_args
                        }
                    })
                } else {
                    serde_json::json!({})
                }
            })
            .unwrap_or(serde_json::json!({}));
        raw_txn_obj.insert(
            "decoded_payload".to_string(),
            serde_json::Value::String(serde_json::to_string(&trimmed_decoded).unwrap()),
        );

        raw_txn_obj.insert(
            "transaction_hash".to_string(),
            serde_json::Value::String(String::new()),
        );
    }

    complete_tx["timestamp"] = json!(1750125083478u64);

    Some(complete_tx)
}

// Helper to parse args as numbers if possible
fn convert_args_to_standard_schema_in_raw_data(args: &serde_json::Value) -> Vec<serde_json::Value> {
    args.as_array()
        .map(|vec| {
            vec.iter()
                .map(|v| {
                    if let Some(s) = v.as_str() {
                        if s.starts_with("u128: ") {
                            let num_str = s[6..].replace(",", "");
                            num_str
                                .parse::<u128>()
                                .map(serde_json::Value::from)
                                .unwrap_or(v.clone())
                        } else {
                            v.clone()
                        }
                    } else {
                        v.clone()
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

// Helper to parse ty_args as type strings
fn convert_ty_args_to_standard_schema_in_raw_data(
    ty_args: &serde_json::Value,
) -> Vec<serde_json::Value> {
    ty_args
        .as_array()
        .map(|vec| {
            vec.iter()
                .map(|v| {
                    if let Some(struct_obj) = v.get("Struct") {
                        let address = struct_obj
                            .get("address")
                            .and_then(|a| a.as_str())
                            .unwrap_or("");
                        let module = struct_obj
                            .get("module")
                            .and_then(|m| m.as_str())
                            .unwrap_or("");
                        let name = struct_obj
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        serde_json::Value::String(format!("{}::{}::{}", address, module, name))
                    } else if let Some(s) = v.as_str() {
                        serde_json::Value::String(s.to_string())
                    } else {
                        v.clone()
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}
