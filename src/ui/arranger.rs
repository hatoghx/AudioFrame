use crate::audio::SourceLibrary;
use crate::model::{Clip, Id, Project};
use crate::ui::config::SnapUnit;
use crate::ui::numeric::{Decimal, Integer};
use eframe::egui::{self, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, Vec2};

const HEADER_W: f32 = 148.0;
const MASTER_W: f32 = 60.0;
const RULER_H: f32 = 24.0;
const TRACK_GAP: f32 = 4.0;
const MIN_PPF: f32 = 0.05;
const MAX_PPF: f32 = 40.0;
const TRIM_GRAB_PX: f32 = 5.0;
const FADE_GRAB_PX: f32 = 12.0;
const GAIN_BOX: f32 = 11.0;
const AUDIO_LAYER_INSET: f32 = 2.0;
const LEVEL_METER_W: f32 = 8.0;
const LEVEL_METER_GAP: f32 = 4.0;
const MASTER_METER_W: f32 = 18.0;
const MASTER_PART_GAP: f32 = 5.0;
const MIN_LANE_H: f32 = 48.0;
const VOLUME_DB_MIN: f32 = -60.0;
pub const MASTER_VOLUME_DB_FLOOR: f32 = -66.0;
const VOLUME_DB_MAX: f32 = 12.0;
const RULER_TARGET_PX: f64 = 48.0;
const RULER_MIN_GRID_PX: f64 = 8.0;
const MASTER_TRACK_GAP: f32 = 4.0;
const MASTER_LABEL_H: f32 = 11.0;
const RULER_MINOR_H: f32 = 12.0;

const MASTER_EDGE_MARGIN: f32 = 5.0;

const MASTER_TICK_INK_HALF: f32 = 2.5;

const MASTER_LABEL_INK_DESCENT: f32 = 2.2;
const TRACK_TIMELINE_GAP: f32 = 2.0;
const TIMELINE_EDGE_GAP: f32 = 2.0;
const FIELD_SHRINK: f32 = 2.0;
const TRACK_ROW_GAP: f32 = 3.0;
const PAGE_MARGIN: f32 = 0.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Split,
    Erase,
    Mute,
    Zoom,
}

impl Tool {
    pub const ALL: [Tool; 5] = [
        Tool::Select,
        Tool::Split,
        Tool::Erase,
        Tool::Mute,
        Tool::Zoom,
    ];
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TimelineEditUnit {
    Frame,
    Millisecond,
}

pub struct ArrangerState {
    pub px_per_frame: f32,
    pub view_start_frame: f32,
    pub scroll_y: f32,
    pub tool: Tool,
    pub drop_frame: Option<i64>,
    pub last_drop_frame: Option<i64>,
    pub drop_preview_frames: Option<i64>,
    pub drop_preview_name: Option<String>,
    pub external_drop_pos: Option<Pos2>,
    pub drop_track_index: Option<usize>,
    initial_view_pending: bool,
    drag: Option<Drag>,

