


use crate::audio::preview::LoopRange;
use crate::audio::{MixSnapshot, PreviewEngine, SourceLibrary};
use crate::ae::{AeLayerSnapshot, AeProjectSnapshot, AeSession, AeState};
use crate::core::loc::{self, Lang};
use crate::mmd::{MmdSession, MmdState, PlaybackRange};
use crate::model::{Clip, Id, Marker, Project, Track, UndoStack};
use crate::ui::arranger::{self, ArrAction, ArrOutput, ArrangerState, Tool};
use crate::ui::config::AppConfig;
use crate::ui::icons::{self, Icon};
use crate::ui::keybindings::{text_input_active, Action, Chord, KeyBindings};
use crate::ui::numeric::Integer;
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;


const AUDIO_EXTS: &[&str] = &[
    "wav", "wave", "mp3", "m4a", "aac", "flac", "ogg", "opus", "wma", "mp4", "mov", "m4v", "mkv",
    "webm", "avi", "wmv",
];
const UI_ROW_HEIGHT: f32 = 18.0;

pub struct App {
    pub doc: Project,
    pub undo: UndoStack,
    pub config: AppConfig,
    pub keys: KeyBindings,
    pub project_path: Option<PathBuf>,
    pub dirty: bool,
    
    show_presets: bool,
    show_keys: bool,
    presets_pos: Option<egui::Pos2>,
    keys_pos: Option<egui::Pos2>,
    last_title: String,

    pub lib: SourceLibrary,
    pub engine: Option<PreviewEngine>,

    pub arr: ArrangerState,
    pub selection: Vec<(usize, Id)>,
    hovered_audio_path: Option<PathBuf>,
    pub mmd_connected: bool,
    pub mmd: MmdSession,
    pub mmd_state: MmdState,
    mmd_procs: Vec<crate::mmd::MmdProcess>,
    mmd_range: PlaybackRange,
    pub ae_connected: bool,
    pub ae: AeSession,
    pub ae_state: AeState,
    ae_procs: Vec<crate::ae::AeProcess>,
    ae_project: Option<AeProjectSnapshot>,
    ae_pending_project: Option<AeProjectSnapshot>,
    loop_enabled: bool,

    
    pub capturing: Option<Action>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        loc::set_lang(match config.lang.as_str() {
            "en" => Lang::En,
            "zh" => Lang::Zh,
            "ja" => Lang::Ja,
            _ => Lang::detect_system(),
        });
        apply_dark_ui(&cc.egui_ctx);
        apply_window_level(&cc.egui_ctx, config.always_on_top);
        crate::ui::fonts::apply(&cc.egui_ctx, loc::lang());
        apply_text_scale(&cc.egui_ctx);
        
        cc.egui_ctx.set_zoom_factor(1.0);

        let engine = match PreviewEngine::new() {
            Ok(e) => Some(e),
            Err(e) => {
                log::error!("オーディオ初期化に失敗（無音で続行）: {e:#}");
                None
            }
        };

