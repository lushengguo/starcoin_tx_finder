use crate::MAINNET_WS_URL;
use crate::block_loader;
use serde_json::json;

use block_loader::{
    get_standalone_decoded_payload, get_timestamp_from_block_header, get_transaction_by_hash_ws,
    get_transaction_events_ws, get_transaction_info_ws, parse_event_data,
};

pub async fn format_raw_data_part(
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
    if let Some(raw_txn_obj) = complete_tx
        .get_mut("user_transaction")
        .and_then(|ut| ut.get_mut("raw_txn"))
        .and_then(|rt| rt.as_object_mut())
    {
        raw_txn_obj.insert(
            "transaction_hash".to_string(),
            tx_result
                .get("transaction_hash")
                .cloned()
                .unwrap_or(json!("")),
        );
    };

    // get_timestamp_from_block_header is async, so we must await it and assign the result
    let timestamp = get_timestamp_from_block_header(
        MAINNET_WS_URL,
        tx_result
            .get("block_hash")
            .and_then(|h| h.as_str())
            .unwrap_or(""),
    )
    .await
    .unwrap_or(0);
    complete_tx["timestamp"] = json!(timestamp);

    Some(complete_tx)
}

// Helper to parse ty_args as type strings
// fn convert_ty_args_to_standard_schema_in_raw_data(
//     ty_args: &serde_json::Value,
// ) -> Vec<serde_json::Value> {
//     ty_args
//         .as_array()
//         .map(|vec| {
//             vec.iter()
//                 .map(|v| {
//                     if let Some(struct_obj) = v.get("Struct") {
//                         let address = struct_obj
//                             .get("address")
//                             .and_then(|a| a.as_str())
//                             .unwrap_or("");
//                         let module = struct_obj
//                             .get("module")
//                             .and_then(|m| m.as_str())
//                             .unwrap_or("");
//                         let name = struct_obj
//                             .get("name")
//                             .and_then(|n| n.as_str())
//                             .unwrap_or("");
//                         serde_json::Value::String(format!("{}::{}::{}", address, module, name))
//                     } else if let Some(s) = v.as_str() {
//                         serde_json::Value::String(s.to_string())
//                     } else {
//                         v.clone()
//                     }
//                 })
//                 .collect()
//         })
//         .unwrap_or_default()
// }

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

pub async fn get_standard_format_output(tx_hash: &str) -> Option<(serde_json::Value, u64)> {
    let tx_response = get_transaction_by_hash_ws(MAINNET_WS_URL, tx_hash).await;
    let block_number = match tx_response
        .as_ref()
        .and_then(|tx| tx.get("result"))
        .and_then(|r| r.get("block_number"))
        .and_then(|n| n.as_str())
        .and_then(|s| s.parse::<u64>().ok())
    {
        Some(n) => n,
        None => {
            println!("not found in any blocks");
            return None;
        }
    };

    let tx_info_response = get_transaction_info_ws(MAINNET_WS_URL, tx_hash).await;
    let events_response = get_transaction_events_ws(MAINNET_WS_URL, tx_hash).await;

    let mut json: serde_json::Value = serde_json::json!({
        "events": serde_json::Value::Null,
        "raw_data": serde_json::Value::Null,
        "decoded_payload": serde_json::Value::Null
    });

    if let (Some(tx), Some(info), Some(events)) = (tx_response, tx_info_response, events_response) {
        if let Some(complete_format) = format_raw_data_part(&tx, &info, &events).await {
            json["raw_data"] = serde_json::to_value(&complete_format).unwrap();
        }

        let decoded_payload = get_standalone_decoded_payload(&tx).await;
        if let Some(payload) = decoded_payload {
            json["decoded_payload"] = serde_json::to_value(&payload).unwrap();
        }

        if let Some(formatted_events) = format_events_part(&events) {
            json["events"] = serde_json::to_value(&formatted_events).unwrap();
        }
    }

    Some((json, block_number))
}

