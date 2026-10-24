use crate::client::{AiError, Cancellation};
use std::io::BufRead;

/// SSE permits comments, CRLF, multiline data and fragmented network reads.
/// Bound each event before allocating a provider-controlled unbounded line.
pub(crate) fn read_event(
    reader: &mut impl BufRead,
    cancel: &Cancellation,
) -> Result<Option<String>, AiError> {
    let mut data = String::new();
    let mut line = Vec::new();
    loop {
        cancel.check()?;
        line.clear();
        loop {
            let available = reader.fill_buf().map_err(|_| AiError::Network)?;
            if available.is_empty() {
                if line.is_empty() && data.is_empty() {
                    return Ok(None);
                }
                break;
            }
            let count = available
                .iter()
                .position(|b| *b == b'\n')
                .map_or(available.len(), |n| n + 1);
            if line.len() + count + data.len() > 256 * 1024 {
                return Err(AiError::InvalidResponse);
            }
            line.extend_from_slice(&available[..count]);
            let ended = available[count - 1] == b'\n';
            reader.consume(count);
            if ended {
                break;
            }
        }
        let text = std::str::from_utf8(&line)
            .map_err(|_| AiError::InvalidResponse)?
            .trim_end_matches(['\r', '\n']);
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        if text.is_empty() {
            if !data.is_empty() {
                data.pop();
                return Ok(Some(data));
            }
            if line.is_empty() {
                return Ok(None);
            }
        } else if let Some(value) = text.strip_prefix("data:") {
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
            data.push('\n');
        }
    }
}
