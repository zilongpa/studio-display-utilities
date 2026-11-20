//! The handful of Win32 declarations this program needs. Everything links against libraries that
//! ship with the Windows SDK, so there are no crate dependencies.
#![allow(non_snake_case, non_camel_case_types, dead_code, clippy::upper_case_acronyms)]

use core::ffi::c_void;

pub type HANDLE = *mut c_void;
pub type HWND = *mut c_void;

#[repr(C)]
#[allow(non_snake_case)]
pub struct SHELLEXECUTEINFOW {
    pub cbSize: u32,
    pub fMask: u32,
    pub hwnd: HWND,
    pub lpVerb: *const u16,
    pub lpFile: *const u16,
    pub lpParameters: *const u16,
    pub lpDirectory: *const u16,
    pub nShow: i32,
    pub hInstApp: HANDLE,
    pub lpIDList: *mut c_void,
    pub lpClass: *const u16,
    pub hkeyClass: HANDLE,
    pub dwHotKey: u32,
    pub hIconOrMonitor: HANDLE,
    pub hProcess: HANDLE,
}
pub type WNDPROC = unsafe extern "system" fn(HWND, u32, usize, isize) -> isize;

pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
pub const HKEY_LOCAL_MACHINE: HANDLE = 0x8000_0002u32 as i32 as isize as HANDLE;
pub const HKEY_CURRENT_USER: HANDLE = 0x8000_0001u32 as i32 as isize as HANDLE;
pub const KEY_READ: u32 = 0x20019;
pub const KEY_SET_VALUE: u32 = 2;
pub const REG_SZ: u32 = 1;
pub const REG_MULTI_SZ: u32 = 7;

pub const GENERIC_READ: u32 = 0x8000_0000;
pub const GENERIC_WRITE: u32 = 0x4000_0000;
pub const FILE_SHARE_READ_WRITE: u32 = 3;
pub const OPEN_EXISTING: u32 = 3;
pub const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
pub const ERROR_SEM_TIMEOUT: u32 = 121;
pub const ERROR_ALREADY_EXISTS: u32 = 183;

pub const WM_DESTROY: u32 = 0x0002;
pub const WM_PAINT: u32 = 0x000F;
pub const WM_ERASEBKGND: u32 = 0x0014;
pub const WM_DISPLAYCHANGE: u32 = 0x007E;
pub const WM_INPUT: u32 = 0x00FF;
pub const WM_TIMER: u32 = 0x0113;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_POWERBROADCAST: u32 = 0x0218;
pub const WM_DEVICECHANGE: u32 = 0x0219;
pub const WM_HOTKEY: u32 = 0x0312;
pub const WM_APP: u32 = 0x8000;
pub const PBT_APMRESUMEAUTOMATIC: usize = 0x12;
pub const PBT_POWERSETTINGCHANGE: usize = 0x8013;
pub const DBT_DEVNODES_CHANGED: usize = 7;

pub const WS_POPUP: u32 = 0x8000_0000;
pub const WS_EX_TOPMOST: u32 = 0x8;
pub const WS_EX_TRANSPARENT: u32 = 0x20;
pub const WS_EX_TOOLWINDOW: u32 = 0x80;
pub const WS_EX_LAYERED: u32 = 0x80000;
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
pub const SW_HIDE: i32 = 0;
pub const SW_SHOWNOACTIVATE: i32 = 4;
pub const HWND_TOPMOST: HWND = -1isize as HWND;
pub const SWP_NOACTIVATE: u32 = 0x10;
pub const LWA_ALPHA: u32 = 2;
pub const SPI_GETWORKAREA: u32 = 0x30;
pub const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
pub const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;

pub const VK_F14: u32 = 0x7D;
pub const VK_F15: u32 = 0x7E;
pub const RIDEV_INPUTSINK: u32 = 0x100;
pub const RID_INPUT: u32 = 0x1000_0003;
pub const RIDI_PREPARSEDDATA: u32 = 0x2000_0005;
pub const RIM_TYPEHID: u32 = 2;
pub const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;