        Self {
            doc: Project::default(),
            undo: UndoStack::default(),
            keys: KeyBindings::load(),
            config,
            project_path: None,
            dirty: false,
            show_presets: false,
            show_keys: false,
            presets_pos: None,
            keys_pos: None,
            last_title: String::new(),
            lib: SourceLibrary::default(),
            engine,
            arr: ArrangerState::default(),
            selection: Vec::new(),
            hovered_audio_path: None,
            mmd_connected: false,
            mmd: MmdSession::start(),
            mmd_state: MmdState::default(),
            mmd_procs: Vec::new(),
            mmd_range: PlaybackRange { start: 0, end: -1 },
            ae_connected: false,
            ae: AeSession::start(),
            ae_state: AeState::default(),
            ae_procs: Vec::new(),
            ae_project: None,
            ae_pending_project: None,
            loop_enabled: false,
            capturing: None,
        }
    }

    

    fn presets_panel(&mut self, ui: &mut egui::Ui) {
        let fps = self.doc.fps.max(1) as f64;
        let ph_frame = self
            .engine
            .as_ref()
            .map(|e| (e.position_seconds() * fps).round() as i64)
            .unwrap_or(0);

        ui.horizontal(|ui| {
            if ui.button(loc::t("preset.addMarker")).clicked() {
                let n = self.doc.markers.len() + 1;
                self.edit("undo.presetAdd", None, |d| {
                    d.markers.push(Marker {
                        name: format!("{}{}", loc::t("preset.defaultName"), n),
                        frame: ph_frame,
                    });
                });
            }
        });
        let mut del: Option<usize> = None;
        let mut seek: Option<i64> = None;
        let mut edits: Vec<(usize, String, i64)> = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("frame_presets")
            .auto_shrink([false, false])
            .show(ui, |ui| {
            for (i, m) in self.doc.markers.iter().enumerate() {
                ui.horizontal(|ui| {
                    let mut name = m.name.clone();
                    let mut frame = m.frame;
                    let r1 = ui.add(
                        egui::TextEdit::singleline(&mut name).desired_width(PRESET_NAME_W),
                    );
                    let r2 = ui.add_sized(
                        [preset_frame_field_width(ui), ui.spacing().interact_size.y],
                        Integer::new(&mut frame).suffix(" f"),
                    );
                    if r1.changed() || r2.changed() {
                        edits.push((i, name.clone(), frame));
                    }
                    if icons::button(ui, Icon::Play, PRESET_ICON).clicked() {
                        seek = Some(m.frame);
                    }
                    if icons::button(ui, Icon::Delete, PRESET_ICON).clicked() {
                        del = Some(i);
                    }
                });
            }
            });

        for (i, name, frame) in edits {
            self.edit("undo.presetEdit", Some(0x00E0_0000 ^ i as u64), |d| {
                if let Some(m) = d.markers.get_mut(i) {
                    m.name = name;
                    m.frame = frame;
                }
            });
        }
        if let Some(f) = seek {
            if let Some(e) = &self.engine {
                e.seek_seconds(f as f64 / fps);
            }
        }
        if let Some(i) = del {
            self.edit("undo.presetDel", None, |d| {
                if i < d.markers.len() {
                    d.markers.remove(i);
                }
            });
        }
    }

    

    fn keys_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(loc::t("keys.reset")).clicked() {
                self.keys.reset();
                self.keys.save();
                self.capturing = None;
            }
            if self.capturing.is_some() && ui.button(loc::t("keys.cancel")).clicked() {
                self.capturing = None;
            }
        });
        ui.separator();

        let shortcut_w = Action::ALL
            .iter()
            .map(|a| text_width(ui, &self.keys.get(*a).to_display(), egui::TextStyle::Monospace))
            .fold(text_width(ui, &loc::t("keys.waiting"), egui::TextStyle::Body), f32::max);
        let button_w =
            text_width(ui, &loc::t("keys.change"), egui::TextStyle::Button) + MENU_ITEM_PADDING_X * 4.0;
        let spacing = ui.spacing().item_spacing.x;
        let desc_w = (ui.available_width() - shortcut_w - button_w - spacing * 3.0).max(80.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
        egui::Grid::new("keys_grid")
            .num_columns(3)
            .min_col_width(0.0)
            .show(ui, |ui| {
            for a in Action::ALL {
                ui.add_sized(
                    [desc_w, ui.spacing().interact_size.y],
                    egui::Label::new(loc::t(a.loc_key())).halign(egui::Align::LEFT),
                );
                if self.capturing == Some(a) {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 180, 60),
                        loc::t("keys.waiting"),
                    );
                } else {
                    ui.monospace(self.keys.get(a).to_display());
                }
                if ui
                    .add_sized(
                        [button_w, ui.spacing().interact_size.y],
                        egui::Button::new(loc::t("keys.change")),
                    )
                    .clicked()
                {
                    self.capturing = Some(a);
                }
                ui.end_row();
            }
        });
            });
    }

    fn capture_key(&mut self, ctx: &egui::Context) -> bool {
        let Some(action) = self.capturing else {
            return false;
        };
        let keys: Vec<(egui::Key, egui::Modifiers)> = ctx.input_mut(|i| {
            let mut out = Vec::new();
            i.events.retain(|ev| {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } = ev
                {
                    if !repeat {
                        out.push((*key, *modifiers));
                    }
                    false
                } else {
                    !matches!(ev, egui::Event::Text(_))
                }
            });
            out
        });

        if let Some((key, m)) = keys.into_iter().next() {
            if key == egui::Key::Escape {
                self.capturing = None;
            } else {
                self.keys.set(
                    action,
                    Chord {
                        key,
                        ctrl: m.ctrl,
                        shift: m.shift,
                        alt: m.alt,
                    },
                );
                self.keys.save();
                self.capturing = None;
            }
        }
        true
    }

    
    fn poll_mmd(&mut self) {
        self.mmd_state = self.mmd.state();
        self.mmd_connected = self.mmd_state.attached;
        if self.mmd_connected {
            if let Some(engine) = &self.engine {
                engine.set_loop_range(None);
            }
        }
        if let Some(range) = self.mmd.take_playback_range() {
            self.mmd_range = range;
        }

        if !self.mmd_connected {
            return;
        }
        let fps = self.doc.fps.max(1) as f64;
        if let Some(e) = &self.engine {
            
            
            e.sync(self.mmd_state.frame / fps, self.mmd_state.playing);
        }

        
        if let Some(p) = self.mmd_state.wav_path.clone() {
            if !self.lib.contains(&p) {
                let _ = self.lib.get_or_load(&p);
            }
        }
    }

    fn poll_ae(&mut self) {
        self.ae_state = self.ae.state();
        self.ae_connected = self.ae_state.attached;
        if let Some(result) = self.ae.take_project() {
            match result {
                Ok(snapshot) => {
                    self.ae_project = Some(snapshot.clone());
                    self.ae_pending_project = Some(snapshot);
                }
                Err(error) => log::error!("After Effectsプロジェクト取得失敗: {error}"),
            }
        }
    }

    fn external_frame(&self) -> f64 {
        if self.mmd_connected {
            self.mmd_state.frame
        } else {
            let fps = self.doc.fps.max(1) as f64;
            self.engine
                .as_ref()
                .map(|e| (e.position_seconds() * fps).max(0.0))
                .unwrap_or(0.0)
        }
    }

    fn external_playing(&self) -> bool {
        self.mmd_connected && self.mmd_state.playing
    }

    fn external_range(&self) -> PlaybackRange {
        if self.mmd_connected {
            self.mmd_range
        } else {
            self.mmd_range
        }
    }

    fn external_range_mut(&mut self) -> &mut PlaybackRange {
        &mut self.mmd_range
    }

    fn external_wav_path(&self) -> Option<PathBuf> {
        if self.mmd_connected {
            self.mmd_state.wav_path.clone()
        } else if self.ae_connected {
            self.ae_project
                .as_ref()
                .and_then(|p| p.layers.first().map(|l| l.source.clone()))
        } else {
            None
        }
    }

    fn normalize_playback_settings(&mut self) {
        if self.mmd_connected {
            self.mmd.normalize_playback();
            self.mmd_range = PlaybackRange { start: 0, end: -1 };
        } else {
            self.mmd_range = PlaybackRange { start: 0, end: -1 };
        }
    }

    
    fn apply_arr(&mut self, out: ArrOutput) {
        let mk = out.merge_key;
        for a in out.actions {
            match a {
                ArrAction::Seek(sec) => {
                    
                    let sec = sec.max(0.0);
                    if let Some(e) = &self.engine {
                        e.seek_seconds(sec);
                    }
                    if self.mmd_connected {
                        let fps = self.doc.fps.max(1) as f64;
                        self.mmd.set_current_frame((sec * fps).round() as i64);
                    }
                }
                ArrAction::SeekStop(sec) => {
                    
                    
                    let sec = sec.max(0.0);
                    if let Some(e) = &self.engine {
                        e.set_playing(false);
                        e.seek_seconds(sec);
                    }
                    if self.mmd_connected {
                        
                        self.mmd.stop_playback();
                        let fps = self.doc.fps.max(1) as f64;
                        self.mmd.set_current_frame((sec * fps).round() as i64);
                        
                        
                        self.mmd_state.playing = false;
                    }
                }
                ArrAction::MoveClips(items) => self.edit("undo.clipMove", mk, |d| {
                    for (track, clip, new_start) in items {
                        if let Some(c) = clip_at_mut(d, track, clip) {
                            let delta = new_start - c.start_frame;
                            c.start_frame += delta;
                            c.in_frame += delta;
                            c.out_frame += delta;
                            c.end_frame += delta;
                        }
                    }
                }),
                ArrAction::TrimIn {
                    track,
                    clip,
                    new_in,
                } => {
                    
                    let lo = self
                        .doc
                        .tracks
                        .get(track)
                        .and_then(|t| t.clip(clip))
                        .map(|c| c.start_frame);
                    self.edit("undo.trimIn", mk, |d| {
                        if let Some(c) = clip_at_mut(d, track, clip) {
                            let lo = lo.unwrap_or(c.start_frame);
                            c.in_frame = new_in.clamp(lo, c.out_frame - 1.0);
                        }
                    });
                }
                ArrAction::TrimOut {
                    track,
                    clip,
                    new_out,
                } => {
                    
                    let src = self
                        .doc
                        .tracks
                        .get(track)
                        .and_then(|t| t.clip(clip))
                        .map(|c| (c.start_frame, c.source.clone()));
                    let hi = src
                        .and_then(|(start, s)| self.source_len_frames(&s).map(|len| start + len as f64));
                    self.edit("undo.trimOut", mk, |d| {
                        if let Some(c) = clip_at_mut(d, track, clip) {
                            let mut v = new_out.max(c.in_frame + 1.0);
                            if let Some(hi) = hi {
                                v = v.min(hi);
                            }
                            c.out_frame = v;
                            c.end_frame = c.out_frame;
                        }
                    });
                }
                ArrAction::FadeIn { track, clip, frames } => self.edit("undo.fadeIn", mk, |d| {
                    if let Some(c) = clip_at_mut(d, track, clip) {
                        c.fade_in_frames = frames.clamp(0, (c.out_frame - c.in_frame - 1.0).max(0.0) as i64);
                    }
                }),
                ArrAction::FadeOut { track, clip, frames } => self.edit("undo.fadeOut", mk, |d| {
                    if let Some(c) = clip_at_mut(d, track, clip) {
                        c.fade_out_frames = frames.clamp(0, (c.out_frame - c.in_frame - 1.0).max(0.0) as i64);
                    }
                }),
                ArrAction::Split {
                    track,
                    clip,
                    at_frame,
                } => self.edit("undo.split", None, |d| {
                    split_clip(d, track, clip, at_frame);
                }),
                ArrAction::Delete { track } => {
                    self.edit("undo.clipDel", None, |d| {
                        if track < d.tracks.len() {
                            d.tracks.remove(track);
                        }
                    });
                    self.selection.clear();
                }
                ArrAction::Duplicate { track } => {
                    
                    self.edit("undo.clipDup", None, |d| {
                        if let Some(src) = d.tracks.get(track).cloned() {
                            let mut dup = src;
                            dup.id = Id::new();
                            for c in &mut dup.clips {
                                c.id = Id::new();
                            }
                            d.tracks.insert(track + 1, dup);
                        }
                    });
                    self.selection.clear();
                }
                ArrAction::ToggleMute { track, clip } => self.edit("undo.clipMute", None, |d| {
                    if let Some(c) = clip_at_mut(d, track, clip) {
                        c.muted = !c.muted;
                    }
                }),
                ArrAction::ToggleTrackMute { track } => self.edit("undo.trackMute", None, |d| {
                    if let Some(t) = d.tracks.get_mut(track) {
                        t.muted = !t.muted;
                    }
                }),
                ArrAction::ToggleTrackSolo { track } => self.edit("undo.trackSolo", None, |d| {
                    if let Some(t) = d.tracks.get_mut(track) {
                        t.soloed = !t.soloed;
                    }
                }),
                ArrAction::RenameTrack { track, name } => {
                    self.edit("undo.trackRename", None, |d| {
                        if let Some(t) = d.tracks.get_mut(track) {
                            if !name.trim().is_empty() {
                                t.name = name;
                            }
                        }
                    });
                }
                ArrAction::SetTrackVolume { track, db } => {
                    self.edit("undo.trackVolume", mk, |d| {
                        if let Some(t) = d.tracks.get_mut(track) {
                            t.volume_db = db.clamp(-60.0, 12.0);
                        }
                    });
                }
                ArrAction::ReorderTrack { from, to } => {
                    if from < self.doc.tracks.len() && to != from && to != from + 1 {
                        self.edit("undo.trackReorder", mk, |d| {
                            let t = d.tracks.remove(from);
                            let ins = if to > from { to - 1 } else { to };
                            d.tracks.insert(ins.min(d.tracks.len()), t);
                        });
                    }
                }
                ArrAction::AddSourceDialog { start_frame } => self.open_audio_dialog(start_frame),
                ArrAction::AddSourceFromMmd { start_frame } => {
                    if self.mmd_connected {
                        self.add_source_from_mmd(start_frame);
                    } else if self.ae_connected {
                        self.add_source_from_ae(start_frame);
                    }
                }
            }
        }
    }

    
    fn nudge_selection(&mut self, delta: i64) {
        if self.selection.is_empty() || delta == 0 {
            return;
        }
        let items = self.selection.clone();
        self.edit("undo.nudge", Some(0x004E_5544_4745), |d| {
            for (ti, ci) in items {
                if let Some(c) = clip_at_mut(d, ti, ci) {
                    c.start_frame += delta as f64;
                    c.in_frame += delta as f64;
                    c.out_frame += delta as f64;
                    c.end_frame += delta as f64;
                }
            }
        });
    }

    
    fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let mut idx: Vec<usize> = self.selection.iter().map(|(ti, _)| *ti).collect();
        idx.sort_unstable();
        idx.dedup();
        self.edit("undo.clipDel", None, |d| {
            for &ti in idx.iter().rev() {
                if ti < d.tracks.len() {
                    d.tracks.remove(ti);
                }
            }
        });
        self.selection.clear();
    }

    
    fn source_len_frames(&self, source: &std::path::Path) -> Option<i64> {
        let fps = self.doc.fps.max(1) as f64;
        self.lib
            .get(source)
            .map(|s| (s.seconds() * fps).ceil().max(1.0) as i64)
    }


    
    fn rebuild_snapshot(&mut self) {
        
        let paths: Vec<PathBuf> = self
            .doc
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|c| c.source.clone()))
            .collect();
        for p in paths {
            if !self.lib.contains(&p) {
                if let Err(e) = self.lib.get_or_load(&p) {
                    log::warn!("音源読込失敗 {}: {e:#}", p.display());
                }
            }
        }
        if let Some(engine) = &self.engine {
            let snap = MixSnapshot::build(&self.doc, &self.lib, self.config.main_volume_db);
            engine.publish(Arc::new(snap));
        }
    }

    

    
    
    pub fn edit(&mut self, label: &str, merge_key: Option<u64>, f: impl FnOnce(&mut Project)) {
        let before = self.doc.clone();
        f(&mut self.doc);
        self.undo.commit(label, merge_key, before);
        self.dirty = true;
        self.rebuild_snapshot();
    }

    pub fn do_undo(&mut self) {
        if self.undo.can_undo() {
            self.undo.undo(&mut self.doc);
            self.dirty = true;
            self.rebuild_snapshot();
        }
    }
    pub fn do_redo(&mut self) {
        if self.undo.can_redo() {
            self.undo.redo(&mut self.doc);
            self.dirty = true;
            self.rebuild_snapshot();
        }
    }

    

    fn playhead_position_frame(&self) -> f64 {
        let fps = self.doc.fps.max(1) as f64;
        self.engine
            .as_ref()
            .map(|e| (e.position_seconds() * fps).max(0.0))
            .unwrap_or(0.0)
    }

    fn playhead_frame(&self) -> i64 {
        self.playhead_position_frame().round() as i64
    }

    
    fn seek_marker(&mut self, forward: bool) {
        let cur = self.playhead_frame();
        let mut frames: Vec<i64> = self.doc.markers.iter().map(|m| m.frame).collect();
        frames.sort_unstable();
        let target = if forward {
            frames.into_iter().find(|&f| f > cur)
        } else {
            frames.into_iter().rev().find(|&f| f < cur)
        };
        if let Some(f) = target {
            let sec = f.max(0) as f64 / self.doc.fps.max(1) as f64;
            self.apply_arr(ArrOutput {
                actions: vec![ArrAction::SeekStop(sec)],
                merge_key: None,
            });
        }
    }

    
    fn open_audio_dialog(&mut self, start_frame: i64) {
        let files = rfd::FileDialog::new()
            .add_filter(loc::t("arr.audioFilter"), AUDIO_EXTS)
            .pick_files();
        for p in files.unwrap_or_default() {
            self.add_source_path(p, start_frame);
        }
    }

    fn add_source_from_mmd(&mut self, start_frame: i64) {
        match self.mmd_state.wav_path.clone() {
            Some(p) => self.add_source_path(p, start_frame),
            None => log::warn!("MMD が音源を読み込んでいません"),
        }
    }

    fn add_source_from_ae(&mut self, start_frame: i64) {
        match self
            .ae_project
            .as_ref()
            .and_then(|p| p.layers.first().map(|l| l.source.clone()))
        {
            Some(p) => self.add_source_path(p, start_frame),
            None => log::warn!("After Effectsの取得済みプロジェクトに音源がありません"),
        }
    }

    fn ae_snapshot_from_doc(&self) -> AeProjectSnapshot {
        let layers = self
            .doc
            .tracks
            .iter()
            .flat_map(|track| {
                track.clips.iter().map(|clip| AeLayerSnapshot {
                    key: format!("{}:{}", track.id.0, clip.id.0),
                    name: track.name.clone(),
                    source: clip.source.clone(),
                    start_frame: clip.start_frame.round() as i64,
                    in_frame: clip.in_frame.round() as i64,
                    out_frame: clip.out_frame.round() as i64,
                    gain_db: clip.gain_db,
                    muted: track.muted || clip.muted,
                    fade_in_frames: clip.fade_in_frames as i64,
                    fade_out_frames: clip.fade_out_frames as i64,
                })
            })
            .collect();
        AeProjectSnapshot {
            fps: self.doc.fps.max(1) as f64,
            duration_frames: self.doc.content_end_frame().max(0),
            layers,
        }
    }

    fn import_ae_project(&mut self) {
        let Some(snapshot) = self.ae_pending_project.take() else {
            return;
        };
        let fps = snapshot.fps.round();
        if !snapshot.fps.is_finite() || fps < 1.0 || (snapshot.fps - fps).abs() > 0.001 {
            log::error!("After Effectsのフレームレートは整数値のみ取り込めます");
            return;
        }
        let tracks = snapshot
            .layers
            .into_iter()
            .map(|layer| {
                let start = layer.start_frame;
                let in_frame = layer.in_frame.max(start);
                let out_frame = layer.out_frame.max(in_frame + 1);
                let mut clip = Clip::new(layer.source, start, out_frame - in_frame);
                clip.in_frame = in_frame as f64;
                clip.out_frame = out_frame as f64;
                clip.end_frame = clip.out_frame;
                clip.gain_db = layer.gain_db;
                clip.muted = layer.muted;
                let mut track = Track::new(layer.name);
                track.clips.push(clip);
                track
            })
            .collect::<Vec<_>>();
        let fps = fps as u32;
        self.edit("undo.aeProjectImport", None, |doc| {
            doc.fps = fps;
            doc.tracks = tracks;
        });
    }

    
    fn add_source_path(&mut self, path: PathBuf, start_frame: i64) {
        self.add_source_path_at(path, start_frame, self.doc.tracks.len());
    }

    fn add_source_path_at(
        &mut self,
        path: PathBuf,
        start_frame: i64,
        track_index: usize,
    ) -> bool {
        // The length comes from a metadata probe, so the layer appears at once;
        // samples and waveform arrive from the worker thread afterwards.
        let seconds = match self.lib.get(&path) {
            Some(src) => src.seconds(),
            None => match crate::audio::decode::probe_seconds(&path) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("音源を読み込めません {}: {e:#}", path.display());
                    return false;
                }
            },
        };
        self.lib.request_load(&path);
        let fps = self.doc.fps.max(1) as f64;
        let len_frames = (seconds * fps).ceil().max(1.0) as i64;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| loc::t("arr.audioDefaultName"));
        self.edit("undo.addSource", None, |d| {
            let mut t = Track::new(name);
            t.clips.push(Clip::new(path.clone(), start_frame, len_frames));
            let index = track_index.min(d.tracks.len());
            d.tracks.insert(index, t);
        });
        true
    }

    fn update_hovered_file_preview(&mut self, ctx: &egui::Context) {
        let (path, external_drag_active) = ctx.input(|input| {
            let path = input
                .raw
                .hovered_files
                .iter()
                .find_map(|file| file.path.clone())
                .or_else(|| {
                    input
                        .raw
                        .dropped_files
                        .iter()
                        .find_map(|file| file.path.clone())
                });
            (
                path,
                !input.raw.hovered_files.is_empty() || !input.raw.dropped_files.is_empty(),
            )
        });
        self.arr.external_drop_pos = if external_drag_active {
            system_cursor_client_pos(ctx)
        } else {
            None
        };
        let Some(path) = path else {
            self.hovered_audio_path = None;
            self.arr.drop_preview_frames = None;
            self.arr.drop_preview_name = None;
            return;
        };
        // OS file drags do not reliably emit egui pointer-move events. Repaint
        // continuously so the Win32 cursor position is sampled every frame.
        ctx.request_repaint();
        if self.hovered_audio_path.as_ref() == Some(&path) {
            return;
        }

        self.hovered_audio_path = Some(path.clone());
        self.arr.drop_preview_frames = None;
        self.arr.drop_preview_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned);
        let seconds = self
            .lib
            .get(&path)
            .map(|source| source.seconds())
            .or_else(|| match crate::audio::decode::probe_seconds(&path) {
                Ok(seconds) => Some(seconds),
                Err(error) => {
                    log::warn!("音源の長さを取得できません {}: {error:#}", path.display());
                    None
                }
            });
        if let Some(seconds) = seconds {
            self.arr.drop_preview_frames = audio_length_frames(seconds, self.doc.fps);
        }
    }

    
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        let start = self
            .arr
            .drop_frame
            .take()
            .or_else(|| self.arr.last_drop_frame.take())
            .unwrap_or_else(|| self.playhead_frame());
        let mut track_index = self
            .arr
            .drop_track_index
            .take()
            .unwrap_or(self.doc.tracks.len())
            .min(self.doc.tracks.len());
        for p in dropped {
            if self.add_source_path_at(p, start, track_index) {
                track_index += 1;
            }
        }
    }

    

    fn file_new(&mut self) {
        self.doc = Project::default();
        self.undo.clear();
        self.project_path = None;
        self.dirty = false;
        self.rebuild_snapshot();
    }

    fn file_open(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(loc::t("dlg.projectFilter"), &["json"])
            .pick_file()
        else {
            return;
        };
        match crate::model::store::load(&path) {
            Ok(doc) => {
                self.doc = doc;
                self.undo.clear();
                self.project_path = Some(path);
                self.dirty = false;
                self.rebuild_snapshot();
            }
            Err(e) => log::error!("開けませんでした: {e:#}"),
        }
    }

    fn file_save(&mut self) {
        match self.project_path.clone() {
            Some(path) => self.write_to(&path),
            None => self.file_save_as(),
        }
    }

    fn file_save_as(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(loc::t("dlg.projectFilter"), &["json"])
            .set_file_name("project.json")
            .save_file()
        else {
            return;
        };
        self.write_to(&path);
        self.project_path = Some(path);
    }

    fn write_to(&mut self, path: &std::path::Path) {
        match crate::model::store::save(path, &self.doc) {
            Ok(()) => self.dirty = false,
            Err(e) => log::error!("保存に失敗: {e:#}"),
        }
    }

    

    fn export_wav(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(loc::t("arr.audioFilter"), &["wav"])
            .set_file_name("mix.wav")
            .save_file()
        else {
            return;
        };
        match crate::export::export_mix_wav(&path, &self.doc, &self.lib, self.config.main_volume_db)
        {
            Ok(()) => log::info!("ミックス WAV を書き出し: {}", path.display()),
            Err(e) => log::error!("WAV 書き出し失敗: {e:#}"),
        }
    }

    fn export_jsx(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("After Effects Script", &["jsx"])
            .set_file_name("AudioFrame.jsx")
            .save_file()
        else {
            return;
        };
        let script = crate::ae::layer_script(&self.ae_snapshot_from_doc());
        match std::fs::write(&path, script) {
            Ok(()) => log::info!("レイヤー構成を書き出し: {}", path.display()),
            Err(e) => log::error!("JSX 書き出し失敗: {e:#}"),
        }
    }

    /// Push every layer's own file path to After Effects through JSX,
    /// under the "AudioFrame" group, in the same shape as the WAV apply.
    fn export_jsx_apply(&mut self) {
        if !self.ae_connected {
            log::error!("After Effects未接続のため、音声レイヤーを反映できません");
            return;
        }
        let snapshot = self.ae_snapshot_from_doc();
        let count = snapshot.layers.len();
        self.ae.apply_project(snapshot);
        log::info!("音声レイヤー {count} 件を After Effects へ反映");
    }

    fn export_wav_apply(&mut self) {
        if !self.mmd_connected {
            log::error!("MMD未接続のため、ミックスWAVを反映できません");
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter(loc::t("arr.audioFilter"), &["wav"])
            .set_file_name("mix.wav")
            .save_file()
        else {
            return;
        };
        match crate::export::export_mix_wav(&path, &self.doc, &self.lib, self.config.main_volume_db)
        {
            Ok(()) => {
                if self.mmd_connected {
                    self.mmd.import_wav(path.clone());
                    log::info!("ミックス WAV を書き出してMMDへ反映: {}", path.display());
                } else if self.ae_connected {
                    self.ae.import_wav(path.clone());
                    log::info!("ミックス WAV を書き出してAfter Effectsへ反映: {}", path.display());
                }
            }
            Err(e) => log::error!("WAV 書き出し失敗: {e:#}"),
        }
    }

    

    fn title(&self) -> String {
        let name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let mark = if self.dirty { "*" } else { "" };
        if name.is_empty() {
            format!("AudioFrame {mark}").trim_end().to_string()
        } else {
            format!("AudioFrame — {name} {mark}").trim_end().to_string()
        }
    }

    

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.focused) || text_input_active(ctx) {
            return;
        }
        let actions = ctx.input_mut(|i| self.keys.consume_actions(i));
        let (redo, save) = ctx.input_mut(|i| {
            (
                Chord::ctrl_shift(egui::Key::Z).consume(i, false),
                Chord::ctrl(egui::Key::S).consume(i, false),
            )
        });
        if actions.contains(&Action::Undo) {
            self.do_undo();
        }
        if redo || actions.contains(&Action::Redo) {
            self.do_redo();
        }
        if save {
            self.file_save();
        }

        if actions.contains(&Action::PlayPause) {
            if self.mmd_connected {
                self.mmd.toggle_playback();
            } else if let Some(e) = &self.engine {
                e.toggle();
            }
        }
        if actions.contains(&Action::Stop) {
            self.apply_arr(ArrOutput {
                actions: vec![ArrAction::SeekStop(0.0)],
                merge_key: None,
            });
        }
        if actions.contains(&Action::PrevMarker) {
            self.seek_marker(false);
        }
        if actions.contains(&Action::NextMarker) {
            self.seek_marker(true);
        }
        if actions.contains(&Action::AddMarker) {
            let frame = self.playhead_frame();
            let n = self.doc.markers.len() + 1;
            self.edit("undo.presetAdd", None, |d| {
                d.markers.push(Marker {
                    name: format!("{}{}", loc::t("preset.defaultName"), n),
                    frame,
                });
            });
        }
        let (backspace, tool_n) = ctx
            .input_mut(|i| {
                (
                    Chord::plain(egui::Key::Backspace).consume(i, false),
                    [
                        egui::Key::Num1,
                        egui::Key::Num2,
                        egui::Key::Num3,
                        egui::Key::Num4,
                        egui::Key::Num5,
                    ]
                    .iter()
                    .position(|k| Chord::plain(*k).consume(i, false)),
                )
            });
        let (ae_prev, ae_next, ae_in, ae_out, ae_move_in, ae_move_out, ae_trim_in, ae_trim_out) = ctx.input_mut(|i| {
            (
                Chord::plain(egui::Key::PageUp).consume(i, true),
                Chord::plain(egui::Key::PageDown).consume(i, true),
                Chord::plain(egui::Key::I).consume(i, false),
                Chord::plain(egui::Key::O).consume(i, false),
                Chord::plain(egui::Key::OpenBracket).consume(i, false),
                Chord::plain(egui::Key::CloseBracket).consume(i, false),
                Chord { alt: true, ..Chord::plain(egui::Key::OpenBracket) }.consume(i, false),
                Chord { alt: true, ..Chord::plain(egui::Key::CloseBracket) }.consume(i, false),
            )
        });
        let cti = self.playhead_position_frame();
        if ae_prev || ae_next {
            let delta = if ae_next { 1 } else { -1 };
            self.apply_arr(ArrOutput {
                actions: vec![ArrAction::SeekStop((cti + delta as f64).max(0.0) / self.doc.fps.max(1) as f64)],
                merge_key: None,
            });
        }
        if let Some((track, clip)) = self.selection.first().copied() {
            let range = self
                .doc
                .tracks
                .get(track)
                .and_then(|t| t.clip(clip))
                .map(|c| (c.in_frame, c.out_frame));
            if let Some((in_frame, out_frame)) = range {
                if ae_in {
                    self.apply_arr(ArrOutput { actions: vec![ArrAction::SeekStop(in_frame.max(0.0) / self.doc.fps.max(1) as f64)], merge_key: None });
                }
                if ae_out {
                    self.apply_arr(ArrOutput { actions: vec![ArrAction::SeekStop(out_frame.max(0.0) / self.doc.fps.max(1) as f64)], merge_key: None });
                }
            }
        }
        if ae_move_in || ae_move_out || ae_trim_in || ae_trim_out {
            let selected = self.selection.clone();
            self.edit("undo.aeTimeline", None, |d| {
                for (track, clip) in selected {
                    let Some(c) = clip_at_mut(d, track, clip) else { continue };
                    if ae_move_in {
                        let delta = cti - c.in_frame;
                        c.start_frame += delta;
                        c.in_frame += delta;
                        c.out_frame += delta;
                        c.end_frame += delta;
                    } else if ae_move_out {
                        let delta = cti - c.out_frame;
                        c.start_frame += delta;
                        c.in_frame += delta;
                        c.out_frame += delta;
                        c.end_frame += delta;
                    } else if ae_trim_in {
                        c.in_frame = cti.min(c.out_frame - 1.0);
                    } else if ae_trim_out {
                        c.out_frame = cti.max(c.in_frame + 1.0);
                        c.end_frame = c.end_frame.max(c.out_frame);
                    }
                }
            });
        }
        if actions.contains(&Action::ToggleSnap) {
            use crate::ui::config::SnapUnit::*;
            self.config.snap_unit = if matches!(self.config.snap_unit, Off) {
                Frame
            } else {
                Off
            };
            self.config.save();
        }
        if actions.contains(&Action::FitAll) {
            let range = self.external_range();
            let composition_range = (range.start >= 0 && range.end > range.start)
                .then_some((range.start, range.end));
            arranger::fit_all(
                &mut self.arr,
                &self.doc,
                self.config.window_size[0],
                composition_range,
            );
        }
        if actions.contains(&Action::ZoomIn) {
            self.arr.px_per_frame = (self.arr.px_per_frame * 1.25).min(40.0);
        }
        if actions.contains(&Action::ZoomOut) {
            self.arr.px_per_frame = (self.arr.px_per_frame / 1.25).max(0.05);
        }
        if let Some(n) = tool_n {
            self.arr.tool = Tool::ALL[n];
        }
        if backspace || actions.contains(&Action::DeleteClip) {
            self.delete_selection();
        }
        if actions.contains(&Action::NudgeLeft) {
            let step = if self.keys.get(Action::NudgeLeft).shift { self.doc.fps.max(1) as i64 } else { 1 };
            self.nudge_selection(-step);
        }
        if actions.contains(&Action::NudgeRight) {
            let step = if self.keys.get(Action::NudgeRight).shift { self.doc.fps.max(1) as i64 } else { 1 };
            self.nudge_selection(step);
        }
        if actions.contains(&Action::Split) {
            self.split_at_playhead();
        }
    }

    
    fn split_at_playhead(&mut self) {
        let sec = self
            .engine
            .as_ref()
            .map(|e| e.position_seconds())
            .unwrap_or(0.0);
        let frame = (sec * self.doc.fps.max(1) as f64).round() as i64;
        let targets: Vec<(usize, Id)> = if self.selection.is_empty() {
            self.doc
                .tracks
                .iter()
                .enumerate()
                .flat_map(|(ti, t)| {
                    t.clips
                        .iter()
                        .filter(|c| c.in_frame < frame as f64 && (frame as f64) < c.out_frame)
                        .map(move |c| (ti, c.id))
                })
                .collect()
        } else {
            self.selection.clone()
        };
        if targets.is_empty() {
            return;
        }
        self.edit("undo.split", None, |d| {
            for (ti, ci) in targets {
                split_clip(d, ti, ci, frame);
            }
        });
    }

    

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar")
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(0.0, 2.0)))
            .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.spacing_mut().button_padding.x = 15.0;
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.menu_button(loc::t("menu.file"), |ui| {
                    menu_popup_style(ui);
                    menu_popup_width(
                        ui,
                        &[
                            loc::t("menu.file.new"),
                            loc::t("menu.file.open"),
                            loc::t("menu.file.save"),
                            loc::t("menu.file.saveas"),
                            loc::t("menu.file.export"),
                            loc::t("menu.file.mmdApply"),
                            loc::t("menu.file.aeApply"),
                        ],
                    );
                    if ui.button(loc::t("menu.file.new")).clicked() {
                        self.file_new();
                        ui.close_menu();
                    }
                    if ui.button(loc::t("menu.file.open")).clicked() {
                        self.file_open();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(loc::t("menu.file.save")).clicked() {
                        self.file_save();
                        ui.close_menu();
                    }
                    if ui.button(loc::t("menu.file.saveas")).clicked() {
                        self.file_save_as();
                        ui.close_menu();
                    }
                    ui.separator();
                    submenu_button(ui, loc::t("menu.file.export"), |ui| {
                        if ui.button(loc::t("menu.file.export.wav")).clicked() {
                            self.export_wav();
                            ui.close_menu();
                        }
                        if ui.button(loc::t("menu.file.export.jsx")).clicked() {
                            self.export_jsx();
                            ui.close_menu();
                        }
                    });
                    submenu_button(ui, loc::t("menu.file.mmdApply"), |ui| {
                        menu_popup_style(ui);
                        menu_popup_width_plain(ui, &[loc::t("menu.file.export.wav_apply")]);
                        if ui
                            .add_enabled(
                                self.mmd_connected,
                                egui::Button::new(loc::t("menu.file.export.wav_apply")),
                            )
                            .clicked()
                        {
                            self.export_wav_apply();
                            ui.close_menu();
                        }
                    });
                    submenu_button(ui, loc::t("menu.file.aeApply"), |ui| {
                        menu_popup_style(ui);
                        menu_popup_width_plain(ui, &[loc::t("menu.file.export.jsx_apply")]);
                        if ui
                            .add_enabled(
                                self.ae_connected && self.config.ae_jsx,
                                egui::Button::new(loc::t("menu.file.export.jsx_apply")),
                            )
                            .clicked()
                        {
                            self.export_jsx_apply();
                            ui.close_menu();
                        }
                    });
                });

                ui.menu_button(loc::t("menu.connect"), |ui| {
                    menu_popup_style(ui);
                    menu_popup_width(
                        ui,
                        &[loc::t("menu.connectMmd"), loc::t("menu.connectAe")],
                    );
                    submenu_button(ui, loc::t("menu.connectMmd"), |ui| {
                        menu_popup_style(ui);
                        let mut labels = vec![loc::t("set.mmd.refresh"), loc::t("set.mmd.none")];
                        labels.extend(self.mmd_procs.iter().map(|p| p.label.clone()));
                        menu_popup_width_plain(ui, &labels);
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(loc::t("set.mmd.refresh")).strong(),
                                )
                                .min_size(egui::vec2(0.0, 28.0)),
                            )
                            .clicked()
                        {
                            self.mmd_procs = self.mmd.list_processes();
                        }
                        let cur = self.mmd.target_pid();
                        if ui
                            .selectable_label(cur == 0, loc::t("set.mmd.none"))
                            .clicked()
                        {
                            self.mmd.detach();
                        }
                        for p in &self.mmd_procs {
                            if ui.selectable_label(cur == p.pid, &p.label).clicked() {
                                self.ae.detach();
                                self.mmd.attach(p.pid);
                                self.mmd.set_frame_stop(self.config.preview_time_mode);
                                self.mmd.set_wav_output(self.config.mmd_wav_output);
                            }
                        }
                    });
                    submenu_button(ui, loc::t("menu.connectAe"), |ui| {
                        menu_popup_style(ui);
                        let mut labels = vec![loc::t("set.ae.refresh"), loc::t("set.ae.none")];
                        labels.extend(self.ae_procs.iter().map(|p| p.label.clone()));
                        menu_popup_width_plain(ui, &labels);
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(loc::t("set.ae.refresh")).strong(),
                                )
                                .min_size(egui::vec2(0.0, 28.0)),
                            )
                            .clicked()
                        {
                            self.ae_procs = self.ae.list_processes();
                        }
                        let cur = self.ae.target_pid();
                        if ui
                            .selectable_label(cur == 0, loc::t("set.ae.none"))
                            .clicked()
                        {
                            self.ae.detach();
                        }
                        for p in &self.ae_procs {
                            if ui.selectable_label(cur == p.pid, &p.label).clicked() {
                                self.mmd.detach();
                                self.ae.attach(p.pid);
                            }
                        }
                    });
                });

                ui.menu_button(loc::t("set.integration"), |ui| {
                    menu_popup_style(ui);
                    ui.add_enabled_ui(self.mmd_connected, |ui| {
                        let mut wav_out = self.config.mmd_wav_output;
                        if ui.checkbox(&mut wav_out, loc::t("set.mmd.wavOutput")).changed() {
                            self.config.mmd_wav_output = wav_out;
                            self.config.save();
                            self.mmd.set_wav_output(wav_out);
                        }
                        let mut ptm = self.config.preview_time_mode;
                        if ui.checkbox(&mut ptm, loc::t("set.previewTimeMode")).changed() {
                            self.config.preview_time_mode = ptm;
                            self.config.save();
                            self.mmd.set_frame_stop(ptm);
                        }
                    });
                    ui.separator();
                    {
                        let ae_ready = self.ae_connected;
                        if ui
                            .add_enabled(ae_ready, egui::Button::new(loc::t("ae.getProject")))
                            .clicked()
                        {
                            self.ae.request_project();
                            ui.close_menu();
                        }
                        let mut jsx = self.config.ae_jsx;
                        if ui
                            .add_enabled(
                                ae_ready,
                                egui::Checkbox::new(&mut jsx, loc::t("set.ae.jsx")),
                            )
                            .changed()
                        {
                            self.config.ae_jsx = jsx;
                            self.config.save();
                        }
                        if let Some((count, duration)) = self
                            .ae_pending_project
                            .as_ref()
                            .map(|p| (p.layers.len(), p.duration_frames))
                        {
                            ui.label(format!(
                                "{}: {} ({}F)",
                                loc::t("ae.pendingProject"),
                                count,
                                duration
                            ));
                            if ui.button(loc::t("ae.importProject")).clicked() {
                                self.import_ae_project();
                                ui.close_menu();
                            }
                            if ui.button(loc::t("ae.discardProject")).clicked() {
                                self.ae_pending_project = None;
                                ui.close_menu();
                            }
                        }
                        if ui
                            .add_enabled(ae_ready, egui::Button::new(loc::t("ae.applyProject")))
                            .clicked()
                        {
                            let snapshot = self.ae_snapshot_from_doc();
                            self.ae.apply_project(snapshot);
                            ui.close_menu();
                        }
                        if self.ae_state.busy {
                            ui.label(loc::t("ae.busy"));
                        }
                        if let Some(error) = &self.ae_state.last_error {
                            ui.label(error);
                        }
                    }
                    if ui
                        .add_enabled(
                            self.mmd_connected,
                            egui::Button::new(loc::t("transport.normalizeSettings")),
                        )
                        .clicked()
                    {
                        self.normalize_playback_settings();
                        ui.close_menu();
                    }
                });

                ui.menu_button(loc::t("menu.edit"), |ui| {
                    menu_popup_style(ui);
                    menu_popup_width(
                        ui,
                        &[loc::t("menu.edit.presets"), loc::t("menu.edit.snap")],
                    );
                    if ui.button(loc::t("menu.edit.presets")).clicked() {
                        self.show_presets = true;
                        ui.close_menu();
                    }
                    submenu_button(ui, loc::t("menu.edit.snap"), |ui| {
                        use crate::ui::config::SnapUnit::*;
                        for u in [Off, Frame, Second, Marker] {
                            if ui
                                .selectable_label(self.config.snap_unit == u, loc::t(snap_key(u)))
                                .clicked()
                            {
                                self.config.snap_unit = u;
                                self.config.save();
                                ui.close_menu();
                            }
                        }
                    });
                });

                ui.menu_button(loc::t("tab.settings"), |ui| {
                    menu_popup_style(ui);
                    menu_popup_width(
                        ui,
                        &[
                            loc::t("set.alwaysOnTop"),
                            loc::t("set.subframeMovement"),
                            loc::t("menu.lang"),
                            loc::t("tab.keys"),
                        ],
                    );
                    let mut always_on_top = self.config.always_on_top;
                    if ui
                        .checkbox(&mut always_on_top, loc::t("set.alwaysOnTop"))
                        .changed()
                    {
                        self.config.always_on_top = always_on_top;
                        apply_window_level(ctx, always_on_top);
                        self.config.save();
                    }
                    let mut subframe_movement = self.config.subframe_movement;
                    if ui
                        .checkbox(&mut subframe_movement, loc::t("set.subframeMovement"))
                        .changed()
                    {
                        self.config.subframe_movement = subframe_movement;
                        self.config.save();
                    }
                    submenu_button(ui, loc::t("menu.lang"), |ui| {
                        let cur = loc::lang();
                        for l in Lang::ALL {
                            if ui
                                .selectable_label(cur == l, crate::ui::fonts::language_text(l))
                                .clicked()
                            {
                                loc::set_lang(l);
                                self.config.lang = l.code().to_string();
                                self.config.save();
                                crate::ui::fonts::apply(ctx, l);
                                ui.close_menu();
                            }
                        }
                    });
                    if ui.button(loc::t("tab.keys")).clicked() {
                        self.show_keys = true;
                        ui.close_menu();
                    }
                });

            });
        });
    }

    fn transport_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("transport_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let fps = self.doc.fps.max(1) as f64;
                let frame_position = self.external_frame();
                let frame = frame_position.round() as i64;
                ui.label(
                    egui::RichText::new(fmt_tc(frame, self.doc.fps))
                        .monospace()
                        .color(if ui.visuals().dark_mode {
                            egui::Color32::from_rgb(64, 156, 255)
                        } else {
                            egui::Color32::from_rgb(0, 102, 204)
                        }),
                );
                ui.separator();

                let connected = self.mmd_connected;
                let mut whole_frame = frame;
                if ui
                    .add(Integer::new(&mut whole_frame).speed(1.0).range(0..=i64::MAX))
                    .changed()
                {
                    self.apply_arr(ArrOutput {
                        actions: vec![ArrAction::SeekStop(whole_frame as f64 / fps)],
                        merge_key: None,
                    });
                }
                ui.separator();
                if icons::button(ui, Icon::SkipPrevious, 16.0).clicked() {
                    self.apply_arr(ArrOutput {
                        actions: vec![ArrAction::SeekStop(0.0)],
                        merge_key: None,
                    });
                }
                if icons::button(ui, Icon::FastRewind, 16.0).clicked() {
                    self.seek_marker(false);
                }
                if icons::button(ui, Icon::FastForward, 16.0).clicked() {
                    self.seek_marker(true);
                }
                if icons::button(ui, Icon::SkipNext, 16.0).clicked() {
                    self.apply_arr(ArrOutput {
                        actions: vec![ArrAction::SeekStop(
                            self.doc.content_end_frame().max(0) as f64 / fps,
                        )],
                        merge_key: None,
                    });
                }
                ui.separator();
                let mut range = self.external_range();
                let range_start_changed = ui.add(
                    Integer::new(&mut range.start)
                        .speed(1.0)
                        .range(0..=i32::MAX as i64),
                );
                ui.label("\u{ff5e}");
                let range_end_changed = ui.add(
                    Integer::new(&mut range.end)
                        .speed(1.0)
                        .range(-1..=i32::MAX as i64),
                );
                if range_start_changed.changed() || range_end_changed.changed() {
                    *self.external_range_mut() = range;
                }
                if ui
                    .add_enabled(connected, egui::Button::new(loc::t("transport.getRange")))
                    .clicked()
                {
                    if self.mmd_connected {
                        self.mmd.request_playback_range();
                    }
                }
                let range = self.external_range();
                if ui
                    .add_enabled(
                        connected && range.is_valid(),
                        egui::Button::new(loc::t("transport.applyRange")),
                    )
                    .clicked()
                {
                    if self.mmd_connected {
                        self.mmd.set_playback_range(range);
                    }
                }
                ui.separator();
                if self.mmd_connected {
                    let mut looping = self.mmd_state.looping.unwrap_or(false);
                    if ui
                        .add_enabled(
                            self.mmd_state.looping.is_some(),
                            egui::Checkbox::new(&mut looping, loc::t("transport.loop")),
                        )
                        .changed()
                    {
                        self.mmd.set_looping(looping);
                    }
                } else {
                    ui.checkbox(&mut self.loop_enabled, loc::t("transport.loop"));
                }
            });
        });
    }

    fn playback_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("playback_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(loc::t("set.previewOffset"));
                let mut off = self.config.preview_offset_ms;
                if ui
                    .add(Integer::new(&mut off).suffix(" ms").range(-500..=500))
                    .changed()
                {
                    self.config.preview_offset_ms = off;
                    self.config.save();
                }
            });
        });
        if let Some(engine) = &self.engine {
            engine.set_loop_range(self.preview_loop_range());
        }
    }

    fn preview_loop_range(&self) -> Option<LoopRange> {
        if self.mmd_connected || !self.loop_enabled || !self.mmd_range.is_valid() {
            return None;
        }
        let end = if self.mmd_range.end == -1 {
            self.doc.content_end_frame()
        } else {
            self.mmd_range.end
        };
        let fps = self.doc.fps.max(1) as f64;
        LoopRange::new(self.mmd_range.start as f64 / fps, end as f64 / fps)
    }

    fn central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0.0))
            .show(ctx, |ui| {
                let master_before = self.config.main_volume_db;
                let ph = self
                    .engine
                    .as_ref()
                    .map(|e| e.position_seconds())
                    .unwrap_or(0.0);
                let alt = ui.input(|i| i.modifiers.alt);
                let snap = self.config.snap_unit;
                let mmd_wav_available = self.external_wav_path().is_some();
                let engine = self.engine.as_ref();
                let master_peak = engine
                    .map(|e| e.main_peak_stereo())
                    .unwrap_or((0.0, 0.0));
                let out = arranger::show(
                    ui,
                    &mut self.arr,
                    &self.doc,
                    &self.lib,
                    &mut self.selection,
                    &mut self.config.main_volume_db,
                    ph,
                    snap,
                    alt,
                    self.config.subframe_movement,
                    mmd_wav_available,
                    master_peak,
                    |i| engine.map(|e| e.track_peak(i)).unwrap_or(0.0),
                );
                self.apply_arr(out);
                if (self.config.main_volume_db - master_before).abs() > f32::EPSILON {
                    self.rebuild_snapshot();
                    self.config.save();
                }
            });
    }

    
    fn aux_windows(&mut self, ctx: &egui::Context) {
        if self.show_presets {
            let mut close = false;
            let pos = *self.presets_pos.get_or_insert_with(|| main_window_origin(ctx));
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("marker_presets"),
                egui::ViewportBuilder::default()
                    .with_title(loc::t("menu.edit.presets"))
                    .with_position(pos)
                    .with_inner_size([presets_window_width(ctx), 260.0])
                    .with_min_inner_size([presets_window_width(ctx), 140.0])
                    .with_max_inner_size([presets_window_width(ctx), 2000.0])
                    .with_resizable(true)
                    .with_window_level(if self.config.always_on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    }),
                |child_ctx, class| {
                    if matches!(class, egui::ViewportClass::Embedded) {
                        let mut open = true;
                        egui::Window::new(loc::t("menu.edit.presets"))
                            .open(&mut open)
                            .resizable(true)
                            .default_width(360.0)
                            .show(child_ctx, |ui| self.presets_panel(ui));
                        close = !open;
                    } else {
                        close = child_ctx.input(|i| i.viewport().close_requested());
                        egui::CentralPanel::default().show(child_ctx, |ui| {
                            self.presets_panel(ui);
                        });
                    }
                },
            );
            if close {
                self.show_presets = false;
                self.presets_pos = None;
            }
        }

        if self.show_keys {
            let mut close = false;
            let pos = *self.keys_pos.get_or_insert_with(|| main_window_origin(ctx));
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("keyboard_shortcuts"),
                egui::ViewportBuilder::default()
                    .with_title(loc::t("tab.keys"))
                    .with_position(pos)
                    .with_inner_size([keys_window_width(ctx), 360.0])
                    .with_min_inner_size([keys_window_width(ctx), 160.0])
                    .with_max_inner_size([keys_window_width(ctx), 2000.0])
                    .with_resizable(true)
                    .with_window_level(if self.config.always_on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    }),
                |child_ctx, class| {
                    if matches!(class, egui::ViewportClass::Embedded) {
                        let mut open = true;
                        egui::Window::new(loc::t("tab.keys"))
                            .open(&mut open)
                            .resizable(true)
                            .default_width(420.0)
                            .show(child_ctx, |ui| self.keys_panel(ui));
                        close = !open;
                    } else {
                        close = child_ctx.input(|i| i.viewport().close_requested());
                        if self.capturing.is_some() {
                            self.capture_key(child_ctx);
                        }
                        egui::CentralPanel::default().show(child_ctx, |ui| {
                            self.keys_panel(ui);
                        });
                    }
                },
            );
            if close {
                self.show_keys = false;
                self.capturing = None;
                self.keys_pos = None;
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_mmd();
        self.poll_ae();
        if !self.capture_key(ctx) {
            self.handle_shortcuts(ctx);
        }

        let title = self.title();
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        self.menu_bar(ctx);
        self.transport_bar(ctx);
        self.playback_bar(ctx);
        self.update_hovered_file_preview(ctx);
        self.central(ctx);
        self.aux_windows(ctx);

        self.handle_dropped_files(ctx);

        // Sources decoded in the background become audible and drawable as they land.
        if self.lib.poll() {
            self.rebuild_snapshot();
        }
        if self.lib.has_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }


        if self.mmd_connected || self.external_playing() || self.engine.as_ref().is_some_and(|e| e.is_playing()) {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.config.save();
        self.keys.save();
    }
}


