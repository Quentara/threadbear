//! Tolerant protocol fields. Read each value before trying its type so an
//! error cannot leave a streaming deserializer halfway through the next row.
//! Diagnostics contain Rust type names and positions only, never serde's
//! error text (which can contain the value it could not read).
use serde::{de::DeserializeOwned, Deserialize, Deserializer};
use serde_json::Value;
use std::{collections::HashMap, sync::OnceLock};

static LOGGER: OnceLock<fn(&str)> = OnceLock::new();

pub fn set_logger(logger: fn(&str)) {
    let _ = LOGGER.set(logger);
}

pub fn dropped<T>(kind: &str) {
    let message = format!("dropped invalid {kind} ({})", std::any::type_name::<T>());
    if let Some(log) = LOGGER.get() { log(&message); } else { eprintln!("threadbear: decode {message}"); }
}

pub fn read<T: DeserializeOwned>(value: Value) -> Option<T> {
    match serde_json::from_value(value) {
        Ok(value) => Some(value),
        Err(_) => { dropped::<T>("row"); None }
    }
}

pub fn field<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    let value = Value::deserialize(d)?;
    Ok(match serde_json::from_value(value) {
        Ok(value) => value,
        Err(_) => { dropped::<T>("field"); T::default() }
    })
}

pub fn id<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = Value::deserialize(d)?;
    value.as_str().filter(|id| !id.is_empty()).map(str::to_owned).ok_or_else(|| serde::de::Error::custom("invalid row identity"))
}

pub fn list<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    let value = Value::deserialize(d)?;
    Ok(rows(value))
}

fn rows<T: DeserializeOwned>(value: Value) -> Vec<T> {
    match value {
        Value::Array(rows) => rows.into_iter().enumerate().filter_map(|(i, row)| {
            match serde_json::from_value(row) {
                Ok(row) => Some(row),
                Err(_) => { dropped::<T>(&format!("list entry at {i}")); None }
            }
        }).collect(),
        _ => { dropped::<Vec<T>>("list field"); vec![] }
    }
}

pub fn optional_list<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<Vec<T>>, D::Error> {
    let value = Value::deserialize(d)?;
    Ok(match value {
        Value::Null => None,
        Value::Array(_) => Some(rows(value)),
        _ => { dropped::<Vec<T>>("optional list field"); None }
    })
}

pub fn map<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<HashMap<String, T>, D::Error> {
    let value = Value::deserialize(d)?;
    Ok(match value {
        Value::Object(entries) => entries.into_iter().filter_map(|(key, value)| {
            // Keys can be user text too.
            read(value).map(|value| (key, value))
        }).collect(),
        _ => { dropped::<HashMap<String, T>>("map field"); HashMap::new() }
    })
}

pub fn default_true<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let value = Value::deserialize(d)?;
    Ok(value.as_bool().unwrap_or_else(|| { dropped::<bool>("default-on field"); true }))
}
