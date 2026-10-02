


use super::{MmdProcess, MmdState, PlaybackRange, Shared};
use std::ffi::c_void;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{PCSTR, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HANDLE, HMODULE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Diagnostics::Debug::{
    FlushInstructionCache, ReadProcessMemory, WriteProcessMemory,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
    PAGE_READWRITE,
};
use windows::Win32::System::ProcessStatus::EnumProcessModules;
use windows::Win32::System::Threading::{
    CreateRemoteThread, OpenProcess, WaitForSingleObject, LPTHREAD_START_ROUTINE,
    PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ,
    PROCESS_VM_WRITE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetClassNameW, GetDlgCtrlID, GetMenu, GetMenuItemCount,
    GetMenuItemID, GetMenuState, GetParent, GetSubMenu, GetWindowThreadProcessId, IsWindow,
    IsWindowVisible, PostMessageW, SendMessageTimeoutW, BM_CLICK, BM_GETCHECK, MF_BYPOSITION,
    MF_CHECKED, SMTO_ABORTIFHUNG, SMTO_NORMAL, WM_COMMAND, WM_GETTEXT, WM_GETTEXTLENGTH, WM_KEYDOWN,
    WM_KEYUP, WM_SETTEXT,
};


unsafe fn sm(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> isize {
    let mut res: usize = 0;
    let r = SendMessageTimeoutW(
        h,
        msg,
        w,
        l,
        SMTO_ABORTIFHUNG | SMTO_NORMAL,
        800,
        Some(&mut res),
    );
    if r.0 == 0 {
        0
    } else {
        res as isize
    }
}



const VK_RETURN: usize = 0x0D;
const ENTER_SCAN: i64 = 0x1C;


const ID_PLAY: i32 = 408;
const ID_RANGE_START: i32 = 409;
const ID_RANGE_END: i32 = 410;
const ID_LOOP: i32 = 411; 
const ID_FRAME_START_TOGGLE: i32 = 414; 
const ID_FRAME_STOP_TOGGLE: i32 = 413; 
const ID_FRAME: i32 = 417;

const MENU_WAV_NO_PLAY: u32 = 297;
const MENU_WAV_LOAD: usize = 206;
const OPEN_DIALOG_FILENAME_EDIT: i32 = 1148;
const OPEN_DIALOG_OK_BUTTON: i32 = 1;


const WAV_PTR_OFF: u64 = 0x0014_45F8;
const WAV_STR_OFF: u64 = 0xD8;


const CLK_SIZE: usize = 16;
const CLK_SEQ: u64 = 0;
const CLK_TIME: u64 = 4;
const CLK_STOP: u64 = 8;



unsafe fn rpm(h: HANDLE, addr: u64, buf: &mut [u8]) -> bool {
    let mut read = 0usize;
    ReadProcessMemory(
        h,
        addr as *const c_void,
        buf.as_mut_ptr() as *mut c_void,
        buf.len(),
        Some(&mut read),
    )
    .is_ok()
        && read == buf.len()
}
unsafe fn wpm(h: HANDLE, addr: u64, buf: &[u8]) -> bool {
    let mut wrote = 0usize;
    WriteProcessMemory(
        h,
        addr as *const c_void,
        buf.as_ptr() as *const c_void,
        buf.len(),
        Some(&mut wrote),
    )
    .is_ok()
        && wrote == buf.len()
}
unsafe fn rpm_u64(h: HANDLE, addr: u64) -> Option<u64> {
    let mut b = [0u8; 8];
    rpm(h, addr, &mut b).then(|| u64::from_le_bytes(b))
}
unsafe fn rpm_utf16z(h: HANDLE, addr: u64, max_chars: usize) -> Option<String> {
    let mut out = Vec::new();
    let mut a = addr;
    for _ in 0..max_chars {
        let mut b = [0u8; 2];
        if !rpm(h, a, &mut b) {
            break;
        }
        let u = u16::from_le_bytes(b);
        if u == 0 {
            break;
        }
        out.push(u);
        a += 2;
    }
    (!out.is_empty()).then(|| String::from_utf16_lossy(&out))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

unsafe fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = GetClassNameW(h, &mut buf);
    if n > 0 {
        String::from_utf16_lossy(&buf[..n as usize])
    } else {
        String::new()
    }
}

unsafe fn control_text(h: HWND) -> String {
    let len = sm(h, WM_GETTEXTLENGTH, WPARAM(0), LPARAM(0));
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let got = sm(
        h,
        WM_GETTEXT,
        WPARAM(buf.len()),
        LPARAM(buf.as_mut_ptr() as isize),
    );
    if got <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..(got as usize).min(buf.len())])
}