fn clip_at_mut(d: &mut Project, track: usize, clip: Id) -> Option<&mut Clip> {
    d.tracks
        .get_mut(track)
        .and_then(|t| t.clips.iter_mut().find(|c| c.id == clip))
}

fn audio_length_frames(seconds: f64, fps: u32) -> Option<i64> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    Some((seconds * fps.max(1) as f64).ceil().max(1.0) as i64)
}

#[cfg(windows)]
fn system_cursor_client_pos(ctx: &egui::Context) -> Option<egui::Pos2> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowThreadProcessId, WindowFromPoint,
    };

    unsafe {
        let mut point = POINT::default();
        GetCursorPos(&mut point).ok()?;
        let window = WindowFromPoint(point);
        if window.0 == 0 {
            return None;
        }
        let mut process_id = 0_u32;
        GetWindowThreadProcessId(window, Some(&mut process_id));
        if process_id != std::process::id() || !ScreenToClient(window, &mut point).as_bool() {
            return None;
        }
        let pixels_per_point = ctx.pixels_per_point().max(f32::EPSILON);
        Some(egui::pos2(
            point.x as f32 / pixels_per_point,
            point.y as f32 / pixels_per_point,
        ))
    }
}

#[cfg(not(windows))]
fn system_cursor_client_pos(ctx: &egui::Context) -> Option<egui::Pos2> {
    ctx.input(|input| input.pointer.hover_pos())
}