mod test {
    use super::*;
    use serde_json::Value;

    fn assert_json_eq(a: &Value, b: &Value) {
        fn compare(a: &Value, b: &Value, path: &str) {
            // Data field is different between what we got from main node and ui
            if path.ends_with("Data") ||
            // ingore decoded_payload in raw_data, it was generated by remote main node
            // and some standard data we got from https://stcscan.io/ may leave this field empty
            // 
            // and the standalone decoded payload we do not make it exactly same as ui's return
            // we do type annotate only
              path.ends_with("decoded_payload")
            {
                return;
            }
            match (a, b) {
                (Value::Object(map_a), Value::Object(map_b)) => {
                    for (k, va) in map_a {
                        let new_path = if path.is_empty() {
                            k.clone()
                        } else {
                            format!("{}.{}", path, k)
                        };
                        match map_b.get(k) {
                            Some(vb) => compare(va, vb, &new_path),
                            None => panic!(
                                "Key '{}' present in left but missing in right at path '{}', value: {}",
                                k, new_path, va
                            ),
                        }
                    }
                    for (k, _) in map_b {
                        if !map_a.contains_key(k) {
                            let new_path = if path.is_empty() {
                                k.clone()
                            } else {
                                format!("{}.{}", path, k)
                            };
                            panic!(
                                "Key '{}' present in right but missing in left at path '{}'",
                                k, new_path
                            );
                        }
                    }
                }
                (Value::Array(arr_a), Value::Array(arr_b)) => {
                    if arr_a.len() != arr_b.len() {
                        panic!(
                            "Array length mismatch at '{}': left={}, right={}",
                            path,
                            arr_a.len(),
                            arr_b.len()
                        );
                    }
                    for (i, (va, vb)) in arr_a.iter().zip(arr_b.iter()).enumerate() {
                        compare(va, vb, &format!("{}[{}]", path, i));
                    }
                }
                _ => {
                    if a != b {
                        panic!(
                            "Value mismatch at '{}': left={}, right={}",
                            path,
                            serde_json::to_string_pretty(a).unwrap(),
                            serde_json::to_string_pretty(b).unwrap()
                        );
                    }
                }
            }
        }
        compare(a, b, "");
    }

    async fn assert_standard_format_eq_to(tx_hash: &str, expected: &serde_json::Value) {
        let result = get_standard_format_output(tx_hash).await;
        assert!(result.is_some(), "Transaction not found");
        let (json, _) = result.unwrap();
        assert_json_eq(&json, expected);
    }

