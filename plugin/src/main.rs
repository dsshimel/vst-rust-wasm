use nih_plug::prelude::*;
use plugin::SimpleSynth;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--probe") {
        probe_audio_devices();
        return;
    }

    // If the user didn't pass any audio config flags, auto-detect a working setup
    let has_backend = args.iter().any(|a| a == "--backend" || a.starts_with("--backend="));
    let has_output = args.iter().any(|a| a == "--output-device" || a.starts_with("--output-device="));
    let has_sample_rate = args.iter().any(|a| a == "--sample-rate" || a.starts_with("--sample-rate="));

    if !has_backend && !has_output && !has_sample_rate {
        // Try to auto-detect a working config and re-invoke with explicit args
        if let Some(extra_args) = auto_detect_config() {
            eprintln!("[INFO] Auto-detected audio config: {}", extra_args.join(" "));
            let mut new_args = args.clone();
            new_args.extend(extra_args.into_iter().map(String::from));
            nih_export_standalone_with_args::<SimpleSynth, _>(new_args);
            return;
        }
    }

    nih_export_standalone::<SimpleSynth>();
}

/// Try ASIO first (best latency), then WASAPI, returning CLI args for the first
/// device that supports 2 channels at a reasonable sample rate.
fn auto_detect_config() -> Option<Vec<&'static str>> {
    use cpal::traits::{DeviceTrait, HostTrait};

    // Try ASIO first — lower latency, no buffer-size mismatch bugs
    // Collect all candidates, then pick the best one (prefer 44100/48000 Hz)
    if let Ok(host) = cpal::host_from_id(cpal::HostId::Asio) {
        let mut best: Option<(String, u32, u32)> = None; // (name, sr, buf)
        let mut best_score: i32 = -1;

        if let Ok(devices) = host.output_devices() {
            for device in devices {
                if let Ok(configs) = device.supported_output_configs() {
                    for cfg in configs {
                        if cfg.channels() < 2 {
                            continue;
                        }
                        let supports_44100 = (cfg.min_sample_rate()..=cfg.max_sample_rate())
                            .contains(&cpal::SampleRate(44100));
                        let supports_48000 = (cfg.min_sample_rate()..=cfg.max_sample_rate())
                            .contains(&cpal::SampleRate(48000));
                        let sr = if supports_44100 {
                            44100
                        } else if supports_48000 {
                            48000
                        } else {
                            continue; // Skip devices that don't support standard rates
                        };
                        let buf = match cfg.buffer_size() {
                            cpal::SupportedBufferSize::Range { min, max } => {
                                if (min..=max).contains(&&512u32) {
                                    512
                                } else if (min..=max).contains(&&256u32) {
                                    256
                                } else {
                                    *min
                                }
                            }
                            _ => 512,
                        };
                        // Score: prefer dedicated hardware over generic wrappers
                        // ASIO4ALL/FL Studio ASIO are wrappers, score lower
                        if let Ok(name) = device.name() {
                            let is_wrapper = name.contains("ASIO4ALL")
                                || name.contains("FL Studio");
                            let score = if is_wrapper { 1 } else { 10 }
                                + if sr == 44100 { 2 } else { 1 };
                            if score > best_score {
                                best_score = score;
                                best = Some((name, sr, buf));
                            }
                        }
                    }
                }
            }
        }

        if let Some((name, sr, buf)) = best {
            eprintln!(
                "[INFO] Found ASIO device: {} (44100/48000Hz capable, using {}Hz, buf {})",
                name, sr, buf
            );
            let sr_str: &'static str = Box::leak(sr.to_string().into_boxed_str());
            let buf_str: &'static str = Box::leak(buf.to_string().into_boxed_str());
            let name_str: &'static str = Box::leak(name.into_boxed_str());
            return Some(vec![
                "--backend", "asio",
                "--output-device", name_str,
                "--sample-rate", sr_str,
                "--period-size", buf_str,
            ]);
        }
    }

    // Fall back to WASAPI — pick first 2-channel device
    if let Ok(host) = cpal::host_from_id(cpal::HostId::Wasapi) {
        if let Ok(devices) = host.output_devices() {
            for device in devices {
                if let Ok(configs) = device.supported_output_configs() {
                    for cfg in configs {
                        if cfg.channels() == 2 {
                            let sr = if (cfg.min_sample_rate()..=cfg.max_sample_rate())
                                .contains(&cpal::SampleRate(44100))
                            {
                                44100
                            } else if (cfg.min_sample_rate()..=cfg.max_sample_rate())
                                .contains(&cpal::SampleRate(48000))
                            {
                                48000
                            } else {
                                cfg.min_sample_rate().0
                            };
                            let buf = match cfg.buffer_size() {
                                cpal::SupportedBufferSize::Range { min, max } => {
                                    // Pick 512 if possible, else min
                                    if (min..=max).contains(&&512u32) {
                                        512
                                    } else {
                                        *min
                                    }
                                }
                                _ => 512,
                            };
                            if let Ok(name) = device.name() {
                                eprintln!(
                                    "[INFO] Found WASAPI device: {} ({}ch, {}Hz, buf {})",
                                    name,
                                    cfg.channels(),
                                    sr,
                                    buf
                                );
                                let sr_str: &'static str =
                                    Box::leak(sr.to_string().into_boxed_str());
                                let buf_str: &'static str =
                                    Box::leak(buf.to_string().into_boxed_str());
                                let name_str: &'static str =
                                    Box::leak(name.into_boxed_str());
                                return Some(vec![
                                    "--backend", "wasapi",
                                    "--output-device", name_str,
                                    "--sample-rate", sr_str,
                                    "--period-size", buf_str,
                                ]);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

fn probe_audio_devices() {
    use cpal::traits::{DeviceTrait, HostTrait};

    println!("=== Audio Device Probe ===\n");

    let available_hosts = cpal::available_hosts();
    println!("Available audio hosts: {:?}\n", available_hosts);

    for host_id in &available_hosts {
        let host = match cpal::host_from_id(*host_id) {
            Ok(h) => h,
            Err(e) => {
                println!("[{:?}] Failed to initialize: {}", host_id, e);
                continue;
            }
        };

        println!("--- Host: {:?} ---", host_id);

        let devices = match host.output_devices() {
            Ok(d) => d,
            Err(e) => {
                println!("  Failed to enumerate output devices: {}", e);
                continue;
            }
        };

        for device in devices {
            let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
            println!("\n  Device: {}", name);

            // Default config
            match device.default_output_config() {
                Ok(config) => {
                    println!("    Default config:");
                    println!(
                        "      Channels: {}, Sample Rate: {}, Format: {:?}, Buffer: {:?}",
                        config.channels(),
                        config.sample_rate().0,
                        config.sample_format(),
                        config.buffer_size()
                    );

                    let sr = config.sample_rate().0;
                    let exe = std::env::args()
                        .next()
                        .unwrap_or_else(|| "simple-synth-standalone".to_string());

                    // Suggest various period sizes with the device's native sample rate
                    println!("\n    Suggested commands:");
                    for period in &[256, 512, 1024, 2048] {
                        let backend_name = if *host_id == cpal::HostId::Wasapi {
                            "wasapi"
                        } else if *host_id == cpal::HostId::Asio {
                            "asio"
                        } else {
                            "auto"
                        };
                        println!(
                            "      {} --backend {} --output-device \"{}\" --sample-rate {} --period-size {}",
                            exe, backend_name, name, sr, period
                        );
                    }
                }
                Err(e) => {
                    println!("    No default config: {}", e);
                }
            }

            // All supported configs
            match device.supported_output_configs() {
                Ok(configs) => {
                    println!("    Supported configs:");
                    for cfg in configs {
                        println!(
                            "      Channels: {}, Rate: {}-{}, Format: {:?}, Buffer: {:?}",
                            cfg.channels(),
                            cfg.min_sample_rate().0,
                            cfg.max_sample_rate().0,
                            cfg.sample_format(),
                            cfg.buffer_size()
                        );
                    }
                }
                Err(e) => {
                    println!("    Could not query supported configs: {}", e);
                }
            }
        }

        println!();
    }
}