    menu_target: Option<MenuTarget>,
    menu_pos: Pos2,
    menu_name: String,
    menu_open: bool,
    menu_time_unit: TimelineEditUnit,
}

impl Default for ArrangerState {
    fn default() -> Self {
        Self {
            px_per_frame: 6.0,
            view_start_frame: 0.0,
            scroll_y: 0.0,
            tool: Tool::Select,
            drop_frame: None,
            last_drop_frame: None,
            drop_preview_frames: None,
            drop_preview_name: None,
            external_drop_pos: None,
            drop_track_index: None,
            initial_view_pending: true,
            drag: None,
            menu_target: None,
            menu_pos: Pos2::ZERO,
            menu_name: String::new(),
            menu_open: false,
            menu_time_unit: TimelineEditUnit::Frame,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum DragKind {
    Body,
    TrimIn,
    TrimOut,
    FadeIn,
    FadeOut,
    Ruler,
    Marquee,

    ReorderTrack,
}

struct Drag {
    kind: DragKind,
    track: usize,
    clip: Id,

    grab_frame: f64,
    grab_pos: Pos2,
    orig: Clip,

    group: Vec<(usize, Id, f64)>,
}

pub enum ArrAction {
    Seek(f64),

    SeekStop(f64),

    MoveClips(Vec<(usize, Id, f64)>),
    TrimIn {
        track: usize,
        clip: Id,
        new_in: f64,
    },
    TrimOut {
        track: usize,
        clip: Id,
        new_out: f64,
    },
    FadeIn {
        track: usize,
        clip: Id,
        frames: i64,
    },
    FadeOut {
        track: usize,
        clip: Id,
        frames: i64,
    },
    Split {
        track: usize,
        clip: Id,
        at_frame: i64,
    },

    Delete {
        track: usize,
    },

    Duplicate {
        track: usize,
    },
    ToggleMute {
        track: usize,
        clip: Id,
    },
    ToggleTrackMute {
        track: usize,
    },
    ToggleTrackSolo {
        track: usize,
    },
    RenameTrack {
        track: usize,
        name: String,
    },
    SetTrackVolume {
        track: usize,
        db: f32,
    },

    ReorderTrack {
        from: usize,
        to: usize,
    },
    AddSourceDialog {
        start_frame: i64,
    },
    AddSourceFromMmd {
        start_frame: i64,
    },
}

pub struct ArrOutput {
    pub actions: Vec<ArrAction>,

    pub merge_key: Option<u64>,
}

impl ArrOutput {
    fn new() -> Self {
        Self {
            actions: Vec::new(),
            merge_key: None,
        }
    }
}

struct Palette {
    bg: Color32,
    header: Color32,
    ruler: Color32,
    ruler_minor: Color32,
    ruler_major: Color32,
    ruler_zero: Color32,
    grid_strong: Color32,
    text: Color32,
    text_dim: Color32,
    clip_fill: Color32,
    clip_sel: Color32,
    clip_border: Color32,
    clip_muted: Color32,
    wave: Color32,
    playhead: Color32,
    marquee: Color32,
    meter_bg: Color32,
    header_hover: Color32,
    header_selected: Color32,
    lane: Color32,
    lane_selected: Color32,
}

fn oklch(lightness: f32, chroma: f32, hue: f32) -> Color32 {
    let radians = hue.to_radians();
    let a = chroma * radians.cos();
    let b = chroma * radians.sin();
    let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (lightness - 0.105_561_35 * a - 0.063_854_17 * b).powi(3);
    let s = (lightness - 0.089_484_18 * a - 1.291_485_55 * b).powi(3);
    let r = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_93 * s;
    let g = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
    let b = -0.004_196_09 * l - 0.703_418_6 * m + 1.707_614_7 * s;
    let to_srgb = |value: f32| {
        let value = value.clamp(0.0, 1.0);
        let value = if value <= 0.003_130_8 {
            12.92 * value
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        };
        (value * 255.0).round() as u8
    };
    Color32::from_rgb(to_srgb(r), to_srgb(g), to_srgb(b))
}

impl Palette {
    fn new() -> Self {
        Self {
            bg: oklch(0.29, 0.0, 0.0),
            header: oklch(0.27, 0.0, 0.0),
            ruler: oklch(0.27, 0.0, 0.0),
            ruler_minor: oklch(0.3, 0.0, 0.0),
            ruler_major: oklch(0.4, 0.0, 0.0),
            ruler_zero: oklch(0.7, 0.0, 0.0),
            grid_strong: oklch(0.42, 0.0, 0.0),
            text: oklch(0.91, 0.0, 0.0),
            text_dim: oklch(0.72, 0.0, 0.0),
            clip_fill: Color32::from_rgb(0x35, 0x5a, 0x7a),
            clip_sel: Color32::from_rgb(0x4c, 0x84, 0xb0),
            clip_border: Color32::from_rgb(0x60, 0xcd, 0xff),
            clip_muted: oklch(0.39, 0.0, 0.0),
            wave: Color32::from_rgba_premultiplied(0xd0, 0xe8, 0xff, 0xcc),
            playhead: Color32::from_rgb(0xe5, 0x3d, 0x3d),
            marquee: Color32::from_rgba_premultiplied(0x60, 0xcd, 0xff, 0x33),
            meter_bg: oklch(0.23, 0.0, 0.0),
            header_hover: oklch(0.34, 0.0, 0.0),
            header_selected: Color32::from_rgb(0x2d, 0x47, 0x58),
            lane: oklch(0.235, 0.0, 0.0),
            lane_selected: Color32::from_rgb(0x1f, 0x2d, 0x37),
        }
    }
}

#[derive(Clone, Copy)]
struct Hit {
    track: usize,
    clip: Id,
    kind: DragKind,
}

#[derive(Clone, Copy)]
enum MenuTarget {
    Track(usize),
    Timeline(Hit),
    EmptyTimeline(i64),
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    st: &mut ArrangerState,
    doc: &Project,
    lib: &SourceLibrary,
    selection: &mut Vec<(usize, Id)>,
    master_volume_db: &mut f32,
    playhead_sec: f64,
    snap_unit: SnapUnit,
    alt_down: bool,
    subframe_movement: bool,
    mmd_wav_available: bool,
    master_peak: (f32, f32),
    track_peak: impl Fn(usize) -> f32,
) -> ArrOutput {
    let pal = Palette::new();
    let fps = doc.fps.max(1) as f64;

    let full = ui.available_rect_before_wrap().shrink(PAGE_MARGIN);
    let painter = ui.painter_at(full);
    painter.rect_filled(full, 0.0, pal.bg);

    let master_right = (full.left() + MASTER_W).min(full.right());
    let header_left = (master_right + MASTER_TRACK_GAP).min(full.right());
    let header_right = (header_left + HEADER_W).min(full.right());
    let lane_x0 = (header_right + TRACK_TIMELINE_GAP).min(full.right());
    let master_rect = Rect::from_min_max(
        Pos2::new(full.left(), full.top()),
        Pos2::new(master_right, full.bottom()),
    );
    painter.rect_filled(master_rect, 0.0, pal.header);
    let master_inner = master_rect.shrink(MASTER_EDGE_MARGIN);
    let gap_rect = Rect::from_min_max(
        Pos2::new(header_right, full.top()),
        Pos2::new(lane_x0, full.bottom()),
    );
    painter.rect_filled(gap_rect, 0.0, pal.bg);
    let ruler_rect = Rect::from_min_max(
        Pos2::new(lane_x0, full.top()),
        Pos2::new(full.right(), full.top() + RULER_H),
    );
    let lanes_rect = Rect::from_min_max(
        Pos2::new(lane_x0, ruler_rect.bottom()),
        Pos2::new(full.right(), full.bottom()),
    );
    let content_x0 = (lane_x0 + TIMELINE_EDGE_GAP).min(full.right());
    let ruler_content = Rect::from_min_max(Pos2::new(content_x0, ruler_rect.top()), ruler_rect.max);
    let lanes_content = Rect::from_min_max(Pos2::new(content_x0, lanes_rect.top()), lanes_rect.max);
    if st.initial_view_pending && lanes_rect.width() > 0.0 {
        st.px_per_frame = (lanes_rect.width() / (30.0 * fps as f32)).clamp(MIN_PPF, MAX_PPF);
        st.initial_view_pending = false;
    }

    let mut out = ArrOutput::new();
    let selected_ids: Vec<Id> = selection.iter().map(|(_, clip)| *clip).collect();
    selection.clear();
    for clip in selected_ids {
        if let Some((ti, _)) = doc
            .tracks
            .iter()
            .enumerate()
            .find(|(_, track)| track.clip(clip).is_some())
        {
            if !selection.contains(&(ti, clip)) {
                selection.push((ti, clip));
            }
        }
    }
    let resp = ui.interact(full, ui.id().with("arr_canvas"), Sense::click_and_drag());
    let menu_was_open = st.menu_open;
    let primary_press = ui.input(|input| {
        if menu_was_open {
            return None;
        }
        input
            .pointer
            .button_pressed(egui::PointerButton::Primary)
            .then(|| input.pointer.interact_pos())
            .flatten()
    });
    if st.tool == Tool::Select {
        if let Some(p) = primary_press {
            if p.x >= header_left && p.x < header_right && p.y >= lanes_rect.top() {
                selection.clear();
                if let Some(ti) = track_row_at_y(doc, st, lanes_rect.top(), p.y) {
                    if let Some(clip) = doc.tracks[ti].clips.first() {
                        selection.push((ti, clip.id));
                    }
                }
            }
        }
    }
    let master_label_top = master_inner.bottom() - MASTER_LABEL_H;
    let master_db_bottom = master_label_top - MASTER_PART_GAP - 1.0;
    let master_db_rect = Rect::from_min_size(
        Pos2::new(master_inner.center().x - 23.5, master_db_bottom - 14.0),
        Vec2::new(47.0, 14.0),
    );

    let fader_top = (master_rect.top() + MASTER_EDGE_MARGIN + MASTER_TICK_INK_HALF)
        .min(master_db_rect.top() - 48.0);
    let fader_bottom = (master_db_rect.top() - MASTER_PART_GAP).max(fader_top + 40.0);
    let meter_left = master_inner.center().x - (MASTER_METER_W + MASTER_PART_GAP + 17.0) * 0.5;
    let master_fader = Rect::from_min_max(
        Pos2::new(meter_left, fader_top),
        Pos2::new(meter_left + MASTER_METER_W, fader_bottom),
    );
    let master_fader_response = ui.interact(
        master_fader,
        ui.id().with("master_volume_fader"),
        Sense::click_and_drag(),
    );
    if (master_fader_response.dragged() || master_fader_response.clicked())
        && master_fader_response.interact_pointer_pos().is_some()
    {
        if let Some(p) = master_fader_response.interact_pointer_pos() {
            *master_volume_db = master_volume_db_from_y(master_fader, p.y);
        }
    }
    for db in [12.0_f32, 0.0, -12.0, -24.0, -36.0, -48.0, -60.0] {
        let y = master_volume_y(master_fader, db);
        painter.text(
            Pos2::new(master_fader.right() + 9.0, y),
            Align2::LEFT_CENTER,
            format!("{db:.0}"),
            FontId::proportional(7.0),
            pal.text_dim,
        );
    }
    draw_level_meter(
        &painter,
        master_fader,
        master_peak.0.max(master_peak.1),
        pal.meter_bg,
    );
    let zero_y = master_volume_y(master_fader, 0.0);
    painter.line_segment(
        [
            Pos2::new(master_fader.left(), zero_y),
            Pos2::new(master_fader.right(), zero_y),
        ],
        Stroke::new(1.0_f32, pal.grid_strong),
    );
    let fader_y = master_volume_y(master_fader, *master_volume_db);
    painter.rect_filled(
        Rect::from_center_size(
            Pos2::new(master_fader.center().x, fader_y),
            Vec2::new(MASTER_METER_W, 9.0),
        ),
        2.0,
        pal.clip_border,
    );
    let master_part_rect = Rect::from_min_max(
        Pos2::new(master_inner.left(), fader_top),
        Pos2::new(master_inner.right(), fader_bottom),
    );
    let master_part_double_clicked = ui.input(|input| {
        input
            .pointer
            .button_double_clicked(egui::PointerButton::Primary)
            && input
                .pointer
                .interact_pos()
                .is_some_and(|pos| master_part_rect.contains(pos))
    });
    ui.scope(|ui| {
        if let Some(font) = ui.style_mut().text_styles.get_mut(&egui::TextStyle::Body) {
            font.size = 10.0;
        }
        shrink_field_height(ui);
        ui.put(
            master_db_rect,
            Decimal::new(master_volume_db)
                .range(MASTER_VOLUME_DB_FLOOR..=12.0)
                .speed(0.1)
                .fixed_decimals(1)
                .negative_infinity_at(MASTER_VOLUME_DB_FLOOR as f64)
                .suffix(" dB"),
        );
    });
    if master_part_double_clicked {
        *master_volume_db = 0.0;
    }
    painter.text(
        Pos2::new(
            master_inner.center().x,
            master_rect.bottom() - MASTER_EDGE_MARGIN + MASTER_LABEL_INK_DESCENT,
        ),
        Align2::CENTER_BOTTOM,
        "MASTER",
        FontId::proportional(7.0),
        pal.text_dim,
    );

    if resp.hovered() {
        let (scroll, modifiers, hover_x) = ui.input(|i| {
            (
                i.raw_scroll_delta,
                i.modifiers,
                i.pointer.hover_pos().map(|p| p.x),
            )
        });
        if scroll != Vec2::ZERO {
            if modifiers.alt {
                let anchor_x = hover_x.unwrap_or(lanes_rect.center().x);
                let anchor_frame = st.view_start_frame as f64
                    + (anchor_x - lane_x0) as f64 / st.px_per_frame as f64;
                let factor = (scroll.y * 0.0025).exp();
                st.px_per_frame = (st.px_per_frame * factor).clamp(MIN_PPF, MAX_PPF);
                st.view_start_frame =
                    (anchor_frame - (anchor_x - lane_x0) as f64 / st.px_per_frame as f64) as f32;
            } else if modifiers.shift {
                st.scroll_y -= scroll.y;
            } else {
                let dx = if scroll.x.abs() > scroll.y.abs() {
                    scroll.x
                } else {
                    scroll.y
                };
                st.view_start_frame -= dx / st.px_per_frame;
            }
        }
    }

    if resp.hovered() {
        let (mid_down, delta) = ui.input(|i| (i.pointer.middle_down(), i.pointer.delta()));
        if mid_down && delta != Vec2::ZERO {
            st.view_start_frame -= delta.x / st.px_per_frame.max(1e-3);
            st.scroll_y -= delta.y;
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }
    }
    st.scroll_y = st
        .scroll_y
        .clamp(0.0, lanes_scroll_max(doc, lanes_rect.height()));
    let ph_frame = (playhead_sec * fps) as f32;
    let view_left_margin = TIMELINE_EDGE_GAP / st.px_per_frame;
    let view_frames = ((full.right() - lane_x0) / st.px_per_frame).max(0.0);
    if ph_frame < st.view_start_frame + view_left_margin {
        st.view_start_frame = ph_frame - view_left_margin;
    } else if ph_frame > st.view_start_frame + view_frames {
        st.view_start_frame = ph_frame - view_frames;
    }
    if st.view_start_frame < -60.0 {
        st.view_start_frame = -60.0;
    }

    let vs = st.view_start_frame as f64;
    let ppf = st.px_per_frame;
    let x_of = move |f: f64| lane_x0 + ((f - vs) * ppf as f64) as f32;
    let frame_of = move |x: f32| vs + (x - lane_x0) as f64 / ppf as f64;

    let eff_snap = if alt_down { SnapUnit::Off } else { snap_unit };
    let markers: Vec<i64> = doc.markers.iter().map(|m| m.frame).collect();
    let snap = |f: f64| -> i64 {
        match eff_snap {
            SnapUnit::Off => f.round() as i64,
            SnapUnit::Frame => f.round() as i64,
            SnapUnit::Second => ((f / fps).round() * fps).round() as i64,
            SnapUnit::Marker => {
                let r = f.round() as i64;
                markers
                    .iter()
                    .min_by_key(|m| (**m - r).abs())
                    .copied()
                    .filter(|m| ((*m - r).abs() as f64) * ppf as f64 <= 12.0)
                    .unwrap_or(r)
            }
        }
    };
    let quantize = |f: f64| -> f64 {
        if subframe_movement {
            f
        } else {
            snap(f) as f64
        }
    };

    let previous_drop_frame = st.drop_frame;
    let previous_last_drop_frame = st.last_drop_frame;
    let previous_drop_track_index = st.drop_track_index;
    let (file_hovered, file_dropped, egui_pointer_pos) = ui.input(|i| {
        (
            !i.raw.hovered_files.is_empty(),
            !i.raw.dropped_files.is_empty(),
            i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()),
        )
    });
    let pointer_pos = if file_hovered || file_dropped {
        st.external_drop_pos.or(egui_pointer_pos)
    } else {
        egui_pointer_pos
    };
    let pointer_drop_frame = pointer_pos
        .filter(|p| {
            p.x >= content_x0
                && p.x <= full.right()
                && p.y >= full.top()
                && p.y <= full.bottom()
        })
        .map(|p| quantize(frame_of(p.x)).round() as i64);
    let pointer_drop_track_index = pointer_pos
        .filter(|p| {
            p.x >= content_x0
                && p.x <= full.right()
                && p.y >= lanes_rect.top()
                && p.y <= lanes_rect.bottom()
        })
        .map(|p| drop_track_index_at_y(doc, st, lanes_rect.top(), p.y));
    if file_dropped {
        st.drop_frame = pointer_drop_frame
            .or(previous_last_drop_frame)
            .or(previous_drop_frame);
        if st.drop_frame.is_some() {
            st.last_drop_frame = st.drop_frame;
        }
        st.drop_track_index = pointer_drop_track_index.or(previous_drop_track_index);
    } else if file_hovered {
        let frame = pointer_drop_frame
            .or(previous_last_drop_frame)
            .or(previous_drop_frame)
            .unwrap_or_else(|| (playhead_sec * fps).round() as i64);
        st.drop_frame = Some(frame);
        st.last_drop_frame = Some(frame);
        st.drop_track_index = Some(
            pointer_drop_track_index
                .or(previous_drop_track_index)
                .unwrap_or(doc.tracks.len()),
        );
    } else {
        st.drop_frame = None;
        st.drop_track_index = None;
    }

    if !menu_was_open && resp.drag_started_by(egui::PointerButton::Primary) {
        let press = ui.input(|i| i.pointer.press_origin());
        if let Some(p) = press.or_else(|| resp.interact_pointer_pos()) {
            if p.x >= header_left && p.x < header_right && p.y >= ruler_rect.bottom() {
                if let Some(ti) = track_row_at_y(doc, st, lanes_rect.top(), p.y) {
                    st.drag = Some(mk_drag(
                        DragKind::ReorderTrack,
                        ti,
                        doc.tracks[ti].id,
                        0.0,
                        p,
                        dummy_clip(),
                    ));
                }
            } else if ruler_rect.contains(p) {
                st.drag = Some(mk_drag(DragKind::Ruler, 0, Id(0), 0.0, p, dummy_clip()));
                out.actions
                    .push(ArrAction::SeekStop(quantize(frame_of(p.x)).max(0.0) / fps));
            } else if lanes_rect.contains(p) {
                let hit = hit_clip(doc, st, lanes_rect, lane_x0, p, x_of, selection);
                match st.tool {
                    Tool::Select => {
                        if let Some(h) = hit {
                            let sel = selection.contains(&(h.track, h.clip));
                            let shift = ui.input(|i| i.modifiers.shift);
                            if shift {
                                if sel {
                                    selection.retain(|s| *s != (h.track, h.clip));
                                } else {
                                    selection.push((h.track, h.clip));
                                }
                            } else if !sel {
                                selection.clear();
                                selection.push((h.track, h.clip));
                            }
                            let c = doc.tracks[h.track].clip(h.clip).unwrap().clone();

                            let group = vec![(h.track, h.clip, c.start_frame)];
                            let mut d =
                                mk_drag(h.kind, h.track, h.clip, quantize(frame_of(p.x)), p, c);
                            d.group = group;
                            st.drag = Some(d);
                        } else {
                            st.drag = Some(mk_drag(
                                DragKind::Ruler,
                                0,
                                Id(0),
                                quantize(frame_of(p.x)),
                                p,
                                dummy_clip(),
                            ));
                            out.actions
                                .push(ArrAction::SeekStop(quantize(frame_of(p.x)).max(0.0) / fps));
                        }
                    }
                    Tool::Split => {
                        if let Some(h) = hit {
                            out.actions.push(ArrAction::Split {
                                track: h.track,
                                clip: h.clip,
                                at_frame: quantize(frame_of(p.x)).round() as i64,
                            });
                        }
                    }
                    Tool::Erase => {
                        if let Some(h) = hit {
                            out.actions.push(ArrAction::Delete { track: h.track });
                            selection.retain(|s| *s != (h.track, h.clip));
                        }
                    }
                    Tool::Mute => {
                        if let Some(h) = hit {
                            out.actions.push(ArrAction::ToggleMute {
                                track: h.track,
                                clip: h.clip,
                            });
                        }
                    }
                    Tool::Zoom => {
                        let f0 = frame_of(p.x);
                        let zin = !ui.input(|i| i.modifiers.alt);
                        st.px_per_frame = if zin {
                            (st.px_per_frame * 1.5).min(MAX_PPF)
                        } else {
                            (st.px_per_frame / 1.5).max(MIN_PPF)
                        };
                        st.view_start_frame =
                            (f0 - (p.x - lane_x0) as f64 / st.px_per_frame as f64) as f32;
                    }
                }
            }
        }
    }

    if !menu_was_open {
        if let Some(d) = st.drag.as_ref() {
            let kind = d.kind;
            let track = if kind == DragKind::ReorderTrack {
                doc.tracks
                    .iter()
                    .position(|candidate| candidate.id == d.clip)
                    .unwrap_or(d.track)
            } else {
                d.track
            };
            let clip = d.clip;
            let grab_frame = d.grab_frame;
            let orig = d.orig.clone();
            let group = d.group.clone();
            out.merge_key = Some(0xA24_0000 ^ clip.0 ^ (kind as u64) << 40);

            if let Some(p) = resp.interact_pointer_pos() {
                let f = quantize(frame_of(p.x));
                match kind {
                    DragKind::Ruler => {
                        out.actions
                            .push(ArrAction::Seek(quantize(frame_of(p.x)).max(0.0) / fps));
                    }
                    DragKind::Body => {
                        let delta = f - grab_frame;
                        let items = group
                            .iter()
                            .map(|(ti, ci, os)| (*ti, *ci, (*os + delta).max(-60.0)))
                            .collect();
                        out.actions.push(ArrAction::MoveClips(items));
                    }
                    DragKind::TrimIn => {
                        let new_in = f.clamp(orig.start_frame, orig.out_frame - 1.0);
                        out.actions.push(ArrAction::TrimIn {
                            track,
                            clip,
                            new_in,
                        });
                    }
                    DragKind::TrimOut => {
                        let src_end = lib
                            .get(&orig.source)
                            .map(|s| orig.start_frame + (s.seconds() * fps).ceil().max(1.0))
                            .unwrap_or(f64::MAX);
                        let new_out = f.max(orig.in_frame + 1.0).min(src_end);
                        out.actions.push(ArrAction::TrimOut {
                            track,
                            clip,
                            new_out,
                        });
                    }
                    DragKind::FadeIn => {
                        let frames = (f - orig.in_frame)
                            .round()
                            .clamp(0.0, (orig.out_frame - orig.in_frame - 1.0).max(0.0))
                            as i64;
                        out.actions.push(ArrAction::FadeIn {
                            track,
                            clip,
                            frames,
                        });
                    }
                    DragKind::FadeOut => {
                        let frames = (orig.out_frame - f)
                            .round()
                            .clamp(0.0, (orig.out_frame - orig.in_frame - 1.0).max(0.0))
                            as i64;
                        out.actions.push(ArrAction::FadeOut {
                            track,
                            clip,
                            frames,
                        });
                    }
                    DragKind::Marquee => {}
                    DragKind::ReorderTrack => {
                        let to = reorder_target_index(doc, st, lanes_rect.top(), p.y);
                        if to != track && to != track + 1 {
                            out.actions
                                .push(ArrAction::ReorderTrack { from: track, to });
                        }
                    }
                }
            }

            if resp.drag_stopped_by(egui::PointerButton::Primary) {
                if kind == DragKind::Marquee {
                    if let (Some(a), Some(b)) = (
                        Some(st.drag.as_ref().unwrap().grab_pos),
                        resp.interact_pointer_pos(),
                    ) {
                        let r = Rect::from_two_pos(a, b);
                        marquee_select(doc, st, lanes_rect, lane_x0, r, x_of, selection);
                    }
                }
                st.drag = None;
            }
        }
    }

    if !menu_was_open && resp.clicked() && st.tool == Tool::Select {
        if let Some(p) = resp.interact_pointer_pos() {
            if p.x >= header_left && p.x < header_right && p.y >= lanes_rect.top() {
                selection.clear();
                if let Some(ti) = track_row_at_y(doc, st, lanes_rect.top(), p.y) {
                    if let Some(clip) = doc.tracks[ti].clips.first() {
                        selection.push((ti, clip.id));
                    }
                }
            } else if lanes_rect.contains(p) {
                if let Some(hit) = hit_clip(doc, st, lanes_rect, lane_x0, p, x_of, selection) {
                    let item = (hit.track, hit.clip);
                    if ui.input(|i| i.modifiers.shift) {
                        if selection.contains(&item) {
                            selection.retain(|s| *s != item);
                        } else {
                            selection.push(item);
                        }
                    } else if selection.as_slice() != [item] {
                        selection.clear();
                        selection.push(item);
                    }
                } else {
                    selection.clear();
                    out.actions
                        .push(ArrAction::SeekStop(quantize(frame_of(p.x)).max(0.0) / fps));
                }
            }
        }
    }

    let secondary_click_pos = ui.input(|i| {
        i.pointer
            .secondary_clicked()
            .then(|| i.pointer.interact_pos())
            .flatten()
    });
    if let Some(p) = secondary_click_pos {
        let over_context_menu = menu_was_open
            && ui
                .ctx()
                .layer_id_at(p)
                .is_some_and(|layer| layer.order == egui::Order::Foreground);
        if !over_context_menu {
            let target = if p.x >= header_left && p.x < header_right && p.y >= lanes_rect.top() {
                track_row_at_y(doc, st, lanes_rect.top(), p.y).map(MenuTarget::Track)
            } else if p.x >= lane_x0 && (ruler_rect.contains(p) || lanes_rect.contains(p)) {
                if lanes_rect.contains(p) {
                    hit_clip(doc, st, lanes_rect, lane_x0, p, x_of, selection)
                        .map(MenuTarget::Timeline)
                        .or_else(|| Some(MenuTarget::EmptyTimeline(snap(frame_of(p.x)))))
                } else {
                    Some(MenuTarget::EmptyTimeline(snap(frame_of(p.x))))
                }
            } else {
                None
            };
            if let Some(target) = target {
                selection.clear();
                match target {
                    MenuTarget::Track(ti) => {
                        if let Some(clip) = doc.tracks[ti].clips.first() {
                            selection.push((ti, clip.id));
                        }
                    }
                    MenuTarget::Timeline(hit) => selection.push((hit.track, hit.clip)),
                    MenuTarget::EmptyTimeline(_) => {}
                }
                st.menu_pos = p;
                st.menu_name = match target {
                    MenuTarget::Track(ti) => doc
                        .tracks
                        .get(ti)
                        .map(|track| track.name.clone())
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                st.menu_target = Some(target);
                st.menu_open = true;
            } else {
                selection.clear();
                st.menu_target = None;
                st.menu_name.clear();
                st.menu_open = false;
            }
        }
    }
    if st.menu_open {
        let target = st.menu_target;
        let mut close = false;
        let area = egui::Area::new(ui.id().with("arr_ctx_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(st.menu_pos)
            .show(ui.ctx(), |ui| {
                let vis = ui.visuals();
                egui::Frame::none()
                    .fill(vis.window_fill())
                    .stroke(vis.window_stroke())
                    .rounding(0.0)
                    .inner_margin(if matches!(target, Some(MenuTarget::Track(_))) {
                        egui::Margin::ZERO
                    } else {
                        egui::Margin::same(2.0)
                    })
                    .show(ui, |ui| {
                        ui.set_min_width(160.0);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        match target {
                            Some(MenuTarget::Track(ti)) => {
                                let name_response = ui.text_edit_singleline(&mut st.menu_name);
                                if name_response.changed() {
                                    out.actions.push(ArrAction::RenameTrack {
                                        track: ti,
                                        name: st.menu_name.clone(),
                                    });
                                }
                                if name_response.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    close = true;
                                }
                                ui.spacing_mut().item_spacing.y = 0.0;
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 22.0],
                                        egui::Button::new(crate::core::loc::t("arr.duplicate")),
                                    )
                                    .clicked()
                                {
                                    out.actions.push(ArrAction::Duplicate { track: ti });
                                    close = true;
                                }
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 22.0],
                                        egui::Button::new(crate::core::loc::t("arr.delete")),
                                    )
                                    .clicked()
                                {
                                    out.actions.push(ArrAction::Delete { track: ti });
                                    close = true;
                                }
                            }
                            Some(MenuTarget::Timeline(h)) => {
                                if let Some(clip) =
                                    doc.tracks.get(h.track).and_then(|t| t.clip(h.clip))
                                {
                                    let edit_unit = if subframe_movement {
                                        ui.horizontal(|ui| {
                                            ui.selectable_value(
                                                &mut st.menu_time_unit,
                                                TimelineEditUnit::Millisecond,
                                                crate::core::loc::t("arr.unit.ms"),
                                            );
                                            ui.selectable_value(
                                                &mut st.menu_time_unit,
                                                TimelineEditUnit::Frame,
                                                crate::core::loc::t("arr.unit.frame"),
                                            );
                                        });
                                        st.menu_time_unit
                                    } else {
                                        TimelineEditUnit::Frame
                                    };
                                    let frame_to_ms = |frame: f64| frame * 1000.0 / fps;
                                    let ms_to_frame = |ms: f32| ms as f64 * fps / 1000.0;
                                    let mut start = clip.start_frame;
                                    ui.horizontal(|ui| {
                                        ui.label(crate::core::loc::t("arr.move"));
                                        let changed = match edit_unit {
                                            TimelineEditUnit::Frame => {
                                                let mut frame = start.round() as i64;
                                                let changed = ui
                                                    .add(
                                                        Integer::new(&mut frame)
                                                            .speed(1.0)
                                                            .suffix(" f"),
                                                    )
                                                    .changed();
                                                start = frame as f64;
                                                changed
                                            }
                                            TimelineEditUnit::Millisecond => {
                                                let mut ms = frame_to_ms(start) as f32;
                                                let changed = ui
                                                    .add(
                                                        Decimal::new(&mut ms)
                                                            .speed(0.1)
                                                            .fixed_decimals(3)
                                                            .suffix(" ms"),
                                                    )
                                                    .changed();
                                                start = ms_to_frame(ms);
                                                changed
                                            }
                                        };
                                        if changed {
                                            out.actions.push(ArrAction::MoveClips(vec![(
                                                h.track, h.clip, start,
                                            )]));
                                        }
                                    });
                                    let mut in_frame = clip.in_frame;
                                    ui.horizontal(|ui| {
                                        ui.label(crate::core::loc::t("arr.trimIn"));
                                        let changed = match edit_unit {
                                            TimelineEditUnit::Frame => {
                                                let mut frame = in_frame.round() as i64;
                                                let changed = ui
                                                    .add(
                                                        Integer::new(&mut frame)
                                                            .speed(1.0)
                                                            .suffix(" f"),
                                                    )
                                                    .changed();
                                                in_frame = frame as f64;
                                                changed
                                            }
                                            TimelineEditUnit::Millisecond => {
                                                let mut ms = frame_to_ms(in_frame) as f32;
                                                let changed = ui
                                                    .add(
                                                        Decimal::new(&mut ms)
                                                            .speed(0.1)
                                                            .fixed_decimals(3)
                                                            .suffix(" ms"),
                                                    )
                                                    .changed();
                                                in_frame = ms_to_frame(ms);
                                                changed
                                            }
                                        };
                                        if changed {
                                            out.actions.push(ArrAction::TrimIn {
                                                track: h.track,
                                                clip: h.clip,
                                                new_in: in_frame,
                                            });
                                        }
                                    });
                                    let mut out_frame = clip.out_frame;
                                    ui.horizontal(|ui| {
                                        ui.label(crate::core::loc::t("arr.trimOut"));
                                        let changed = match edit_unit {
                                            TimelineEditUnit::Frame => {
                                                let mut frame = out_frame.round() as i64;
                                                let changed = ui
                                                    .add(
                                                        Integer::new(&mut frame)
                                                            .speed(1.0)
                                                            .suffix(" f"),
                                                    )
                                                    .changed();
                                                out_frame = frame as f64;
                                                changed
                                            }
                                            TimelineEditUnit::Millisecond => {
                                                let mut ms = frame_to_ms(out_frame) as f32;
                                                let changed = ui
                                                    .add(
                                                        Decimal::new(&mut ms)
                                                            .speed(0.1)
                                                            .fixed_decimals(3)
                                                            .suffix(" ms"),
                                                    )
                                                    .changed();
                                                out_frame = ms_to_frame(ms);
                                                changed
                                            }
                                        };
                                        if changed {
                                            out.actions.push(ArrAction::TrimOut {
                                                track: h.track,
                                                clip: h.clip,
                                                new_out: out_frame,
                                            });
                                        }
                                    });
                                    let mut fade_in = clip.fade_in_frames;
                                    ui.horizontal(|ui| {
                                        ui.label(crate::core::loc::t("arr.fadeIn"));
                                        if ui
                                            .add(
                                                Integer::new(&mut fade_in)
                                                    .speed(1.0)
                                                    .suffix(" f")
                                                    .range(
                                                        0..=((clip.out_frame - clip.in_frame - 1.0)
                                                            .max(0.0)
                                                            as i64),
                                                    ),
                                            )
                                            .changed()
                                        {
                                            out.actions.push(ArrAction::FadeIn {
                                                track: h.track,
                                                clip: h.clip,
                                                frames: fade_in,
                                            });
                                        }
                                    });
                                    let mut fade_out = clip.fade_out_frames;
                                    ui.horizontal(|ui| {
                                        ui.label(crate::core::loc::t("arr.fadeOut"));
                                        if ui
                                            .add(
                                                Integer::new(&mut fade_out)
                                                    .speed(1.0)
                                                    .suffix(" f")
                                                    .range(
                                                        0..=((clip.out_frame - clip.in_frame - 1.0)
                                                            .max(0.0)
                                                            as i64),
                                                    ),
                                            )
                                            .changed()
                                        {
                                            out.actions.push(ArrAction::FadeOut {
                                                track: h.track,
                                                clip: h.clip,
                                                frames: fade_out,
                                            });
                                        }
                                    });
                                }
                            }
                            Some(MenuTarget::EmptyTimeline(frame)) => {
                                ui.label(
                                    crate::core::loc::t("arr.addAt")
                                        .replace("{frame}", &frame.to_string()),
                                );
                                if ui
                                    .add_enabled(
                                        mmd_wav_available,
                                        egui::Button::new(crate::core::loc::t(
                                            "arr.addFromConnected",
                                        )),
                                    )
                                    .clicked()
                                {
                                    out.actions
                                        .push(ArrAction::AddSourceFromMmd { start_frame: frame });
                                    close = true;
                                }
                                if ui.button(crate::core::loc::t("arr.addSource")).clicked() {
                                    out.actions
                                        .push(ArrAction::AddSourceDialog { start_frame: frame });
                                    close = true;
                                }
                            }
                            None => {}
                        }
                    });
            });
        ui.ctx().move_to_top(area.response.layer_id);
        let menu_rect = area.response.rect;
        let (esc, primary_click, click_pos) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Escape),
                i.pointer.primary_clicked(),
                i.pointer.interact_pos(),
            )
        });
        let clicked_outside =
            primary_click && click_pos.map(|p| !menu_rect.contains(p)).unwrap_or(false);
        if close || esc || clicked_outside {
            st.menu_open = false;
            st.menu_target = None;
            st.menu_name.clear();
        }
    }

    if !st.menu_open {
        apply_cursor(
            ui,
            st,
            doc,
            lanes_rect,
            header_left,
            header_right,
            lane_x0,
            x_of,
            selection,
        );
    }

    painter.rect_filled(ruler_rect, 0.0, pal.ruler);
    painter.line_segment(
        [
            Pos2::new(lane_x0, full.top()),
            Pos2::new(lane_x0, full.bottom()),
        ],
        Stroke::new(1.0_f32, pal.grid_strong),
    );
    let (step, subdiv) = ruler_step(st.px_per_frame, fps);
    let first = (vs / step).floor() as i64;
    let last = (frame_of(full.right()) / step).ceil() as i64;
    let ruler_tick_painter = painter.with_clip_rect(ruler_content);
    let lanes_grid_painter = painter.with_clip_rect(lanes_content);
    let external_insert_index = if file_hovered || file_dropped {
        st.drop_track_index
    } else {
        None
    };
    let external_insert_height = MIN_LANE_H + TRACK_GAP;

    {
        if let Some(index) = external_insert_index {
            let slot_y = reorder_line_y(doc, st, lanes_rect.top(), index);
            let slot = Rect::from_min_max(
                Pos2::new(content_x0, slot_y),
                Pos2::new(full.right(), slot_y + MIN_LANE_H),
            );
            lanes_grid_painter.rect_filled(slot, 0.0, pal.lane);
        }
        let mut lane_y = lanes_rect.top() - st.scroll_y;
        for (ti, track) in doc.tracks.iter().enumerate() {
            if external_insert_index == Some(ti) {
                lane_y += external_insert_height;
            }
            let h = track_row_height(track);
            let row = Rect::from_min_max(
                Pos2::new(content_x0, lane_y),
                Pos2::new(full.right(), lane_y + h),
            );
            if row.bottom() >= lanes_rect.top() && row.top() <= lanes_rect.bottom() {
                let selected = selection.iter().any(|(track, _)| *track == ti);
                lanes_grid_painter.rect_filled(
                    row,
                    0.0,
                    if selected { pal.lane_selected } else { pal.lane },
                );
            }
            lane_y += h + TRACK_GAP;
        }
    }
    for k in first..=last {
        let f = k as f64 * step;
        let x = x_of(f);
        if x >= content_x0 && x <= full.right() + 1.0 {
            let marker_x = x;
            let major_grid = if f.abs() < f64::EPSILON {
                pal.ruler_zero
            } else {
                pal.ruler_major
            };
            ruler_tick_painter.line_segment(
                [
                    Pos2::new(marker_x, ruler_rect.top()),
                    Pos2::new(marker_x, ruler_rect.bottom()),
                ],
                Stroke::new(1.0_f32, major_grid),
            );
            lanes_grid_painter.line_segment(
                [
                    Pos2::new(marker_x, lanes_rect.top()),
                    Pos2::new(marker_x, lanes_rect.bottom()),
                ],
                Stroke::new(1.0_f32, major_grid),
            );
        }
        if subdiv > 1 {
            for s in 1..subdiv {
                let xs = x_of(f + step * s as f64 / subdiv as f64);
                if xs >= content_x0 && xs <= full.right() + 1.0 {
                    ruler_tick_painter.line_segment(
                        [
                            Pos2::new(
                                xs,
                                ruler_rect.bottom() - RULER_MINOR_H.min(ruler_rect.height()),
                            ),
                            Pos2::new(xs, ruler_rect.bottom()),
                        ],
                        Stroke::new(1.0_f32, pal.ruler_minor),
                    );
                }
            }
        }
    }

    for m in &doc.markers {
        let x = x_of(m.frame as f64);
        if x < content_x0 || x > full.right() {
            continue;
        }
        let marker_color = pal.playhead.gamma_multiply(0.5);
        painter
            .with_clip_rect(ruler_content)
            .add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(x - 5.0, ruler_rect.top()),
                    Pos2::new(x + 5.0, ruler_rect.top()),
                    Pos2::new(x, ruler_rect.top() + 7.0),
                ],
                marker_color,
                Stroke::NONE,
            ));
        painter.with_clip_rect(ruler_content).text(
            Pos2::new(x + 2.0, ruler_rect.bottom() - 12.0),
            Align2::LEFT_TOP,
            &m.name,
            FontId::proportional(9.5),
            pal.text_dim,
        );
    }

    let any_solo = doc.tracks.iter().any(|t| t.soloed);
    let hover_pos = ui.input(|i| i.pointer.hover_pos());
    let mut y = lanes_rect.top() - st.scroll_y;
    for (ti, track) in doc.tracks.iter().enumerate() {
        if external_insert_index == Some(ti) {
            y += external_insert_height;
        }
        let h = track_row_height(track);
        let lane = Rect::from_min_max(Pos2::new(header_left, y), Pos2::new(full.right(), y + h));
        if lane.bottom() >= lanes_rect.top() && lane.top() <= lanes_rect.bottom() {
            let row_selected = selection.iter().any(|(track, _)| *track == ti);
            let row_hovered = hover_pos.map(|pos| lane.contains(pos)).unwrap_or(false);
            let header_color = if row_selected {
                pal.header_selected
            } else if row_hovered {
                pal.header_hover
            } else {
                pal.header
            };
            let timeline_painter = painter.with_clip_rect(lanes_content);
            let header_painter = painter.with_clip_rect(Rect::from_min_max(
                Pos2::new(header_left, lanes_rect.top()),
                Pos2::new(header_right, lanes_rect.bottom()),
            ));
            let layer_left = (content_x0 + AUDIO_LAYER_INSET).min(full.right());
            let layer_right = (full.right() - AUDIO_LAYER_INSET).max(layer_left);
            let clip_h = track_base_height(track);
            let clip_bottom = lane.top() + clip_h;
            let lane_body = Rect::from_min_max(
                Pos2::new(layer_left, lane.top()),
                Pos2::new(layer_right, clip_bottom),
            );
            let hd = Rect::from_min_max(
                Pos2::new(header_left, lane.top()),
                Pos2::new(header_right, clip_bottom),
            );
            header_painter.rect_filled(hd, 0.0, header_color);
            if let Some(col) = track.color_tag.as_deref().and_then(parse_hex) {
                header_painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(hd.left(), lane.top()),
                        Pos2::new(hd.left() + 4.0, lane.bottom()),
                    ),
                    0.0,
                    col,
                );
            }
            let rh = hd.height() / 2.0;
            let pad = 5.0;
            let sq = (rh - 4.0).clamp(11.0, 16.0);

            let r1 = Rect::from_min_max(
                hd.min,
                Pos2::new(hd.right(), hd.top() + rh - TRACK_ROW_GAP * 0.5),
            );
            header_painter
                .with_clip_rect(Rect::from_min_max(
                    Pos2::new(hd.left() + pad, r1.top()),
                    Pos2::new(
                        hd.right() - pad - LEVEL_METER_GAP - LEVEL_METER_W,
                        r1.bottom(),
                    ),
                ))
                .text(
                    Pos2::new(hd.left() + pad, r1.center().y),
                    Align2::LEFT_CENTER,
                    &track.name,
                    FontId::proportional(9.5),
                    pal.text,
                );

            let r2 = Rect::from_min_max(
                Pos2::new(hd.left(), hd.top() + rh + TRACK_ROW_GAP * 0.5),
                hd.max,
            );
            let ms_x0 = hd.left() + pad;
            for (index, label, active, on_col) in [
                (0_u8, "M", track.muted, Color32::from_rgb(0xC8, 0x3A, 0x3A)),
                (1_u8, "S", track.soloed, Color32::from_rgb(0xC8, 0xA0, 0x28)),
            ] {
                let x = ms_x0 + index as f32 * (sq + 3.0);
                let rect =
                    Rect::from_min_size(Pos2::new(x, r2.center().y - sq / 2.0), Vec2::splat(sq));
                let response = ui.interact(
                    rect,
                    ui.id().with(("track_button", ti, index)),
                    Sense::click(),
                );
                header_painter.rect_filled(rect, 0.0, if active { on_col } else { pal.ruler });
                header_painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, pal.grid_strong));
                header_painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(9.0),
                    if active {
                        Color32::from_rgb(0x18, 0x14, 0x10)
                    } else {
                        pal.text_dim
                    },
                );
                if response.clicked() {
                    match index {
                        0 => out.actions.push(ArrAction::ToggleTrackMute { track: ti }),
                        _ => out.actions.push(ArrAction::ToggleTrackSolo { track: ti }),
                    }
                }
            }

            let meter = Rect::from_min_max(
                Pos2::new(hd.right() - pad - LEVEL_METER_W, hd.top() + 2.0),
                Pos2::new(hd.right() - pad, hd.bottom() - 2.0),
            );
            let db_left = ms_x0 + sq * 2.0 + 6.0;
            let mut db = track.volume_db;
            let db_h = (sq - 2.0).max(1.0);
            let db_rect = Rect::from_min_max(
                Pos2::new(db_left, r2.center().y - db_h / 2.0),
                Pos2::new(meter.left() - LEVEL_METER_GAP, r2.center().y + db_h / 2.0),
            );
            let db_response = ui
                .scope(|ui| {
                    if let Some(font) = ui.style_mut().text_styles.get_mut(&egui::TextStyle::Body) {
                        font.size = 10.0;
                    }
                    shrink_field_height(ui);
                    ui.put(
                        db_rect,
                        Decimal::new(&mut db)
                            .range(VOLUME_DB_MIN..=VOLUME_DB_MAX)
                            .speed(0.1)
                            .fixed_decimals(1)
                            .suffix(" dB"),
                    )
                })
                .inner;
            if db_response.changed() {
                out.actions
                    .push(ArrAction::SetTrackVolume { track: ti, db });
                out.merge_key = Some(0x00D1_0000 ^ ti as u64);
            }

            draw_level_meter(&header_painter, meter, track_peak(ti), pal.meter_bg);

            for clip in &track.clips {
                let is_sel = selection.contains(&(ti, clip.id));

                let audible = (!any_solo || track.soloed) && !track.muted && !clip.muted;
                draw_clip(
                    &timeline_painter,
                    &pal,
                    lib,
                    fps,
                    clip,
                    lane_body,
                    layer_left,
                    layer_right,
                    x_of,
                    is_sel,
                    !audible,
                );
            }
        }
        y += h + TRACK_GAP;
    }

    if file_hovered || file_dropped {
        if let (Some(start_frame), Some(length_frames)) =
            (st.drop_frame, st.drop_preview_frames.filter(|frames| *frames > 0))
        {
            let end_frame = start_frame.saturating_add(length_frames);
            let left = x_of(start_frame as f64).max(content_x0);
            let right = x_of(end_frame as f64).min(full.right());
            let target_index = st.drop_track_index.unwrap_or(doc.tracks.len());
            let insertion_y = reorder_line_y(doc, st, lanes_rect.top(), target_index)
                .clamp(lanes_rect.top(), lanes_rect.bottom());
            let height = MIN_LANE_H.min((lanes_rect.bottom() - insertion_y).max(0.0));
            let top = insertion_y;
            if right > left && height > 0.0 {
                let preview = Rect::from_min_max(
                    Pos2::new(left, top),
                    Pos2::new(right, top + height),
                );
                let preview_painter = painter.with_clip_rect(lanes_content);
                preview_painter.rect_filled(preview, 0.0, pal.clip_fill.gamma_multiply(0.20));
                stroke_open_box(
                    &preview_painter,
                    preview,
                    Stroke::new(1.0_f32, pal.clip_border.gamma_multiply(0.4)),
                    x_of(start_frame as f64) >= content_x0,
                    x_of(end_frame as f64) <= full.right(),
                );
                preview_painter.rect_filled(preview, 0.0, pal.clip_fill.gamma_multiply(0.50));
                stroke_open_box(
                    &preview_painter,
                    preview,
                    Stroke::new(1.0_f32, pal.clip_border.gamma_multiply(0.6)),
                    x_of(start_frame as f64) >= content_x0,
                    x_of(end_frame as f64) <= full.right(),
                );
                if let Some(label) = st.drop_preview_name.as_deref() {
                    preview_painter.with_clip_rect(preview).text(
                        Pos2::new(preview.left() + 4.0, preview.top() + 3.0),
                        Align2::LEFT_TOP,
                        label,
                        FontId::proportional(10.5),
                        pal.text,
                    );
                }
            }
        }
    }

    if let Some(d) = st.drag.as_ref() {
        if d.kind == DragKind::Marquee {
            if let Some(p) = ui.input(|i| i.pointer.hover_pos()) {
                let r = Rect::from_two_pos(d.grab_pos, p).intersect(lanes_rect);
                painter.rect_filled(r, 0.0, pal.marquee);
                painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, pal.clip_border));
            }
        }
    }

    if let Some(d) = st.drag.as_ref() {
        if d.kind == DragKind::ReorderTrack {
            if let Some(p) = ui.input(|i| i.pointer.hover_pos()) {
                let to = reorder_target_index(doc, st, lanes_rect.top(), p.y);
                let ly = reorder_line_y(doc, st, lanes_rect.top(), to)
                    .clamp(lanes_rect.top(), lanes_rect.bottom());
                painter
                    .with_clip_rect(Rect::from_min_max(
                        Pos2::new(header_left, lanes_rect.top()),
                        Pos2::new(header_right, lanes_rect.bottom()),
                    ))
                    .hline(
                        header_left..=header_right,
                        ly,
                        Stroke::new(2.5_f32, pal.clip_border),
                    );
            }
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }
    }

    let ruler_label_painter = painter.with_clip_rect(ruler_content);
    for k in first..=last {
        let f = k as f64 * step;
        let x = x_of(f);
        if x >= content_x0 - 1.0 && x <= full.right() + 1.0 {
            ruler_label_painter.text(
                Pos2::new(x + 3.0, ruler_rect.top() + 2.0),
                Align2::LEFT_TOP,
                ruler_label(f, fps, step),
                FontId::proportional(10.0),
                pal.text,
            );
        }
    }

    let ph_x = x_of(playhead_sec * fps);
    if ph_x >= content_x0 && ph_x <= full.right() {
        let ph_painter = painter.with_clip_rect(Rect::from_min_max(
            Pos2::new(lane_x0, ruler_rect.top()),
            Pos2::new(full.right(), lanes_rect.bottom()),
        ));
        ph_painter.line_segment(
            [
                Pos2::new(ph_x, ruler_rect.top()),
                Pos2::new(ph_x, lanes_rect.bottom()),
            ],
            Stroke::new(1.5_f32, pal.playhead),
        );
        ph_painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(ph_x - 5.0, ruler_rect.top()),
                Pos2::new(ph_x + 5.0, ruler_rect.top()),
                Pos2::new(ph_x + 5.0, ruler_rect.top() + 6.0),
                Pos2::new(ph_x, ruler_rect.top() + 11.0),
                Pos2::new(ph_x - 5.0, ruler_rect.top() + 6.0),
            ],
            pal.playhead,
            Stroke::NONE,
        ));
    }

    out
}