    #[tokio::test]
    async fn test_get_standard_format_output_1() {
        let tx_hash = "0xe29d7508fe37d756d83e672be53843d10d084f9c69fca1b7e9a34ea8eb96f918";

        let events = r#"
            [
                {
                    "Data": "0x00000000000000000e78d000000000000bfa368dcc0090000000000000000000000000000567b957b9701000",
                    "Module": "Oracle",
                    "Name": "OracleUpdateEvent<0x82e35b34096f32c42061717c06e44a59::BTC_USD::BTC_USD, u128>",
                    "Key": {
                        "address": "0x82e35b34096f32c42061717c06e44a59",
                        "salt": "4"
                    },
                    "Seq": "36,326"
                }
            ]
        "#;

        let raw_data = r#"
            {
                "_id": "",
                "block_hash": "0x7c31a1ba0eed615fe20ee0addb8b0ecf6f0bb8829b1404ee34eb03d24fe30f36",
                "block_number": "24789529",
                "event_root_hash": "0xd183abcc1747ed00b38523c95b2b4edad48e3819b25f2ac58512f96464f1ca23",
                "events": [
                    {
                    "_id": "",
                    "block_hash": "0x7c31a1ba0eed615fe20ee0addb8b0ecf6f0bb8829b1404ee34eb03d24fe30f36",
                    "block_number": "24789529",
                    "data": "0x0000000000000000e78d000000000000bfa368dcc00900000000000000000000567b957b97010000",
                    "decode_event_data": "",
                    "event_index": 0,
                    "event_key": "0x040000000000000082e35b34096f32c42061717c06e44a59",
                    "event_seq_number": "36326",
                    "transaction_global_index": 0,
                    "transaction_hash": "0xe29d7508fe37d756d83e672be53843d10d084f9c69fca1b7e9a34ea8eb96f918",
                    "transaction_index": 1,
                    "type_tag": "0x00000000000000000000000000000001::Oracle::OracleUpdateEvent<0x82e35b34096f32c42061717c06e44a59::BTC_USD::BTC_USD, u128>"
                    }
                ],
                "gas_used": "42173",
                "state_root_hash": "0xceb1a29e30bc95ffc731b3b4a9e44ad2e5c464e9548a53059bfc0269f5f2327e",
                "status": "Executed",
                "timestamp": 1750125083478,
                "transaction_global_index": 26353670,
                "transaction_hash": "0xe29d7508fe37d756d83e672be53843d10d084f9c69fca1b7e9a34ea8eb96f918",
                "transaction_index": 1,
                "transaction_type": "ScriptFunction",
                "user_transaction": {
                    "authenticator": {
                    "Ed25519": {
                        "public_key": "0x671f257c6c31231bb272fb67e3090b1f6218010a2e7e31e677ce56924ae12074",
                        "signature": "0xd0efb48dfc703349353a83f85f3323ee0639a74a4a1b8d3007b1f3b70a4adeb704c9e8316d074b27f5a7b8f87d8239c1c06f28bec53e18fa5046448d240e8f0e"
                    }
                    },
                    "raw_txn": {
                    "chain_id": 1,
                    "decoded_payload": "{\"ScriptFunction\":{\"args\":[10723936215999],\"function\":\"update\",\"module\":\"0x00000000000000000000000000000001::PriceOracleScripts\",\"ty_args\":[\"0x82e35b34096f32c42061717c06e44a59::BTC_USD::BTC_USD\"]}}",
                    "expiration_timestamp_secs": "1750132277",
                    "gas_token_code": "0x1::STC::STC",
                    "gas_unit_price": "1",
                    "max_gas_amount": "10000000",
                    "payload": "0x02000000000000000000000000000000011250726963654f7261636c655363726970747306757064617465010782e35b34096f32c42061717c06e44a59074254435f555344074254435f555344000110bfa368dcc00900000000000000000000",
                    "sender": "0x82e35b34096f32c42061717c06e44a59",
                    "sequence_number": "311007",
                    "transaction_hash": "0xe29d7508fe37d756d83e672be53843d10d084f9c69fca1b7e9a34ea8eb96f918"
                    },
                    "transaction_hash": "0xe29d7508fe37d756d83e672be53843d10d084f9c69fca1b7e9a34ea8eb96f918"
                }
            }
        "#;

        let decoded_payload = r#"
            {
                "ScriptFunction": {
                    "func": {
                    "address": "0x00000000000000000000000000000001",
                    "module": "PriceOracleScripts",
                    "functionName": "update"
                    },
                    "ty_args": [
                    {
                        "Struct": {
                        "module": "BTC_USD",
                        "name": "BTC_USD",
                        "type_params": [],
                        "address": "0x82e35b34096f32c42061717c06e44a59"
                        }
                    }
                    ],
                    "args": [
                    "u128: 10,723,936,215,999"
                    ]
                }
            }
        "#;

        let json = serde_json::json!({
            "events" : serde_json::from_str::<serde_json::Value>(events).unwrap(),
            "raw_data" : serde_json::from_str::<serde_json::Value>(raw_data).unwrap(),
            "decoded_payload" : serde_json::from_str::<serde_json::Value>(decoded_payload).unwrap()
        });
        assert_standard_format_eq_to(&tx_hash, &json).await;
    }

