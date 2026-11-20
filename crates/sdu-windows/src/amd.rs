//! The AMD driver side of getting HDR to work: reading the display's own description from the
//! driver and telling the driver to use an EDID instead (see `edid`). AMD only; other vendors'
//! drivers pass the display's description on to Windows themselves.
//!
//! The same driver has a colour temperature control for each display, used for True Tone (see
//! `whitepoint::driver`).
//!
//! This goes through ADL, AMD's display library (atiadlxx.dll). The override is per display output
//! and the driver forgets it when the machine restarts or the display is plugged in again.

use crate::ffi::*;
use crate::util::{registry_strings, registry_subkeys, wide};
use core::ffi::c_void;
use std::ptr::null_mut;

const DISPLAY_CLASS: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
const ADAPTER_INFO_SIZE: usize = 1572; // AdapterInfo: iAdapterIndex at +4
const DISPLAY_INFO_SIZE: usize = 552; // ADLDisplayInfo: logical display index at +0, name at +20, flags at +548
const DISPLAY_CONNECTED: i32 = 1;
/// ADLDisplayEDIDData: { iSize, iFlag, iEDIDSize, iBlockIndex, cEDIDData[256], iReserved[4] }.
/// When writing, the whole EDID (up to 512 bytes) follows the header in one call and iSize stays 288.
const EDID_DATA_SIZE: i32 = 288;
const EDID_DATA_OFFSET: usize = 16;

type Malloc = unsafe extern "system" fn(i32) -> *mut c_void;
type Create = unsafe extern "C" fn(Malloc, i32, *mut *mut c_void) -> i32;
type Destroy = unsafe extern "C" fn(*mut c_void) -> i32;
type AdapterCount = unsafe extern "C" fn(*mut c_void, *mut i32) -> i32;
type AdapterInfo = unsafe extern "C" fn(*mut c_void, *mut u8, i32) -> i32;
type DisplayInfo = unsafe extern "C" fn(*mut c_void, i32, *mut i32, *mut *mut u8, i32) -> i32;
type EdidData = unsafe extern "C" fn(*mut c_void, i32, i32, *mut u8) -> i32;
/// The driver's view of what is on a connector. These take ADLDevicePort by value, which on x64 means
/// a pointer to the caller's copy.
///
/// The same family has calls to make the driver present a stored EDID in place of the display's
/// ("connection emulation": ADL2_Adapter_ConnectionData_Set, ADL2_Adapter_EmulationMode_Set), which
/// would have made the HDR override permanent. On the Radeon AI PRO R9700 they answer "not supported"
/// (-8) for every variation tried, with administrator rights too, so they are not used.
type ConnectionState = unsafe extern "C" fn(*mut c_void, i32, *const u8, *mut i32) -> i32;
type ConnectionDataGet = unsafe extern "C" fn(*mut c_void, i32, *const u8, i32, *mut u8) -> i32;

/// ADLConnectionData: { type, properties (6 ints), ports, active connections, data size, data[1024] }.
const CONNECTION_DATA_SIZE: usize = 40 + 1024;
const CONNECTION_DATA_LENGTH: usize = 36;
const CONNECTION_DATA: usize = 40;
const QUERY_REAL: i32 = 0;

type ColorGet = unsafe extern "C" fn(*mut c_void, i32, i32, i32, *mut i32, *mut i32, *mut i32, *mut i32, *mut i32) -> i32;
type ColorSet = unsafe extern "C" fn(*mut c_void, i32, i32, i32, i32) -> i32;
type SourceGet = unsafe extern "C" fn(*mut c_void, i32, i32, *mut i32) -> i32;
type SourceSet = unsafe extern "C" fn(*mut c_void, i32, i32, i32) -> i32;

const COLOR_TEMPERATURE: i32 = 1 << 4; // ADL_DISPLAY_COLOR_TEMPERATURE
/// Where the driver takes the colour temperature from: the display's description, or the value set here.
pub const TEMPERATURE_FROM_DISPLAY: i32 = 0x20;
pub const TEMPERATURE_FROM_USER: i32 = 0x40;
const NO_COLOR_CONTROL: &str = "this AMD driver has no colour temperature control";

