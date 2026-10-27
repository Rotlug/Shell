use nvml_wrapper::Nvml;

use crate::MemoryInfo;

/// The first NVIDIA GPU, through NVML.
pub struct Nvidia {
    nvml: Nvml,
}

impl Nvidia {
    /// `None` when there is no NVIDIA driver to talk to.
    pub fn new() -> Option<Self> {
        Some(Self { nvml: Nvml::init().ok()? })
    }

    pub fn memory(&self) -> Option<MemoryInfo> {
        let memory = self.nvml.device_by_index(0).ok()?.memory_info().ok()?;

        Some(MemoryInfo {
            total: memory.total,
            used: memory.used,
        })
    }
}
