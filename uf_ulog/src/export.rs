#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportStep {
    Progressed,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportError<WriteError> {
    Write(WriteError),
    InvalidTopicIndex,
    InvalidMultiId,
    TooManyStreams,
    MessageTooLarge,
}