/// The driver's colour temperature control for a display, in kelvin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Temperature {
    pub current: i32,
    /// What the driver uses when nobody changed it: the display's own white.
    pub default: i32,
    pub min: i32,
    pub max: i32,
    pub step: i32,
}

extern "C" {
    fn malloc(size: usize) -> *mut c_void;
}

/// ADL allocates what it returns through this; the few small buffers are left to the process.
unsafe extern "system" fn adl_malloc(size: i32) -> *mut c_void {
    malloc(size.max(0) as usize)
}

/// The display on an AMD output, with ADL loaded and open.
pub struct Display {
    library: HANDLE,
    context: *mut c_void,
    destroy: Destroy,
    get_edid: EdidData,
    set_edid: EdidData,
    /// The connector queries; a driver may lack them.
    emulation: Option<(ConnectionState, ConnectionDataGet)>,
    /// The colour controls; a driver may lack them.
    color: Option<(ColorGet, ColorSet, SourceGet, SourceSet)>,
    adapter: i32,
    display: i32,
    pub description: String,
}

impl Display {
    /// Looks for the Studio Display on the outputs of the AMD adapters.
    ///
    /// With an AMD integrated GPU next to an AMD card there are two driver packages, each with its own
    /// build of ADL, and the integrated GPU's build does not see the card's displays. So every running
    /// package's ADL is tried until one of them knows the display.
    pub fn find() -> Result<Display, String> {
        let mut reason = "no AMD display driver is installed".to_string();
        for library in adl_libraries() {
            match unsafe { Display::open(&library) } {
                Ok(display) => return Ok(display),
                Err(error) => reason = error,
            }
        }
        Err(reason)
    }

    unsafe fn open(path: &str) -> Result<Display, String> {
        let library = LoadLibraryW(wide(path).as_ptr());
        if library.is_null() {
            return Err(format!("{path} could not be loaded"));
        }
        macro_rules! function {
            ($name:literal, $kind:ty) => {{
                let address = GetProcAddress(library, concat!($name, "\0").as_ptr());
                if address.is_null() {
                    FreeLibrary(library);
                    return Err(format!("{} is missing in {path}", $name));
                }
                std::mem::transmute::<*mut c_void, $kind>(address)
            }};
        }
        let create = function!("ADL2_Main_Control_Create", Create);
        let destroy = function!("ADL2_Main_Control_Destroy", Destroy);
        let adapter_count = function!("ADL2_Adapter_NumberOfAdapters_Get", AdapterCount);
        let adapter_info = function!("ADL2_Adapter_AdapterInfo_Get", AdapterInfo);
        let display_info = function!("ADL2_Display_DisplayInfo_Get", DisplayInfo);
        let get_edid = function!("ADL2_Display_EdidData_Get", EdidData);
        let set_edid = function!("ADL2_Display_EdidData_Set", EdidData);

        macro_rules! optional {
            ($name:literal, $kind:ty) => {{
                let address = GetProcAddress(library, concat!($name, "\0").as_ptr());
                (!address.is_null()).then(|| std::mem::transmute::<*mut c_void, $kind>(address))
            }};
        }
        let color = (|| {
            Some((
                optional!("ADL2_Display_Color_Get", ColorGet)?,
                optional!("ADL2_Display_Color_Set", ColorSet)?,
                optional!("ADL2_Display_ColorTemperatureSource_Get", SourceGet)?,
                optional!("ADL2_Display_ColorTemperatureSource_Set", SourceSet)?,
            ))
        })();

        let emulation = (|| {
            Some((
                optional!("ADL2_Adapter_ConnectionState_Get", ConnectionState)?,
                optional!("ADL2_Adapter_ConnectionData_Get", ConnectionDataGet)?,
            ))
        })();

        let mut context = null_mut();
        if create(adl_malloc, 1, &mut context) != 0 {
            FreeLibrary(library);
            return Err("the AMD driver is not ready (ADL could not be started)".into());
        }

        let mut count = 0;
        adapter_count(context, &mut count);
        let mut adapters = vec![0u8; ADAPTER_INFO_SIZE * count.max(0) as usize];
        adapter_info(context, adapters.as_mut_ptr(), adapters.len() as i32);
        for adapter in adapters.chunks_exact(ADAPTER_INFO_SIZE) {
            let adapter_index = i32::from_le_bytes(adapter[4..8].try_into().unwrap());
            let mut displays = 0;
            let mut list: *mut u8 = null_mut();
            if display_info(context, adapter_index, &mut displays, &mut list, 0) != 0 || list.is_null() {
                continue;
            }
            let list = std::slice::from_raw_parts(list, DISPLAY_INFO_SIZE * displays.max(0) as usize);
            for info in list.chunks_exact(DISPLAY_INFO_SIZE) {
                let connected = i32::from_le_bytes(info[548..552].try_into().unwrap()) & DISPLAY_CONNECTED != 0;
                let name: String = info[20..276].iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
                if connected && (name.contains("Studio") || name.contains("XDR")) {
                    let display = i32::from_le_bytes(info[0..4].try_into().unwrap());
                    let description = format!("'{name}' on AMD adapter {adapter_index}, display {display}");
                    return Ok(Display { library, context, destroy, get_edid, set_edid, emulation, color, adapter: adapter_index, display, description });
                }
            }
        }
        destroy(context);
        FreeLibrary(library);
        Err("no Studio Display is connected to an AMD adapter".into())
    }

