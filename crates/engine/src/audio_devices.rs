use cpal::traits::{DeviceTrait, HostTrait};

/// Info about an available audio device.
#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    pub is_default: bool,
    pub sample_rates: Vec<u32>,
    pub max_channels: u16,
}

/// List all available audio devices on the system.
pub fn list_devices() -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let mut devices = Vec::new();

    let default_in_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();
    let default_out_name = host
        .default_output_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    // Input devices
    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            let name = device.name().unwrap_or_else(|_| "Unknown".into());
            let is_default = name == default_in_name;

            let mut sample_rates = Vec::new();
            let mut max_channels = 0u16;

            if let Ok(configs) = device.supported_input_configs() {
                for config in configs {
                    max_channels = max_channels.max(config.channels());
                    // Collect common sample rates within the supported range
                    for &sr in &[44100, 48000, 88200, 96000, 176400, 192000] {
                        let sr_val = cpal::SampleRate(sr);
                        if sr_val >= config.min_sample_rate()
                            && sr_val <= config.max_sample_rate()
                            && !sample_rates.contains(&sr)
                        {
                            sample_rates.push(sr);
                        }
                    }
                }
            }

            // Check if this device is also an output
            let is_output = host.output_devices().map_or(false, |mut devs| {
                devs.any(|d| d.name().ok().as_deref() == Some(&name))
            });

            devices.push(AudioDeviceInfo {
                name,
                is_input: true,
                is_output,
                is_default,
                sample_rates,
                max_channels,
            });
        }
    }

    // Output devices (add those not already listed)
    if let Ok(output_devices) = host.output_devices() {
        for device in output_devices {
            let name = device.name().unwrap_or_else(|_| "Unknown".into());

            // Skip if already added as input device
            if devices.iter().any(|d| d.name == name) {
                // Mark it as output too
                if let Some(d) = devices.iter_mut().find(|d| d.name == name) {
                    d.is_output = true;
                    if name == default_out_name {
                        d.is_default = true;
                    }
                }
                continue;
            }

            let is_default = name == default_out_name;

            let mut sample_rates = Vec::new();
            let mut max_channels = 0u16;

            if let Ok(configs) = device.supported_output_configs() {
                for config in configs {
                    max_channels = max_channels.max(config.channels());
                    for &sr in &[44100, 48000, 88200, 96000, 176400, 192000] {
                        let sr_val = cpal::SampleRate(sr);
                        if sr_val >= config.min_sample_rate()
                            && sr_val <= config.max_sample_rate()
                            && !sample_rates.contains(&sr)
                        {
                            sample_rates.push(sr);
                        }
                    }
                }
            }

            devices.push(AudioDeviceInfo {
                name,
                is_input: false,
                is_output: true,
                is_default,
                sample_rates,
                max_channels,
            });
        }
    }

    devices
}
