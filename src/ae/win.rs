use super::{AeLayerSnapshot, AeProcess, AeProjectSnapshot, AeState, Shared};
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

const WORKER_INTERVAL: Duration = Duration::from_millis(100);
const PROCESS_INTERVAL: Duration = Duration::from_millis(1000);
const SCRIPT_TIMEOUT: Duration = Duration::from_millis(1500);
const SCRIPT_SETTLE: Duration = Duration::from_millis(80);

pub(super) fn list_processes() -> Vec<AeProcess> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
                if name.eq_ignore_ascii_case("AfterFX.exe") {
                    out.push(AeProcess {
                        pid: entry.th32ProcessID,
                        label: format!("PID {}", entry.th32ProcessID),
                    });
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    out.sort_by_key(|p| p.pid);
    out
}

fn process_path(pid: u32) -> Option<PathBuf> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, BOOL(0), pid).ok()?;
        let mut buffer = [0u16; 2048];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
        .ok()
        .map(|_| PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize])));
        let _ = CloseHandle(process);
        result
    }
}

fn process_exists(pid: u32) -> bool {
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, BOOL(0), pid) else {
            return false;
        };
        let _ = CloseHandle(process);
        true
    }
}

struct Attach {
    pid: u32,
    exe: PathBuf,
    dir: PathBuf,
    next_id: u64,
    last_process_check: Instant,
}

fn attach(pid: u32) -> Option<Attach> {
    let exe = process_path(pid)?;
    let dir = std::env::temp_dir().join(format!("AudioFrame_AfterEffects_{pid}"));
    std::fs::create_dir_all(&dir).ok()?;
    Some(Attach {
        pid,
        exe,
        dir,
        next_id: 0,
        last_process_check: Instant::now() - PROCESS_INTERVAL,
    })
}

fn detach(at: Attach) {
    let _ = std::fs::remove_dir_all(at.dir);
}