struct FindById {
    class: Vec<u16>,
    id: i32,
    hwnd: isize,
}
unsafe extern "system" fn cb_find_by_id(h: HWND, lp: LPARAM) -> BOOL {
    let ctx = &mut *(lp.0 as *mut FindById);
    let want = String::from_utf16_lossy(&ctx.class[..ctx.class.len().saturating_sub(1)]);
    if class_name(h).eq_ignore_ascii_case(&want) && GetDlgCtrlID(h) == ctx.id {
        ctx.hwnd = h.0;
        return BOOL(0);
    }
    BOOL(1)
}
unsafe fn find_control_by_id(parent: HWND, class: &str, id: i32) -> HWND {
    let mut ctx = FindById {
        class: wide(class),
        id,
        hwnd: 0,
    };
    let _ = EnumChildWindows(
        parent,
        Some(cb_find_by_id),
        LPARAM(&mut ctx as *mut _ as isize),
    );
    HWND(ctx.hwnd)
}

struct FindPlay {
    hwnd: isize,
}
unsafe extern "system" fn cb_find_play(h: HWND, lp: LPARAM) -> BOOL {
    const TOKENS: [&str; 7] = ["再生", "停止", "Play", "Stop", "播放", "暂停", "暫停"];
    let ctx = &mut *(lp.0 as *mut FindPlay);
    if !class_name(h).eq_ignore_ascii_case("Button") {
        return BOOL(1);
    }
    let cap: String = control_text(h)
        .chars()
        .filter(|c| *c != '&' && !c.is_whitespace())
        .collect();
    if !cap.is_empty() && TOKENS.iter().any(|t| cap.contains(t)) {
        ctx.hwnd = h.0;
        return BOOL(0);
    }
    BOOL(1)
}
unsafe fn find_playback_button(main: HWND) -> HWND {
    let mut ctx = FindPlay { hwnd: 0 };
    let _ = EnumChildWindows(
        main,
        Some(cb_find_play),
        LPARAM(&mut ctx as *mut _ as isize),
    );
    HWND(ctx.hwnd)
}

struct FindMain {
    pid: u32,
    hwnd: isize,
}
unsafe extern "system" fn cb_find_main(h: HWND, lp: LPARAM) -> BOOL {
    let ctx = &mut *(lp.0 as *mut FindMain);
    let mut wpid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut wpid));
    if wpid == ctx.pid && IsWindowVisible(h).as_bool() && class_name(h) == "Polygon Movie Maker" {
        ctx.hwnd = h.0;
        return BOOL(0);
    }
    BOOL(1)
}
unsafe fn find_main_window(pid: u32) -> HWND {
    let mut ctx = FindMain { pid, hwnd: 0 };
    let _ = EnumWindows(Some(cb_find_main), LPARAM(&mut ctx as *mut _ as isize));
    if ctx.hwnd == 0 {
        
        unsafe extern "system" fn cb_any(h: HWND, lp: LPARAM) -> BOOL {
            let ctx = &mut *(lp.0 as *mut FindMain);
            let mut wpid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut wpid));
            if wpid == ctx.pid && IsWindowVisible(h).as_bool() {
                ctx.hwnd = h.0;
                return BOOL(0);
            }
            BOOL(1)
        }
        let _ = EnumWindows(Some(cb_any), LPARAM(&mut ctx as *mut _ as isize));
    }
    HWND(ctx.hwnd)
}

struct FindDialog {
    pid: u32,
    hwnd: isize,
}

unsafe extern "system" fn cb_find_dialog(h: HWND, lp: LPARAM) -> BOOL {
    let ctx = &mut *(lp.0 as *mut FindDialog);
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    if pid == ctx.pid && IsWindowVisible(h).as_bool() && class_name(h) == "#32770" {
        ctx.hwnd = h.0;
        return BOOL(0);
    }
    BOOL(1)
}

unsafe fn find_owned_dialog(pid: u32) -> HWND {
    let mut ctx = FindDialog { pid, hwnd: 0 };
    let _ = EnumWindows(Some(cb_find_dialog), LPARAM(&mut ctx as *mut _ as isize));
    HWND(ctx.hwnd)
}

