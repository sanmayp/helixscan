//! NVIDIA system-inventory helpers used only to size a conservative memory cap.
//! GPU computation remains portable core WebGPU/WGSL.
use std::{path::PathBuf, process::Command};

/// Core WebGPU has no physical-VRAM query. Read only the local NVIDIA system
/// inventory when available; adapter buffer limits remain the hard bound.
pub fn memories() -> Vec<u64> {
    let Some(path) = nvidia_smi_path() else {
        return Vec::new();
    };
    let Ok(output) = Command::new(path)
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    std::str::from_utf8(&output.stdout)
        .ok()
        .map(|text| {
            text.lines()
                .filter_map(|line| line.trim().parse::<u64>().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn nvidia_smi_path() -> Option<PathBuf> {
    let override_path = std::env::var_os("HELIXSCAN_NVIDIA_SMI");
    if override_path.is_some() {
        return override_path.map(Into::into);
    }
    // A regular executable-name lookup avoids treating an unset PATH as empty.
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("nvidia-smi"))
        .find(|path| path.is_file())
}

pub fn default_cap(memories: &[u64], ordinal: usize, override_mb: Option<u64>) -> u64 {
    override_mb
        .unwrap_or_else(|| {
            memories
                .get(ordinal)
                .or_else(|| memories.first())
                .map_or(256, |memory| (memory / 4).min(2048))
        })
        .min(2048)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn default_memory_policy_and_system_parser() {
        assert_eq!(default_cap(&[16_384], 0, None), 2048);
        assert_eq!(default_cap(&[4_096], 0, None), 1024);
        assert_eq!(default_cap(&[], 0, None), 256);
        assert_eq!(default_cap(&[16_384], 0, Some(1024)), 1024);
        assert_eq!(default_cap(&[16_384], 0, Some(4096)), 2048);
        let root = std::env::temp_dir().join(format!(
            "helixscan-memory-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        fs::create_dir_all(&root).unwrap();
        let executable = root.join("inventory");
        fs::write(&executable, "#!/bin/sh\nprintf '16384\\n8192\\n'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        }
        unsafe {
            std::env::set_var("HELIXSCAN_NVIDIA_SMI", &executable);
        }
        assert_eq!(memories(), vec![16_384, 8192]);
        unsafe {
            std::env::remove_var("HELIXSCAN_NVIDIA_SMI");
        }
        let _ = fs::remove_dir_all(&root);
    }
}
