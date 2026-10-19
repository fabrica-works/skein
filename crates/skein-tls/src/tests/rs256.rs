//! RFC 7515, Appendix A.2's public key and signature.

use alloc::boxed::Box;
use skein_lib::base64::{Alphabet, decode};

use crate::verify_rs256;

const N: &str = "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ";
const INPUT: &[u8] = b"eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ";
const SIGNATURE: &str = "cC4hiUPoj9Eetdgtv3hF80EGrhuB__dzERat0XF9g2VtQgr9PJbu3XOiZj5RZmh7AAuHIm4Bh-0Qc_lF5YKt_O8W2Fp5jujGbds9uJdbF9CUAr7t1dnZcAcQjbKBYNX4BAynRFdiuB--f_nZLgrnbyTyWzO75vRK5h6xBArLIARNPvkSjtQBMHlb1L07Qe7K0GarZRmB_eSN9383LcOLn6_dO--xi12jzDwusC-eOkHWEsqtFZESc6BfI7noOPqvhJ1phCnvWh6IeYI2w9QOYEUipUTI8np6LbgGY9Fs98rqVt5AXLIhWkWywlVmtVrBp0igcN_IoypGlUPQGe77Rw";
// oauth.md, 2.2 caps a modulus at 8192 bits (1024 bytes).
const PAST_MODULUS: usize = 1025;

fn raw(text: &str) -> Box<[u8]> {
    decode(Alphabet::Url, text.as_bytes(), 1024).expect("base64url")
}

#[test]
fn rs256_checks_rfc_7515_appendix_a2() {
    let (n, e, sig) = (raw(N), raw("AQAB"), raw(SIGNATURE));
    let mut bad = sig.clone();
    bad[0] ^= 1;
    let oversized = [0x81; PAST_MODULUS];
    for (label, modulus, exponent, message, signature, expected) in [
        ("RFC vector", n.as_ref(), e.as_ref(), INPUT, sig.as_ref(), true),
        ("flipped signature", &n, &e, INPUT, &bad, false),
        ("another message", &n, &e, b"other", &sig, false),
        ("exponent three", &n, &[3], INPUT, &sig, false),
        ("oversized modulus", &oversized, &e, INPUT, &sig, false),
        ("empty modulus", &[], &e, INPUT, &sig, false),
        ("empty exponent", &n, &[], INPUT, &sig, false),
    ] {
        assert_eq!(verify_rs256(modulus, exponent, message, signature), expected, "{label}");
    }
}