unsafe fn wait_owned_dialog(pid: u32, timeout: Duration) -> HWND {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let dialog = find_owned_dialog(pid);
        if dialog.0 != 0 {
            return dialog;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    HWND(0)
}




unsafe fn edit_text(h: HWND) -> Option<i64> {
    control_text(h).trim().parse::<i64>().ok()
}

unsafe fn set_edit_and_commit(h: HWND, value: i64) {
    send_edit_commit(value, |message, wparam, lparam| {
        sm(h, message, wparam, lparam);
    });
}

fn send_edit_commit(value: i64, mut send: impl FnMut(u32, WPARAM, LPARAM)) {
    let buf = wide(&value.to_string());
    send(WM_SETTEXT, WPARAM(0), LPARAM(buf.as_ptr() as isize));
    let down = 1i64 | (ENTER_SCAN << 16);
    let up = down | (1 << 30) | (1 << 31);
    send(WM_KEYDOWN, WPARAM(VK_RETURN), LPARAM(down as isize));
    send(WM_KEYUP, WPARAM(VK_RETURN), LPARAM(up as isize));
}


unsafe fn set_check(h: HWND, on: bool) {
    if h.0 == 0 {
        return;
    }
    let checked = sm(h, BM_GETCHECK, WPARAM(0), LPARAM(0)) == 1;
    if checked != on {
        sm(h, BM_CLICK, WPARAM(0), LPARAM(0));
    }
}


unsafe fn menu_item_checked(hmenu: windows::Win32::UI::WindowsAndMessaging::HMENU, id: u32) -> Option<bool> {
    let count = GetMenuItemCount(hmenu);
    if count < 0 {
        return None;
    }
    for i in 0..count {
        let sub = GetSubMenu(hmenu, i);
        if sub.0 != 0 {
            if let Some(v) = menu_item_checked(sub, id) {
                return Some(v);
            }
        } else if GetMenuItemID(hmenu, i) == id {
            let st = GetMenuState(hmenu, i as u32, MF_BYPOSITION);
            return Some(st != u32::MAX && (st & MF_CHECKED.0) != 0);
        }
    }
    None
}



unsafe fn set_wav_output(main: HWND, play: bool) {
    if main.0 == 0 {
        return;
    }
    let want_no_play = !play;
    let hmenu = GetMenu(main);
    if hmenu.0 != 0 {
        if let Some(checked) = menu_item_checked(hmenu, MENU_WAV_NO_PLAY) {
            if checked != want_no_play {
                let _ = PostMessageW(
                    main,
                    WM_COMMAND,
                    WPARAM(MENU_WAV_NO_PLAY as usize),
                    LPARAM(0),
                );
            }
            return;
        }
    }
    
    let _ = PostMessageW(main, WM_COMMAND, WPARAM(MENU_WAV_NO_PLAY as usize), LPARAM(0));
}



unsafe fn module_base(h: HANDLE) -> Option<u64> {
    let mut mods = [HMODULE::default(); 256];
    let mut needed = 0u32;
    EnumProcessModules(
        h,
        mods.as_mut_ptr(),
        size_of::<[HMODULE; 256]>() as u32,
        &mut needed,
    )
    .ok()?;
    Some(mods[0].0 as u64)
}


unsafe fn process_image_path(h: HANDLE) -> Option<PathBuf> {
    use windows::Win32::System::ProcessStatus::GetModuleFileNameExW;
    let mut buf = [0u16; 1024];
    let n = GetModuleFileNameExW(h, HMODULE::default(), &mut buf);
    if n == 0 {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..n as usize])))
}


fn find_export_rva(exe: &std::path::Path, name: &str) -> Option<u32> {
    let img = std::fs::read(exe).ok()?;
    let rd_u16 =
        |o: usize| -> Option<u16> { img.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])) };
    let rd_u32 = |o: usize| -> Option<u32> {
        img.get(o..o + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };
    let pe = rd_u32(0x3C)? as usize;
    if rd_u32(pe)? != 0x0000_4550 {
        return None;
    }
    let file_hdr = pe + 4;
    let section_count = rd_u16(file_hdr + 2)? as usize;
    let optional_size = rd_u16(file_hdr + 16)? as usize;
    let optional = file_hdr + 20;
    if rd_u16(optional)? != 0x20B {
        return None; 
    }
    let export_rva = rd_u32(optional + 112)?;
    let export_size = rd_u32(optional + 116)?;
    if export_rva == 0 {
        return None;
    }
    let sections = optional + optional_size;
    let rva_to_off = |rva: u32| -> Option<usize> {
        for i in 0..section_count {
            let s = sections + i * 40;
            let vsize = rd_u32(s + 8)?;
            let vaddr = rd_u32(s + 12)?;
            let rsize = rd_u32(s + 16)?;
            let rptr = rd_u32(s + 20)?;
            let span = vsize.max(rsize);
            if rva >= vaddr && rva < vaddr + span {
                let off = rptr + (rva - vaddr);
                return (off < img.len() as u32).then_some(off as usize);
            }
        }
        (rva < img.len() as u32).then_some(rva as usize)
    };
    let exp = rva_to_off(export_rva)?;
    let num_names = rd_u32(exp + 24)?;
    let addr_funcs = rd_u32(exp + 28)?;
    let addr_names = rd_u32(exp + 32)?;
    let addr_ords = rd_u32(exp + 36)?;
    let names_off = rva_to_off(addr_names)?;
    let ords_off = rva_to_off(addr_ords)?;
    let funcs_off = rva_to_off(addr_funcs)?;
    for i in 0..num_names as usize {
        let name_rva = rd_u32(names_off + i * 4)?;
        let no = rva_to_off(name_rva)?;
        let end = img[no..].iter().position(|&c| c == 0)? + no;
        if &img[no..end] == name.as_bytes() {
            let ord = rd_u16(ords_off + i * 2)? as usize;
            let func_rva = rd_u32(funcs_off + ord * 4)?;
            if func_rva >= export_rva && func_rva < export_rva + export_size {
                return None; 
            }
            return Some(func_rva);
        }
    }
    None
}

