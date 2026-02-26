use crate::export::ExportError;
use crate::{MessageMeta, ULogRegistry};

const ULOG_HEADER_MAGIC: [u8; 8] = [0x55, 0x4c, 0x6f, 0x67, 0x01, 0x12, 0x35, 0x01];

pub fn registry_entry<R: ULogRegistry, E>(
    topic_index: usize,
) -> Result<&'static MessageMeta, ExportError<E>> {
    R::REGISTRY
        .get(topic_index)
        .ok_or(ExportError::InvalidTopicIndex)
}

pub fn stream_slot<const MAX_MULTI_IDS: usize>(
    topic_index: usize,
    instance: usize,
) -> Option<usize> {
    topic_index
        .checked_mul(MAX_MULTI_IDS)
        .and_then(|v| v.checked_add(instance))
}

pub fn slot_msg_id<E>(slot: usize) -> Result<u16, ExportError<E>> {
    u16::try_from(slot).map_err(|_| ExportError::TooManyStreams)
}

pub fn checked_total_len<E>(
    base: usize,
    extra: usize,
    capacity: usize,
) -> Result<usize, ExportError<E>> {
    let total_len = base
        .checked_add(extra)
        .ok_or(ExportError::MessageTooLarge)?;
    if total_len > capacity {
        return Err(ExportError::MessageTooLarge);
    }
    Ok(total_len)
}

pub fn write_header(timestamp_micros: u64) -> [u8; 16] {
    let mut header = [0u8; 16];
    header[..8].copy_from_slice(&ULOG_HEADER_MAGIC);
    header[8..16].copy_from_slice(&timestamp_micros.to_le_bytes());
    header
}

pub fn message_header<E>(payload_len: usize, msg_type: u8) -> Result<[u8; 3], ExportError<E>> {
    let size = u16::try_from(payload_len).map_err(|_| ExportError::MessageTooLarge)?;
    let mut header = [0u8; 3];
    header[0..2].copy_from_slice(&size.to_le_bytes());
    header[2] = msg_type;
    Ok(header)
}

pub fn format_payload<E>(
    payload: &mut [u8; 512],
    name: &str,
    format: &str,
) -> Result<usize, ExportError<E>> {
    let name_bytes = name.as_bytes();
    let format_bytes = format.as_bytes();
    let format_offset = checked_total_len(name_bytes.len(), 1, payload.len())?;
    let total_len = checked_total_len(format_offset, format_bytes.len(), payload.len())?;

    payload[..name_bytes.len()].copy_from_slice(name_bytes);
    payload[name_bytes.len()] = b':';
    payload[format_offset..total_len].copy_from_slice(format_bytes);
    Ok(total_len)
}

pub fn add_subscription_payload<E>(
    payload: &mut [u8; 256],
    multi_id: u8,
    msg_id: u16,
    name: &str,
) -> Result<usize, ExportError<E>> {
    let name_bytes = name.as_bytes();
    let total_len = checked_total_len(3, name_bytes.len(), payload.len())?;

    payload[0] = multi_id;
    payload[1..3].copy_from_slice(&msg_id.to_le_bytes());
    payload[3..total_len].copy_from_slice(name_bytes);
    Ok(total_len)
}

pub fn data_payload<E>(
    payload: &mut [u8; 1024],
    msg_id: u16,
    data: &[u8],
) -> Result<usize, ExportError<E>> {
    let total_len = checked_total_len(2, data.len(), payload.len())?;

    payload[0..2].copy_from_slice(&msg_id.to_le_bytes());
    payload[2..total_len].copy_from_slice(data);
    Ok(total_len)
}

pub fn log_payload<E>(
    payload: &mut [u8; 512],
    level: u8,
    timestamp: u64,
    text: &[u8],
) -> Result<usize, ExportError<E>> {
    let total_len = checked_total_len(9, text.len(), payload.len())?;

    payload[0] = level;
    payload[1..9].copy_from_slice(&timestamp.to_le_bytes());
    payload[9..total_len].copy_from_slice(text);
    Ok(total_len)
}

pub fn tagged_log_payload<E>(
    payload: &mut [u8; 512],
    level: u8,
    tag: u16,
    timestamp: u64,
    text: &[u8],
) -> Result<usize, ExportError<E>> {
    let total_len = checked_total_len(11, text.len(), payload.len())?;

    payload[0] = level;
    payload[1..3].copy_from_slice(&tag.to_le_bytes());
    payload[3..11].copy_from_slice(&timestamp.to_le_bytes());
    payload[11..total_len].copy_from_slice(text);
    Ok(total_len)
}

pub fn parameter_payload<E>(
    payload: &mut [u8; 512],
    key: &[u8],
    value: &[u8],
) -> Result<usize, ExportError<E>> {
    let key_len = u8::try_from(key.len()).map_err(|_| ExportError::MessageTooLarge)?;
    let value_offset = checked_total_len(1, key.len(), payload.len())?;
    let total_len = checked_total_len(value_offset, value.len(), payload.len())?;

    payload[0] = key_len;
    payload[1..value_offset].copy_from_slice(key);
    payload[value_offset..total_len].copy_from_slice(value);
    Ok(total_len)
}