pub fn fit_all(
    st: &mut ArrangerState,
    doc: &Project,
    view_w: f32,
    composition_range: Option<(i64, i64)>,
) {
    let range = composition_range
        .filter(|(start, end)| *start >= 0 && *end > *start)
        .map(|(start, end)| (start as f64, end as f64))
        .or_else(|| {
            let mut range: Option<(f64, f64)> = None;
            for clip in doc.tracks.iter().flat_map(|t| t.clips.iter()) {
                if clip.out_frame <= clip.in_frame {
                    continue;
                }
                range = Some(match range {
                    Some((start, end)) => (start.min(clip.in_frame), end.max(clip.out_frame)),
                    None => (clip.in_frame, clip.out_frame),
                });
            }
            range
        })
        .unwrap_or((0.0, 60.0));
    let start = range.0 as f32;
    let end = range.1.max(range.0 + 60.0) as f32;
    let usable = (view_w
        - PAGE_MARGIN * 2.0
        - MASTER_W
        - HEADER_W
        - MASTER_TRACK_GAP
        - TRACK_TIMELINE_GAP
        - TIMELINE_EDGE_GAP
        - 20.0)
        .max(100.0);
    st.px_per_frame = (usable / (end - start)).clamp(MIN_PPF, MAX_PPF);
    st.view_start_frame = start;
    st.scroll_y = 0.0;
}