    #[tokio::test]
    async fn test_get_standard_format_output_2() {
        let tx_hash = "0x3e85d64cc798c2c73c3ae23f40ad6ecbe1abd6cb78eae6319ae6980991676189";
        let events = r#"
            [
                {
                "Data": "0x23f4d890508a3800000000000000000000000000000000000000000000000001035354430353544300",
                "Key": {
                    "address": "0xd4b5c70450d95ac1bf92e8e8a9b9d298",
                    "salt": "0"
                },
                "Module": "Account",
                "Name": "WithdrawEvent",
                "Seq": "3"
                },
                {
                "Data": "0x23f4d890508a3800000000000000000000000000000000000000000000000001035354430353544300",
                "Key": {
                    "address": "0xc3299e6c57d775a6fdc333f36b8be396",
                    "salt": "1"
                },
                "Module": "Account",
                "Name": "DepositEvent",
                "Seq": "9"
                }
            ]
            "#;

        let raw_data = r#"
            {
                "_id": "",
                "block_hash": "0x42dc623f0c5908e1864249dde9aa43d9f1af146b2dce120571b901e683db3977",
                "block_number": "10044",
                "event_root_hash": "0x2c75c709b3ad08972979e8a09be98524649c8b1c645855c5e85de6e44baa66b7",
                "events": [
                    {
                        "_id": "",
                        "block_hash": "0x42dc623f0c5908e1864249dde9aa43d9f1af146b2dce120571b901e683db3977",
                        "block_number": "10044",
                        "data": "0x23f4d890508a3800000000000000000000000000000000000000000000000001035354430353544300",
                        "decode_event_data": "",
                        "event_index": 0,
                        "event_key": "0x0000000000000000d4b5c70450d95ac1bf92e8e8a9b9d298",
                        "event_seq_number": "3",
                        "transaction_global_index": 0,
                        "transaction_hash": "0x3e85d64cc798c2c73c3ae23f40ad6ecbe1abd6cb78eae6319ae6980991676189",
                        "transaction_index": 1,
                        "type_tag": "0x00000000000000000000000000000001::Account::WithdrawEvent"
                    },
                    {
                        "_id": "",
                        "block_hash": "0x42dc623f0c5908e1864249dde9aa43d9f1af146b2dce120571b901e683db3977",
                        "block_number": "10044",
                        "data": "0x23f4d890508a3800000000000000000000000000000000000000000000000001035354430353544300",
                        "decode_event_data": "",
                        "event_index": 1,
                        "event_key": "0x0100000000000000c3299e6c57d775a6fdc333f36b8be396",
                        "event_seq_number": "9",
                        "transaction_global_index": 0,
                        "transaction_hash": "0x3e85d64cc798c2c73c3ae23f40ad6ecbe1abd6cb78eae6319ae6980991676189",
                        "transaction_index": 1,
                        "type_tag": "0x00000000000000000000000000000001::Account::DepositEvent"
                    }
                ],
                "gas_used": "124191",
                "state_root_hash": "0x5648c0e963b793df141326ca293de7b8400551086e6f6b2ba64627545755e894",
                "status": "Executed",
                "timestamp": 1621366200816,
                "transaction_global_index": 10185,
                "transaction_hash": "0x3e85d64cc798c2c73c3ae23f40ad6ecbe1abd6cb78eae6319ae6980991676189",
                "transaction_index": 1,
                "transaction_type": "ScriptFunction",
                "user_transaction": {
                    "authenticator": {
                    "Ed25519": {
                        "public_key": "0xea1821295e2ee6d19f05465ae5109e5695b879c7f9cf1405a4e0508fefb76e69",
                        "signature": "0x23229976628db2cfefdbd9306098f289864d5810aa0770ba87220a67f28eaa6eb225cca53c63f16af63558ac494ee4569f631c6c7ab1d5571845fe149c49250f"
                    }
                    },
                    "raw_txn": {
                    "chain_id": 1,
                    "decoded_payload": "",
                    "expiration_timestamp_secs": "1621409397",
                    "gas_token_code": "0x1::STC::STC",
                    "gas_unit_price": "1",
                    "max_gas_amount": "10000000",
                    "payload": "0x02000000000000000000000000000000010f5472616e73666572536372697074730c706565725f746f5f706565720107000000000000000000000000000000010353544303535443000310c3299e6c57d775a6fdc333f36b8be39601001023f4d890508a38000000000000000000",
                    "sender": "0xd4b5c70450d95ac1bf92e8e8a9b9d298",
                    "sequence_number": "3",
                    "transaction_hash": "0x3e85d64cc798c2c73c3ae23f40ad6ecbe1abd6cb78eae6319ae6980991676189"
                    },
                    "transaction_hash": "0x3e85d64cc798c2c73c3ae23f40ad6ecbe1abd6cb78eae6319ae6980991676189"
                }
            }
        "#;

        let decoded_payload = r#"
            {
                "ScriptFunction": {
                    "func": {
                        "address": "0x00000000000000000000000000000001",
                        "module": "TransferScripts",
                        "functionName": "peer_to_peer"
                    },
                    "ty_args": [
                    {
                        "Struct": {
                            "module": "STC",
                            "name": "STC",
                            "type_params": [],
                            "address": "0x00000000000000000000000000000001"
                        }
                    }
                    ],
                    "args": [
                        "address: 0xc3299e6c57d775a6fdc333f36b8be396",
                        "vector<u8>: []",
                        "u128: 15,914,677,327,950,883"
                    ]
                }
            }
        "#;

        let json = serde_json::json!({
            "events" : serde_json::from_str::<serde_json::Value>(events).unwrap(),
            "raw_data" : serde_json::from_str::<serde_json::Value>(raw_data).unwrap(),
            "decoded_payload" : serde_json::from_str::<serde_json::Value>(decoded_payload).unwrap()
        });
        assert_standard_format_eq_to(&tx_hash, &json).await;
    }