fn split_clip(d: &mut Project, track: usize, clip: Id, at: i64) {
    let Some(t) = d.tracks.get_mut(track) else {
        return;
    };
    let Some(idx) = t.clips.iter().position(|c| c.id == clip) else {
        return;
    };
    let orig = t.clips[idx].clone();
    let at = at as f64;
    if at <= orig.in_frame || at >= orig.out_frame {
        return;
    }
    let mut left = orig.clone();
    left.out_frame = at;
    left.end_frame = at;
    left.fade_out_frames = 0;

    let mut right = orig.clone();
    right.id = Id::new();
    right.in_frame = at;
    right.fade_in_frames = 0;
    if right.end_frame < right.out_frame {
        right.end_frame = right.out_frame;
    }
    
    t.clips[idx] = left;
    t.clips.insert(idx + 1, right);
}


fn snap_key(u: crate::ui::config::SnapUnit) -> &'static str {
    use crate::ui::config::SnapUnit::*;
    match u {
        Off => "arr.snap.off",
        Frame => "arr.snap.frame",
        Second => "arr.snap.second",
        Marker => "arr.snap.marker",
    }
}

#[cfg(test)]
fn master_volume_control(ui: &mut egui::Ui, db: &mut f32) -> egui::Response {
    ui.add(
        crate::ui::numeric::Decimal::new(db)
            .range(crate::ui::arranger::MASTER_VOLUME_DB_FLOOR..=12.0)
            .speed(0.1)
            .fixed_decimals(1)
            .suffix(" dB"),
    )
}

