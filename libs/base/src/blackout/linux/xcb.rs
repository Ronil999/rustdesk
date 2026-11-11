#![allow(non_camel_case_types)]

use super::{BlackoutError, Crtc, GammaApi, Ramp};
use hbb_common::libc::{self, c_char, c_int, c_uint, c_void};
use std::ptr;

type xcb_connection_t = c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct Cookie {
    sequence: c_uint,
}

#[repr(C)]
struct ScreenIterator {
    // `root` is the first field of xcb_screen_t
    data: *const u32,
    rem: c_int,
    index: c_int,
}

#[repr(C)]
struct GenericError {
    response_type: u8,
    error_code: u8,
}

#[repr(C)]
struct QueryVersionReply {
    response_type: u8,
    pad0: u8,
    sequence: u16,
    length: u32,
    major_version: u32,
    minor_version: u32,
}

#[repr(C)]
struct ScreenResourcesReply {
    response_type: u8,
    pad0: u8,
    sequence: u16,
    length: u32,
    timestamp: u32,
    config_timestamp: u32,
}

#[repr(C)]
struct OutputInfoReply {
    response_type: u8,
    status: u8,
    sequence: u16,
    length: u32,
    timestamp: u32,
    crtc: u32,
    mm_width: u32,
    mm_height: u32,
    connection: u8,
}

#[repr(C)]
struct GammaReply {
    response_type: u8,
    pad0: u8,
    sequence: u16,
    length: u32,
    size: u16,
}

const XCB_RANDR_CONNECTION_CONNECTED: u8 = 0;

#[link(name = "xcb")]
#[link(name = "xcb-randr")]
extern "C" {
    fn xcb_connect(displayname: *const c_char, screenp: *mut c_int) -> *mut xcb_connection_t;
    fn xcb_disconnect(c: *mut xcb_connection_t);
    fn xcb_connection_has_error(c: *mut xcb_connection_t) -> c_int;
    fn xcb_get_setup(c: *mut xcb_connection_t) -> *const c_void;
    fn xcb_setup_roots_iterator(r: *const c_void) -> ScreenIterator;
    fn xcb_screen_next(i: *mut ScreenIterator);
    fn xcb_request_check(c: *mut xcb_connection_t, cookie: Cookie) -> *mut GenericError;

    fn xcb_randr_query_version(c: *mut xcb_connection_t, major: u32, minor: u32) -> Cookie;
    fn xcb_randr_query_version_reply(
        c: *mut xcb_connection_t,
        cookie: Cookie,
        e: *mut *mut GenericError,
    ) -> *mut QueryVersionReply;
    fn xcb_randr_get_screen_resources_current(c: *mut xcb_connection_t, window: u32) -> Cookie;
    fn xcb_randr_get_screen_resources_current_reply(
        c: *mut xcb_connection_t,
        cookie: Cookie,
        e: *mut *mut GenericError,
    ) -> *mut ScreenResourcesReply;
    fn xcb_randr_get_screen_resources_current_outputs(r: *const ScreenResourcesReply)
        -> *const u32;
    fn xcb_randr_get_screen_resources_current_outputs_length(
        r: *const ScreenResourcesReply,
    ) -> c_int;
    fn xcb_randr_get_output_info(
        c: *mut xcb_connection_t,
        output: u32,
        config_timestamp: u32,
    ) -> Cookie;
    fn xcb_randr_get_output_info_reply(
        c: *mut xcb_connection_t,
        cookie: Cookie,
        e: *mut *mut GenericError,
    ) -> *mut OutputInfoReply;
    fn xcb_randr_get_output_info_name(r: *const OutputInfoReply) -> *const u8;
    fn xcb_randr_get_output_info_name_length(r: *const OutputInfoReply) -> c_int;
    fn xcb_randr_get_crtc_gamma(c: *mut xcb_connection_t, crtc: u32) -> Cookie;
    fn xcb_randr_get_crtc_gamma_reply(
        c: *mut xcb_connection_t,
        cookie: Cookie,
        e: *mut *mut GenericError,
    ) -> *mut GammaReply;
    fn xcb_randr_get_crtc_gamma_red(r: *const GammaReply) -> *const u16;
    fn xcb_randr_get_crtc_gamma_red_length(r: *const GammaReply) -> c_int;
    fn xcb_randr_get_crtc_gamma_green(r: *const GammaReply) -> *const u16;
    fn xcb_randr_get_crtc_gamma_green_length(r: *const GammaReply) -> c_int;
    fn xcb_randr_get_crtc_gamma_blue(r: *const GammaReply) -> *const u16;
    fn xcb_randr_get_crtc_gamma_blue_length(r: *const GammaReply) -> c_int;
    fn xcb_randr_set_crtc_gamma_checked(
        c: *mut xcb_connection_t,
        crtc: u32,
        size: u16,
        red: *const u16,
        green: *const u16,
        blue: *const u16,
    ) -> Cookie;
}

/// A malloc'd xcb reply, freed on drop.
struct Reply<T>(*mut T);

