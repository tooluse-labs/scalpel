#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod raw;

#[cfg(test)]
mod tests {
    use super::raw;
    use std::ptr;

    #[test]
    fn fake_context_smoke() {
        unsafe {
            let mut ctx: *mut raw::pdbg_context = ptr::null_mut();
            let mut err = raw::pdbg_error::default();
            let status = raw::pdbg_context_new(&mut ctx, &mut err);
            assert_eq!(status, raw::pdbg_status::PDBG_OK);
            assert!(!ctx.is_null());
            raw::pdbg_context_drop(ctx);
        }
    }
}
