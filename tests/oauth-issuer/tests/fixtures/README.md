These are public test keys, never secrets.

- `rfc7515-a2-public-test-key.pk8.der`: RFC 7515 Appendix A.2 RSA key, encoded as PKCS#8 PrivateKeyInfo DER.
- `rfc9500-2.1-testRSA2048-public-test-key.pk8.der`: RFC 9500 section 2.1, testRSA2048, converted from its published PKCS#1 DER to PKCS#8 PrivateKeyInfo DER.

Every test obtains these fixtures through this crate’s one `test_key` function. No release build reads these files.
