//! Stable history encoding shared by protocol experiments.

use detersim_check::models::{AppendLogInput, AppendLogOutput, SingleKeyInput, SingleKeyOutput};
use detersim_check::OpRecord;
use detersim_core::SimTime;

/// A client operation recorded by a protocol harness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordedOp {
    /// A completed single-key put.
    KvPut {
        id: u64,
        process: u32,
        value: i32,
        invoke: SimTime,
        complete: SimTime,
    },
    /// A completed single-key get.
    KvGet {
        id: u64,
        process: u32,
        value: Option<i32>,
        invoke: SimTime,
        complete: SimTime,
    },
    /// A completed append-only log append.
    LogAppend {
        id: u64,
        process: u32,
        value: String,
        index: usize,
        invoke: SimTime,
        complete: SimTime,
    },
    /// A completed append-only log read.
    LogRead {
        id: u64,
        process: u32,
        entries: Vec<String>,
        invoke: SimTime,
        complete: SimTime,
    },
}

impl RecordedOp {
    /// Encode the operation into the deterministic `RunReport.history` format.
    pub fn to_history_line(&self) -> String {
        match self {
            RecordedOp::KvPut {
                id,
                value,
                invoke,
                complete,
                ..
            } => format!(
                "kv:{id}:put:{value}:ok:{}:{}",
                invoke.as_nanos(),
                complete.as_nanos()
            ),
            RecordedOp::KvGet {
                id,
                value,
                invoke,
                complete,
                ..
            } => format!(
                "kv:{id}:get:{}:value:{}:{}",
                kv_value_msg(*value),
                invoke.as_nanos(),
                complete.as_nanos()
            ),
            RecordedOp::LogAppend {
                id,
                value,
                index,
                invoke,
                complete,
                ..
            } => format!(
                "log:{id}:append:{value}:index:{index}:{}:{}",
                invoke.as_nanos(),
                complete.as_nanos()
            ),
            RecordedOp::LogRead {
                id,
                entries,
                invoke,
                complete,
                ..
            } => format!(
                "log:{id}:read:{}:entries:{}:{}",
                entries.join(","),
                invoke.as_nanos(),
                complete.as_nanos()
            ),
        }
    }
}

/// Parse single-key KV history lines emitted by [`RecordedOp::to_history_line`].
pub fn single_key_kv_history(
    entries: &[String],
) -> Vec<OpRecord<SingleKeyInput<i32>, SingleKeyOutput<i32>>> {
    entries
        .iter()
        .filter_map(|entry| parse_kv_line(entry))
        .collect()
}

/// Parse append-log history lines emitted by [`RecordedOp::to_history_line`].
pub fn append_log_history(
    entries: &[String],
) -> Vec<OpRecord<AppendLogInput<String>, AppendLogOutput<String>>> {
    entries
        .iter()
        .filter_map(|entry| parse_log_line(entry))
        .collect()
}

fn parse_kv_line(entry: &str) -> Option<OpRecord<SingleKeyInput<i32>, SingleKeyOutput<i32>>> {
    let parts: Vec<_> = entry.split(':').collect();
    if parts.len() != 7 || parts.first().copied() != Some("kv") {
        return None;
    }
    let id = parts.get(1)?.parse::<u64>().ok()?;
    let invoke = SimTime::from_nanos(parts.get(5)?.parse::<u64>().ok()?);
    let complete = SimTime::from_nanos(parts.get(6)?.parse::<u64>().ok()?);
    match parts.get(2).copied()? {
        "put" => Some(OpRecord::completed_at(
            id,
            3,
            SingleKeyInput::Put(parts.get(3)?.parse::<i32>().ok()?),
            SingleKeyOutput::Ok,
            invoke,
            complete,
        )),
        "get" => Some(OpRecord::completed_at(
            id,
            3,
            SingleKeyInput::Get,
            SingleKeyOutput::Value(parse_optional_i32(parts.get(3).copied()?)),
            invoke,
            complete,
        )),
        _ => None,
    }
}

fn parse_log_line(
    entry: &str,
) -> Option<OpRecord<AppendLogInput<String>, AppendLogOutput<String>>> {
    let parts: Vec<_> = entry.split(':').collect();
    if parts.first().copied() != Some("log") {
        return None;
    }
    let id = parts.get(1)?.parse::<u64>().ok()?;
    match parts.get(2).copied()? {
        "append" if parts.len() == 8 => {
            let invoke = SimTime::from_nanos(parts.get(6)?.parse::<u64>().ok()?);
            let complete = SimTime::from_nanos(parts.get(7)?.parse::<u64>().ok()?);
            Some(OpRecord::completed_at(
                id,
                3,
                AppendLogInput::Append(parts.get(3)?.to_string()),
                AppendLogOutput::Index(parts.get(5)?.parse::<usize>().ok()?),
                invoke,
                complete,
            ))
        }
        "read" if parts.len() == 7 => {
            let invoke = SimTime::from_nanos(parts.get(5)?.parse::<u64>().ok()?);
            let complete = SimTime::from_nanos(parts.get(6)?.parse::<u64>().ok()?);
            Some(OpRecord::completed_at(
                id,
                3,
                AppendLogInput::ReadAll,
                AppendLogOutput::Entries(parse_entries(parts.get(3).copied()?)),
                invoke,
                complete,
            ))
        }
        _ => None,
    }
}

fn parse_optional_i32(value: &str) -> Option<i32> {
    (value != "none")
        .then(|| value.parse::<i32>().ok())
        .flatten()
}

fn parse_entries(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split(',').map(str::to_string).collect()
    }
}

fn kv_value_msg(value: Option<i32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}