fn fmt_tc(frame: i64, fps: u32) -> String {
    let frame = frame.max(0) as u64;
    let fps = fps.max(1) as u64;
    let total_s = frame / fps;
    let subframe = frame % fps;
    let sec = total_s % 60;
    let min = (total_s / 60) % 60;
    let hr = total_s / 3600;
    format!("{hr}:{min:02}:{sec:02}:{subframe:02}")
}

fn apply_dark_ui(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    let text = egui::Color32::from_gray(232);
    let panel = egui::Color32::from_gray(43);
    let panel_alt = egui::Color32::from_gray(48);
    visuals.override_text_color = Some(text);
    visuals.faint_bg_color = panel_alt;
    visuals.extreme_bg_color = egui::Color32::from_gray(31);
    visuals.code_bg_color = egui::Color32::from_gray(54);
    visuals.window_fill = panel;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_gray(82));
    visuals.panel_fill = panel;
    visuals.widgets.noninteractive.weak_bg_fill = panel;
    visuals.widgets.noninteractive.bg_fill = panel;
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(82));
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, text);
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_gray(61);
    visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(61);
    visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(224));
    visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_gray(76);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(76);
    visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(166));
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, egui::Color32::WHITE);
    visuals.widgets.active.weak_bg_fill = egui::Color32::from_gray(84);
    visuals.widgets.active.bg_fill = egui::Color32::from_gray(84);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(2.0_f32, egui::Color32::WHITE);
    visuals.widgets.open.weak_bg_fill = egui::Color32::from_gray(70);
    visuals.widgets.open.bg_fill = egui::Color32::from_gray(48);
    visuals.widgets.open.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(92));
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, text);
    visuals.selection.bg_fill = egui::Color32::from_rgb(55, 105, 145);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    ctx.set_visuals(visuals);
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark));
}

