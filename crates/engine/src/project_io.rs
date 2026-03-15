use std::collections::HashMap;
use std::fs;
use std::path::Path;

use uuid::Uuid;

use jamhub_model::{ClipSource, Project};

use crate::audio_file;

/// Save a project and its audio buffers to a directory.
pub fn save_project(
    dir: &Path,
    project: &Project,
    audio_buffers: &HashMap<Uuid, Vec<f32>>,
    sample_rate: u32,
) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create project dir: {e}"))?;

    let audio_dir = dir.join("audio");
    fs::create_dir_all(&audio_dir).map_err(|e| format!("Failed to create audio dir: {e}"))?;

    // Save audio buffers as WAV files
    for (id, samples) in audio_buffers {
        let wav_path = audio_dir.join(format!("{id}.wav"));
        audio_file::save_wav(&wav_path, samples, sample_rate)?;
    }

    // Save project metadata as JSON
    let json =
        serde_json::to_string_pretty(project).map_err(|e| format!("Failed to serialize: {e}"))?;
    let project_path = dir.join("project.json");
    fs::write(&project_path, json).map_err(|e| format!("Failed to write project file: {e}"))?;

    Ok(())
}

/// Load a project and its audio buffers from a directory.
pub fn load_project(dir: &Path) -> Result<(Project, HashMap<Uuid, Vec<f32>>), String> {
    let project_path = dir.join("project.json");
    let json = fs::read_to_string(&project_path)
        .map_err(|e| format!("Failed to read project file: {e}"))?;
    let project: Project =
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse project: {e}"))?;

    let audio_dir = dir.join("audio");
    let mut audio_buffers = HashMap::new();

    // Load all referenced audio buffers
    for track in &project.tracks {
        for clip in &track.clips {
            if let ClipSource::AudioBuffer { buffer_id } = &clip.source {
                if !audio_buffers.contains_key(buffer_id) {
                    let wav_path = audio_dir.join(format!("{buffer_id}.wav"));
                    if wav_path.exists() {
                        let data = audio_file::load_wav(&wav_path)?;
                        audio_buffers.insert(*buffer_id, data.samples);
                    }
                }
            }
        }
    }

    Ok((project, audio_buffers))
}