    /// One 256 byte portion of the display's description as the driver has it. `index` counts in
    /// steps of 128 bytes. Returns the driver's status and the bytes it gave.
    fn read_portion(&self, index: i32) -> (i32, Vec<u8>) {
        let mut request = vec![0u8; EDID_DATA_SIZE as usize];
        request[0..4].copy_from_slice(&EDID_DATA_SIZE.to_le_bytes());
        request[12..16].copy_from_slice(&index.to_le_bytes());
        let status = unsafe { (self.get_edid)(self.context, self.adapter, self.display, request.as_mut_ptr()) };
        let size = i32::from_le_bytes(request[8..12].try_into().unwrap()).clamp(0, 256) as usize;
        (status, request[EDID_DATA_OFFSET..EDID_DATA_OFFSET + size].to_vec())
    }

    /// The display's description as the driver has it: DisplayID 2.0 data straight from the display,
    /// or an EDID if an override is already in place.
    pub fn description_data(&self) -> Result<Vec<u8>, String> {
        let mut data: Vec<u8> = Vec::new();
        for index in 0..4 {
            let (status, bytes) = self.read_portion(index);
            crate::util::log(&format!("amd: description portion {index}: status {status}, {} bytes", bytes.len()));
            if status != 0 || bytes.is_empty() {
                if index == 0 {
                    return Err(format!("the driver did not return the display's description (ADL error {status})"));
                }
                break;
            }
            // portions overlap: each starts 128 bytes after the previous one
            let start = index as usize * 128;
            if start > data.len() {
                break;
            }
            data.truncate(start);
            data.extend_from_slice(&bytes);
        }
        Ok(data)
    }

    /// Makes the driver use this EDID for the display from now on (until restart or replug).
    pub fn override_edid(&self, edid: &[u8]) -> Result<(), String> {
        if edid.is_empty() || edid.len() > 512 || edid.len() % 128 != 0 {
            return Err(format!("an EDID of {} bytes cannot be used", edid.len()));
        }
        let mut request = vec![0u8; EDID_DATA_OFFSET + 512 + 64];
        request[0..4].copy_from_slice(&EDID_DATA_SIZE.to_le_bytes());
        request[8..12].copy_from_slice(&(edid.len() as i32).to_le_bytes());
        request[EDID_DATA_OFFSET..EDID_DATA_OFFSET + edid.len()].copy_from_slice(edid);
        match unsafe { (self.set_edid)(self.context, self.adapter, self.display, request.as_mut_ptr()) } {
            0 => Ok(()),
            status => Err(format!("the driver refused the EDID (ADL error {status})")),
        }
    }

    /// The connector the display is on, as an ADLDevicePort { connector, { link number 1, address 0 } }.
    fn port(&self) -> Result<[u8; 24], String> {
        let (state, _) = self.emulation.ok_or("this AMD driver does not tell about its connectors")?;
        for connector in 0..16i32 {
            let mut port = [0u8; 24];
            port[0..4].copy_from_slice(&connector.to_le_bytes());
            port[4..8].copy_from_slice(&1i32.to_le_bytes());
            let mut answer = [0i32; 3]; // emulation status, emulation mode, display index
            if unsafe { state(self.context, self.adapter, port.as_ptr(), answer.as_mut_ptr()) } == 0 && answer[2] == self.display && answer[0] != 0 {
                return Ok(port);
            }
        }
        Err("the display's connector was not found".into())
    }

