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
                    .filter_map(|type_tag| {
                        if let Some(type_str) = type_tag.as_str() {
                            Some(type_str.to_string())
                        } else if let Some(vector_type) = type_tag.get("Vector") {
                            vector_type.as_str().map(|inner_type| format!("Vector<{inner_type}>"))
                        } else {
                            Some(format!("{type_tag:?}"))
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn parse_module_info(module: &str) -> (String, String) {
    if let Some(idx) = module.find("::") {
        (module[..idx].to_string(), module[idx + 2..].to_string())
    } else {
        (module.to_string(), String::new())
    }
}

fn parse_ty_arg(ty_str: &str) -> serde_json::Value {
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
}

fn create_script_function_json(
    address: String,
    module_name: String,
    function: String,
    ty_args: Vec<serde_json::Value>,
    args: Vec<serde_json::Value>,
) -> serde_json::Value {
    json!({
        "ScriptFunction": {
            "func": {
                "address": address,
                "module": module_name,
                "functionName": function
            },
            "ty_args": ty_args,
            "args": args
        }
    })
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
            let (address, module_name) = parse_module_info(&module);

            let param = module + "::" + &function;
            let function_signature = resolve_function(MAINNET_WS_URL, &param).await;
            let type_tags = collect_type_tags(&function_signature);

            let ty_args = sf
                .ty_args()
                .iter()
                .map(|ty| parse_ty_arg(&ty.to_string()))
                .collect::<Vec<_>>();

            let args = sf
                .args()
                .iter()
                .enumerate()
                .map(|(index, arg)| annotate_arg(arg, &type_tags, index))
                .collect::<Vec<_>>();

            Some(create_script_function_json(address, module_name, function, ty_args, args))
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

    #[test]
    fn test_annotate_arg() {
        let type_tags = vec![
            "Signer".to_string(),
            "Address".to_string(),
            "U128".to_string(),
        ];

        // Test with valid index
        let arg1 = vec![0x01, 0x02, 0x03, 0x04];
        let result1 = annotate_arg(&arg1, &type_tags, 0);
        assert_eq!(
            result1,
            serde_json::Value::String("Address: 01020304".to_string())
        );

        // Test with another valid index
        let arg2 = vec![0xab, 0xcd, 0xef];
        let result2 = annotate_arg(&arg2, &type_tags, 1);
        assert_eq!(
            result2,
            serde_json::Value::String("U128: abcdef".to_string())
        );

        // Test with index out of bounds
        let arg3 = vec![0xff, 0xee];
        let result3 = annotate_arg(&arg3, &type_tags, 2);
        assert_eq!(
            result3,
            serde_json::Value::String("no_type ffee".to_string())
        );

        // Test with empty type_tags
        let empty_tags: Vec<String> = vec![];
        let result4 = annotate_arg(&arg1, &empty_tags, 0);
        assert_eq!(
            result4,
            serde_json::Value::String("no_type 01020304".to_string())
        );
    }

    #[test]
    fn test_collect_type_tags() {
        // Test with valid JSON response
        let valid_response = json!({
            "result": {
                "args": [
                    {"type_tag": "Signer", "name": "p0"},
                    {"type_tag": "Address", "name": "p1"},
                    {"type_tag": {"Vector": "U8"}, "name": "p2"},
                    {"type_tag": "U128", "name": "p3"}
                ]
            }
        });
        let result1 = collect_type_tags(&Some(valid_response));
        assert_eq!(result1, vec!["Signer", "Address", "Vector<U8>", "U128"]);

        // Test with Vector type having unknown inner type (should be filtered out)
        let vector_unknown_response = json!({
            "result": {
                "args": [
                    {"type_tag": "Signer", "name": "p0"},
                    {"type_tag": {"Vector": null}, "name": "p1"},
                    {"type_tag": "U128", "name": "p2"}
                ]
            }
        });
        let result2 = collect_type_tags(&Some(vector_unknown_response));
        assert_eq!(result2, vec!["Signer", "U128"]);

        // Test with complex type tag
        let complex_response = json!({
            "result": {
                "args": [
                    {"type_tag": {"Struct": {"address": "0x1", "module": "STC"}}, "name": "p0"}
                ]
            }
        });
        let result3 = collect_type_tags(&Some(complex_response));
        assert_eq!(result3.len(), 1);
        assert!(result3[0].contains("Struct"));

        // Test with None response
        let result4 = collect_type_tags(&None);
        assert_eq!(result4, Vec::<String>::new());

        // Test with empty args array
        let empty_args_response = json!({
            "result": {
                "args": []
            }
        });
        let result5 = collect_type_tags(&Some(empty_args_response));
        assert_eq!(result5, Vec::<String>::new());

        // Test with missing result field
        let no_result_response = json!({
            "error": "something went wrong"
        });
        let result6 = collect_type_tags(&Some(no_result_response));
        assert_eq!(result6, Vec::<String>::new());

        // Test with missing args field
        let no_args_response = json!({
            "result": {
                "other_field": "value"
            }
        });
        let result7 = collect_type_tags(&Some(no_args_response));
        assert_eq!(result7, Vec::<String>::new());

        // Test with args not being an array
        let invalid_args_response = json!({
            "result": {
                "args": "not_an_array"
            }
        });
        let result8 = collect_type_tags(&Some(invalid_args_response));
        assert_eq!(result8, Vec::<String>::new());
    }

    #[test]
    fn test_parse_module_info() {
        // Test with valid module format
        let result1 = parse_module_info("0x00000000000000000000000000000001::TransferScripts");
        assert_eq!(
            result1,
            (
                "0x00000000000000000000000000000001".to_string(),
                "TransferScripts".to_string()
            )
        );

        // Test with nested module format
        let result2 = parse_module_info("0x1::Token::STC");
        assert_eq!(result2, ("0x1".to_string(), "Token::STC".to_string()));

        // Test with module without namespace separator
        let result3 = parse_module_info("SimpleModule");
        assert_eq!(result3, ("SimpleModule".to_string(), String::new()));

        // Test with empty string
        let result4 = parse_module_info("");
        assert_eq!(result4, (String::new(), String::new()));
    }

    #[test]
    fn test_parse_ty_arg() {
        // Test with valid Struct type
        let result1 = parse_ty_arg("0x1::STC::STC");
        let expected1 = json!({
            "Struct": {
                "address": "0x1",
                "module": "STC",
                "name": "STC",
                "type_params": []
            }
        });
        assert_eq!(result1, expected1);

        // Test with incomplete module path (only address::module)
        let result2 = parse_ty_arg("0x1::Token");
        assert_eq!(result2, json!("0x1::Token"));

        // Test with simple type (no :: separator)
        let result3 = parse_ty_arg("U64");
        assert_eq!(result3, json!("U64"));

        // Test with complex nested type
        let result4 = parse_ty_arg("0x00000000000000000000000000000001::Account::Balance");
        let expected4 = json!({
            "Struct": {
                "address": "0x00000000000000000000000000000001",
                "module": "Account",
                "name": "Balance",
                "type_params": []
            }
        });
        assert_eq!(result4, expected4);
    }

    #[test]
    fn test_create_script_function_json() {
        let address = "0x1".to_string();
        let module_name = "TransferScripts".to_string();
        let function = "peer_to_peer".to_string();
        let ty_args = vec![json!({
            "Struct": {
                "address": "0x1",
                "module": "STC",
                "name": "STC",
                "type_params": []
            }
        })];
        let args = vec![
            json!("Signer: 0x01"),
            json!("Address: 0x02"),
            json!("U128: 0x03"),
        ];

        let result = create_script_function_json(
            address,
            module_name,
            function,
            ty_args.clone(),
            args.clone(),
        );

        let expected = json!({
            "ScriptFunction": {
                "func": {
                    "address": "0x1",
                    "module": "TransferScripts",
                    "functionName": "peer_to_peer"
                },
                "ty_args": ty_args,
                "args": args
            }
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_event_data() {
        // Test with complete event data
        let complete_event = json!({
            "data": "0x1234567890abcdef",
            "type_tag": "0x00000000000000000000000000000001::TransferScripts::TransferEvent",
            "event_key": "0x12345678901234567890123456789012000000000000000000000001",
            "event_seq_number": "12345"
        });
        let result1 = parse_event_data(&complete_event);
        assert_eq!(result1["Data"], json!("0x1234567890abcdef"));
        assert_eq!(result1["Module"], json!("TransferScripts"));
        assert_eq!(result1["Name"], json!("TransferEvent"));
        assert_eq!(result1["Seq"], json!("12,345"));
        assert!(result1.get("Key").is_some());

        // Test with minimal event data (only data field)
        let minimal_event = json!({
            "data": "0xabcd"
        });
        let result2 = parse_event_data(&minimal_event);
        assert_eq!(result2["Data"], json!("0xabcd"));
        assert!(result2.get("Module").is_none());
        assert!(result2.get("Name").is_none());
        assert!(result2.get("Key").is_none());
        assert!(result2.get("Seq").is_none());

        // Test with missing data field
        let no_data_event = json!({
            "type_tag": "0x1::Token::TransferEvent",
            "event_seq_number": "100"
        });
        let result3 = parse_event_data(&no_data_event);
        assert!(result3.get("Data").is_none());
        assert_eq!(result3["Module"], json!("Token"));
        assert_eq!(result3["Name"], json!("TransferEvent"));
        assert_eq!(result3["Seq"], json!("100"));

        // Test with different type_tag formats

        // Simple type_tag without generics
        let simple_type_event = json!({
            "type_tag": "0x1::Account::DepositEvent"
        });
        let result4 = parse_event_data(&simple_type_event);
        assert_eq!(result4["Module"], json!("Account"));
        assert_eq!(result4["Name"], json!("DepositEvent"));

        // Type_tag with generics
        let generic_type_event = json!({
            "type_tag": "0x1::Token::TransferEvent<0x1::STC::STC>"
        });
        let result5 = parse_event_data(&generic_type_event);
        assert_eq!(result5["Module"], json!("Token"));
        assert_eq!(result5["Name"], json!("TransferEvent<0x1::STC::STC>"));

        // Invalid type_tag format (missing module separator)
        let invalid_type_event = json!({
            "type_tag": "InvalidTypeTag"
        });
        let result6 = parse_event_data(&invalid_type_event);
        assert!(result6.get("Module").is_none());
        assert!(result6.get("Name").is_none());

        // Type_tag with only one separator
        let single_sep_type_event = json!({
            "type_tag": "0x1::OnlyModule"
        });
        let result7 = parse_event_data(&single_sep_type_event);
        assert!(result7.get("Module").is_none());
        assert!(result7.get("Name").is_none());

        // Test various event_key formats

        // Valid event_key
        let valid_key_event = json!({
            "event_key": "0x12345678901234567890123456789012000000000000000000000001"
        });
        let result8 = parse_event_data(&valid_key_event);
        let key = result8.get("Key").unwrap();
        assert_eq!(
            key["address"],
            json!("0x7890123456789012000000000000000000000001")
        );
        assert!(key.get("salt").is_some());

        // Event_key too short
        let short_key_event = json!({
            "event_key": "0x123456"
        });
        let result9 = parse_event_data(&short_key_event);
        assert!(result9.get("Key").is_none());

        // Event_key with invalid hex
        let invalid_hex_event = json!({
            "event_key": "0xzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz000000000000000000000001"
        });
        let result10 = parse_event_data(&invalid_hex_event);
        assert!(result10.get("Key").is_none());

        // Event_key exactly 42 characters (minimum length)
        let min_length_key_event = json!({
            "event_key": "0x123456789012345678901234567890ab000000000000000000000001"
        });
        let result11 = parse_event_data(&min_length_key_event);
        assert!(result11.get("Key").is_some());

        // Test event_seq_number variations

        // Valid sequence number
        let valid_seq_event = json!({
            "event_seq_number": "1000000"
        });
        let result12 = parse_event_data(&valid_seq_event);
        assert_eq!(result12["Seq"], json!("1,000,000"));

        // Invalid sequence number (not a number)
        let invalid_seq_event = json!({
            "event_seq_number": "not_a_number"
        });
        let result13 = parse_event_data(&invalid_seq_event);
        assert!(result13.get("Seq").is_none());

        // Sequence number as integer instead of string
        let int_seq_event = json!({
            "event_seq_number": 12345
        });
        let result14 = parse_event_data(&int_seq_event);
        assert!(result14.get("Seq").is_none());

        // Test edge cases

        // Empty JSON object
        let empty_event = json!({});
        let result15 = parse_event_data(&empty_event);
        assert_eq!(result15, json!({}));

        // All fields present but with null values
        let null_fields_event = json!({
            "data": null,
            "type_tag": null,
            "event_key": null,
            "event_seq_number": null
        });
        let result16 = parse_event_data(&null_fields_event);
        assert!(result16.get("Data").is_some()); // null is cloned as null
        assert!(result16.get("Module").is_none());
        assert!(result16.get("Name").is_none());
        assert!(result16.get("Key").is_none());
        assert!(result16.get("Seq").is_none());

        // Very long type_tag with multiple generics
        let complex_type_event = json!({
            "type_tag": "0x00000000000000000000000000000001::ComplexModule::ComplexEvent<0x1::Token::STC, 0x2::NFT::Collection>"
        });
        let result17 = parse_event_data(&complex_type_event);
        assert_eq!(result17["Module"], json!("ComplexModule"));
        assert_eq!(
            result17["Name"],
            json!("ComplexEvent<0x1::Token::STC, 0x2::NFT::Collection>")
        );
    }
}