fn local_sleep_addr() -> Option<u64> {
    unsafe {
        let k32 = GetModuleHandleW(PCWSTR(wide("kernel32.dll").as_ptr())).ok()?;
        
        let p = GetProcAddress(k32, PCSTR(c"Sleep".as_ptr() as *const u8));
        p.map(|f| f as usize as u64)
    }
}



struct FrameClock {
    state: *mut c_void,
    code: *mut c_void,
    thread: HANDLE,
}



fn build_clock_code(exp_get_frame_time: u64, sleep: u64, state: u64) -> Vec<u8> {
    let mut c: Vec<u8> = Vec::with_capacity(96);
    c.extend_from_slice(&[0x48, 0x83, 0xEC, 0x28]); 
    let loop_start = c.len();
    c.extend_from_slice(&[0x48, 0xB8]); 
    c.extend_from_slice(&exp_get_frame_time.to_le_bytes());
    c.extend_from_slice(&[0xFF, 0xD0]); 
    c.extend_from_slice(&[0x49, 0xBA]); 
    c.extend_from_slice(&state.to_le_bytes());
    c.extend_from_slice(&[0xF3, 0x41, 0x0F, 0x11, 0x42, 0x04]); 
    c.extend_from_slice(&[0xF0, 0x41, 0xFF, 0x02]); 
    c.extend_from_slice(&[0x41, 0x83, 0x7A, 0x08, 0x00]); 
    c.push(0x75); 
    let jne_at = c.len();
    c.push(0x00);
    c.extend_from_slice(&[0xB9, 0x01, 0x00, 0x00, 0x00]); 
    c.extend_from_slice(&[0x48, 0xB8]); 
    c.extend_from_slice(&sleep.to_le_bytes());
    c.extend_from_slice(&[0xFF, 0xD0]); 
    c.push(0xE9); 
    let jmp_at = c.len();
    c.extend_from_slice(&[0, 0, 0, 0]);
    let exit = c.len();
    c.extend_from_slice(&[0x48, 0x83, 0xC4, 0x28]); 
    c.extend_from_slice(&[0x33, 0xC0]); 
    c.push(0xC3); 
    let jne_rel = (exit - (jne_at + 1)) as i8;
    c[jne_at] = jne_rel as u8;
    let jmp_rel = (loop_start as i64 - (jmp_at as i64 + 4)) as i32;
    c[jmp_at..jmp_at + 4].copy_from_slice(&jmp_rel.to_le_bytes());
    c
}