impl<T> Drop for Reply<T> {
    fn drop(&mut self) {
        unsafe { libc::free(self.0 as *mut c_void) };
    }
}

fn reply<T>(what: &str, r: *mut T, e: *mut GenericError) -> Result<Reply<T>, BlackoutError> {
    let code = if e.is_null() {
        None
    } else {
        let code = unsafe { (*e).error_code };
        unsafe { libc::free(e as *mut c_void) };
        Some(code)
    };
    if r.is_null() || code.is_some() {
        if !r.is_null() {
            drop(Reply(r));
        }
        return Err(BlackoutError::Failed(format!(
            "{} failed (X error {})",
            what,
            code.unwrap_or(0)
        )));
    }
    Ok(Reply(r))
}

unsafe fn slice<'a>(p: *const u16, len: c_int) -> &'a [u16] {
    if p.is_null() || len <= 0 {
        &[]
    } else {
        std::slice::from_raw_parts(p, len as usize)
    }
}

/// A connection to the X server named by this process's `DISPLAY` / `XAUTHORITY`.
pub struct XcbGamma {
    conn: *mut xcb_connection_t,
    root: u32,
}

impl Drop for XcbGamma {
    fn drop(&mut self) {
        unsafe { xcb_disconnect(self.conn) };
    }
}

impl XcbGamma {
    pub fn connect() -> Result<Self, BlackoutError> {
        let mut screen: c_int = 0;
        let conn = unsafe { xcb_connect(ptr::null(), &mut screen) };
        // xcb_connect never returns null; a failed connection reports it here and must still
        // be disconnected.
        let this = Self { conn, root: 0 };
        if conn.is_null() || unsafe { xcb_connection_has_error(conn) } != 0 {
            return Err(BlackoutError::NotSupported(
                "Cannot connect to the X11 display".to_owned(),
            ));
        }
        let root = unsafe {
            let mut it = xcb_setup_roots_iterator(xcb_get_setup(conn));
            for _ in 0..screen.max(0) {
                xcb_screen_next(&mut it);
            }
            if it.rem <= 0 || it.data.is_null() {
                0
            } else {
                *it.data
            }
        };
        if root == 0 {
            return Err(BlackoutError::Failed("X11 screen not found".to_owned()));
        }
        let mut this = this;
        this.root = root;
        this.check_randr()?;
        Ok(this)
    }

    fn check_randr(&self) -> Result<(), BlackoutError> {
        let mut e = ptr::null_mut();
        let r = unsafe {
            let cookie = xcb_randr_query_version(self.conn, 1, 3);
            xcb_randr_query_version_reply(self.conn, cookie, &mut e)
        };
        let v = reply("RandR query", r, e)
            .map_err(|_| BlackoutError::NotSupported("The X server has no RandR".to_owned()))?;
        let (major, minor) = unsafe { ((*v.0).major_version, (*v.0).minor_version) };
        if (major, minor) < (1, 3) {
            return Err(BlackoutError::NotSupported(format!(
                "RandR {}.{} is too old, 1.3 is required",
                major, minor
            )));
        }
        Ok(())
    }
}

