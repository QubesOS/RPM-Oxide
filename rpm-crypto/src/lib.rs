//! FFI bindings to RPM’s cryptographic API
//!
//! `librpmio`, which is part of RPM, exposes some cryptographic routines for
//! use by third party applications.  This crate provides Rust bindings to that
//! code.

#![forbid(improper_ctypes)]

use openpgp_parser::{AllowWeakHashes, Error};
extern crate openpgp_parser;

mod digests;
mod signatures;
pub mod transaction;
pub use digests::{rpm_hash_len, DigestCtx};
pub use signatures::Signature as RawSignature;

/// An OpenPGP signature
pub struct Signature {
    sig: RawSignature,
    ctx: DigestCtx,
}

use init::grab_mutex;
pub use init::{init, InitToken};

mod init {
    use std;
    static mut GLOBAL_MUTEX: *const std::sync::Mutex<()> = 0 as _;
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
    #[repr(C)]
    pub struct InitToken {
        _unused: (),
    }
    impl InitToken {
        /// SAFETY: one must know that the library is initialized
        pub(super) unsafe fn new() -> Self {
            InitToken { _unused: () }
        }
    }
    pub(super) fn grab_mutex<'a>(_token: InitToken) -> std::sync::MutexGuard<'a, ()> {
        // SAFETY: this is ordered after all writes to GLOBAL_MUTEX
        unsafe { &*GLOBAL_MUTEX }
            .lock()
            .expect("the code never panics while the mutex is held")
    }
    pub fn init(path: Option<&std::ffi::CStr>) -> InitToken {
        unsafe extern "C" fn lock_at_exit() {
            if std::panic::catch_unwind(|| {
                // SAFETY: this is ordered after all writes to GLOBAL_MUTEX
                std::mem::forget(grab_mutex(InitToken { _unused: () }));
            })
            .is_err()
            {
                abort()
            }
        }
        #[allow(deprecated)] // we need to support old Rust
        use std::sync::{Once, ONCE_INIT};
        #[allow(deprecated)] // we need to support old Rust
        static RPM_CRYPTO_INIT_ONCE: Once = ONCE_INIT;
        use std::os::raw::{c_char, c_int, c_void};
        use std::ptr;
        #[link(name = ":librpm.so.9")]
        extern "C" {
            fn rpmReadConfigFiles(file: *const c_char, target: *const c_char) -> c_int;
        }
        #[link(name = "c")]
        extern "C" {
            fn abort() -> !;
            fn atexit(_: unsafe extern "C" fn()) -> c_int;
        }
        #[link(name = ":librpmio.so.9")]
        extern "C" {
            fn rpmPushMacro(
                mc: *mut c_void,
                n: *const c_char,
                o: *const c_char,
                b: *const c_char,
                level: c_int,
            ) -> c_int;
        }
        // Indicate that this macro was set on the command line
        const RMIL_CMDLINE: c_int = -7;
        // Safety: the C function is called correctly.
        RPM_CRYPTO_INIT_ONCE.call_once(|| unsafe {
            // SAFETY: this is synchronized by call_once()
            GLOBAL_MUTEX = Box::into_raw(Box::new(std::sync::Mutex::new(())));
            assert_eq!(rpmReadConfigFiles(ptr::null(), ptr::null()), 0);
            if let Some(path) = path {
                assert_eq!(
                    rpmPushMacro(
                        ptr::null_mut(),
                        b"_dbpath\0".as_ptr() as _,
                        ptr::null(),
                        path.as_ptr(),
                        RMIL_CMDLINE,
                    ),
                    0
                );
            }
            assert_eq!(atexit(lock_at_exit), 0, "atexit() failed?");
        });
        InitToken { _unused: () }
    }
}

impl Signature {
    /// Parse an OpenPGP signature.  The signature is validated before being
    /// passed to RPM.  If the time is not zero, the signature is checked to not
    /// be from the future and to not have expired.
    pub fn parse(
        untrusted_buffer: &[u8],
        time: u32,
        allow_weak_hashes: AllowWeakHashes,
        token: InitToken,
    ) -> Result<Self, Error> {
        let _mutex = init::grab_mutex(token);
        let sig = RawSignature::parse(untrusted_buffer, time, allow_weak_hashes, token)?;
        let ctx = DigestCtx::init(sig.hash_algorithm(), allow_weak_hashes, token)
            .expect("Digest algorithm already validated");
        Ok(Self { sig, ctx })
    }

    /// Update the sigatures’s internal digest context with data from `buf`.
    pub fn update(&mut self, buf: &[u8]) {
        self.ctx.update(buf)
    }

    pub fn public_key_algorithm(&self) -> u8 {
        self.sig.public_key_algorithm()
    }
}