unsafe fn make_clock(h: HANDLE, func: u64, sleep: u64) -> Option<FrameClock> {
    let state = VirtualAllocEx(h, None, CLK_SIZE, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if state.is_null() {
        return None;
    }
    let _ = wpm(h, state as u64, &[0u8; CLK_SIZE]);
    let code_bytes = build_clock_code(func, sleep, state as u64);
    let code = VirtualAllocEx(
        h,
        None,
        code_bytes.len(),
        MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE,
    );
    if code.is_null() {
        let _ = VirtualFreeEx(h, state, 0, MEM_RELEASE);
        return None;
    }
    if !wpm(h, code as u64, &code_bytes) {
        let _ = VirtualFreeEx(h, state, 0, MEM_RELEASE);
        let _ = VirtualFreeEx(h, code, 0, MEM_RELEASE);
        return None;
    }
    let _ = FlushInstructionCache(h, Some(code), code_bytes.len());
    let start: LPTHREAD_START_ROUTINE =
        std::mem::transmute::<usize, LPTHREAD_START_ROUTINE>(code as usize);
    let Ok(thread) = CreateRemoteThread(h, None, 0, start, None, 0, None) else {
        let _ = VirtualFreeEx(h, state, 0, MEM_RELEASE);
        let _ = VirtualFreeEx(h, code, 0, MEM_RELEASE);
        return None;
    };
    Some(FrameClock {
        state,
        code,
        thread,
    })
}

unsafe fn clock_seconds(h: HANDLE, fc: &FrameClock) -> Option<f64> {
    let mut b = [0u8; CLK_SIZE];
    if !rpm(h, fc.state as u64, &mut b) {
        return None;
    }
    let seq = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let secs = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    if seq <= 0 || !secs.is_finite() || !(-1.0..1.0e8).contains(&secs) {
        return None;
    }
    Some(secs as f64)
}

unsafe fn destroy_clock(h: HANDLE, fc: FrameClock) {
    
    let _ = wpm(h, fc.state as u64 + CLK_STOP, &1i32.to_le_bytes());
    WaitForSingleObject(fc.thread, 1000);
    let _ = VirtualFreeEx(h, fc.code, 0, MEM_RELEASE);
    let _ = VirtualFreeEx(h, fc.state, 0, MEM_RELEASE);
    let _ = CloseHandle(fc.thread);
    let _ = CLK_SEQ; 
    let _ = CLK_TIME;
}



pub(super) fn list_processes() -> Vec<MmdProcess> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let mut e = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                let end = e
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(e.szExeFile.len());
                let name = String::from_utf16_lossy(&e.szExeFile[..end]);
                if name.eq_ignore_ascii_case("MikuMikuDance.exe") {
                    out.push(MmdProcess {
                        pid: e.th32ProcessID,
                        label: format!("PID {}", e.th32ProcessID),
                    });
                }
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out.sort_by_key(|p| p.pid);
    out
}



struct Attach {
    h: HANDLE,
    base: u64,
    main: HWND,
    pid: u32,
    play_btn: HWND,
    play_parent: HWND,
    play_id: i32,
    clock: Option<FrameClock>,
    
    h_frame: HWND,
    h_rs: HWND,
    h_re: HWND,
    c_playing: bool,
    c_looping: Option<bool>,
    c_range: (i64, i64),
    c_wav: Option<PathBuf>,
    last_ctrl: Instant,
    last_wav: Instant,
    
    
    prev_frame: f64,
    last_move: Instant,
    
    
    play_override_until: Instant,
    play_override_val: bool,
}

unsafe fn attach(pid: u32) -> Option<Attach> {
    let rights = PROCESS_QUERY_INFORMATION
        | PROCESS_VM_READ
        | PROCESS_VM_WRITE
        | PROCESS_VM_OPERATION
        | PROCESS_CREATE_THREAD;
    let h = OpenProcess(rights, BOOL(0), pid).ok()?;
    let main = find_main_window(pid);
    let base = module_base(h).unwrap_or(0);

    let clock = (|| {
        let exe = process_image_path(h)?;
        let rva = find_export_rva(&exe, "ExpGetFrameTime")?;
        let sleep = local_sleep_addr()?;
        make_clock(h, base + rva as u64, sleep)
    })();

    let play_btn = if main.0 != 0 {
        find_playback_button(main)
    } else {
        HWND(0)
    };
    let (play_parent, play_id) = if play_btn.0 != 0 {
        (GetParent(play_btn), GetDlgCtrlID(play_btn))
    } else {
        (HWND(0), 0)
    };

    
    let (h_frame, h_rs, h_re) = if main.0 != 0 {
        (
            find_control_by_id(main, "Edit", ID_FRAME),
            find_control_by_id(main, "Edit", ID_RANGE_START),
            find_control_by_id(main, "Edit", ID_RANGE_END),
        )
    } else {
        (HWND(0), HWND(0), HWND(0))
    };

    log::info!(
        "MMD アタッチ pid={pid} base={base:#x} clock={} play_btn={:#x} id={play_id}",
        clock.is_some(),
        play_btn.0
    );
    let mut at = Attach {
        h,
        base,
        main,
        pid,
        play_btn,
        play_parent,
        play_id,
        clock,
        h_frame,
        h_rs,
        h_re,
        c_playing: false,
        c_looping: None,
        c_range: (0, 0),
        c_wav: None,
        last_ctrl: Instant::now() - Duration::from_secs(1),
        last_wav: Instant::now() - Duration::from_secs(10),
        prev_frame: f64::NAN,
        last_move: Instant::now() - Duration::from_secs(1),
        play_override_until: Instant::now() - Duration::from_secs(1),
        play_override_val: false,
    };
    at.refresh_controls();
    at.refresh_wav();
    Some(at)
}

unsafe fn detach(at: Attach) {
    if let Some(fc) = at.clock {
        destroy_clock(at.h, fc);
    }
    let _ = CloseHandle(at.h);
}

impl Attach {
    unsafe fn set_looping(&mut self, on: bool) -> bool {
        let button = find_control_by_id(self.main, "Button", ID_LOOP);
        if button.0 == 0 {
            return false;
        }
        set_check(button, on);
        self.refresh_controls();
        self.c_looping == Some(on)
    }

