//! Per-call resource limits (`spec.md` D7, Appendix E).

/// The limits applied to one component call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum linear memory in bytes.
    pub memory_bytes: usize,
    /// Maximum fuel (roughly instructions) per call.
    pub fuel: u64,
    /// Maximum wall-clock time per call in milliseconds.
    pub wall_time_ms: u64,
    /// Maximum input payload in bytes.
    pub max_input_bytes: usize,
    /// Maximum output payload in bytes.
    pub max_output_bytes: usize,
}

impl Limits {
    /// Limits for a kernel plugin.
    pub const PLUGIN: Limits = Limits {
        memory_bytes: 64 * 1024 * 1024,
        fuel: 100_000_000,
        wall_time_ms: 10_000,
        max_input_bytes: 8 * 1024 * 1024,
        max_output_bytes: 8 * 1024 * 1024,
    };

    /// Limits for a skill.
    pub const SKILL: Limits = Limits {
        memory_bytes: 32 * 1024 * 1024,
        fuel: 50_000_000,
        wall_time_ms: 5_000,
        max_input_bytes: 8 * 1024 * 1024,
        max_output_bytes: 8 * 1024 * 1024,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Limits::PLUGIN
    }
}