pub const NIM_ADD: u32 = 0;
pub const NIM_MODIFY: u32 = 1;
pub const NIM_DELETE: u32 = 2;
pub const NIF_MESSAGE: u32 = 1;
pub const NIF_ICON: u32 = 2;
pub const NIF_TIP: u32 = 4;
pub const MF_STRING: u32 = 0;
pub const MF_GRAYED: u32 = 1;
pub const MF_CHECKED: u32 = 8;
pub const MF_POPUP: u32 = 0x10;
pub const MF_SEPARATOR: u32 = 0x800;
pub const TPM_RIGHTBUTTON: u32 = 2;
pub const TPM_RETURNCMD: u32 = 0x100;

pub const TRANSPARENT: i32 = 1;
pub const DT_LEFT: u32 = 0;
pub const DT_RIGHT: u32 = 2;
pub const DT_SINGLELINE: u32 = 0x20;
pub const DT_NOPREFIX: u32 = 0x800;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[repr(C)]
pub struct WNDCLASSW {
    pub style: u32,
    pub lpfnWndProc: Option<WNDPROC>,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HANDLE,
    pub hIcon: HANDLE,
    pub hCursor: HANDLE,
    pub hbrBackground: HANDLE,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: usize,
    pub lParam: isize,
    pub time: u32,
    pub pt: POINT,
}

#[repr(C)]
pub struct PAINTSTRUCT {
    pub hdc: HANDLE,
    pub fErase: i32,
    pub rcPaint: RECT,
    pub fRestore: i32,
    pub fIncUpdate: i32,
    pub rgbReserved: [u8; 32],
}

#[repr(C)]
pub struct NOTIFYICONDATAW {
    pub cbSize: u32,
    pub hWnd: HWND,
    pub uID: u32,
    pub uFlags: u32,
    pub uCallbackMessage: u32,
    pub hIcon: HANDLE,
    pub szTip: [u16; 128],
    pub dwState: u32,
    pub dwStateMask: u32,
    pub szInfo: [u16; 256],
    pub uVersion: u32,
    pub szInfoTitle: [u16; 64],
    pub dwInfoFlags: u32,
    pub guidItem: [u8; 16],
    pub hBalloonIcon: HANDLE,
}

#[repr(C)]
pub struct ICONINFO {
    pub fIcon: i32,
    pub xHotspot: u32,
    pub yHotspot: u32,
    pub hbmMask: HANDLE,
    pub hbmColor: HANDLE,
}

#[repr(C)]
pub struct BITMAPINFOHEADER {
    pub biSize: u32,
    pub biWidth: i32,
    pub biHeight: i32,
    pub biPlanes: u16,
    pub biBitCount: u16,
    pub biCompression: u32,
    pub biSizeImage: u32,
    pub biXPelsPerMeter: i32,
    pub biYPelsPerMeter: i32,
    pub biClrUsed: u32,
    pub biClrImportant: u32,
}