fn mk_drag(
    kind: DragKind,
    track: usize,
    clip: Id,
    grab_frame: f64,
    grab_pos: Pos2,
    orig: Clip,
) -> Drag {
    Drag {
        kind,
        track,
        clip,
        grab_frame,
        grab_pos,
        orig,
        group: Vec::new(),
    }
}

fn dummy_clip() -> Clip {
    Clip::new(std::path::PathBuf::new(), 0, 1)
}

fn track_row_height(t: &crate::model::Track) -> f32 {
    track_base_height(t)
}

fn track_base_height(t: &crate::model::Track) -> f32 {
    (t.height * (2.0 / 3.0)).max(MIN_LANE_H)
}

fn master_volume_db_from_y(rect: Rect, y: f32) -> f32 {
    let t = ((rect.bottom() - y) / rect.height().max(1.0)).clamp(0.0, 1.0);
    MASTER_VOLUME_DB_FLOOR + (VOLUME_DB_MAX - MASTER_VOLUME_DB_FLOOR) * t
}

fn master_volume_y(rect: Rect, db: f32) -> f32 {
    rect.bottom()
        - ((db - MASTER_VOLUME_DB_FLOOR) / (VOLUME_DB_MAX - MASTER_VOLUME_DB_FLOOR)).clamp(0.0, 1.0)
            * rect.height()
}