    fn connection_data(&self, query: i32) -> Option<Vec<u8>> {
        let (_, get) = self.emulation?;
        let port = self.port().ok()?;
        let mut data = vec![0u8; CONNECTION_DATA_SIZE + 64];
        (unsafe { get(self.context, self.adapter, port.as_ptr(), query, data.as_mut_ptr()) } == 0).then_some(data)
    }

    /// The description the display itself gives (DisplayID), whatever the driver has been told to
    /// use instead.
    pub fn real_description(&self) -> Option<Vec<u8>> {
        let data = self.connection_data(QUERY_REAL)?;
        let length = (i32::from_le_bytes(data[CONNECTION_DATA_LENGTH..CONNECTION_DATA].try_into().unwrap()).clamp(0, 1024)) as usize;
        (length > 0).then(|| data[CONNECTION_DATA..CONNECTION_DATA + length].to_vec())
    }

    /// The driver's colour temperature for the display and the range it accepts.
    pub fn temperature(&self) -> Result<Temperature, String> {
        let (get, ..) = self.color.ok_or(NO_COLOR_CONTROL)?;
        let (mut current, mut default, mut min, mut max, mut step) = (0, 0, 0, 0, 0);
        match unsafe { get(self.context, self.adapter, self.display, COLOR_TEMPERATURE, &mut current, &mut default, &mut min, &mut max, &mut step) } {
            0 if max > min && step > 0 => Ok(Temperature { current, default, min, max, step }),
            status => Err(format!("the driver offers no colour temperature for this display (ADL error {status})")),
        }
    }

    pub fn set_temperature(&self, kelvin: i32) -> Result<(), String> {
        let (_, set, ..) = self.color.ok_or(NO_COLOR_CONTROL)?;
        match unsafe { set(self.context, self.adapter, self.display, COLOR_TEMPERATURE, kelvin) } {
            0 => Ok(()),
            status => Err(format!("the driver refused {kelvin} K (ADL error {status})")),
        }
    }

    /// `TEMPERATURE_FROM_DISPLAY` or `TEMPERATURE_FROM_USER`.
    pub fn temperature_source(&self) -> Result<i32, String> {
        let (_, _, get, _) = self.color.ok_or(NO_COLOR_CONTROL)?;
        let mut source = 0;
        match unsafe { get(self.context, self.adapter, self.display, &mut source) } {
            0 => Ok(source),
            status => Err(format!("the driver did not tell the colour temperature source (ADL error {status})")),
        }
    }

    pub fn set_temperature_source(&self, source: i32) -> Result<(), String> {
        let (.., set) = self.color.ok_or(NO_COLOR_CONTROL)?;
        match unsafe { set(self.context, self.adapter, self.display, source) } {
            0 => Ok(()),
            status => Err(format!("the driver refused the colour temperature source (ADL error {status})")),
        }
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.context);
            FreeLibrary(self.library);
        }
    }
}

/// atiadlxx.dll of every installed AMD display driver package (it sits next to the user mode driver).
/// A copy in System32 can be a leftover of an older driver that sees no adapters, so it is not used.
fn adl_libraries() -> Vec<String> {
    let mut libraries = Vec::new();
    for device in registry_subkeys(DISPLAY_CLASS) {
        let key = format!(r"{DISPLAY_CLASS}\{device}");
        if !registry_strings(&key, "ProviderName").iter().any(|name| name.contains("Advanced Micro Devices")) {
            continue;
        }
        if let Some(driver) = registry_strings(&key, "UserModeDriverName").first() {
            if let Some(folder) = driver.rfind('\\') {
                let library = format!(r"{}\atiadlxx.dll", &driver[..folder]);
                if !libraries.contains(&library) {
                    libraries.push(library);
                }
            }
        }
    }
    libraries.reverse(); // the discrete card's package usually comes last
    libraries
}
