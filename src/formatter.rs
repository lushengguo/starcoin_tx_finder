use crate::block_loader;

use block_loader::{
    format_events_part, format_raw_data_part, get_decoded_payload, get_transaction_by_hash_ws,
    get_transaction_events_ws, get_transaction_info_ws,
};

pub async fn get_standard_format_output(tx_hash: &str) -> Option<(serde_json::Value, u64)> {
    let ws_url = "ws://main.seed.starcoin.org:9870";

    let tx_response = get_transaction_by_hash_ws(ws_url, tx_hash).await;
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

    let tx_info_response = get_transaction_info_ws(ws_url, tx_hash).await;
    let events_response = get_transaction_events_ws(ws_url, tx_hash).await;

    let mut json: serde_json::Value = serde_json::json!({
        "events": serde_json::Value::Null,
        "raw_data": serde_json::Value::Null,
        "decoded_payload": serde_json::Value::Null
    });

    if let (Some(tx), Some(info), Some(events)) = (tx_response, tx_info_response, events_response) {
        println!("tx:{}", tx);
        println!("info:{}", info);
        println!("events:{}", events);

        if let Some(complete_format) = format_raw_data_part(&tx, &info, &events) {
            json["raw_data"] = serde_json::to_value(&complete_format).unwrap();
        }

        let decoded_payload = get_decoded_payload(&tx);
        if let Some(payload) = decoded_payload {
            json["decoded_payload"] = serde_json::to_value(&payload).unwrap();
        }

        if let Some(formatted_events) = format_events_part(&events) {
            // If only one event, output as object, else as array
            if formatted_events.len() == 1 {
                json["events"] = formatted_events[0].clone();
            } else {
                json["events"] = serde_json::to_value(&formatted_events).unwrap();
            }
        }
    }

    Some((json, block_number))
}

mod test {
    use super::*;
    use serde_json::Value;

    fn assert_json_eq(a: &Value, b: &Value) {
        fn compare(a: &Value, b: &Value, path: &str) {
            // Skip Data field
            if path.ends_with("Data") {
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
    async fn test_get_standard_format_output() {
        let tx_hash = "0xe29d7508fe37d756d83e672be53843d10d084f9c69fca1b7e9a34ea8eb96f918";

        let events = r#"
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
                    "transaction_hash": ""
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
