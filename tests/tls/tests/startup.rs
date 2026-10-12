//! The shell's bounded trust bytes composed with TLS's own parser (tls.md, 8).

use std::fs;

use skein_scratch::Scratch;
use skein_shell::{RootBounds, Trust, read_trust_roots};
use skein_tls::{Config, Parsed, Refusal};
use skein_tls_world::pki;

fn certificate(bytes: &[u8]) -> Vec<u8> {
    let alphabet = skein_lib::base64::Alphabet::Standard;
    let len =
        skein_lib::base64::encoded_len(alphabet, u32::try_from(bytes.len()).expect("fixture fits")).expect("fits");
    let mut writer = skein_lib::Writer::new(usize::try_from(len).expect("fits"));
    skein_lib::base64::encode(alphabet, bytes, &mut writer).expect("measured room");
    let mut text = Vec::from(b"-----BEGIN CERTIFICATE-----\n".as_slice());
    text.extend_from_slice(&writer.finish());
    text.extend_from_slice(b"\n-----END CERTIFICATE-----\n");
    text
}

#[test]
fn startup_roots_share_configuration_and_report_skip_counts() {
    let scratch = Scratch::new("tls-startup");
    let path = scratch.path().join("roots.pem");
    let mut text = certificate(pki::ROOT);
    text.extend_from_slice(&certificate(b"well-framed garbage DER"));
    text.extend_from_slice(b"-----BEGIN CERTIFICATE-----\ninvalid!\n-----END CERTIFICATE-----\n");
    text.extend_from_slice(b"-----BEGIN PRIVATE KEY-----\nnever decoded!\n-----END PRIVATE KEY-----\n");
    fs::write(&path, &text).expect("plant fixture bundle");
    let bounds = RootBounds {
        bundle_bytes: u32::try_from(text.len()).expect("bundle fits"),
        certificates: 2,
        certificate_bytes: u32::try_from(pki::ROOT.len()).expect("root fits"),
    };
    let roots = read_trust_roots(&Trust::Machine { ssl_cert_file: Some(path) }, &bounds).expect("bounded PEM roots");
    assert_eq!((roots.malformed, roots.other_blocks), (1, 1));
    let (_, parsed) = Config::from_der(&roots.certificates, &[b"h2"]).expect("fixture root remains");
    assert_eq!(parsed, Parsed { taken: 1, skipped: 1 });
    let roots = read_trust_roots(&Trust::Der("fixtures/root.der".into()), &bounds).expect("DER file");
    let (_, parsed) = Config::from_der(&roots.certificates, &[]).expect("DER root remains");
    assert_eq!(parsed, Parsed { taken: 1, skipped: 0 });
    fs::write(scratch.path().join("empty.pem"), b"no certificate blocks").expect("empty roots bundle");
    let roots = read_trust_roots(&Trust::Machine { ssl_cert_file: Some(scratch.path().join("empty.pem")) }, &bounds)
        .expect("shell passes empty roots to TLS");
    assert_eq!(Config::from_der(&roots.certificates, &[]).expect_err("TLS refuses no roots"), Refusal::Roots);
}
