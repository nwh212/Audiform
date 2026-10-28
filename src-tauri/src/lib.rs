// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Audiform core (based on Altered Audio Editor).
//!
//! The interface never touches audio. It sends commands to this core
//! and renders the state it gets back.

pub mod browse;
mod carry_over;
pub mod codecs;
pub mod commands;
pub mod dsp;
pub mod engine;
pub mod export;
pub mod library;
pub mod peaks;
pub mod plugins;
pub mod session;
pub mod sesx;

/// Starts the desktop app.
pub fn run() {
    let cwd = std::env::current_dir().unwrap_or_default();
    let launch = commands::app::file_args(std::env::args().skip(1), &cwd);
    tauri::Builder::default()
        // Opening a session or audio file from File Explorer while Audiform
        // runs hands it to the running window, which comes to the front,
        // instead of starting a second, empty Audiform.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            use tauri::{Emitter, Manager};
            let paths =
                commands::app::file_args(argv.into_iter().skip(1), std::path::Path::new(&cwd));
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            if !paths.is_empty() {
                let _ = app.emit("open-paths", paths);
            }
        }))
        .manage(commands::app::LaunchPaths(std::sync::Mutex::new(launch)))
        .manage(engine::PreviewPlayer::default())
        .manage(engine::Transport::default())
        .manage(engine::Recorder::default())
        .manage(commands::session::OpenSession::default())
        .manage(commands::export::ExportCancel::default())
        .manage(commands::batch::BatchCancel::default())
        .manage(commands::wave::OpenWave::default())
        .manage(commands::wave::WaveChannelsOff::default())
        .manage(commands::wave::ParkedWaves::default())
        .manage(commands::session::ParkedSessions::default())
        .manage(commands::wave::Playing::default())
        .setup(|app| {
            use tauri::Manager;
            // Settings from Altered Audio Editor, the first time.
            for dir in [app.path().app_config_dir(), app.path().app_data_dir()]
                .into_iter()
                .flatten()
            {
                carry_over::carry_over(&dir);
            }
            if let Ok(dir) = app.path().app_config_dir() {
                plugins::load(&dir);
            }
            // Reversed clips read reversed copies kept with the other working files.
            if let Ok(root) = commands::cache::cache_root(app.handle()) {
                engine::reverse::set_dir(root.join("Reversed"));
            }
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::app::app_info,
            commands::app::app_exit,
            commands::app::app_minimize,
            commands::browse::browse_locations,
            commands::browse::browse_list,
            commands::browse::preview_play,
            commands::browse::preview_stop,
            commands::browse::preview_status,
            commands::keymap::keymap_load,
            commands::keymap::keymap_save,
            commands::library::library_default_root,
            commands::library::library_create,
            commands::library::library_open,
            commands::library::library_save,
            commands::library::library_import,
            commands::library::library_clean_up,
            commands::library::library_pick_folder,
            commands::links::open_link,
            commands::media::media_expand,
            commands::media::media_probe,
            commands::media::media_pick_files,
            commands::media::media_pick_tracks,
            commands::media::media_unzip,
            commands::media::media_pick_folder,
            commands::media::media_reveal,
            commands::media::media_copy_in,
            commands::peaks::peaks_ensure,
            commands::peaks::peaks_level,
            commands::peaks::media_frames,
            commands::peaks::scope_phase,
            commands::peaks::scope_phase_at,
            commands::peaks::scope_spectrum,
            commands::preferences::preferences_load,
            commands::preferences::preferences_save,
            commands::transport::transport_play,
            commands::transport::transport_stop,
            commands::transport::transport_audition_level,
            commands::transport::transport_audition_clip,
            commands::transport::transport_status,
            commands::transport::transport_set_mono_check,
            commands::transport::transport_set_metronome,
            commands::amp::amp_pick,
            commands::amp::amp_load_model,
            commands::amp::amp_load_ir,
            commands::firewall::firewall_status,
            commands::app::alt_down,
            commands::app::launch_paths,
            commands::firewall::firewall_block,
            commands::reverb::reverb_pick,
            commands::reverb::reverb_load,
            commands::plugins::plugins_registry,
            commands::plugins::plugins_set_shared,
            commands::plugins::plugins_scan,
            commands::plugins::plugins_set_folder,
            commands::plugins::plugins_scan_stop,
            commands::plugins::midi_audition,
            commands::plugins::midi_play_notes,
            commands::plugins::plugin_open_window,
            commands::plugins::plugin_close_window,
            commands::plugins::plugin_events,
            commands::plugins::plugin_state,
            commands::plugins::plugin_restart,
            commands::plugins::plugin_presets,
            commands::plugins::plugin_preset_read,
            commands::plugins::plugin_preset_save,
            commands::plugins::plugin_preset_delete,
            commands::devices::audio_inputs,
            commands::devices::audio_outputs,
            commands::devices::set_audio_output,
            commands::devices::set_output_map,
            commands::devices::output_channels,
            commands::devices::audio_driver,
            commands::devices::set_audio_driver,
            commands::cache::cache_info,
            commands::cache::cache_clear_peaks,
            commands::cache::cache_set_folder,
            commands::devices::open_sound_settings,
            commands::record::record_start,
            commands::record::record_stop,
            commands::record::record_add,
            commands::plugins::plugin_show_bypassed,
            commands::plugins::plugin_hide_windows,
            commands::record::record_remove,
            commands::record::record_status,
            commands::record::record_monitor,
            commands::quickpass::quickpass_levels,
            commands::quickpass::quickpass_spots,
            commands::quickpass::quickpass_repair_copy,
            commands::generate::generate_audio,
            commands::spectral::wave_spectrogram,
            commands::wave::wave_set_channels_off,
            commands::live::wave_live_render,
            commands::live::wave_live_clear,
            commands::prerender::prerender_status,
            commands::spectral::wave_spectral_edit,
            commands::wave_fx::wave_apply_chain,
            commands::wave_fx::wave_reverse_invert,
            commands::wave_fx::wave_channel_edit,
            commands::wave_fx::wave_stretch,
            commands::spectral::wave_pitch,
            commands::session::session_get,
            commands::session::session_apply,
            commands::session::session_set_view,
            commands::session::session_new,
            commands::session::session_park,
            commands::session::session_resume,
            commands::session::session_forget,
            commands::session::session_save,
            commands::session::session_open,
            commands::session::session_autosave,
            commands::session::session_clear_autosave,
            commands::session::session_recoverable,
            commands::session::session_recover,
            commands::session::session_discard_recovery,
            commands::session::session_pick_save,
            commands::session::session_pick_open,
            commands::session::file_pick_open,
            commands::effects::effects_catalog,
            commands::effects::effect_channels,
            commands::effects::learn_noise_print,
            commands::effects::effect_response,
            commands::effects::shaped_noise_curves,
            commands::effects::effect_meter,
            commands::effects::vox_tone_curve,
            commands::align::phase_align,
            commands::musicfit::music_fit,
            commands::export::export_pick_path,
            commands::export::export_text,
            commands::export::export_embed_chapters,
            commands::export::export_pick_folder,
            commands::export::paths_exist,
            commands::batch::batch_convert,
            commands::batch::batch_cancel,
            commands::export::export_mixdown,
            commands::export::export_bounce,
            commands::export::clip_reverse_prepare,
            commands::export::export_cancel,
            commands::export::loudness_check,
            commands::export::export_merge,
            commands::export::export_region_copy,
            commands::wave::wave_open,
            commands::wave::wave_get,
            commands::wave::wave_close,
            commands::wave::wave_park,
            commands::wave::wave_resume,
            commands::wave::wave_forget,
            commands::wave::wave_apply,
            commands::wave::wave_set_view,
            commands::wave::wave_play,
            commands::wave::wave_save,
            commands::wave::wave_peak,
            commands::wave::wave_new_from_clips,
            commands::wave::wave_new,
            commands::template::template_list,
            commands::template::template_save,
            commands::template::template_delete,
            commands::session::session_default_folder,
            commands::session::session_next_name,
            commands::workspace::workspace_list,
            commands::workspace::workspace_get,
            commands::workspace::workspace_put,
            commands::workspace::workspace_delete,
            commands::session::session_import_sesx,
            commands::session::session_export_sesx,
            commands::app::app_set_title,
            commands::workspace::workspace_load,
            commands::workspace::workspace_save,
            commands::workspace::workspace_clear,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Audiform");
}