const METER_STOPS: [(f32, Color32); 4] = [
    (0.0, Color32::from_rgb(0x3E, 0xB4, 0x6E)),
    (0.70, Color32::from_rgb(0x7C, 0xC8, 0x4A)),
    (0.90, Color32::from_rgb(0xE2, 0xC2, 0x36)),
    (1.0, Color32::from_rgb(0xE0, 0x3F, 0x3F)),
];

fn draw_level_meter(painter: &egui::Painter, meter: Rect, level: f32, bg: Color32) {
    painter.rect_filled(meter, 1.0, bg);
    let level = level.clamp(0.0, 1.0);
    if level <= 0.0 || meter.height() <= 0.0 || meter.width() <= 0.0 {
        return;
    }
    let y_at = |t: f32| meter.bottom() - t * meter.height();
    let mut mesh = egui::epaint::Mesh::default();
    for pair in METER_STOPS.windows(2) {
        let (lo, lo_color) = pair[0];
        let (hi, hi_color) = pair[1];
        let base = mesh.vertices.len() as u32;
        for (t, color) in [(hi, hi_color), (lo, lo_color)] {
            for x in [meter.left(), meter.right()] {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: Pos2::new(x, y_at(t)),
                    uv: egui::epaint::WHITE_UV,
                    color,
                });
            }
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
    }
    let filled = Rect::from_min_max(Pos2::new(meter.left(), y_at(level)), meter.max);
    painter
        .with_clip_rect(filled)
        .add(egui::Shape::mesh(mesh));
}

