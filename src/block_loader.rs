use crate::MAINNET_WS_URL;
use bcs::from_bytes;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use starcoin_types::transaction::TransactionPayload;
use tokio_tungstenite::connect_async;
use url::Url;

fn annotate_arg(arg: &[u8], type_tags: &[String], index: usize) -> serde_json::Value {
    if index + 1 < type_tags.len() {
        serde_json::Value::String(format!("{}: {}", &type_tags[index + 1], hex::encode(arg)))
    } else {
        serde_json::Value::String(format!("no_type {}", hex::encode(arg)))
    }
}

fn collect_type_tags(resolve_function_response: &Option<serde_json::Value>) -> Vec<String> {
    if let Some(resolve_function_response) = resolve_function_response {
        resolve_function_response
            .get("result")
            .and_then(|result| result.get("args"))
            .and_then(|args| args.as_array())
            .map(|args_array| {
                args_array
                    .iter()
                    .filter_map(|arg| arg.get("type_tag"))
                    .map(|type_tag| {
                        if let Some(type_str) = type_tag.as_str() {
                            type_str.to_string()
                        } else if let Some(vector_type) = type_tag.get("Vector") {
                            format!("Vector<{}>", vector_type.as_str().unwrap_or("Unknown"))
                        } else {
                            format!("{type_tag:?}")
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub async fn decode_payload_for_standalone_decoded_payload(
    hex_payload: &str,
) -> Option<serde_json::Value> {
    let payload_bytes = hex::decode(hex_payload.strip_prefix("0x")?).ok()?;
    let payload: TransactionPayload = from_bytes(&payload_bytes).ok()?;

    match payload {
        TransactionPayload::ScriptFunction(sf) => {
            let module = sf.module().to_string();
            let function = sf.function().to_string();
            let (address, module_name) = if let Some(idx) = module.find("::") {
                (module[..idx].to_string(), module[idx + 2..].to_string())
            } else {
                (module.clone(), String::new())
            };

            let param = module + "::" + &function;
            let function_signature = resolve_function(MAINNET_WS_URL, &param).await;

            let type_tags = collect_type_tags(&function_signature);

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
                .enumerate()
                .map(|(index, arg)| annotate_arg(arg, &type_tags, index))
                .collect::<Vec<_>>();

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
    for (count, c) in s.chars().rev().enumerate() {
        if count != 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
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

        if let Some(result) = json.get("result")
            && let Some(header) = result.get("header")
                && let Some(timestamp) = header.get("timestamp") {
                    return timestamp.as_str().and_then(|s| s.parse::<i64>().ok());
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
        "params": [transaction_hash, {"decode": true}]
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

    if let Some(type_tag) = event.get("type_tag")
        && let Some(type_str) = type_tag.as_str()
            && let Some(module_start) = type_str.find("::") {
                let after_address = &type_str[module_start + 2..];
                if let Some(module_end) = after_address.find("::") {
                    let module_name = &after_address[..module_end];
                    parsed_event["Module"] = serde_json::Value::String(module_name.to_string());

                    let event_name_with_generics = &after_address[module_end + 2..];
                    parsed_event["Name"] =
                        serde_json::Value::String(event_name_with_generics.to_string());
                }
            }

    if let Some(event_key) = event.get("event_key")
        && let Some(key_str) = event_key.as_str()
            && key_str.len() >= 42 {
                let salt_part = &key_str[2..18];
                let address_part = &key_str[18..];

                if let Ok(salt_bytes) = hex::decode(salt_part)
                    && salt_bytes.len() >= 4 {
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

    if let Some(seq) = event.get("event_seq_number")
        && let Some(seq_str) = seq.as_str()
            && let Ok(seq_num) = seq_str.parse::<u64>() {
                parsed_event["Seq"] =
                    serde_json::Value::String(thousands_separator(seq_num as u128));
            }

    parsed_event
}

pub async fn get_standalone_decoded_payload(
    tx_response: &serde_json::Value,
) -> Option<serde_json::Value> {
    let payload_opt = tx_response
        .get("result")
        .and_then(|r| r.get("user_transaction"))
        .and_then(|u| u.get("raw_txn"))
        .and_then(|t| t.get("payload"))
        .and_then(|p| p.as_str());

    let original_decoded_payload = tx_response
        .get("result")
        .and_then(|r| r.get("user_transaction"))
        .and_then(|u| u.get("raw_txn"))
        .and_then(|t| t.get("decoded_payload"));

    if let Some(payload_str) = payload_opt {
        match decode_payload_for_standalone_decoded_payload(payload_str).await {
            Some(decoded_payload) => Some(decoded_payload),
            None => original_decoded_payload.cloned(),
        }
    } else {
        original_decoded_payload.cloned()
    }
}

async fn resolve_function(ws_url: &str, arg: &str) -> Option<serde_json::Value> {
    let url = Url::parse(ws_url).ok()?;

    let request_payload = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "contract.resolve_function",
        "params": [arg]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thousands_separator_basic() {
        assert_eq!(thousands_separator(0), "0");
        assert_eq!(thousands_separator(123), "123");
        assert_eq!(thousands_separator(1234), "1,234");
        assert_eq!(thousands_separator(12345), "12,345");
        assert_eq!(thousands_separator(123456), "123,456");
        assert_eq!(thousands_separator(1234567), "1,234,567");
        assert_eq!(thousands_separator(12345678), "12,345,678");
        assert_eq!(thousands_separator(123456789), "123,456,789");
        assert_eq!(thousands_separator(1234567890), "1,234,567,890");
    }

    #[test]
    fn test_thousands_separator_edge_cases() {
        assert_eq!(thousands_separator(999), "999");
        assert_eq!(thousands_separator(1000), "1,000");
        assert_eq!(thousands_separator(1001), "1,001");
        assert_eq!(thousands_separator(9999), "9,999");
        assert_eq!(thousands_separator(10000), "10,000");
        assert_eq!(thousands_separator(100000), "100,000");
        assert_eq!(thousands_separator(1000000), "1,000,000");
    }

    #[test]
    fn test_thousands_separator_large_numbers() {
        assert_eq!(
            thousands_separator(12345678901234567890_u128),
            "12,345,678,901,234,567,890"
        );
        assert_eq!(
            thousands_separator(u128::MAX),
            "340,282,366,920,938,463,463,374,607,431,768,211,455"
        );
    }

    #[test]
    fn test_thousands_separator_boundaries() {
        // Test around thousand boundaries
        assert_eq!(thousands_separator(999), "999");
        assert_eq!(thousands_separator(1000), "1,000");

        // Test around million boundaries
        assert_eq!(thousands_separator(999999), "999,999");
        assert_eq!(thousands_separator(1000000), "1,000,000");

        // Test around billion boundaries
        assert_eq!(thousands_separator(999999999), "999,999,999");
        assert_eq!(thousands_separator(1000000000), "1,000,000,000");
    }
}