    unsafe fn playback_range(&mut self) -> Option<PlaybackRange> {
        self.revalidate();
        if self.h_rs.0 == 0 || self.h_re.0 == 0 {
            return None;
        }
        let range = PlaybackRange {
            start: edit_text(self.h_rs)?,
            end: edit_text(self.h_re)?,
        };
        range.is_valid().then_some(range)
    }

    unsafe fn set_playback_range(&mut self, range: PlaybackRange) -> bool {
        if !range.is_valid() {
            return false;
        }
        self.revalidate();
        if self.h_rs.0 == 0 || self.h_re.0 == 0 {
            return false;
        }
        set_edit_and_commit(self.h_rs, range.start);
        set_edit_and_commit(self.h_re, range.end);
        self.refresh_controls();
        self.playback_range() == Some(range)
    }

    unsafe fn import_wav(&self, path: &Path) -> bool {
        if self.main.0 == 0 || !IsWindow(self.main).as_bool() {
            return false;
        }
        if PostMessageW(self.main, WM_COMMAND, WPARAM(MENU_WAV_LOAD), LPARAM(0)).is_err() {
            return false;
        }
        let dialog = wait_owned_dialog(self.pid, Duration::from_secs(3));
        if dialog.0 == 0 {
            return false;
        }
        let edit = find_control_by_id(dialog, "Edit", OPEN_DIALOG_FILENAME_EDIT);
        let ok = find_control_by_id(dialog, "Button", OPEN_DIALOG_OK_BUTTON);
        if edit.0 == 0 || ok.0 == 0 {
            return false;
        }
        let text = wide_path(path);
        if sm(edit, WM_SETTEXT, WPARAM(0), LPARAM(text.as_ptr() as isize)) == 0 {
            return false;
        }
        sm(ok, BM_CLICK, WPARAM(0), LPARAM(0));
        true
    }

    unsafe fn toggle_playback(&self) -> bool {
        if self.play_btn.0 != 0 && IsWindow(self.play_btn).as_bool() {
            if self.play_parent.0 != 0 && self.play_id != 0 {
                sm(
                    self.play_parent,
                    WM_COMMAND,
                    WPARAM((self.play_id as usize) & 0xFFFF),
                    LPARAM(self.play_btn.0),
                );
            } else {
                sm(self.play_btn, BM_CLICK, WPARAM(0), LPARAM(0));
            }
            return true;
        }
        
        if self.main.0 != 0 {
            let down = 1i64 | (0x19 << 16); 
            let up = down | (1 << 30) | (1 << 31);
            let _ = PostMessageW(self.main, WM_KEYDOWN, WPARAM(0x50), LPARAM(down as isize));
            let _ = PostMessageW(self.main, WM_KEYUP, WPARAM(0x50), LPARAM(up as isize));
            return true;
        }
        false
    }

    unsafe fn set_current_frame(&self, frame: i64) -> bool {
        if self.main.0 == 0 {
            return false;
        }
        let e = find_control_by_id(self.main, "Edit", ID_FRAME);
        if e.0 == 0 {
            return false;
        }
        set_edit_and_commit(e, frame.max(0));
        true
    }

    
    unsafe fn set_frame_stop(&self, on: bool) {
        if self.main.0 != 0 {
            set_check(find_control_by_id(self.main, "Button", ID_FRAME_STOP_TOGGLE), on);
        }
    }

    
    unsafe fn set_wav_output(&self, on: bool) {
        set_wav_output(self.main, on);
    }

    
    
