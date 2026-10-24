//! A bounded byte ring buffer for a session's scrollback.

/// A fixed-capacity ring that keeps the **newest** bytes.
#[derive(Debug)]
pub struct RingBuffer {
    capacity: usize,
    data: Vec<u8>,
}

impl RingBuffer {
    /// Create a ring holding at most `capacity` bytes.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            data: Vec::new(),
        }
    }

    /// Append bytes, dropping the oldest ones past the capacity.
    pub fn push(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
        let overflow = self.data.len().saturating_sub(self.capacity);
        if overflow > 0 {
            self.data.drain(..overflow);
        }
    }

    /// The retained byte count.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the ring is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// The newest `max_bytes` bytes, decoded lossily.
    pub fn tail(&self, max_bytes: usize) -> String {
        let start = self.data.len().saturating_sub(max_bytes);
        String::from_utf8_lossy(&self.data[start..]).into_owned()
    }

    /// The newest `max_lines` complete lines, capped at `max_bytes`.
    pub fn tail_lines(&self, max_lines: usize, max_bytes: usize) -> String {
        let text = self.tail(max_bytes);
        if max_lines == 0 {
            return String::new();
        }
        let mut lines: Vec<&str> = text.lines().collect();
        if lines.len() > max_lines {
            lines = lines.split_off(lines.len() - max_lines);
        }
        lines.join("\n")
    }
}

/// Redact values of secret-looking environment assignments in terminal output.
pub fn redact(text: &str) -> String {
    const MARKERS: [&str; 5] = ["TOKEN", "SECRET", "PASSWORD", "API_KEY", "APIKEY"];
    text.lines()
        .map(|line| match line.split_once('=') {
            Some((name, _))
                if MARKERS
                    .iter()
                    .any(|m| name.to_ascii_uppercase().contains(m)) =>
            {
                format!("{name}=[redacted]")
            }
            _ => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_drops_the_oldest_bytes() {
        let mut ring = RingBuffer::new(8);
        ring.push(b"abcdefgh");
        ring.push(b"ij");
        assert_eq!(ring.len(), 8);
        assert_eq!(ring.tail(8), "cdefghij");
    }

    #[test]
    fn the_tail_is_bounded() {
        let mut ring = RingBuffer::new(1024);
        ring.push(b"0123456789");
        assert_eq!(ring.tail(4), "6789");
        assert_eq!(ring.tail(1000).len(), 10);
    }

    #[test]
    fn line_tails_keep_the_newest_lines() {
        let mut ring = RingBuffer::new(1024);
        ring.push(b"one\ntwo\nthree\nfour");
        assert_eq!(ring.tail_lines(2, 1024), "three\nfour");
        assert_eq!(ring.tail(0), "");
    }

    #[test]
    fn secrets_are_redacted() {
        let text = "ok\nGITHUB_TOKEN=abcdef\nPASSWORD=hunter2\nplain=value";
        assert_eq!(
            redact(text),
            "ok\nGITHUB_TOKEN=[redacted]\nPASSWORD=[redacted]\nplain=value"
        );
    }
}