fn blend_color(from: Color32, to: Color32, amount: f32) -> Color32 {
    let t = amount.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (from.r() as f32 + (to.r() as f32 - from.r() as f32) * t).round() as u8,
        (from.g() as f32 + (to.g() as f32 - from.g() as f32) * t).round() as u8,
        (from.b() as f32 + (to.b() as f32 - from.b() as f32) * t).round() as u8,
        (from.a() as f32 + (to.a() as f32 - from.a() as f32) * t).round() as u8,
    )
}

fn lanes_scroll_max(doc: &Project, lanes_h: f32) -> f32 {
    let content_h: f32 = doc
        .tracks
        .iter()
        .map(|track| track_row_height(track) + TRACK_GAP)
        .sum();
    (content_h - lanes_h).max(0.0)
}

fn track_row_at_y(doc: &Project, st: &ArrangerState, lanes_top: f32, y: f32) -> Option<usize> {
    let mut cy = lanes_top - st.scroll_y;
    for (ti, t) in doc.tracks.iter().enumerate() {
        let h = track_row_height(t);
        if y >= cy && y < cy + h {
            return Some(ti);
        }
        cy += h + TRACK_GAP;
    }
    None
}

fn reorder_target_index(doc: &Project, st: &ArrangerState, lanes_top: f32, y: f32) -> usize {
    let mut cy = lanes_top - st.scroll_y;
    for (ti, t) in doc.tracks.iter().enumerate() {
        let h = track_row_height(t);
        if y < cy + h * 0.5 {
            return ti;
        }
        cy += h + TRACK_GAP;
    }
    doc.tracks.len()
}

fn drop_track_index_at_y(doc: &Project, st: &ArrangerState, lanes_top: f32, y: f32) -> usize {
    track_row_at_y(doc, st, lanes_top, y)
        .unwrap_or_else(|| reorder_target_index(doc, st, lanes_top, y))
}

fn reorder_line_y(doc: &Project, st: &ArrangerState, lanes_top: f32, to: usize) -> f32 {
    let mut cy = lanes_top - st.scroll_y;
    for t in doc.tracks.iter().take(to) {
        cy += track_row_height(t) + TRACK_GAP;
    }
    cy
}

