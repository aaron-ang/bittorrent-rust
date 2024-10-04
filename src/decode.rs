pub fn decode_bencoded_value(encoded_value: &str) -> anyhow::Result<()> {
    let value = serde_bencode::from_str(encoded_value)?;
    println!("{}", bencode_to_json(value)?);
    Ok(())
}

fn bencode_to_json(value: serde_bencode::value::Value) -> anyhow::Result<serde_json::Value> {
    match value {
        serde_bencode::value::Value::Bytes(b) => {
            Ok(serde_json::Value::String(String::from_utf8(b)?))
        }
        serde_bencode::value::Value::Int(i) => {
            Ok(serde_json::Value::Number(serde_json::Number::from(i)))
        }
        serde_bencode::value::Value::List(l) => {
            let json_list = l
                .into_iter()
                .map(|v| bencode_to_json(v))
                .collect::<anyhow::Result<Vec<serde_json::Value>>>()?;
            Ok(serde_json::Value::Array(json_list))
        }
        serde_bencode::value::Value::Dict(d) => {
            let json_map = d
                .into_iter()
                .map(|(k, v)| Ok((String::from_utf8(k)?, bencode_to_json(v)?)))
                .collect::<anyhow::Result<serde_json::Map<String, serde_json::Value>>>()?;
            Ok(serde_json::Value::Object(json_map))
        }
    }
}