    #[test]
    #[should_panic(expected = "Key 'b' present in left but missing in right at path 'b', value: 2")]
    fn test_missing_key_in_right() {
        let a = serde_json::json!({"a": 1, "b": 2});
        let b = serde_json::json!({"a": 1});
        assert_json_eq(&a, &b);
    }

    #[test]
    #[should_panic(expected = "Key 'b' present in right but missing in left at path 'b'")]
    fn test_missing_key_in_left() {
        let a = serde_json::json!({"a": 1});
        let b = serde_json::json!({"a": 1, "b": 2});
        assert_json_eq(&a, &b);
    }

    #[test]
    #[should_panic(expected = "Value mismatch at 'a': left=1, right=2")]
    fn test_value_mismatch() {
        let a = serde_json::json!({"a": 1});
        let b = serde_json::json!({"a": 2});
        assert_json_eq(&a, &b);
    }

    #[test]
    #[should_panic(expected = "Array length mismatch at 'arr': left=2, right=1")]
    fn test_array_length_mismatch() {
        let a = serde_json::json!({"arr": [1, 2]});
        let b = serde_json::json!({"arr": [1]});
        assert_json_eq(&a, &b);
    }

    #[test]
    #[should_panic(expected = "Value mismatch at 'arr[1]': left=2, right=3")]
    fn test_array_value_mismatch() {
        let a = serde_json::json!({"arr": [1, 2]});
        let b = serde_json::json!({"arr": [1, 3]});
        assert_json_eq(&a, &b);
    }

    #[test]
    fn test_equal_simple() {
        let a = serde_json::json!({"a": 1, "b": [1, 2], "c": {"d": 3}});
        let b = serde_json::json!({"a": 1, "b": [1, 2], "c": {"d": 3}});
        assert_json_eq(&a, &b);
    }

    #[test]
    #[should_panic(expected = "Value mismatch at 'c.d': left=3, right=4")]
    fn test_nested_value_mismatch() {
        let a = serde_json::json!({"c": {"d": 3}});
        let b = serde_json::json!({"c": {"d": 4}});
        assert_json_eq(&a, &b);
    }
}