impl Attach {
    fn invoke(&mut self, body: &str) -> Result<String, String> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let script_path = self.dir.join(format!("command_{id}.jsx"));
        let result_path = self.dir.join(format!("result_{id}.txt"));
        let script = format!(
            "function afWrite(v){{var f=new File({});f.encoding=\"UTF-8\";if(f.open(\"w\")){{f.write(v);f.close();}}}}\n{}",
            jsx_string(&result_path.to_string_lossy()),
            body
        );
        std::fs::write(&script_path, script).map_err(|e| format!("JSX書き込み失敗: {e}"))?;
        let status = Command::new(&self.exe)
            .arg("-r")
            .arg(&script_path)
            .status()
            .map_err(|e| format!("After Effects起動失敗: {e}"))?;
        let started = Instant::now();
        let output = loop {
            if let Ok(text) = std::fs::read_to_string(&result_path) {
                if !text.is_empty() {
                    std::thread::sleep(SCRIPT_SETTLE);
                    break Ok(text);
                }
            }
            if started.elapsed() >= SCRIPT_TIMEOUT {
                break Err(if status.success() {
                    "After Effectsから応答がありません".to_string()
                } else {
                    format!("After Effectsスクリプト実行失敗: {status}")
                });
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let _ = std::fs::remove_file(script_path);
        let _ = std::fs::remove_file(result_path);
        output
    }

    fn read_project(&mut self) -> Result<AeProjectSnapshot, String> {
        let result = self.invoke(
            r#"var c=app.project&&app.project.activeItem;if(!(c&&c instanceof CompItem)){afWrite("ERR\tNO_COMP");}else{var fps=1/c.frameDuration;var duration=Math.round(c.duration*fps);var rows=[];for(var i=1;i<=c.numLayers;i++){var layer=c.layer(i);if(layer.hasAudio&&layer.source instanceof FootageItem){try{var file=layer.source.mainSource.file;if(file){var gain=0;try{gain=Number(layer.audioLevels.value[0]);}catch(e){}rows.push(encodeURIComponent(layer.comment||("layer_"+i))+"\t"+encodeURIComponent(layer.name)+"\t"+encodeURIComponent(file.fsName)+"\t"+Math.round(layer.startTime*fps)+"\t"+Math.round(layer.inPoint*fps)+"\t"+Math.round(layer.outPoint*fps)+"\t"+gain.toFixed(4)+"\t"+(layer.audioEnabled?"0":"1"));}}catch(e){}}}afWrite("AF_PROJECT_V1\n"+fps.toFixed(9)+"\n"+duration+"\n"+rows.length+"\n"+rows.join("\n"));}"#,
        )?;
        parse_project(&result)
    }

    fn apply_project(&mut self, snapshot: &AeProjectSnapshot) -> Result<(), String> {
        let body = apply_body(snapshot);
        let result = self.invoke(&body)?;
        parse_command_result(&result)
    }

    fn import_wav(&mut self, path: &Path) -> Result<(), String> {
        let body = format!(
            r#"var c=app.project&&app.project.activeItem;var file=new File({});if(!(c&&c instanceof CompItem)){{afWrite("ERR\tNO_COMP");}}else if(!file.exists){{afWrite("ERR\tNO_FILE");}}else{{app.beginUndoGroup("AudioFrame");var item=null;for(var i=1;i<=app.project.numItems;i++){{var it=app.project.item(i);if(it instanceof FootageItem){{try{{if(it.mainSource.file&&it.mainSource.file.fsName===file.fsName){{item=it;break;}}}}catch(e){{}}}}}}if(!item){{item=app.project.importFile(new ImportOptions(file));}}var target=null;for(var j=0;j<c.selectedLayers.length;j++){{if(c.selectedLayers[j].hasAudio){{target=c.selectedLayers[j];break;}}}}if(!target){{for(var k=1;k<=c.numLayers;k++){{if(c.layer(k).name==="AudioFrame Mix"&&c.layer(k).hasAudio){{target=c.layer(k);break;}}}}}}if(target){{target.replaceSource(item,false);target.startTime=0;target.inPoint=0;target.outPoint=Math.min(c.duration,item.duration);}}else{{target=c.layers.add(item);target.name="AudioFrame Mix";target.startTime=0;target.inPoint=0;target.outPoint=Math.min(c.duration,item.duration);}}app.endUndoGroup();afWrite("OK");}}"#,
            jsx_string(&path.to_string_lossy())
        );
        let result = self.invoke(&body)?;
        parse_command_result(&result)
    }
}

fn update_state(shared: &Arc<Shared>, attached: bool, busy: bool, last_error: Option<String>) {
    let mut state = shared.state.lock().unwrap();
    *state = AeState {
        attached,
        busy,
        last_error,
    };
}

/// Standalone script for export: the apply body without the result-file callback.
pub(super) fn layer_script(snapshot: &AeProjectSnapshot) -> String {
    format!(
        "// Generated by AudioFrame\nfunction afWrite(v){{if(v&&v.indexOf(\"ERR\")===0){{alert(v);}}}}\n{}\n",
        apply_body(snapshot)
    )
}

/// Times are sent in seconds, computed from AudioFrame's own frame rate, so a
/// destination composition with a different frame rate lands on the same wall time.
fn apply_body(snapshot: &AeProjectSnapshot) -> String {
    let fps = if snapshot.fps.is_finite() && snapshot.fps >= 1.0 {
        snapshot.fps
    } else {
        30.0
    };
    let seconds = |frame: i64| frame as f64 / fps;
    let mut body = String::from(
        "var c=app.project&&app.project.activeItem;if(!(c&&c instanceof CompItem)){afWrite(\"ERR\\tNO_COMP\");}else{",
    );
    body.push_str("var files=[];var keep={};");
    for layer in &snapshot.layers {
        body.push_str(&format!(
            "files.push(new File({}));",
            jsx_string(&layer.source.to_string_lossy())
        ));
    }
    body.push_str("var valid=true;for(var fi=0;fi<files.length;fi++){if(!files[fi].exists){valid=false;}}if(!valid){afWrite(\"ERR\\tNO_FILE\");}else{app.beginUndoGroup(\"AudioFrame\");");
    // Top level project folder that holds every imported source.
    body.push_str("var afFolder=null;for(var fo=1;fo<=app.project.numItems;fo++){var fit=app.project.item(fo);if(fit instanceof FolderItem&&fit.name===\"AudioFrame\"&&fit.parentFolder===app.project.rootFolder){afFolder=fit;break;}}if(!afFolder){afFolder=app.project.items.addFolder(\"AudioFrame\");}");
    for (index, layer) in snapshot.layers.iter().enumerate() {
        let marker = format!("AudioFrame:{}", layer.key);
        let in_frame = layer.in_frame.max(layer.start_frame);
        let out_frame = layer.out_frame.max(in_frame + 1);
        let fade_in = layer.fade_in_frames.max(0).min(out_frame - in_frame);
        let fade_out = layer
            .fade_out_frames
            .max(0)
            .min((out_frame - in_frame - fade_in).max(0));
        body.push_str(&format!(
            "keep[{marker}]=1;var item=null;for(var ii=1;ii<=app.project.numItems;ii++){{var it=app.project.item(ii);if(it instanceof FootageItem){{try{{if(it.mainSource.file&&it.mainSource.file.fsName===files[{index}].fsName){{item=it;break;}}}}catch(e){{}}}}}}if(!item){{item=app.project.importFile(new ImportOptions(files[{index}]));}}try{{item.parentFolder=afFolder;}}catch(e){{}}var target=null;for(var li=1;li<=c.numLayers;li++){{if(c.layer(li).comment==={marker}){{target=c.layer(li);break;}}}}if(!target){{target=c.layers.add(item);}}target.comment={marker};target.name={name};target.startTime={start:.6};target.inPoint=Math.max(0,Math.min(c.duration,{in_sec:.6}));target.outPoint=Math.max(target.inPoint+c.frameDuration,Math.min(c.duration,{out_sec:.6}));",
            marker = jsx_string(&marker),
            name = jsx_string(&layer.name),
            start = seconds(layer.start_frame),
            in_sec = seconds(in_frame),
            out_sec = seconds(out_frame),
        ));
        // Trimmed range is expressed above; fades become audio level keyframes.
        if fade_in > 0 || fade_out > 0 {
            body.push_str(&format!(
                "try{{var lv=target.audioLevels;while(lv.numKeys>0){{lv.removeKey(1);}}var g={gain};var q=-96;",
                gain = layer.gain_db,
            ));
            if fade_in > 0 {
                body.push_str(&format!(
                    "lv.setValueAtTime(target.inPoint,[q,q]);lv.setValueAtTime(Math.min(target.outPoint,target.inPoint+{fade:.6}),[g,g]);",
                    fade = seconds(fade_in),
                ));
            } else {
                body.push_str("lv.setValueAtTime(target.inPoint,[g,g]);");
            }
            if fade_out > 0 {
                body.push_str(&format!(
                    "lv.setValueAtTime(Math.max(target.inPoint,target.outPoint-{fade:.6}),[g,g]);lv.setValueAtTime(target.outPoint,[q,q]);",
                    fade = seconds(fade_out),
                ));
            } else {
                body.push_str("lv.setValueAtTime(target.outPoint,[g,g]);");
            }
            body.push_str("}catch(e){}");
        } else {
            body.push_str(&format!(
                "try{{var lv2=target.audioLevels;while(lv2.numKeys>0){{lv2.removeKey(1);}}lv2.setValue([{gain},{gain}]);}}catch(e){{}}",
                gain = layer.gain_db,
            ));
        }
        body.push_str(&format!(
            "target.audioEnabled={muted};try{{target.moveToEnd();}}catch(e){{}}",
            muted = if layer.muted { "false" } else { "true" },
        ));
    }
    body.push_str("for(var ri=c.numLayers;ri>=1;ri--){var mark=c.layer(ri).comment;if(mark&&mark.indexOf(\"AudioFrame:\")===0&&!keep[mark]){c.layer(ri).remove();}}app.endUndoGroup();afWrite(\"OK\");}}");
    body
}

fn parse_command_result(text: &str) -> Result<(), String> {
    let value = text.trim();
    if value == "OK" {
        Ok(())
    } else if let Some(rest) = value.strip_prefix("ERR\t") {
        Err(rest.to_string())
    } else {
        Err(format!("After Effects応答不正: {value}"))
    }
}

fn parse_project(text: &str) -> Result<AeProjectSnapshot, String> {
    let mut lines = text.lines();
    if lines.next() != Some("AF_PROJECT_V1") {
        let error = text.lines().nth(1).unwrap_or("不明なエラー");
        return Err(format!("After Effectsから取得できません: {error}"));
    }
    let fps = lines
        .next()
        .ok_or_else(|| "FPSがありません".to_string())?
        .parse::<f64>()
        .map_err(|_| "FPSを解釈できません".to_string())?;
    let duration_frames = lines
        .next()
        .ok_or_else(|| "コンポジション尺がありません".to_string())?
        .parse::<i64>()
        .map_err(|_| "コンポジション尺を解釈できません".to_string())?;
    let count = lines
        .next()
        .ok_or_else(|| "レイヤー数がありません".to_string())?
        .parse::<usize>()
        .map_err(|_| "レイヤー数を解釈できません".to_string())?;
    if !fps.is_finite() || fps <= 0.0 || duration_frames < 0 {
        return Err("コンポジション情報が不正です".to_string());
    }
    let mut layers = Vec::with_capacity(count);
    for _ in 0..count {
        let line = lines
            .next()
            .ok_or_else(|| "レイヤー情報が不足しています".to_string())?;
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 8 {
            return Err("レイヤー情報の形式が不正です".to_string());
        }
        let key = decode_component(fields[0])?;
        let name = decode_component(fields[1])?;
        let source = PathBuf::from(decode_component(fields[2])?);
        let start_frame = parse_frame(fields[3])?;
        let in_frame = parse_frame(fields[4])?;
        let out_frame = parse_frame(fields[5])?;
        let gain_db = fields[6]
            .parse::<f32>()
            .map_err(|_| "音量を解釈できません".to_string())?;
        let muted = fields[7] == "1";
        if !gain_db.is_finite() || out_frame <= in_frame || source.as_os_str().is_empty() {
            continue;
        }
        layers.push(AeLayerSnapshot {
            key,
            name,
            source,
            start_frame,
            in_frame,
            out_frame,
            gain_db,
            muted,
            fade_in_frames: 0,
            fade_out_frames: 0,
        });
    }
    Ok(AeProjectSnapshot {
        fps,
        duration_frames,
        layers,
    })
}

fn parse_frame(value: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
        .map_err(|_| "フレーム位置を解釈できません".to_string())
}

fn decode_component(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err("URIエスケープが不正です".to_string());
            }
            let high = hex_value(bytes[i + 1]).ok_or_else(|| "URIエスケープが不正です".to_string())?;
            let low = hex_value(bytes[i + 2]).ok_or_else(|| "URIエスケープが不正です".to_string())?;
            out.push(high * 16 + low);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "URI文字列がUTF-8ではありません".to_string())
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn jsx_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn poll_loop(shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    let mut current: Option<Attach> = None;
    while !stop.load(Ordering::Relaxed) {
        let want = shared.target_pid.load(Ordering::Relaxed);
        let process_dead = current.as_mut().is_some_and(|at| {
            if at.last_process_check.elapsed() < PROCESS_INTERVAL {
                false
            } else {
                at.last_process_check = Instant::now();
                !process_exists(at.pid)
            }
        });
        let switch = current
            .as_ref()
            .map(|at| at.pid != want || process_dead)
            .unwrap_or(want != 0);
        if switch {
            if let Some(at) = current.take() {
                detach(at);
            }
            if want != 0 {
                current = attach(want);
                if current.is_none() {
                    shared.target_pid.store(0, Ordering::Relaxed);
                }
            }
            update_state(&shared, current.is_some(), false, None);
        }

        let read = shared.pending_project_read.lock().ok().and_then(|mut g| g.take());
        let apply = if read.is_none() {
            shared.pending_project_apply.lock().ok().and_then(|mut g| g.take())
        } else {
            None
        };
        let import = if read.is_none() && apply.is_none() {
            shared.pending_import.lock().ok().and_then(|mut g| g.take())
        } else {
            None
        };
        if let Some(at) = current.as_mut() {
            let operation = if let Some(pid) = read {
                (pid == at.pid && pid == want).then_some(0)
            } else if let Some((pid, snapshot)) = apply {
                if pid == at.pid && pid == want {
                    update_state(&shared, true, true, None);
                    let result = at.apply_project(&snapshot);
                    let error = result.err();
                    update_state(&shared, true, false, error.clone());
                }
                Some(1)
            } else if let Some((pid, path)) = import {
                if pid == at.pid && pid == want {
                    update_state(&shared, true, true, None);
                    let result = at.import_wav(&path);
                    let error = result.err();
                    update_state(&shared, true, false, error.clone());
                }
                Some(2)
            } else {
                None
            };
            if operation == Some(0) {
                update_state(&shared, true, true, None);
                let result = at.read_project();
                let error = result.as_ref().err().cloned();
                if let Ok(snapshot) = result {
                    *shared.project_result.lock().unwrap() = Some((at.pid, Ok(snapshot)));
                } else if let Some(error) = error.clone() {
                    *shared.project_result.lock().unwrap() = Some((at.pid, Err(error)));
                }
                update_state(&shared, true, false, error);
            }
        }
        std::thread::sleep(WORKER_INTERVAL);
    }
    if let Some(at) = current {
        detach(at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsx_paths_escape_windows_separators() {
        assert_eq!(jsx_string(r#"C:\Audio\mix.wav"#), r#""C:\\Audio\\mix.wav""#);
    }

    #[test]
    fn component_decoder_restores_utf8_text() {
        assert_eq!(decode_component("%E3%83%86%E3%82%B9%E3%83%88").unwrap(), "テスト");
    }

    #[test]
    fn apply_script_places_sources_in_a_top_level_group_using_seconds() {
        let snapshot = AeProjectSnapshot {
            fps: 30.0,
            duration_frames: 300,
            layers: vec![AeLayerSnapshot {
                key: "1:2".into(),
                name: "voice".into(),
                source: PathBuf::from(r"C:\Audio\a.wav"),
                start_frame: 0,
                in_frame: 30,
                out_frame: 120,
                gain_db: -3.0,
                muted: false,
                fade_in_frames: 15,
                fade_out_frames: 15,
            }],
        };
        let body = apply_body(&snapshot);
        assert!(body.contains("app.project.items.addFolder(\"AudioFrame\")"));
        assert!(body.contains("item.parentFolder=afFolder"));
        // 30f in / 120f out at 30fps become plain seconds, so the destination fps is irrelevant.
        assert!(body.contains("1.000000"));
        assert!(body.contains("4.000000"));
        assert!(!body.contains("ERR\\tFPS"));
        assert!(body.contains("setValueAtTime"));
    }

    #[test]
    fn project_parser_reads_audio_layers() {
        let text = "AF_PROJECT_V1\n30.000000000\n900\n1\nlayer_1\t%E3%83%86%E3%82%B9%E3%83%88%20Audio\tC%3A%5CAudio%5Ca.wav\t-30\t0\t900\t-3.5\t0\n";
        let snapshot = parse_project(text).unwrap();
        assert_eq!(snapshot.duration_frames, 900);
        assert_eq!(snapshot.layers[0].name, "テスト Audio");
        assert_eq!(snapshot.layers[0].out_frame, 900);
    }
}