impl GammaApi for XcbGamma {
    fn active_crtcs(&mut self) -> Result<Vec<Crtc>, BlackoutError> {
        let mut e = ptr::null_mut();
        let res = unsafe {
            let cookie = xcb_randr_get_screen_resources_current(self.conn, self.root);
            xcb_randr_get_screen_resources_current_reply(self.conn, cookie, &mut e)
        };
        let res = reply("RandR screen resources", res, e)?;
        let outputs = unsafe {
            let p = xcb_randr_get_screen_resources_current_outputs(res.0);
            let n = xcb_randr_get_screen_resources_current_outputs_length(res.0);
            if p.is_null() || n <= 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(p, n as usize).to_vec()
            }
        };
        let config_timestamp = unsafe { (*res.0).config_timestamp };
        let mut crtcs: Vec<Crtc> = Vec::new();
        for output in outputs {
            let mut e = ptr::null_mut();
            let info = unsafe {
                let cookie = xcb_randr_get_output_info(self.conn, output, config_timestamp);
                xcb_randr_get_output_info_reply(self.conn, cookie, &mut e)
            };
            let info = reply("RandR output info", info, e)?;
            let (crtc, connection) = unsafe { ((*info.0).crtc, (*info.0).connection) };
            if crtc == 0 || connection != XCB_RANDR_CONNECTION_CONNECTED {
                continue;
            }
            let name = unsafe {
                let p = xcb_randr_get_output_info_name(info.0);
                let n = xcb_randr_get_output_info_name_length(info.0);
                if p.is_null() || n <= 0 {
                    format!("output-{}", output)
                } else {
                    String::from_utf8_lossy(std::slice::from_raw_parts(p, n as usize)).into_owned()
                }
            };
            // Mirrored outputs share one CRTC.
            match crtcs.iter_mut().find(|c| c.id == crtc) {
                Some(c) => c.outputs.push(name),
                None => crtcs.push(Crtc {
                    id: crtc,
                    outputs: vec![name],
                }),
            }
        }
        Ok(crtcs)
    }

    fn gamma(&mut self, crtc: u32) -> Result<Ramp, BlackoutError> {
        let mut e = ptr::null_mut();
        let r = unsafe {
            let cookie = xcb_randr_get_crtc_gamma(self.conn, crtc);
            xcb_randr_get_crtc_gamma_reply(self.conn, cookie, &mut e)
        };
        let r = reply("RandR get gamma", r, e)?;
        let ramp = unsafe {
            Ramp {
                red: slice(
                    xcb_randr_get_crtc_gamma_red(r.0),
                    xcb_randr_get_crtc_gamma_red_length(r.0),
                )
                .to_vec(),
                green: slice(
                    xcb_randr_get_crtc_gamma_green(r.0),
                    xcb_randr_get_crtc_gamma_green_length(r.0),
                )
                .to_vec(),
                blue: slice(
                    xcb_randr_get_crtc_gamma_blue(r.0),
                    xcb_randr_get_crtc_gamma_blue_length(r.0),
                )
                .to_vec(),
            }
        };
        let size = unsafe { (*r.0).size } as usize;
        if ramp.green.len() != size || ramp.blue.len() != size || ramp.red.len() != size {
            return Err(BlackoutError::Failed(format!(
                "Inconsistent gamma ramp on CRTC {}",
                crtc
            )));
        }
        Ok(ramp)
    }

    fn set_gamma(&mut self, crtc: u32, ramp: &Ramp) -> Result<(), BlackoutError> {
        let size = ramp.len();
        if size == 0
            || size > u16::MAX as usize
            || ramp.green.len() != size
            || ramp.blue.len() != size
        {
            return Err(BlackoutError::Failed(format!(
                "Invalid gamma ramp for CRTC {}",
                crtc
            )));
        }
        let e = unsafe {
            let cookie = xcb_randr_set_crtc_gamma_checked(
                self.conn,
                crtc,
                size as u16,
                ramp.red.as_ptr(),
                ramp.green.as_ptr(),
                ramp.blue.as_ptr(),
            );
            xcb_request_check(self.conn, cookie)
        };
        if e.is_null() {
            return Ok(());
        }
        let code = unsafe { (*e).error_code };
        unsafe { libc::free(e as *mut c_void) };
        Err(BlackoutError::Failed(format!(
            "Setting gamma on CRTC {} failed (X error {})",
            crtc, code
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Needs a real X11 display: `cargo test -p base -- --ignored xcb_gamma_round_trip`.
    // Writes back the ramps it reads, so the screen does not change.
    #[test]
    #[ignore]
    fn xcb_gamma_round_trip() {
        let mut x = XcbGamma::connect().unwrap();
        let crtcs = x.active_crtcs().unwrap();
        assert!(!crtcs.is_empty());
        for c in crtcs {
            let ramp = x.gamma(c.id).unwrap();
            assert!(ramp.len() > 0);
            x.set_gamma(c.id, &ramp).unwrap();
            assert_eq!(x.gamma(c.id).unwrap(), ramp);
            println!("{:?} crtc {} ramp size {}", c.outputs, c.id, ramp.len());
        }
        assert!(x.set_gamma(0x7fff_fff0, &Ramp::black(256)).is_err());
    }
}

#[cfg(test)]
mod hardware_tests {
    use super::super::{blank_all, restore_all};
    use super::*;

    fn capture_mean() -> f64 {
        // `xwd -root` reads the root window like RustDesk's MIT-SHM capturer does.
        let out = std::process::Command::new("xwd")
            .args(["-root", "-silent"])
            .output()
            .unwrap();
        let d = out.stdout;
        let header = u32::from_be_bytes([d[0], d[1], d[2], d[3]]) as usize;
        let ncolors = u32::from_be_bytes([d[76], d[77], d[78], d[79]]) as usize;
        let px = &d[header + ncolors * 12..];
        let (mut sum, mut n) = (0u64, 0u64);
        for i in (0..px.len().saturating_sub(4)).step_by(4 * 97) {
            sum += px[i] as u64 + px[i + 1] as u64 + px[i + 2] as u64;
            n += 3;
        }
        sum as f64 / n as f64
    }

    // Blanks the real monitors for ~1.5 s: `cargo test -p base -- --ignored blank_keeps_capture`.
    #[test]
    #[ignore]
    fn blank_keeps_capture() {
        let before = capture_mean();
        let mut x = XcbGamma::connect().unwrap();
        let saved = blank_all(&mut x, |_| Ok(())).unwrap();
        for c in &saved {
            assert!(x.gamma(c.crtc).unwrap().is_black());
        }
        let during = capture_mean();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        restore_all(&mut x, &saved).unwrap();
        for c in &saved {
            assert_eq!(x.gamma(c.crtc).unwrap(), c.ramp);
        }
        println!(
            "capture mean before {:.2} during blackout {:.2}",
            before, during
        );
        assert!((before - during).abs() < 2.0);
    }
}
