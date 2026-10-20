//! Public test fixture, never a secret (oauth.md, 5).
//! Generated once with:
//! ```text
//! openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out test-key.pem
//! openssl pkcs8 -topk8 -nocrypt -in test-key.pem -outform DER -out test-key.der
//! openssl pkey -in test-key.pem -text -noout
//! ```
//! PKCS8 is the DER in standard base64; MODULUS is its unsigned modulus in base64url.
pub(super) const PKCS8: &str = concat!(
    "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC+k8OYXiQEGpI75hV6PQfqBnu1SlUC",
    "j6Hs51VV6bVys34aqOqfvo98KNym2DAYlwToEdtP5SX8WXHjbaAk0Pw8PVs30hVfAf85ZMExrIgnRUxn",
    "HoELWukx0q/Z9nYzmLt/wrY3XEy6SuptANIF2+UBBHJLc/jZvFgCvTUm4A36MZ+Sh/dMzymlVzTbGS0e",
    "14qBRYwau3yszVmp44bd3sKeOxhsz7pEQ6LEnLZkCPaiAG3l3VhAu4YR3MRRj4eSEn9rnNh+0gfvcWOp",
    "0mGHzGhKwM29+S9MP0V+xDqjtTOl/O5xDC4sOWYm9Yh/r10COoQTNBYq5c3DZd4jGs7OOD+3AgMBAAEC",
    "ggEAMcYM7TQVKv83ly5SCLF86inUUHZ/+mHCFEoc9wnJipIHivX4BU2zUuWnL2pCMYHr1aKoSVcbUyuX",
    "O6NrkoIEhsUtmEglrcErQ/WlIZCipMMMH9iPMoSFaC5TIFWm7HXk7orPpWjVX0hmrp5qu/4A8wGSylMZ",
    "OODgiaKaCbO14tR0onXrF5Yz24MP0452ayFpRm/YX/mOpzTHh9SFlw5qtvecaHBQ1UPeTdt4ZPs188uY",
    "fj1YoFVRp+YMtE6hWvc5mJCY204KbuOuf+NcnLfFBw0EE+wNI2bPovNy8WXyweM3mr1ciHfk5ArIRxk1",
    "w+IBY66mbgqTCgpeyaFVLGtEzQKBgQDiw4Q4OXvBoDr1GZ8KhCGO0ya78PwWsGnh/VBa4hhnMn2Ial/3",
    "UNiC2dDTRERHuPjlVcZmdCSWkq/tyGbBLLIN+HGLr5U662I6p5yky6vpCzxaJk0DWRSz0PT6aw5pCrtz",
    "5/yhWXcO3V5s9j/r4eNw7qx2gR1QuL8SWKkqPobXkwKBgQDXJeM+2Oq8PGAMaplTtF754VQ7g8sR49a3",
    "p9kQXY8oH1TJsBdV7OylPAj8zgmxVzCGQSrgNx/UuqsZCo5yF91GdgfHqYqnBx+g9v6nMGgcnSLI54AB",
    "Nn4yYXqKMKqeMEnatIoPjyCx8FA3noBhyYeRDLvR0P+jge/lzqy8+zFFzQKBgQDcmCqlr3XutHrmOJh4",
    "2HT0xgAmUJZLVC6Mw6pJ3yyyJ3HrJOBabPFSyaXV/9EUnKK0Jq8st8toFybCAHPFdD96S7fZlWNsv9L9",
    "EFTYE7IWsPFzGVNN8HitPJc0jLSmBuS4KnOeWrPxZ/QVt7ATPdnN4MExPwVPo4lsxYc7KzxttQKBgBdh",
    "ZKqlxAykuwNeaj8wcMnS9OZq0hyll+XpdatTn4UKspogasjJsIXcWlWuxlrhUMKmPlSTaQAjMQjNnuFD",
    "Oqa3mOzXvduc6JEDhnUAy6+gW/FdRwnatCghc8GkvEXolX4Q9JfwQ+tSZEVHEkiOMA9RRO/uOHm6Wyms",
    "JhV7bsVZAoGAKA3Y2HjdcpfhimmuswgTrnkMqvvUPW1/KnEnm7Kagm4xwu93O0bzQMlFrZDyRmIUyf50",
    "NF9tAsxiHoHr50BbCzVQHR1PDnObtbBULR2wAvLv4B/0LH67rA2s88OWGoVVVIyL3b5Vf3OdquwA4ksM",
    "darj8zInZ4UJtYQ5L6ofLgU=",
);
pub(super) const MODULUS: &str = "vpPDmF4kBBqSO-YVej0H6gZ7tUpVAo-h7OdVVem1crN-Gqjqn76PfCjcptgwGJcE6BHbT-Ul_Flx422gJND8PD1bN9IVXwH_OWTBMayIJ0VMZx6BC1rpMdKv2fZ2M5i7f8K2N1xMukrqbQDSBdvlAQRyS3P42bxYAr01JuAN-jGfkof3TM8ppVc02xktHteKgUWMGrt8rM1ZqeOG3d7CnjsYbM-6REOixJy2ZAj2ogBt5d1YQLuGEdzEUY-HkhJ_a5zYftIH73FjqdJhh8xoSsDNvfkvTD9FfsQ6o7UzpfzucQwuLDlmJvWIf69dAjqEEzQWKuXNw2XeIxrOzjg_tw";
pub(super) const EXPONENT: &str = "AQAB";
pub(super) const KID: &[u8] = b"skein-test";