#[allow(clippy::too_many_arguments)]
fn hit_clip(
    doc: &Project,
    st: &ArrangerState,
    lanes_rect: Rect,
    _lane_x0: f32,
    p: Pos2,
    x_of: impl Fn(f64) -> f32,
    _selection: &[(usize, Id)],
) -> Option<Hit> {
    let mut y = lanes_rect.top() - st.scroll_y;
    for (ti, track) in doc.tracks.iter().enumerate() {
        let h = track_base_height(track);
        let lane_top = y;
        let lane_bot = y + h;
        if p.y >= lane_top && p.y < lane_bot {
            for clip in track.clips.iter().rev() {
                let x_in = x_of(clip.in_frame as f64);
                let x_out = x_of(clip.out_frame as f64);
                let right = x_out;
                if p.x < x_in || p.x > right {
                    continue;
                }
                let edge = TRIM_GRAB_PX.min((right - x_in) * 0.25);
                if p.y <= lane_top + FADE_GRAB_PX {
                    if p.x <= x_in + FADE_GRAB_PX {
                        return Some(Hit {
                            track: ti,
                            clip: clip.id,
                            kind: DragKind::FadeIn,
                        });
                    }
                    if p.x >= x_out - FADE_GRAB_PX {
                        return Some(Hit {
                            track: ti,
                            clip: clip.id,
                            kind: DragKind::FadeOut,
                        });
                    }
                }
                if edge > 0.0 && p.x <= x_in + edge {
                    return Some(Hit {
                        track: ti,
                        clip: clip.id,
                        kind: DragKind::TrimIn,
                    });
                }
                if edge > 0.0 && p.x >= x_out - edge {
                    return Some(Hit {
                        track: ti,
                        clip: clip.id,
                        kind: DragKind::TrimOut,
                    });
                }
                if p.x >= x_in && p.x <= right {
                    return Some(Hit {
                        track: ti,
                        clip: clip.id,
                        kind: DragKind::Body,
                    });
                }
            }
            return None;
        }
        y += h + TRACK_GAP;
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn marquee_select(
    doc: &Project,
    st: &ArrangerState,
    lanes_rect: Rect,
    _lane_x0: f32,
    r: Rect,
    x_of: impl Fn(f64) -> f32,
    selection: &mut Vec<(usize, Id)>,
) {
    selection.clear();
    let mut y = lanes_rect.top() - st.scroll_y;
    for (ti, track) in doc.tracks.iter().enumerate() {
        let h = track_base_height(track);
        let lane = Rect::from_min_max(
            Pos2::new(x_of(f64::MIN), y),
            Pos2::new(x_of(f64::MAX), y + h),
        );
        if r.intersects(lane) {
            for clip in &track.clips {
                let cx = Rect::from_min_max(
                    Pos2::new(x_of(clip.in_frame as f64), y),
                    Pos2::new(x_of(clip.out_frame as f64), y + h),
                );
                if r.intersects(cx) {
                    selection.push((ti, clip.id));
                }
            }
        }
        y += h + TRACK_GAP;
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_cursor(
    ui: &egui::Ui,
    st: &ArrangerState,
    doc: &Project,
    lanes_rect: Rect,
    header_left: f32,
    header_right: f32,
    lane_x0: f32,
    x_of: impl Fn(f64) -> f32,
    selection: &[(usize, Id)],
) {
    if st.drag.is_none() {
        if let Some(p) = ui.input(|i| i.pointer.hover_pos()) {
            if p.x >= header_left
                && p.x < header_right
                && p.y >= lanes_rect.top()
                && p.y <= lanes_rect.bottom()
                && track_row_at_y(doc, st, lanes_rect.top(), p.y).is_some()
            {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            }
        }
    }

    let kind = if let Some(d) = st.drag.as_ref() {
        Some(d.kind)
    } else {
        ui.input(|i| i.pointer.hover_pos())
            .filter(|p| lanes_rect.contains(*p))
            .and_then(|p| hit_clip(doc, st, lanes_rect, lane_x0, p, &x_of, selection))
            .map(|h| h.kind)
    };
    match (st.tool, kind) {
        (_, Some(DragKind::TrimIn | DragKind::TrimOut)) => {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        (Tool::Select, Some(DragKind::Body)) => {
            let grab = st.drag.is_some();
            ui.ctx().set_cursor_icon(if grab {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            });
        }
        (Tool::Split, Some(_)) => ui.ctx().set_cursor_icon(CursorIcon::Text),
        (Tool::Erase, Some(_)) => ui.ctx().set_cursor_icon(CursorIcon::NotAllowed),
        (Tool::Zoom, _) => ui.ctx().set_cursor_icon(CursorIcon::ZoomIn),
        _ => {}
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;
    use crate::ui::config::SnapUnit;
    use eframe::egui::{CentralPanel, Context, Event, Modifiers, PointerButton, RawInput};
    use std::path::PathBuf;

    fn document() -> Project {
        let mut doc = Project::default();
        let mut track = crate::model::Track::new("test");
        track
            .clips
            .push(Clip::new(PathBuf::from("test.wav"), 0, 30));
        doc.tracks.push(track);
        doc
    }

    fn inject(
        ctx: &Context,
        state: &mut ArrangerState,
        doc: &Project,
        selection: &mut Vec<(usize, Id)>,
        events: Vec<Event>,
    ) -> ArrOutput {
        let mut master_volume_db = 0.0;
        inject_with_master(ctx, state, doc, selection, &mut master_volume_db, events)
    }

    fn inject_with_master(
        ctx: &Context,
        state: &mut ArrangerState,
        doc: &Project,
        selection: &mut Vec<(usize, Id)>,
        master_volume_db: &mut f32,
        events: Vec<Event>,
    ) -> ArrOutput {
        ctx.begin_pass(RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 400.0))),
            events,
            ..Default::default()
        });
        let mut result = ArrOutput::new();
        CentralPanel::default().show(ctx, |ui| {
            result = show(
                ui,
                state,
                doc,
                &SourceLibrary::default(),
                selection,
                master_volume_db,
                0.0,
                SnapUnit::Frame,
                false,
                false,
                false,
                (0.0, 0.0),
                |_| 0.0,
            );
        });
        let _ = ctx.end_pass();
        result
    }

    fn pointer(pos: Pos2, pressed: bool) -> Vec<Event> {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn hovered_file_without_pointer_uses_playhead_for_preview() {
        let ctx = Context::default();
        let mut state = ArrangerState::default();
        state.initial_view_pending = false;
        state.drop_preview_frames = Some(90);
        state.drop_preview_name = Some("hover.wav".to_owned());
        let doc = document();
        let mut selection = Vec::new();
        let mut master_volume_db = 0.0;

        ctx.begin_pass(RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 400.0))),
            hovered_files: vec![egui::HoveredFile {
                path: Some(PathBuf::from("hover.wav")),
                ..Default::default()
            }],
            ..Default::default()
        });
        CentralPanel::default().show(&ctx, |ui| {
            let _ = show(
                ui,
                &mut state,
                &doc,
                &SourceLibrary::default(),
                &mut selection,
                &mut master_volume_db,
                2.0,
                SnapUnit::Frame,
                false,
                false,
                false,
                (0.0, 0.0),
                |_| 0.0,
            );
        });
        let _ = ctx.end_pass();

        assert_eq!(state.drop_frame, Some(60));
        assert_eq!(state.drop_preview_frames, Some(90));
    }

    #[test]
    fn external_audio_drop_target_uses_the_exact_hovered_row() {
        let mut doc = document();
        doc.tracks.push(crate::model::Track::new("second"));
        let state = ArrangerState::default();

        assert_eq!(drop_track_index_at_y(&doc, &state, 0.0, 1.0), 0);
        assert_eq!(drop_track_index_at_y(&doc, &state, 0.0, 47.0), 0);
        assert_eq!(drop_track_index_at_y(&doc, &state, 0.0, 53.0), 1);
        assert_eq!(drop_track_index_at_y(&doc, &state, 0.0, 99.0), 1);
        assert_eq!(drop_track_index_at_y(&doc, &state, 0.0, 103.0), 2);
    }

    fn secondary_pointer(pos: Pos2, pressed: bool) -> Vec<Event> {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Secondary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn frame_ruler_adds_one_minor_grid_line_per_frame() {
        let (step, subdiv) = ruler_step(1.0, 30.0);
        assert_eq!(step, 60.0);
        assert_eq!(subdiv, 6);
        let (_, subdiv) = ruler_step(60.0, 30.0);
        assert_eq!(subdiv, 1);
    }

    #[test]
    fn fit_all_prefers_valid_composition_range_and_falls_back_to_layer_bounds() {
        let mut doc = Project::default();
        let mut early = crate::model::Track::new("early");
        early
            .clips
            .push(Clip::new(PathBuf::from("early.wav"), -30, 60));
        early.clips[0].in_frame = -10.0;
        let mut late = crate::model::Track::new("late");
        late.clips
            .push(Clip::new(PathBuf::from("late.wav"), 120, 60));
        late.clips[0].out_frame = 150.0;
        doc.tracks.extend([early, late]);

        let mut state = ArrangerState::default();
        fit_all(&mut state, &doc, 800.0, Some((10, 900)));
        assert_eq!(state.view_start_frame, 10.0);

        fit_all(&mut state, &doc, 800.0, Some((0, -1)));
        assert_eq!(state.view_start_frame, -10.0);
        assert!((state.px_per_frame - 564.0 / 160.0).abs() < f32::EPSILON);

        fit_all(&mut state, &doc, 800.0, Some((40, 40)));
        assert_eq!(state.view_start_frame, -10.0);
    }

    #[test]
    fn ruler_labels_keep_negative_frames_visible() {
        assert_eq!(ruler_label(-30.0, 30.0, 1.0), "-30f");
        assert_eq!(ruler_label(-60.0, 30.0, 1.0), "-60f");
        assert_eq!(ruler_label(-90.0, 30.0, 1.0), "-90f");
    }

    #[test]
    fn layer_top_handles_select_fade_edges() {
        let doc = document();
        let state = ArrangerState::default();
        let clip = doc.tracks[0].clips[0].id;
        let lane = Rect::from_min_max(Pos2::new(148.0, 24.0), Pos2::new(800.0, 400.0));
        let x_of = |frame: f64| 148.0 + frame as f32 * 6.0;
        assert!(
            matches!(hit_clip(&doc, &state, lane, 148.0, Pos2::new(150.0, 25.0), x_of, &[]), Some(Hit { clip: id, kind: DragKind::FadeIn, .. }) if id == clip)
        );
        assert!(
            matches!(hit_clip(&doc, &state, lane, 148.0, Pos2::new(326.0, 25.0), x_of, &[]), Some(Hit { clip: id, kind: DragKind::FadeOut, .. }) if id == clip)
        );
    }

    #[test]
    fn double_clicking_master_volume_resets_to_zero_db() {
        let ctx = Context::default();
        let mut state = ArrangerState::default();
        state.initial_view_pending = false;
        let doc = document();
        let mut selection = Vec::new();
        let mut master_volume_db = 6.0;
        let pos = Pos2::new(36.0, 160.0);

        for _ in 0..2 {
            let _ = inject_with_master(
                &ctx,
                &mut state,
                &doc,
                &mut selection,
                &mut master_volume_db,
                pointer(pos, true),
            );
            let _ = inject_with_master(
                &ctx,
                &mut state,
                &doc,
                &mut selection,
                &mut master_volume_db,
                pointer(pos, false),
            );
        }

        assert_eq!(master_volume_db, 0.0);
    }

    #[test]
    fn injected_pointer_selects_moves_and_trims_one_layer() {
        let ctx = Context::default();
        let mut state = ArrangerState::default();
        state.initial_view_pending = false;
        let doc = document();
        let mut selection = Vec::new();
        let body = Pos2::new(320.0, 38.0);

        let _ = inject(
            &ctx,
            &mut state,
            &doc,
            &mut selection,
            vec![Event::PointerMoved(body)],
        );
        let _ = inject(&ctx, &mut state, &doc, &mut selection, pointer(body, true));
        let _ = inject(&ctx, &mut state, &doc, &mut selection, pointer(body, false));
        assert_eq!(selection, vec![(0, doc.tracks[0].clips[0].id)]);

        let _ = inject(&ctx, &mut state, &doc, &mut selection, pointer(body, true));
        let moved = inject(
            &ctx,
            &mut state,
            &doc,
            &mut selection,
            vec![Event::PointerMoved(Pos2::new(350.0, 38.0))],
        );
        assert!(
            matches!(moved.actions.first(), Some(ArrAction::MoveClips(items)) if items.len() == 1)
        );
        let _ = inject(
            &ctx,
            &mut state,
            &doc,
            &mut selection,
            pointer(Pos2::new(350.0, 38.0), false),
        );

        let lane = Rect::from_min_max(Pos2::new(148.0, 24.0), Pos2::new(800.0, 400.0));
        let trimmed = hit_clip(
            &doc,
            &state,
            lane,
            148.0,
            Pos2::new(328.0, 50.0),
            |frame| 148.0 + frame as f32 * 6.0,
            &selection,
        );
        assert!(matches!(trimmed.map(|h| h.kind), Some(DragKind::TrimOut)));
    }

    #[test]
    fn secondary_click_replaces_open_menu_without_an_intermediate_action() {
        let ctx = Context::default();
        let mut state = ArrangerState::default();
        state.initial_view_pending = false;
        let doc = document();
        let mut selection = Vec::new();
        let timeline_pos = Pos2::new(320.0, 38.0);
        let track_pos = Pos2::new(120.0, 38.0);

        let _ = inject(
            &ctx,
            &mut state,
            &doc,
            &mut selection,
            secondary_pointer(timeline_pos, true),
        );
        let _ = inject(
            &ctx,
            &mut state,
            &doc,
            &mut selection,
            secondary_pointer(timeline_pos, false),
        );
        assert!(state.menu_open);

        let _ = inject(
            &ctx,
            &mut state,
            &doc,
            &mut selection,
            secondary_pointer(track_pos, true),
        );
        let _ = inject(
            &ctx,
            &mut state,
            &doc,
            &mut selection,
            secondary_pointer(track_pos, false),
        );
        assert!(state.menu_open);
        assert_eq!(state.menu_pos, track_pos);
        assert!(matches!(state.menu_target, Some(MenuTarget::Track(0))));
    }

    #[test]
    fn selection_indices_are_repaired_from_clip_ids() {
        let ctx = Context::default();
        let mut state = ArrangerState::default();
        state.initial_view_pending = false;
        let doc = document();
        let clip = doc.tracks[0].clips[0].id;
        let mut selection = vec![(9, clip)];
        let _ = inject(&ctx, &mut state, &doc, &mut selection, Vec::new());
        assert_eq!(selection, vec![(0, clip)]);
    }

    #[test]
    fn track_rows_select_layers_and_empty_areas_clear_selection() {
        let ctx = Context::default();
        let mut state = ArrangerState::default();
        state.initial_view_pending = false;
        let doc = document();
        let clip = doc.tracks[0].clips[0].id;
        let mut selection = Vec::new();

        for pressed in [true, false] {
            let _ = inject(
                &ctx,
                &mut state,
                &doc,
                &mut selection,
                pointer(Pos2::new(120.0, 38.0), pressed),
            );
        }
        assert_eq!(selection, vec![(0, clip)]);

        for pos in [Pos2::new(500.0, 150.0), Pos2::new(120.0, 150.0)] {
            selection = vec![(0, clip)];
            for pressed in [true, false] {
                let _ = inject(
                    &ctx,
                    &mut state,
                    &doc,
                    &mut selection,
                    pointer(pos, pressed),
                );
            }
            assert!(selection.is_empty());
        }
    }

    #[test]
    fn track_reorder_is_emitted_while_dragging() {
        let ctx = Context::default();
        let mut state = ArrangerState::default();
        state.initial_view_pending = false;
        let mut doc = document();
        let mut second = crate::model::Track::new("second");
        second
            .clips
            .push(Clip::new(PathBuf::from("second.wav"), 0, 30));
        doc.tracks.push(second);
        let mut selection = Vec::new();
        let start = Pos2::new(120.0, 38.0);

        let _ = inject(
            &ctx,
            &mut state,
            &doc,
            &mut selection,
            vec![Event::PointerMoved(start)],
        );
        let _ = inject(&ctx, &mut state, &doc, &mut selection, pointer(start, true));
        let dragged = inject(
            &ctx,
            &mut state,
            &doc,
            &mut selection,
            vec![Event::PointerMoved(Pos2::new(120.0, 155.0))],
        );
        assert!(matches!(
            dragged.actions.first(),
            Some(ArrAction::ReorderTrack { from: 0, to: 2 })
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn shrink_field_height(ui: &mut egui::Ui) {
    let spacing = ui.spacing_mut();
    spacing.interact_size.y = (spacing.interact_size.y - FIELD_SHRINK).max(1.0);
    spacing.button_padding.y = (spacing.button_padding.y - FIELD_SHRINK * 0.5).max(0.0);
}

fn stroke_open_box(
    painter: &egui::Painter,
    rect: Rect,
    stroke: Stroke,
    draw_left: bool,
    draw_right: bool,
) {
    painter.line_segment(
        [
            Pos2::new(rect.left(), rect.top()),
            Pos2::new(rect.right(), rect.top()),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(rect.left(), rect.bottom()),
            Pos2::new(rect.right(), rect.bottom()),
        ],
        stroke,
    );
    if draw_left {
        painter.line_segment(
            [
                Pos2::new(rect.left(), rect.top()),
                Pos2::new(rect.left(), rect.bottom()),
            ],
            stroke,
        );
    }
    if draw_right {
        painter.line_segment(
            [
                Pos2::new(rect.right(), rect.top()),
                Pos2::new(rect.right(), rect.bottom()),
            ],
            stroke,
        );
    }
}

fn draw_clip(
    painter: &egui::Painter,
    pal: &Palette,
    lib: &SourceLibrary,
    fps: f64,
    clip: &Clip,
    lane_body: Rect,
    lane_x0: f32,
    right_edge: f32,
    x_of: impl Fn(f64) -> f32,
    selected: bool,
    dimmed: bool,
) {
    let x_in = x_of(clip.in_frame as f64);
    let x_out = x_of(clip.out_frame as f64);

    let src = lib.get(&clip.source);
    let src_len_frames = src
        .as_ref()
        .map(|s| (s.seconds() * fps).ceil().max(1.0))
        .unwrap_or((clip.out_frame - clip.start_frame).max(1.0));
    let ghost_x0 = x_of(clip.start_frame as f64);
    let ghost_x1 = x_of(clip.start_frame + src_len_frames);

    let g_l = ghost_x0.max(lane_x0);
    let g_r = ghost_x1.min(right_edge);
    if g_r <= g_l {
        return;
    }

    let top = lane_body.top();
    let bot = lane_body.bottom();

    let d = if dimmed { 0.35_f32 } else { 1.0 };
    let a = |c: Color32, alpha: f32| c.gamma_multiply(alpha * d);

    let ghost_rect = Rect::from_min_max(Pos2::new(g_l, top), Pos2::new(g_r, bot));
    painter.rect_filled(ghost_rect, 0.0, a(pal.clip_fill, 0.20));
    stroke_open_box(
        painter,
        ghost_rect,
        Stroke::new(1.0_f32, a(pal.clip_border, 0.4)),
        ghost_x0 >= lane_x0,
        ghost_x1 <= right_edge,
    );

    if let Some(src) = &src {
        let span = (ghost_x1 - ghost_x0).max(1.0);
        let source_frames = src.frames;
        if source_frames > 0 {
            let visible_start_ratio = ((g_l - ghost_x0) / span).clamp(0.0, 1.0);
            let visible_end_ratio = ((g_r - ghost_x0) / span).clamp(0.0, 1.0);
            let visible_start = (visible_start_ratio * source_frames as f32).floor() as usize;
            let visible_end = (visible_end_ratio * source_frames as f32)
                .ceil()
                .min(source_frames as f32) as usize;
            let midy = (top + bot) * 0.5;
            let amp = ((bot - top) * 0.5 - 3.0).max(1.0);
            let bright_center = a(pal.wave, 0.70);
            let bright_edge = a(pal.clip_border, 0.92);
            let faint_center = a(pal.wave, 0.20);
            let faint_edge = a(pal.clip_border, 0.32);
            let xs = g_l.floor().max(lane_x0) as i32;
            let xe = g_r.ceil().min(right_edge) as i32;
            let columns = xe.saturating_sub(xs) as usize;
            let samples_per_pixel = source_frames as f64 / span as f64;
            src.peaks.for_each_column(
                src.samples.as_ref(),
                visible_start,
                visible_end,
                columns,
                samples_per_pixel,
                |column, lo, hi| {
                    let x = xs as f32 + column as f32 + 0.5;
                    let y0 =
                        (midy - hi.clamp(-1.0, 1.0) * amp).min(midy - lo.clamp(-1.0, 1.0) * amp);
                    let y1 =
                        (midy - hi.clamp(-1.0, 1.0) * amp).max(midy - lo.clamp(-1.0, 1.0) * amp);
                    let active = x >= x_in && x <= x_out;
                    let center = if active { bright_center } else { faint_center };
                    let edge = if active { bright_edge } else { faint_edge };
                    let steps = (((y1 - y0) / 2.0).ceil() as usize).clamp(1, 16);
                    for si in 0..steps {
                        let sy0 = y0 + (y1 - y0) * si as f32 / steps as f32;
                        let sy1 = y0 + (y1 - y0) * (si + 1) as f32 / steps as f32;
                        let distance = (((sy0 + sy1) * 0.5 - midy).abs() / amp).clamp(0.0, 1.0);
                        painter.line_segment(
                            [Pos2::new(x, sy0), Pos2::new(x, sy1)],
                            Stroke::new(1.0_f32, blend_color(center, edge, distance)),
                        );
                    }
                },
            );
        }
    }

    let u_l = x_in.max(lane_x0);
    let u_r = x_out.min(right_edge);
    if u_r > u_l {
        let used = Rect::from_min_max(Pos2::new(u_l, top), Pos2::new(u_r, bot));
        let fill = if dimmed {
            pal.clip_muted
        } else if selected {
            pal.clip_sel
        } else {
            pal.clip_fill
        };
        painter.rect_filled(used, 0.0, a(fill, 0.50));

        if clip.fade_in_frames > 0 {
            let fx = x_of(clip.in_frame + clip.fade_in_frames as f64).min(used.right());
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(used.left(), used.bottom()),
                    Pos2::new(used.left(), used.top()),
                    Pos2::new(fx, used.top()),
                ],
                a(pal.bg, 0.35),
                Stroke::NONE,
            ));
            painter.line_segment(
                [Pos2::new(fx, used.top()), Pos2::new(fx, used.top() + 10.0)],
                Stroke::new(1.0_f32, a(pal.clip_border, 0.9)),
            );
        }
        if clip.fade_out_frames > 0 {
            let fx = x_of(clip.out_frame - clip.fade_out_frames as f64).max(used.left());
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(used.right(), used.bottom()),
                    Pos2::new(used.right(), used.top()),
                    Pos2::new(fx, used.top()),
                ],
                a(pal.bg, 0.35),
                Stroke::NONE,
            ));
            painter.line_segment(
                [Pos2::new(fx, used.top()), Pos2::new(fx, used.top() + 10.0)],
                Stroke::new(1.0_f32, a(pal.clip_border, 0.9)),
            );
        }

        let border = if selected {
            Stroke::new(1.5_f32, a(pal.clip_border, 1.0))
        } else {
            Stroke::new(1.0_f32, a(pal.clip_border, 0.6))
        };
        stroke_open_box(
            painter,
            used,
            border,
            x_in >= lane_x0,
            x_out <= right_edge,
        );

        if selected {
            for hx in [x_in, x_out] {
                if hx >= lane_x0 && hx <= right_edge {
                    painter.rect_filled(
                        Rect::from_min_max(
                            Pos2::new(hx - 1.5, top + 1.0),
                            Pos2::new(hx + 1.5, bot - 1.0),
                        ),
                        0.0,
                        pal.clip_border,
                    );
                }
            }
            if ghost_x1 >= lane_x0 && ghost_x1 <= right_edge {
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(ghost_x1 - 2.0, top + 1.0),
                        Pos2::new(ghost_x1, bot - 1.0),
                    ),
                    0.0,
                    a(pal.clip_border, 0.5),
                );
            }
            let gb = Rect::from_min_size(
                Pos2::new(used.left() + 2.0, used.top() + 2.0),
                Vec2::new(GAIN_BOX, GAIN_BOX),
            );
            painter.rect_filled(gb, 2.0, pal.clip_border);
            crate::ui::icons::paint(
                painter,
                pal.bg,
                crate::ui::icons::Icon::ArrowUp,
                gb.shrink(1.5),
            );
        }

        let name = clip
            .source
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| crate::core::loc::t("arr.clipDefaultName"));
        let label = if clip.gain_db.abs() > 0.05 {
            format!("{name}  {:+.1}dB", clip.gain_db)
        } else {
            name.to_string()
        };
        painter.with_clip_rect(used).text(
            Pos2::new(
                used.left() + (if selected { GAIN_BOX + 6.0 } else { 4.0 }),
                used.top() + 2.0,
            ),
            Align2::LEFT_TOP,
            label,
            FontId::proportional(10.5),
            if dimmed { pal.text_dim } else { pal.text },
        );
    }
}

fn ruler_step(ppf: f32, _fps: f64) -> (f64, i64) {
    let raw = (RULER_TARGET_PX / ppf.max(MIN_PPF) as f64).max(1.0);
    let candidates: &[f64] = &[
        1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 45.0, 60.0, 90.0, 120.0, 150.0, 300.0, 600.0, 1500.0,
        3000.0,
    ];
    let mut step = *candidates.last().unwrap();
    for &c in candidates {
        if c >= raw {
            step = c;
            break;
        }
    }
    let step_frames = step.round().max(1.0) as i64;
    let max_subdiv = (step * ppf.max(MIN_PPF) as f64 / RULER_MIN_GRID_PX).floor() as i64;
    let divisor_limit = max_subdiv.clamp(1, step_frames);
    let subdiv = (1..=divisor_limit)
        .rev()
        .find(|divisor| step_frames % divisor == 0)
        .unwrap_or(1);
    (step.max(1.0), subdiv)
}

fn ruler_label(frame: f64, _fps: f64, _step: f64) -> String {
    format!("{}f", frame.round() as i64)
}

fn parse_hex(s: &str) -> Option<Color32> {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        Some(Color32::from_rgb(
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
        ))
    } else {
        None
    }
}
