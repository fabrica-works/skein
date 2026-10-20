//! RFC 7515, Appendix A.2's key and signature.

use alloc::boxed::Box;
use alloc::vec::Vec;
use skein_lib::base64::{Alphabet, decode};

use super::fixture;
use crate::rs256::integers;
use crate::{RS256_KEY_BYTES, sign_rs256, verify_rs256};

const N: &str = "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ";
const E: &str = "AQAB";
const D: &str = "Eq5xpGnNCivDflJsRQBXHx1hdR1k6Ulwe2JZD50LpXyWPEAeP88vLNO97IjlA7_GQ5sLKMgvfTeXZx9SE-7YwVol2NXOoAJe46sui395IW_GO-pWJ1O0BkTGoVEn2bKVRUCgu-GjBVaYLU6f3l9kJfFNS3E0QbVdxzubSu3Mkqzjkn439X0M_V51gfpRLI9JYanrC4D4qAdGcopV_0ZHHzQlBjudU2QvXt4ehNYTCBr6XCLQUShb1juUO1ZdiYoFaFQT5Tw8bGUl_x_jTj3ccPDVZFD9pIuhLhBOneufuBiB4cS98l2SR_RQyGWSeWjnczT0QU91p1DhOVRuOopznQ";
const P: &str = "4BzEEOtIpmVdVEZNCqS7baC4crd0pqnRH_5IB3jw3bcxGn6QLvnEtfdUdiYrqBdss1l58BQ3KhooKeQTa9AB0Hw_Py5PJdTJNPY8cQn7ouZ2KKDcmnPGBY5t7yLc1QlQ5xHdwW1VhvKn-nXqhJTBgIPgtldC-KDV5z-y2XDwGUc";
const Q: &str = "uQPEfgmVtjL0Uyyx88GZFF1fOunH3-7cepKmtH4pxhtCoHqpWmT8YAmZxaewHgHAjLYsp1ZSe7zFYHj7C6ul7TjeLQeZD_YwD66t62wDmpe_HlB-TnBA-njbglfIsRLtXlnDzQkv5dTltRJ11BKBBypeeF6689rjcJIDEz9RWdc";
const DP: &str = "BwKfV3Akq5_MFZDFZCnW-wzl-CCo83WoZvnLQwCTeDv8uzluRSnm71I3QCLdhrqE2e9YkxvuxdBfpT_PI7Yz-FOKnu1R6HsJeDCjn12Sk3vmAktV2zb34MCdy7cpdTh_YVr7tss2u6vneTwrA86rZtu5Mbr1C1XsmvkxHQAdYo0";
const DQ: &str = "h_96-mK1R_7glhsum81dZxjTnYynPbZpHziZjeeHcXYsXaaMwkOlODsWa7I9xXDoRwbKgB719rrmI2oKr6N3Do9U0ajaHF-NKJnwgjMd2w9cjz3_-kyNlxAr2v4IKhGNpmM5iIgOS1VZnOZ68m6_pbLBSp3nssTdlqvd0tIiTHU";
const QI: &str = "IYd7DHOhrWvxkwPQsRM2tOgrjbcrfvtQJipd-DlcxyVuuM9sQLdgjVk2oy26F0EmpScGLq2MowX7fhd_QJQ3ydy5cY7YIBi87w93IKLEdfnbJtoOPLUW0ITrJReOgo1cq9SbsxYawBgfp_gh6A5603k2-ZQwVK0JKSHuLFkuQ3U";
const INPUT: &[u8] = b"eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ";
const SIGNATURE: &str = "cC4hiUPoj9Eetdgtv3hF80EGrhuB__dzERat0XF9g2VtQgr9PJbu3XOiZj5RZmh7AAuHIm4Bh-0Qc_lF5YKt_O8W2Fp5jujGbds9uJdbF9CUAr7t1dnZcAcQjbKBYNX4BAynRFdiuB--f_nZLgrnbyTyWzO75vRK5h6xBArLIARNPvkSjtQBMHlb1L07Qe7K0GarZRmB_eSN9383LcOLn6_dO--xi12jzDwusC-eOkHWEsqtFZESc6BfI7noOPqvhJ1phCnvWh6IeYI2w9QOYEUipUTI8np6LbgGY9Fs98rqVt5AXLIhWkWywlVmtVrBp0igcN_IoypGlUPQGe77Rw";
// oauth.md, 2.2 caps a modulus at 8192 bits (1024 bytes).
const PAST_MODULUS: usize = 1025;

