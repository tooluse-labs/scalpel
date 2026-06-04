use pdbg_shim::raw;
use std::os::raw::c_char;
use std::panic::{self, AssertUnwindSafe};

pub const CALLBACK_PANIC_MESSAGE: &str = "Rust callback panicked across C ABI boundary";

/// Runs a Rust callback entered from C and maps any panic to `PDBG_ERROR_GENERIC`.
///
/// # Safety
///
/// If `err` is non-null, it must be a valid writable `pdbg_error` pointer for
/// the duration of this call. Every Rust `extern "C"` callback entry point in
/// this workspace must route through this helper before invoking panic-capable
/// Rust code.
pub unsafe fn catch_ffi_callback<F>(err: *mut raw::pdbg_error, callback: F) -> raw::pdbg_status
where
    F: FnOnce() -> raw::pdbg_status,
{
    match panic::catch_unwind(AssertUnwindSafe(callback)) {
        Ok(status) => status,
        Err(_) => {
            fill_error(
                err,
                raw::pdbg_status::PDBG_ERROR_GENERIC,
                CALLBACK_PANIC_MESSAGE,
            );
            raw::pdbg_status::PDBG_ERROR_GENERIC
        }
    }
}

fn fill_error(err: *mut raw::pdbg_error, status: raw::pdbg_status, message: &str) {
    if err.is_null() {
        return;
    }

    unsafe {
        (*err).status = status;
        (*err).mupdf_code = 0;
        (*err).message.fill(0);

        let limit = (*err).message.len().saturating_sub(1);
        for (slot, byte) in (*err)
            .message
            .iter_mut()
            .take(limit)
            .zip(message.as_bytes())
        {
            *slot = *byte as c_char;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    use std::os::raw::c_void;

    #[test]
    fn ffi_callback_boundary_passes_through_status() {
        let mut err = raw::pdbg_error::default();

        let status =
            unsafe { catch_ffi_callback(&mut err, || raw::pdbg_status::PDBG_ERROR_UNSUPPORTED) };

        assert_eq!(status, raw::pdbg_status::PDBG_ERROR_UNSUPPORTED);
        assert_eq!(err.status, raw::pdbg_status::PDBG_OK);
        assert!(error_message(&err).is_empty());
    }

    #[test]
    fn ffi_callback_boundary_catches_panic_and_fills_error() {
        let mut err = raw::pdbg_error::default();

        let status = unsafe {
            catch_ffi_callback(&mut err, || {
                panic!("callback bug");
            })
        };

        assert_eq!(status, raw::pdbg_status::PDBG_ERROR_GENERIC);
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_GENERIC);
        assert_eq!(err.mupdf_code, 0);
        assert_eq!(error_message(&err), CALLBACK_PANIC_MESSAGE);
    }

    #[test]
    fn ffi_callback_boundary_allows_null_error_pointer() {
        let status = unsafe {
            catch_ffi_callback(std::ptr::null_mut(), || {
                panic!("callback bug");
            })
        };

        assert_eq!(status, raw::pdbg_status::PDBG_ERROR_GENERIC);
    }

    #[test]
    fn c_invoked_rust_callback_catches_panic_before_returning_to_c() {
        let mut err = raw::pdbg_error::default();

        let status = unsafe {
            raw::pdbg_test_invoke_callback(Some(panicking_callback), std::ptr::null_mut(), &mut err)
        };

        assert_eq!(status, raw::pdbg_status::PDBG_ERROR_GENERIC);
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_GENERIC);
        assert_eq!(error_message(&err), CALLBACK_PANIC_MESSAGE);
    }

    #[test]
    fn c_callback_hook_rejects_null_callback() {
        let mut err = raw::pdbg_error::default();

        let status =
            unsafe { raw::pdbg_test_invoke_callback(None, std::ptr::null_mut(), &mut err) };

        assert_eq!(status, raw::pdbg_status::PDBG_ERROR_GENERIC);
        assert_eq!(err.status, raw::pdbg_status::PDBG_ERROR_GENERIC);
        assert_eq!(error_message(&err), "callback is null");
    }

    unsafe extern "C" fn panicking_callback(
        _user: *mut c_void,
        err: *mut raw::pdbg_error,
    ) -> raw::pdbg_status {
        unsafe {
            catch_ffi_callback(err, || {
                panic!("callback bug");
            })
        }
    }

    fn error_message(err: &raw::pdbg_error) -> String {
        unsafe { CStr::from_ptr(err.message.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }
}