    unsafe fn normalize_playback(&self) {
        if self.main.0 == 0 {
            return;
        }
        
        let loop_btn = find_control_by_id(self.main, "Button", ID_LOOP);
        if loop_btn.0 != 0 && sm(loop_btn, BM_GETCHECK, WPARAM(0), LPARAM(0)) == 1 {
            sm(loop_btn, BM_CLICK, WPARAM(0), LPARAM(0));
        }
        
        set_wav_output(self.main, false);
        
        for id in [ID_FRAME_START_TOGGLE, ID_FRAME_STOP_TOGGLE] {
            let b = find_control_by_id(self.main, "Button", id);
            if b.0 != 0 && sm(b, BM_GETCHECK, WPARAM(0), LPARAM(0)) != 1 {
                sm(b, BM_CLICK, WPARAM(0), LPARAM(0));
            }
        }
        
        let hs = find_control_by_id(self.main, "Edit", ID_RANGE_START);
        if hs.0 != 0 {
            set_edit_and_commit(hs, 0);
        }
        let he = find_control_by_id(self.main, "Edit", ID_RANGE_END);
        if he.0 != 0 {
            set_edit_and_commit(he, -1);
        }
    }

    
    unsafe fn revalidate(&mut self) {
        let m = self.main;
        if m.0 == 0 {
            return;
        }
        if self.h_frame.0 == 0 || !IsWindow(self.h_frame).as_bool() {
            self.h_frame = find_control_by_id(m, "Edit", ID_FRAME);
        }
        if self.h_rs.0 == 0 || !IsWindow(self.h_rs).as_bool() {
            self.h_rs = find_control_by_id(m, "Edit", ID_RANGE_START);
        }
        if self.h_re.0 == 0 || !IsWindow(self.h_re).as_bool() {
            self.h_re = find_control_by_id(m, "Edit", ID_RANGE_END);
        }
        if self.play_btn.0 == 0 || !IsWindow(self.play_btn).as_bool() {
            self.play_btn = find_playback_button(m);
            if self.play_btn.0 != 0 {
                self.play_parent = GetParent(self.play_btn);
                self.play_id = GetDlgCtrlID(self.play_btn);
            }
        }
    }

    
    unsafe fn refresh_controls(&mut self) {
        self.last_ctrl = Instant::now();
        self.revalidate();
        let hplay = if self.play_btn.0 != 0 {
            self.play_btn
        } else {
            find_control_by_id(self.main, "Button", ID_PLAY)
        };
        self.c_playing = hplay.0 != 0 && sm(hplay, BM_GETCHECK, WPARAM(0), LPARAM(0)) == 1;
        let loop_button = find_control_by_id(self.main, "Button", ID_LOOP);
        self.c_looping = (loop_button.0 != 0)
            .then(|| sm(loop_button, BM_GETCHECK, WPARAM(0), LPARAM(0)) == 1);
        self.c_range = (
            edit_text(self.h_rs).unwrap_or(self.c_range.0),
            edit_text(self.h_re).unwrap_or(self.c_range.1),
        );
    }

    
    unsafe fn refresh_wav(&mut self) {
        self.last_wav = Instant::now();
        self.c_wav = if self.base != 0 {
            rpm_u64(self.h, self.base + WAV_PTR_OFF)
                .filter(|p| *p != 0)
                .and_then(|p| rpm_utf16z(self.h, p + WAV_STR_OFF, 1024))
                .map(PathBuf::from)
                .filter(|p| {
                    p.extension()
                        .map(|e| e.eq_ignore_ascii_case("wav"))
                        .unwrap_or(false)
                        && p.exists()
                })
        } else {
            None
        };
    }

    
    unsafe fn frame_now(&self) -> f64 {
        if let Some(fc) = &self.clock {
            if let Some(s) = clock_seconds(self.h, fc) {
                return s * 30.0;
            }
        }
        edit_text(self.h_frame).unwrap_or(0) as f64
    }

    
    
    fn note_motion(&mut self, frame: f64) {
        let fi = frame.round();
        if self.prev_frame.is_nan() || (fi - self.prev_frame.round()).abs() >= 1.0 {
            self.last_move = Instant::now();
        }
        self.prev_frame = frame;
    }

    fn snapshot(&self, frame: f64) -> MmdState {
        
        
        
        
        
        let playing = if Instant::now() < self.play_override_until {
            self.play_override_val
        } else {
            !self.prev_frame.is_nan() && self.last_move.elapsed() < Duration::from_millis(200)
        };
        MmdState {
            attached: true,
            frame,
            playing,
            looping: self.c_looping,
            wav_path: self.c_wav.clone(),
        }
    }
}



