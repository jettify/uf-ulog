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

pub fn format_payload_len<E>(name: &str, format: &str) -> Result<usize, ExportError<E>> {
    let name_bytes = name.as_bytes();
    let format_bytes = format.as_bytes();
    let format_offset = checked_total_len(name_bytes.len(), 1, usize::MAX)?;
    checked_total_len(format_offset, format_bytes.len(), usize::MAX)
}

pub fn add_subscription_payload_len<E>(name: &str) -> Result<usize, ExportError<E>> {
    checked_total_len(3, name.len(), usize::MAX)
}

pub fn data_payload_len<E>(data_len: usize) -> Result<usize, ExportError<E>> {
    checked_total_len(2, data_len, usize::MAX)
}

pub fn log_payload_len<E>(text_len: usize) -> Result<usize, ExportError<E>> {
    checked_total_len(9, text_len, usize::MAX)
}

pub fn tagged_log_payload_len<E>(text_len: usize) -> Result<usize, ExportError<E>> {
    checked_total_len(11, text_len, usize::MAX)
}

pub fn parameter_payload_len<E>(key: &[u8], value: &[u8]) -> Result<usize, ExportError<E>> {
    let _ = u8::try_from(key.len()).map_err(|_| ExportError::MessageTooLarge)?;
    let value_offset = checked_total_len(1, key.len(), usize::MAX)?;
    checked_total_len(value_offset, value.len(), usize::MAX)
}

pub fn add_subscription_prefix(multi_id: u8, msg_id: u16) -> [u8; 3] {
    let mut prefix = [0u8; 3];
    prefix[0] = multi_id;
    prefix[1..3].copy_from_slice(&msg_id.to_le_bytes());
    prefix
}

pub fn data_prefix(msg_id: u16) -> [u8; 2] {
    msg_id.to_le_bytes()
}

pub fn log_prefix(level: u8, timestamp: u64) -> [u8; 9] {
    let mut prefix = [0u8; 9];
    prefix[0] = level;
    prefix[1..9].copy_from_slice(&timestamp.to_le_bytes());
    prefix
}

pub fn tagged_log_prefix(level: u8, tag: u16, timestamp: u64) -> [u8; 11] {
    let mut prefix = [0u8; 11];
    prefix[0] = level;
    prefix[1..3].copy_from_slice(&tag.to_le_bytes());
    prefix[3..11].copy_from_slice(&timestamp.to_le_bytes());
    prefix
}

pub fn parameter_prefix<E>(key: &[u8]) -> Result<[u8; 1], ExportError<E>> {
    let key_len = u8::try_from(key.len()).map_err(|_| ExportError::MessageTooLarge)?;
    Ok([key_len])
}