fn raw(text: &str) -> Box<[u8]> {
    decode(Alphabet::Url, text.as_bytes(), 1024).expect("base64url")
}

#[test]
fn rs256_checks_rfc_7515_appendix_a2() {
    let (n, e, sig) = (raw(N), raw(E), raw(SIGNATURE));
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

fn private_key() -> Box<[u8]> {
    let modulus = raw(N);
    let exponent = raw(E);
    let private_exponent = raw(D);
    let prime_p = raw(P);
    let prime_q = raw(Q);
    let (dp, dq, qi) = (raw(DP), raw(DQ), raw(QI));
    integers(&[&[0], &modulus, &exponent, &private_exponent, &prime_p, &prime_q, &dp, &dq, &qi])
        .expect("RFC private key DER")
}

#[test]
fn rs256_signs_rfc_7515_appendix_a2() {
    let der = private_key();
    assert!(der.len() <= RS256_KEY_BYTES, "RFC key fits the bound");
    assert_eq!(der.len(), 1191, "RFC key DER length");
    let signature = sign_rs256(&der, INPUT).expect("RFC key signs");
    assert_eq!(signature.as_slice(), raw(SIGNATURE).as_ref(), "RFC signature bytes");
    assert_eq!(sign_rs256(&der, INPUT), Some(signature), "blinding leaves the signature deterministic");
    let (n, e) = (raw(N), raw(E));
    assert!(verify_rs256(&n, &e, INPUT, &signature), "RFC signature verifies");
    let other = sign_rs256(&der, b"other").expect("RFC key signs another message");
    assert_ne!(other, signature, "another message has another signature");
    assert!(verify_rs256(&n, &e, b"other", &other), "another message's signature verifies");
}

#[test]
fn rs256_key_bytes_is_the_largest_2048_bit_key() {
    let n = [0xff; 256];
    let e = [0x01, 0xff, 0xff, 0xff, 0xff];
    let d = [0xff; 256];
    let prime = [0xff; 128];
    let der = integers(&[&[0], &n, &e, &d, &prime, &prime, &prime, &prime, &prime]).expect("largest key DER");
    assert_eq!(der.len(), RS256_KEY_BYTES, "largest 2048-bit PKCS#1 key DER");
}

#[test]
fn rs256_signs_with_a_2048_bit_pkcs1_key_alone() {
    let der = private_key();
    let truncated = Vec::from(&der[..der.len().checked_sub(1).expect("RFC key is nonempty")]);
    let mut trailing = Vec::from(der.as_ref());
    trailing.push(0);
    let mut version_one = Vec::from(der.as_ref());
    assert_eq!(&version_one[4..7], &[2, 1, 0], "PKCS#1 version INTEGER");
    version_one[6] = 1;
    let mut oversized = Vec::from(der.as_ref());
    oversized.resize(RS256_KEY_BYTES.checked_add(1).expect("past the key bound"), 0);
    for (label, key) in [
        ("empty", Vec::new()),
        ("truncated RFC key", truncated),
        ("trailing zero", trailing),
        ("version one", version_one),
        ("past the key bound", oversized),
        ("P-256 PKCS#8", fixture("leaf.key")),
        ("1024-bit PKCS#1", fixture("rsa-1024.der")),
        ("3072-bit PKCS#1", fixture("rsa-3072.der")),
    ] {
        assert_eq!(sign_rs256(&key, INPUT), None, "{label}");
    }
}