fn apply_window_level(ctx: &egui::Context, always_on_top: bool) {
    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if always_on_top {
        egui::WindowLevel::AlwaysOnTop
    } else {
        egui::WindowLevel::Normal
    }));
}

const MENU_ITEM_PADDING_X: f32 = 6.0;

const PRESET_NAME_W: f32 = 160.0;
const PRESET_FRAME_SAMPLE: &str = "-999999 f";
const PRESET_ICON: f32 = 14.0;

fn preset_frame_field_width(ui: &egui::Ui) -> f32 {
    text_width(ui, PRESET_FRAME_SAMPLE, egui::TextStyle::Body) + MENU_ITEM_PADDING_X * 3.0
}

/// The marker window is exactly as wide as one row, so no space is left over.
fn presets_window_width(ctx: &egui::Context) -> f32 {
    let style = ctx.style();
    let font = egui::TextStyle::Body.resolve(&style);
    let frame_text = ctx.fonts(|fonts| {
        fonts
            .layout_no_wrap(PRESET_FRAME_SAMPLE.to_owned(), font, egui::Color32::WHITE)
            .rect
            .width()
    });
    let frame_field = frame_text + MENU_ITEM_PADDING_X * 3.0;
    let buttons = (PRESET_ICON + 6.0) * 2.0;
    let spacing = style.spacing.item_spacing.x * 3.0;
    let margins = style.spacing.window_margin.left + style.spacing.window_margin.right;
    PRESET_NAME_W + frame_field + buttons + spacing + margins + style.spacing.scroll.bar_width + 8.0
}

