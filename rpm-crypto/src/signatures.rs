use super::InitToken;
use openpgp_parser::{signature, AllowWeakHashes, Error};
use std;
use std::os::raw::{c_int, c_uint};
#[repr(C)]
struct RpmPgpDigParams(u8);

#[repr(C)]
pub struct Signature(*mut RpmPgpDigParams);

impl Signature {
    pub fn parse(
        untrusted_buffer: &[u8],
        time: u32,
        allow_weak_hashes: AllowWeakHashes,
        _: InitToken,
    ) -> Result<Self, Error> {
        // Check that the signature is valid
        let sig_info = signature::parse(
            untrusted_buffer,
            time,
            allow_weak_hashes,
            signature::SignatureType::Binary,
        )?;
        // We can now pass the buffer to RPM, since it is a valid signature
        let slice = untrusted_buffer;
        let mut params = Signature(std::ptr::null_mut());
        // SAFETY: we just validated that the signature is well-formed.
        let r = unsafe { pgpPrtParams(slice.as_ptr(), slice.len(), 2, &mut params) };
        assert!(r == 0, "we accepted a signature RPM rejected");
        assert!(!params.0.is_null());
        assert_eq!(params.hash_algorithm(), sig_info.hash_alg);
        assert_eq!(params.public_key_algorithm(), sig_info.pkey_alg);
        Ok(params)
    }

    /// Retrieve the hash algorithm of the signature
    pub fn hash_algorithm(&self) -> u8 {
        // SAFETY: ‘self.0’ is a valid pointer of the type RPM needs
        let alg = unsafe { pgpDigParamsAlgo(self.0, 9) };
        assert!(alg <= 255, "invalid hash algorithm not rejected earlier?");
        alg as _
    }

    /// Retrieve the public key algorithm of the signature
    pub fn public_key_algorithm(&self) -> u8 {
        // SAFETY: ‘self.0’ is a valid pointer of the type RPM needs
        (unsafe { pgpDigParamsAlgo(self.0, 6) }) as _
    }
}

impl Drop for Signature {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: ‘self.0’ is a valid pointer.
            self.0 = unsafe { pgpDigParamsFree(self.0) }
        }
    }
}

#[link(name = ":librpmio.so.9")]
extern "C" {
    fn pgpPrtParams(pkts: *const u8, pktlen: usize, pkttype: c_uint, ret: &mut Signature) -> c_int;
    fn pgpDigParamsFree(digp: *mut RpmPgpDigParams) -> *mut RpmPgpDigParams;
    fn pgpDigParamsAlgo(digp: *const RpmPgpDigParams, algotype: c_uint) -> c_uint;
}
