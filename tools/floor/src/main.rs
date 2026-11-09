//! A bare window with no toolkit, to measure what Windows itself costs. `floor gdi` paints text with GDI;
//! `floor gl` makes an OpenGL context (the same path egui's glow backend takes) and clears it once.
#![allow(non_snake_case, non_camel_case_types)]
use std::ffi::c_void;
type H = *mut c_void;

#[repr(C)] struct WNDCLASSW { style: u32, proc_: unsafe extern "system" fn(H, u32, usize, isize) -> isize, cls: i32, wnd: i32, inst: H, icon: H, cursor: H, bg: H, menu: *const u16, name: *const u16 }
#[repr(C)] struct MSG { hwnd: H, msg: u32, w: usize, l: isize, time: u32, x: i32, y: i32, p: u32 }
#[repr(C)] struct PAINTSTRUCT { hdc: H, erase: i32, rc: [i32; 4], restore: i32, inc: i32, rgb: [u8; 32] }
#[repr(C)] struct PFD { size: u16, ver: u16, flags: u32, ptype: u8, color: u8, b: [u8; 13], depth: u8, stencil: u8, aux: u8, layer: u8, res: u8, lm: u32, vm: u32, dm: u32 }

#[link(name = "user32")] extern "system" {
    fn RegisterClassW(c: *const WNDCLASSW) -> u16;
    fn CreateWindowExW(ex: u32, cls: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: H, menu: H, inst: H, p: *mut c_void) -> H;
    fn DefWindowProcW(h: H, m: u32, w: usize, l: isize) -> isize;
    fn GetMessageW(m: *mut MSG, h: H, a: u32, b: u32) -> i32;
    fn TranslateMessage(m: *const MSG) -> i32;
    fn DispatchMessageW(m: *const MSG) -> isize;
    fn ShowWindow(h: H, c: i32) -> i32;
    fn PostQuitMessage(c: i32);
    fn BeginPaint(h: H, p: *mut PAINTSTRUCT) -> H;
    fn EndPaint(h: H, p: *const PAINTSTRUCT) -> i32;
    fn GetDC(h: H) -> H;
    fn LoadCursorW(i: H, n: usize) -> H;
}
#[link(name = "gdi32")] extern "system" {
    fn TextOutW(dc: H, x: i32, y: i32, s: *const u16, n: i32) -> i32;
    fn ChoosePixelFormat(dc: H, p: *const PFD) -> i32;
    fn SetPixelFormat(dc: H, f: i32, p: *const PFD) -> i32;
    fn SwapBuffers(dc: H) -> i32;
}
#[link(name = "opengl32")] extern "system" {
    fn wglCreateContext(dc: H) -> H;
    fn wglMakeCurrent(dc: H, rc: H) -> i32;
    fn glClearColor(r: f32, g: f32, b: f32, a: f32);
    fn glClear(m: u32);
}
#[link(name = "kernel32")] extern "system" { fn GetModuleHandleW(n: *const u16) -> H; }

static mut GL: bool = false;
static mut DC: H = std::ptr::null_mut();

unsafe extern "system" fn wndproc(h: H, m: u32, w: usize, l: isize) -> isize {
    match m {
        0x000F => { // WM_PAINT
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(h, &mut ps);
            if GL {
                glClearColor(0.07, 0.07, 0.08, 1.0);
                glClear(0x4000);
                SwapBuffers(DC);
            } else {
                let t: Vec<u16> = "floor: bare GDI window".encode_utf16().collect();
                TextOutW(dc, 20, 20, t.as_ptr(), t.len() as i32);
            }
            EndPaint(h, &ps);
            0
        }
        0x0002 => { PostQuitMessage(0); 0 }
        _ => DefWindowProcW(h, m, w, l),
    }
}

fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() }

fn main() {
    unsafe {
        GL = std::env::args().nth(1).as_deref() == Some("gl");
        let inst = GetModuleHandleW(std::ptr::null());
        let name = wide("floorwnd");
        let wc = WNDCLASSW { style: 0x3, proc_: wndproc, cls: 0, wnd: 0, inst, icon: std::ptr::null_mut(), cursor: LoadCursorW(std::ptr::null_mut(), 32512), bg: 6 as H, menu: std::ptr::null(), name: name.as_ptr() };
        RegisterClassW(&wc);
        let h = CreateWindowExW(0, name.as_ptr(), wide("floor").as_ptr(), 0x10CF0000, 100, 100, 1000, 700, std::ptr::null_mut(), std::ptr::null_mut(), inst, std::ptr::null_mut());
        if GL {
            DC = GetDC(h);
            let pfd = PFD { size: 40, ver: 1, flags: 0x25, ptype: 0, color: 32, b: [0; 13], depth: 24, stencil: 8, aux: 0, layer: 0, res: 0, lm: 0, vm: 0, dm: 0 };
            let f = ChoosePixelFormat(DC, &pfd);
            SetPixelFormat(DC, f, &pfd);
            let rc = wglCreateContext(DC);
            wglMakeCurrent(DC, rc);
        }
        ShowWindow(h, 5);
        let mut m: MSG = std::mem::zeroed();
        while GetMessageW(&mut m, std::ptr::null_mut(), 0, 0) > 0 { TranslateMessage(&m); DispatchMessageW(&m); }
    }
}