/// The shortcut window keeps a fixed width that fits the longest description
/// in the active language, so no column is squeezed and no space is wasted.
fn keys_window_width(ctx: &egui::Context) -> f32 {
    let style = ctx.style();
    let width_of = |text: String, text_style: egui::TextStyle| {
        let font = text_style.resolve(&style);
        ctx.fonts(|fonts| {
            fonts
                .layout_no_wrap(text, font, egui::Color32::WHITE)
                .rect
                .width()
        })
    };
    let desc = Action::ALL
        .iter()
        .map(|a| width_of(loc::t(a.loc_key()), egui::TextStyle::Body))
        .fold(0.0_f32, f32::max);
    let shortcut = Action::ALL
        .iter()
        .map(|a| width_of(KeyBindings::default().get(*a).to_display(), egui::TextStyle::Monospace))
        .fold(
            width_of(loc::t("keys.waiting"), egui::TextStyle::Body),
            f32::max,
        );
    let button = width_of(loc::t("keys.change"), egui::TextStyle::Button) + MENU_ITEM_PADDING_X * 4.0;
    let spacing = style.spacing.item_spacing.x;
    (desc + shortcut + button + spacing * 3.0 + 32.0).clamp(360.0, 900.0)
}

fn text_width(ui: &egui::Ui, text: &str, style: egui::TextStyle) -> f32 {
    let font = style.resolve(ui.style());
    ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE)
            .rect
            .width()
    })
}
const SUBMENU_ARROW_COL: f32 = 18.0;

fn menu_popup_style(ui: &mut egui::Ui) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
    ui.spacing_mut().button_padding.x = MENU_ITEM_PADDING_X;
}

/// Width for a popup whose rows carry no submenu arrow.
fn menu_popup_width_plain(ui: &mut egui::Ui, labels: &[String]) {
    let width = menu_labels_width(ui, labels);
    ui.set_min_width(width + MENU_ITEM_PADDING_X * 2.0);
}

/// Widen the popup so that the widest label is followed by a fixed arrow column.
fn menu_popup_width(ui: &mut egui::Ui, labels: &[String]) {
    let width = menu_labels_width(ui, labels);
    ui.set_min_width(width + MENU_ITEM_PADDING_X * 2.0 + SUBMENU_ARROW_COL);
}

fn menu_labels_width(ui: &egui::Ui, labels: &[String]) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    labels
        .iter()
        .map(|label| {
            ui.fonts(|fonts| {
                fonts
                    .layout_no_wrap(label.clone(), font.clone(), egui::Color32::WHITE)
                    .rect
                    .width()
            })
        })
        .fold(0.0_f32, f32::max)
}

fn main_window_origin(ctx: &egui::Context) -> egui::Pos2 {
    ctx.input(|i| i.viewport().outer_rect)
        .map(|rect| rect.min)
        .unwrap_or(egui::pos2(64.0, 64.0))
}

fn submenu_button<R>(
    ui: &mut egui::Ui,
    title: impl Into<egui::WidgetText>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let result = ui.menu_button(title, add_contents);
    let rect = result.response.rect;
    let column = egui::Rect::from_min_max(
        egui::pos2(rect.right() - SUBMENU_ARROW_COL, rect.top() + 1.0),
        egui::pos2(rect.right() - 1.0, rect.bottom() - 1.0),
    );
    // egui draws its own "⏵" in this column; cover it and use src/img/arrow_forward.svg.
    let cover = if result.response.hovered() {
        ui.style().interact(&result.response).weak_bg_fill
    } else {
        ui.visuals().window_fill
    };
    ui.painter().rect_filled(column, 0.0, cover);
    let side = column.width().min(column.height()) * 0.55;
    icons::paint_at(
        ui,
        Icon::ArrowForward,
        egui::Rect::from_center_size(column.center(), egui::vec2(side, side)),
    );
    result
}