pub(super) fn poll_loop(shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    let mut cur: Option<Attach> = None;
    let mut tick: u32 = 0;

    while !stop.load(Ordering::Relaxed) {
        tick = tick.wrapping_add(1);
        let want = shared.target_pid.load(Ordering::Relaxed);

        unsafe {
            
            let need_switch = match &cur {
                Some(at) => at.pid != want || !IsWindow(at.main).as_bool(),
                None => want != 0,
            };
            if need_switch {
                if let Some(old) = cur.take() {
                    detach(old);
                }
                if want != 0 {
                    cur = attach(want);
                    if cur.is_none() {
                        if let Ok(mut st) = shared.state.lock() {
                            *st = MmdState::default();
                        }
                    }
                } else if let Ok(mut st) = shared.state.lock() {
                    *st = MmdState::default();
                }
            }

            let range_read = shared.pending_range_read.lock().ok().and_then(|mut g| g.take());
            let range_write = shared.pending_range_write.lock().ok().and_then(|mut g| g.take());
            let loop_write = shared.pending_loop.lock().ok().and_then(|mut g| g.take());
            if let Some(at) = cur.as_mut() {
                
                
                if shared.pending_stop.swap(false, Ordering::Relaxed) {
                    let advancing = !at.prev_frame.is_nan()
                        && at.last_move.elapsed() < Duration::from_millis(200);
                    if advancing {
                        
                        at.toggle_playback();
                        at.refresh_controls();
                        at.prev_frame = f64::NAN;
                        at.last_move = Instant::now() - Duration::from_secs(1);
                    }
                    
                    at.play_override_val = false;
                    at.play_override_until = Instant::now() + Duration::from_millis(300);
                }
                let pf = shared.pending_frame.swap(-1, Ordering::Relaxed);
                if pf >= 0 && at.set_current_frame(pf) {
                    
                    at.prev_frame = pf as f64;
                    at.last_move = Instant::now() - Duration::from_secs(1);
                }
                if shared.pending_toggle.swap(false, Ordering::Relaxed) {
                    let was_playing = !at.prev_frame.is_nan()
                        && at.last_move.elapsed() < Duration::from_millis(200);
                    at.toggle_playback();
                    at.refresh_controls(); 
                    
                    at.play_override_val = !was_playing;
                    at.play_override_until = Instant::now() + Duration::from_millis(250);
                    if !was_playing {
                        
                        at.last_move = Instant::now();
                    }
                }
                if shared.pending_normalize.swap(false, Ordering::Relaxed) {
                    at.normalize_playback();
                    at.refresh_controls();
                }
                if let Some((pid, on)) = loop_write {
                    if pid == at.pid && pid == shared.target_pid.load(Ordering::Relaxed)
                        && !at.set_looping(on)
                    {
                        log::error!("MMDのループ切替に失敗");
                    }
                }
                if let Some(on) = shared.pending_frame_stop.lock().ok().and_then(|mut g| g.take()) {
                    at.set_frame_stop(on);
                }
                if let Some(on) = shared.pending_wav_output.lock().ok().and_then(|mut g| g.take()) {
                    at.set_wav_output(on);
                }
                if let Some(path) = shared.pending_import.lock().ok().and_then(|mut g| g.take()) {
                    if at.import_wav(&path) {
                        at.refresh_wav();
                    } else {
                        log::error!("MMDへのWAV反映に失敗: {}", path.display());
                    }
                }
                if let Some((pid, range)) = range_write {
                    if pid == at.pid && pid == shared.target_pid.load(Ordering::Relaxed) {
                        if at.set_playback_range(range) {
                            log::info!("MMDへ再生範囲を送信: {} - {}", range.start, range.end);
                        } else {
                            log::error!("MMDへの再生範囲の適用に失敗");
                        }
                    }
                }
                if let Some(pid) = range_read {
                    if pid == at.pid && pid == shared.target_pid.load(Ordering::Relaxed) {
                        if let Some(range) = at.playback_range() {
                            *shared.range_result.lock().unwrap() = Some((pid, range));
                        } else {
                            log::error!("MMDの再生範囲を取得できませんでした");
                        }
                    }
                }

                
                let frame = at.frame_now();
                at.note_motion(frame);
                if at.last_ctrl.elapsed() >= Duration::from_millis(120) {
                    at.refresh_controls();
                }
                if at.last_wav.elapsed() >= Duration::from_millis(1000) {
                    at.refresh_wav();
                }
                if tick.is_multiple_of(120) {
                    log::debug!(
                        "MMD pid={} F={frame:.1} play={} clk={}",
                        at.pid,
                        at.c_playing,
                        at.clock.is_some()
                    );
                }

                let snap = at.snapshot(frame);
                if let Ok(mut st) = shared.state.lock() {
                    *st = snap;
                }
            }
        }

        
        std::thread::sleep(Duration::from_millis(6));
    }

    if let Some(old) = cur {
        unsafe { detach(old) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_commit_sends_value_then_enter_without_character_input() {
        for value in [0, 1, 30, 900, i32::MAX as i64, -1] {
            let expected = wide(&value.to_string());
            let mut messages = Vec::new();
            let mut actual = Vec::new();
            send_edit_commit(value, |message, wparam, lparam| {
                if message == WM_SETTEXT {
                    actual = unsafe {
                        std::slice::from_raw_parts(lparam.0 as *const u16, expected.len()).to_vec()
                    };
                    messages.push((message, wparam.0, 0));
                } else {
                    messages.push((message, wparam.0, lparam.0));
                }
            });
            assert_eq!(actual, expected);
            assert_eq!(messages, vec![
                (WM_SETTEXT, 0, 0),
                (WM_KEYDOWN, VK_RETURN, 0x001c0001),
                (WM_KEYUP, VK_RETURN, 0xc01c0001),
            ]);
        }
    }
}