#[repr(C)]
pub struct RAWINPUTDEVICE {
    pub usUsagePage: u16,
    pub usUsage: u16,
    pub dwFlags: u32,
    pub hwndTarget: HWND,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct USAGE_AND_PAGE {
    pub usage: u16,
    pub page: u16,
}

#[repr(C)]
#[derive(Default)]
pub struct WINUSB_PIPE_INFORMATION {
    pub pipe_type: i32,
    pub pipe_id: u8,
    pub maximum_packet_size: u16,
    pub interval: u8,
}

#[repr(C)]
#[derive(Default)]
pub struct SYSTEMTIME {
    pub year: u16,
    pub month: u16,
    pub day_of_week: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub milliseconds: u16,
}

#[link(name = "kernel32")]
extern "system" {
    pub fn CreateFileW(name: *const u16, access: u32, share: u32, security: *const c_void, disposition: u32, flags: u32, template: HANDLE) -> HANDLE;
    pub fn CloseHandle(handle: HANDLE) -> i32;
    pub fn GetCurrentProcess() -> HANDLE;
    pub fn WaitForSingleObject(handle: HANDLE, milliseconds: u32) -> u32;
    pub fn GetExitCodeProcess(process: HANDLE, code: *mut u32) -> i32;
    pub fn GetLastError() -> u32;
    pub fn CreateMutexW(security: *const c_void, initial_owner: i32, name: *const u16) -> HANDLE;
    pub fn GetModuleHandleW(name: *const u16) -> HANDLE;
    pub fn GetLocalTime(time: *mut SYSTEMTIME);
    pub fn LocalFree(memory: *mut c_void) -> *mut c_void;
    pub fn LoadLibraryW(name: *const u16) -> HANDLE;
    pub fn GetProcAddress(module: HANDLE, name: *const u8) -> *mut c_void;
    pub fn FreeLibrary(module: HANDLE) -> i32;
    pub fn AttachConsole(process: u32) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn RegOpenKeyExW(key: HANDLE, sub_key: *const u16, options: u32, access: u32, result: *mut HANDLE) -> i32;
    pub fn RegEnumKeyExW(key: HANDLE, index: u32, name: *mut u16, name_len: *mut u32, reserved: *mut u32, class: *mut u16, class_len: *mut u32, last_write: *mut c_void) -> i32;
    pub fn RegQueryValueExW(key: HANDLE, value: *const u16, reserved: *mut u32, kind: *mut u32, data: *mut u8, data_len: *mut u32) -> i32;
    pub fn RegCloseKey(key: HANDLE) -> i32;
    pub fn OpenProcessToken(process: HANDLE, access: u32, token: *mut HANDLE) -> i32;
    pub fn GetTokenInformation(token: HANDLE, class: u32, information: *mut c_void, length: u32, needed: *mut u32) -> i32;
    pub fn RegSetValueExW(key: HANDLE, value: *const u16, reserved: u32, kind: u32, data: *const u8, data_len: u32) -> i32;
    pub fn RegDeleteValueW(key: HANDLE, value: *const u16) -> i32;
}

#[link(name = "user32")]
extern "system" {
    pub fn RegisterClassW(class: *const WNDCLASSW) -> u16;
    pub fn CreateWindowExW(ex_style: u32, class: *const u16, name: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: HWND, menu: HANDLE, instance: HANDLE, param: *mut c_void) -> HWND;
    pub fn DefWindowProcW(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize;
    pub fn DestroyWindow(hwnd: HWND) -> i32;
    pub fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
    pub fn TranslateMessage(msg: *const MSG) -> i32;
    pub fn DispatchMessageW(msg: *const MSG) -> isize;
    pub fn PostMessageW(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> i32;
    pub fn PostQuitMessage(code: i32);
    pub fn RegisterWindowMessageW(name: *const u16) -> u32;
    pub fn SetTimer(hwnd: HWND, id: usize, ms: u32, callback: *const c_void) -> usize;
    pub fn KillTimer(hwnd: HWND, id: usize) -> i32;
    pub fn RegisterHotKey(hwnd: HWND, id: i32, modifiers: u32, vk: u32) -> i32;
    pub fn UnregisterHotKey(hwnd: HWND, id: i32) -> i32;
    pub fn RegisterRawInputDevices(devices: *const RAWINPUTDEVICE, count: u32, size: u32) -> i32;
    pub fn GetRawInputData(raw_input: isize, command: u32, data: *mut u8, size: *mut u32, header_size: u32) -> u32;
    pub fn GetRawInputDeviceInfoW(device: HANDLE, command: u32, data: *mut u8, size: *mut u32) -> u32;
    pub fn CreatePopupMenu() -> HANDLE;
    pub fn GetDC(hwnd: HWND) -> HANDLE;
    pub fn EnumDisplaySettingsExW(device: *const u16, mode_number: u32, mode: *mut u8, flags: u32) -> i32;
    pub fn ChangeDisplaySettingsExW(device: *const u16, mode: *const u8, hwnd: HWND, flags: u32, param: *mut c_void) -> i32;
    pub fn ReleaseDC(hwnd: HWND, dc: HANDLE) -> i32;
    pub fn AppendMenuW(menu: HANDLE, flags: u32, id: usize, text: *const u16) -> i32;
    pub fn TrackPopupMenu(menu: HANDLE, flags: u32, x: i32, y: i32, reserved: i32, hwnd: HWND, rect: *const RECT) -> i32;
    pub fn DestroyMenu(menu: HANDLE) -> i32;
    pub fn GetCursorPos(point: *mut POINT) -> i32;
    pub fn SetForegroundWindow(hwnd: HWND) -> i32;
    pub fn ShowWindow(hwnd: HWND, command: i32) -> i32;
    pub fn IsWindowVisible(hwnd: HWND) -> i32;
    pub fn SetWindowPos(hwnd: HWND, after: HWND, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    pub fn SetLayeredWindowAttributes(hwnd: HWND, color_key: u32, alpha: u8, flags: u32) -> i32;
    pub fn InvalidateRect(hwnd: HWND, rect: *const RECT, erase: i32) -> i32;
    pub fn GetClientRect(hwnd: HWND, rect: *mut RECT) -> i32;
    pub fn BeginPaint(hwnd: HWND, paint: *mut PAINTSTRUCT) -> HANDLE;
    pub fn EndPaint(hwnd: HWND, paint: *const PAINTSTRUCT) -> i32;
    pub fn FillRect(dc: HANDLE, rect: *const RECT, brush: HANDLE) -> i32;
    pub fn DrawTextW(dc: HANDLE, text: *const u16, len: i32, rect: *mut RECT, format: u32) -> i32;
    pub fn SystemParametersInfoW(action: u32, param: u32, data: *mut c_void, win_ini: u32) -> i32;
    pub fn GetDpiForSystem() -> u32;
    pub fn SetProcessDpiAwarenessContext(context: isize) -> i32;
    pub fn CreateIconIndirect(info: *const ICONINFO) -> HANDLE;
    pub fn RegisterPowerSettingNotification(recipient: HANDLE, setting: *const GUID, flags: u32) -> HANDLE;

    pub fn GetDisplayConfigBufferSizes(flags: u32, paths: *mut u32, modes: *mut u32) -> i32;
    pub fn QueryDisplayConfig(flags: u32, num_paths: *mut u32, paths: *mut u8, num_modes: *mut u32, modes: *mut u8, topology: *mut c_void) -> i32;
    pub fn DisplayConfigGetDeviceInfo(packet: *mut u8) -> i32;
    pub fn DisplayConfigSetDeviceInfo(packet: *const u8) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    pub fn CreateSolidBrush(color: u32) -> HANDLE;
    pub fn CreateDCW(driver: *const u16, device: *const u16, port: *const u16, mode: *const c_void) -> HANDLE;
    pub fn DeleteDC(dc: HANDLE) -> i32;
    pub fn SetDeviceGammaRamp(dc: HANDLE, ramp: *const u16) -> i32;
    pub fn GetDeviceGammaRamp(dc: HANDLE, ramp: *mut u16) -> i32;
    pub fn DeleteObject(object: HANDLE) -> i32;
    pub fn SelectObject(dc: HANDLE, object: HANDLE) -> HANDLE;
    pub fn SetTextColor(dc: HANDLE, color: u32) -> u32;
    pub fn SetBkMode(dc: HANDLE, mode: i32) -> i32;
    pub fn CreateFontW(height: i32, width: i32, escapement: i32, orientation: i32, weight: i32, italic: u32, underline: u32, strike_out: u32, charset: u32, out_precision: u32, clip_precision: u32, quality: u32, pitch_and_family: u32, face: *const u16) -> HANDLE;
    pub fn CreateDIBSection(dc: HANDLE, info: *const BITMAPINFOHEADER, usage: u32, bits: *mut *mut c_void, section: HANDLE, offset: u32) -> HANDLE;
    pub fn CreateBitmap(width: i32, height: i32, planes: u32, bits_per_pixel: u32, bits: *const c_void) -> HANDLE;
}

#[link(name = "shell32")]
extern "system" {
    pub fn Shell_NotifyIconW(message: u32, data: *const NOTIFYICONDATAW) -> i32;
    pub fn ShellExecuteExW(info: *mut SHELLEXECUTEINFOW) -> i32;
}

#[link(name = "powrprof")]
extern "system" {
    pub fn PowerGetActiveScheme(root: HANDLE, scheme: *mut *mut GUID) -> u32;
    pub fn PowerReadACValueIndex(root: HANDLE, scheme: *const GUID, sub_group: *const GUID, setting: *const GUID, value: *mut u32) -> u32;
    pub fn PowerWriteACValueIndex(root: HANDLE, scheme: *const GUID, sub_group: *const GUID, setting: *const GUID, value: u32) -> u32;
    pub fn PowerWriteDCValueIndex(root: HANDLE, scheme: *const GUID, sub_group: *const GUID, setting: *const GUID, value: u32) -> u32;
    pub fn PowerSetActiveScheme(root: HANDLE, scheme: *const GUID) -> u32;
}

#[link(name = "dwmapi")]
extern "system" {
    pub fn DwmSetWindowAttribute(hwnd: HWND, attribute: u32, value: *const c_void, size: u32) -> i32;
}

#[link(name = "hid")]
extern "system" {
    pub fn HidD_GetFeature(device: HANDLE, buffer: *mut u8, len: u32) -> u8;
    pub fn HidD_SetFeature(device: HANDLE, buffer: *const u8, len: u32) -> u8;
    pub fn HidD_GetPreparsedData(device: HANDLE, preparsed: *mut isize) -> u8;
    pub fn HidD_FreePreparsedData(preparsed: isize) -> u8;
    /// `caps` is HIDP_CAPS, 32 USHORTs: [0] usage, [1] usage page, [4] feature report byte length.
    pub fn HidP_GetCaps(preparsed: isize, caps: *mut [u16; 32]) -> i32;
    pub fn HidP_GetUsageValue(report_type: i32, usage_page: u16, link_collection: u16, usage: u16, value: *mut u32, preparsed: isize, report: *const u8, report_len: u32) -> i32;
    pub fn HidP_SetUsageValue(report_type: i32, usage_page: u16, link_collection: u16, usage: u16, value: u32, preparsed: isize, report: *mut u8, report_len: u32) -> i32;
    pub fn HidP_GetUsageValueArray(report_type: i32, usage_page: u16, link_collection: u16, usage: u16, value: *mut u8, value_len: u16, preparsed: isize, report: *const u8, report_len: u32) -> i32;
    pub fn HidP_GetUsagesEx(report_type: i32, link_collection: u16, list: *mut USAGE_AND_PAGE, len: *mut u32, preparsed: *const u8, report: *const u8, report_len: u32) -> i32;
}

#[link(name = "winusb")]
extern "system" {
    pub fn WinUsb_Initialize(device: HANDLE, interface: *mut HANDLE) -> i32;
    pub fn WinUsb_Free(interface: HANDLE) -> i32;
    pub fn WinUsb_QueryPipe(interface: HANDLE, alternate: u8, index: u8, info: *mut WINUSB_PIPE_INFORMATION) -> i32;
    pub fn WinUsb_SetPipePolicy(interface: HANDLE, pipe: u8, policy: u32, len: u32, value: *const c_void) -> i32;
    pub fn WinUsb_ReadPipe(interface: HANDLE, pipe: u8, buffer: *mut u8, len: u32, transferred: *mut u32, overlapped: *mut c_void) -> i32;
    /// `setup` is the 8 byte WINUSB_SETUP_PACKET, passed by value.
    pub fn WinUsb_ControlTransfer(interface: HANDLE, setup: u64, buffer: *mut u8, len: u32, transferred: *mut u32, overlapped: *mut c_void) -> i32;
}

#[link(name = "magnification")]
extern "system" {
    pub fn MagInitialize() -> i32;
    pub fn MagUninitialize() -> i32;
    /// `effect` is a 5 x 5 matrix; a colour is a row vector multiplied from the left.
    pub fn MagSetFullscreenColorEffect(effect: *const f32) -> i32;
}