fn apply_text_scale(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.interact_size.y = UI_ROW_HEIGHT;
    for (text_style, font_id) in style.text_styles.iter_mut() {
        if matches!(
            text_style,
            egui::TextStyle::Body | egui::TextStyle::Button | egui::TextStyle::Monospace
        ) {
            font_id.size = 12.0;
        }
    }
    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App {
            doc: Project::default(),
            undo: UndoStack::default(),
            config: AppConfig::default(),
            keys: KeyBindings::default(),
            project_path: None,
            dirty: false,
            show_presets: false,
            show_keys: false,
            presets_pos: None,
            keys_pos: None,
            last_title: String::new(),
            lib: SourceLibrary::default(),
            engine: None,
            arr: ArrangerState::default(),
            selection: Vec::new(),
            hovered_audio_path: None,
            mmd_connected: false,
            mmd: MmdSession::disconnected(),
            mmd_state: MmdState::default(),
            mmd_procs: Vec::new(),
            mmd_range: PlaybackRange { start: 0, end: -1 },
            ae_connected: false,
            ae: AeSession::disconnected(),
            ae_state: AeState::default(),
            ae_procs: Vec::new(),
            ae_project: None,
            ae_pending_project: None,
            loop_enabled: false,
            capturing: None,
        }
    }

    fn text_shapes(shape: &egui::Shape, texts: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Text(text) => texts.push((
                text.galley.text().to_owned(),
                text.galley.rect.translate(text.pos.to_vec2()),
            )),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    text_shapes(shape, texts);
                }
            }
            _ => {}
        }
    }

    fn draw(app: &mut App, ctx: &egui::Context, size: egui::Vec2, events: Vec<egui::Event>) -> Vec<(String, egui::Rect)> {
        let output = ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: true,
            events,
            ..Default::default()
        }, |ctx| {
            app.menu_bar(ctx);
            app.transport_bar(ctx);
            app.playback_bar(ctx);
            egui::CentralPanel::default().show(ctx, |_ui| {});
        });
        let mut texts = Vec::new();
        for shape in output.shapes {
            text_shapes(&shape.shape, &mut texts);
        }
        texts
    }

    #[test]
    fn timecode_uses_hours_minutes_seconds_and_project_frames() {
        for (frame, fps, expected) in [
            (0, 30, "0:00:00:00"),
            (29, 30, "0:00:00:29"),
            (30, 30, "0:00:01:00"),
            (900, 30, "0:00:30:00"),
            (108000, 30, "1:00:00:00"),
            (6638, 60, "0:01:50:38"),
            (-1, 30, "0:00:00:00"),
            (1, 0, "0:00:01:00"),
            (119, 120, "0:00:00:119"),
        ] {
            assert_eq!(fmt_tc(frame, fps), expected);
        }
    }

    #[test]
    fn preview_loop_uses_selected_range_or_content_end_only_when_local() {
        let mut app = app();
        app.mmd_range = PlaybackRange { start: 30, end: 90 };
        assert_eq!(app.preview_loop_range(), None);
        app.loop_enabled = true;
        assert_eq!(app.preview_loop_range(), LoopRange::new(1.0, 3.0));
        app.mmd_connected = true;
        assert_eq!(app.preview_loop_range(), None);
        app.mmd_connected = false;
        app.mmd_range.end = -1;
        assert_eq!(app.preview_loop_range(), None);
        let mut track = Track::new("test");
        track.clips.push(Clip::new(PathBuf::from("test.wav"), 0, 120));
        app.doc.tracks.push(track);
        assert_eq!(app.preview_loop_range(), LoopRange::new(1.0, 4.0));
        app.mmd_range.end = 29;
        assert_eq!(app.preview_loop_range(), None);
    }

    #[test]
    fn bars_keep_requested_order_at_minimum_and_initial_window_sizes() {
        loc::init();
        for size in [egui::vec2(600.0, 250.0), egui::vec2(800.0, 400.0)] {
            for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
                let mut app = app();
                app.mmd_range = PlaybackRange { start: 12, end: 900 };
                let ctx = egui::Context::default();
                ctx.set_visuals(visuals);
                apply_text_scale(&ctx);
                draw(&mut app, &ctx, size, vec![]);
                let texts = draw(&mut app, &ctx, size, vec![]);
                let mut right = 0.0;
                for label in ["0:00:00:00", "0", "12", "\u{ff5e}", "900", &loc::t("transport.getRange"), &loc::t("transport.applyRange"), &loc::t("transport.loop")] {
                    let (_, rect) = texts.iter().find(|(text, rect)| text == label && rect.top() < 80.0).unwrap_or_else(|| panic!("Missing {label}: {texts:?}"));
                    assert!(rect.left() >= right, "Incorrect order: {label}");
                    assert!(rect.right() <= size.x, "Overflow: {label}");
                    right = rect.right();
                }
                let mut right = 0.0;
                for label in [&loc::t("set.previewOffset"), "0 ms"] {
                    let (_, rect) = texts.iter().find(|(text, _)| text == label).unwrap();
                    assert!(rect.top() > size.y - 40.0);
                    assert!(rect.left() >= right);
                    assert!(rect.right() <= size.x);
                    right = rect.right();
                }
                assert_eq!(app.config.window_size, [800.0, 400.0]);
            }
        }
    }

    #[test]
    fn master_volume_db_edit_updates_mix_gain_and_preserves_other_settings() {
        for (text, gain) in [("-60.0", 0.0_f32), ("-40.0", 0.01), ("-6.0", 0.5011872), ("0.0", 1.0), ("6.0", 1.9952623), ("10.0", 3.1622777), ("12.0", 3.9810717), ("20.0", 3.9810717), ("abc", 1.0)] {
            let ctx = egui::Context::default();
            let mut config = AppConfig::default();
            config.preview_offset_ms = 120;
            config.mmd_wav_output = true;
            let mut rect = egui::Rect::NOTHING;
            let mut id = egui::Id::NULL;
            let mut frame = |events| {
                let _ = ctx.run(egui::RawInput { events, focused: true, ..Default::default() }, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = master_volume_control(ui, &mut config.main_volume_db);
                        rect = response.rect;
                        id = response.id;
                    });
                });
                (rect, id)
            };
            let (rect, id) = frame(vec![]);
            for pressed in [true, false] {
                frame(vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton { pos: rect.center(), pressed, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE },
                ]);
            }
            frame(vec![]);
            assert!(ctx.memory(|memory| memory.has_focus(id)));
            frame(vec![egui::Event::Text(text.into())]);
            frame(vec![egui::Event::Key { key: egui::Key::Enter, physical_key: Some(egui::Key::Enter), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
            let restored: AppConfig = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
            let snapshot = MixSnapshot::build(&Project::default(), &SourceLibrary::default(), restored.main_volume_db);
            assert!((snapshot.main_gain - gain).abs() < 0.0001, "{text}: {}", snapshot.main_gain);
            assert_eq!(restored.preview_offset_ms, 120);
            assert!(restored.mmd_wav_output);
            assert_eq!(restored.window_size, [800.0, 400.0]);
        }
    }

    #[test]
    fn fade_actions_update_clip_frames_with_length_limits() {
        let mut app = app();
        let mut track = Track::new("fade");
        let clip = Clip::new(PathBuf::from("fade.wav"), 0, 30);
        let clip_id = clip.id;
        track.clips.push(clip);
        app.doc.tracks.push(track);
        app.apply_arr(ArrOutput { actions: vec![ArrAction::FadeIn { track: 0, clip: clip_id, frames: 12 }, ArrAction::FadeOut { track: 0, clip: clip_id, frames: 50 }], merge_key: None });
        let result = &app.doc.tracks[0].clips[0];
        assert_eq!(result.fade_in_frames, 12);
        assert_eq!(result.fade_out_frames, 29);
        app.undo.undo(&mut app.doc);
        assert_eq!(app.doc.tracks[0].clips[0].fade_in_frames, 12);
        assert_eq!(app.doc.tracks[0].clips[0].fade_out_frames, 0);
    }

    #[test]
    fn rename_track_action_updates_name_and_ignores_empty_name() {
        let mut app = app();
        app.doc.tracks.push(Track::new("before"));
        app.apply_arr(ArrOutput {
            actions: vec![ArrAction::RenameTrack {
                track: 0,
                name: "after".into(),
            }],
            merge_key: None,
        });
        assert_eq!(app.doc.tracks[0].name, "after");
        app.apply_arr(ArrOutput {
            actions: vec![ArrAction::RenameTrack {
                track: 0,
                name: "  ".into(),
            }],
            merge_key: None,
        });
        assert_eq!(app.doc.tracks[0].name, "after");
    }

    #[test]
    fn add_source_path_decodes_wav_and_creates_track() {
        let path = std::env::temp_dir().join("audioframe_add_source_test.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..44_100 {
            writer.write_sample(0_i16).unwrap();
            writer.write_sample(0_i16).unwrap();
        }
        writer.finalize().unwrap();
        let mut loaded = app();
        loaded.add_source_path(path.clone(), 30);
        assert_eq!(loaded.doc.tracks.len(), 1);
        assert_eq!(loaded.doc.tracks[0].clips[0].start_frame, 30.0);
        assert!(loaded.doc.tracks[0].clips[0].end_frame > 30.0);

        let mut positioned = app();
        positioned.doc.tracks.push(Track::new("first"));
        positioned.doc.tracks.push(Track::new("last"));
        assert!(positioned.add_source_path_at(path.clone(), 45, 1));
        assert_eq!(positioned.doc.tracks.len(), 3);
        assert_eq!(positioned.doc.tracks[0].name, "first");
        assert_eq!(positioned.doc.tracks[1].clips[0].start_frame, 45.0);
        assert_eq!(positioned.doc.tracks[2].name, "last");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn audio_length_frames_is_available_without_waveform_samples() {
        assert_eq!(audio_length_frames(1.25, 30), Some(38));
        assert_eq!(audio_length_frames(0.0, 30), None);
        assert_eq!(audio_length_frames(f64::NAN, 30), None);
    }

    #[test]
    fn hovered_audio_reads_length_before_waveform_decode() {
        let path = std::env::temp_dir().join("audioframe_hover_length_test.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..44_100 {
            writer.write_sample(0_i16).unwrap();
        }
        writer.finalize().unwrap();

        let mut app = app();
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                hovered_files: vec![egui::HoveredFile {
                    path: Some(path.clone()),
                    ..Default::default()
                }],
                ..Default::default()
            },
            |ctx| app.update_hovered_file_preview(ctx),
        );
        assert_eq!(app.arr.drop_preview_frames, Some(30));
        assert!(!app.lib.has_pending());

        let _ = ctx.run(Default::default(), |ctx| app.update_hovered_file_preview(ctx));
        assert_eq!(app.arr.drop_preview_frames, None);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn normalize_playback_settings_updates_local_range() {
        let mut app = app();
        app.mmd_range = PlaybackRange { start: 30, end: 900 };
        app.normalize_playback_settings();
        assert_eq!(app.mmd_range, PlaybackRange { start: 0, end: -1 });
    }

    #[test]
    fn ae_project_import_rebuilds_audio_layers_without_live_sync() {
        let mut app = app();
        app.ae_pending_project = Some(AeProjectSnapshot {
            fps: 30.0,
            duration_frames: 900,
            layers: vec![AeLayerSnapshot {
                key: "AudioFrame:1".into(),
                name: "voice".into(),
                source: PathBuf::from("voice.wav"),
                start_frame: -15,
                in_frame: -15,
                out_frame: 120,
                gain_db: -3.5,
                muted: true,
                fade_in_frames: 0,
                fade_out_frames: 0,
            }],
        });
        app.import_ae_project();
        assert_eq!(app.doc.fps, 30);
        assert_eq!(app.doc.tracks.len(), 1);
        let clip = &app.doc.tracks[0].clips[0];
        assert_eq!(clip.start_frame, -15.0);
        assert_eq!(clip.out_frame, 120.0);
        assert_eq!(clip.gain_db, -3.5);
        assert!(clip.muted);
    }

    #[test]
    fn bottom_loop_checkbox_switches_local_playback_on_and_off() {
        loc::init();
        let mut app = app();
        let ctx = egui::Context::default();
        let size = egui::vec2(800.0, 400.0);
        draw(&mut app, &ctx, size, vec![]);
        let texts = draw(&mut app, &ctx, size, vec![]);
        let pos = texts.iter().find(|(text, _)| text == &loc::t("transport.loop")).unwrap().1.center();
        for expected in [true, false] {
            for pressed in [true, false] {
                draw(&mut app, &ctx, size, vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton { pos, pressed, button: egui::PointerButton::Primary, modifiers: egui::Modifiers::NONE },
                ]);
            }
            assert_eq!(app.loop_enabled, expected);
        }
    }
}
