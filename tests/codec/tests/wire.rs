//! Hand-written bytes for codec.md, sections 3-4.

#[cfg(test)]
mod tests {

    use skein_codec::Reason;
    use skein_codec_tests::lints::{CEILINGS as LINT_CEILINGS, HostTool, HostToolParts};
    use skein_codec_tests::scalars::{CEILINGS as SCALAR_CEILINGS, Flag, Scalars, ScalarsParts};
    use skein_codec_tests::v1::{CEILINGS, Choice, Effect, Field, FieldParts, Path, Report, ReportParts};
    use skein_lib::{Duration, List, Reader, Writer};

    #[test]
    fn a_credentials_debug_shows_its_secrets_length_and_not_its_bytes() {
        use skein_codec_tests::secrets::{CEILINGS, Credential, CredentialParts};
        let parts =
            CredentialParts { name: Box::from(&b"llm"[..]), value: skein_lib::Secret::new(Box::from(&b"hunter2"[..])) };
        let credential = Credential::new(&CEILINGS, parts).expect("within its limits");
        let shown = format!("{credential:?}");
        assert!(!shown.contains("hunter2"), "the secret's bytes are not shown: {shown}");
        assert!(shown.contains("len: 7"), "its length is: {shown}");
    }

    #[test]
    fn a_credential_encodes_its_secret_as_bytes_and_decodes_back() {
        use skein_codec_tests::secrets::{CEILINGS, Credential, CredentialParts};
        let credential = Credential::new(
            &CEILINGS,
            CredentialParts { name: Box::from(&b"llm"[..]), value: skein_lib::Secret::new(Box::from(&b"hunter2"[..])) },
        )
        .expect("within ceilings");
        let wire = [0, 0, 0, 3, b'l', b'l', b'm', 0, 0, 0, 7, b'h', b'u', b'n', b't', b'e', b'r', b'2'];
        let mut writer = Writer::new(18);
        credential.encode(&mut writer).expect("measured room");
        assert_eq!(writer.finish().as_ref(), &wire);
        assert_eq!(Credential::decode(&CEILINGS, &mut Reader::new(&wire)), Ok(credential));
    }

    fn report() -> Report {
        let field = Field::new(&CEILINGS, FieldParts { name: Box::from(&b"A"[..]), value: Box::from(&[0x7f][..]) })
            .expect("within ceilings");
        let mut fields = List::with_capacity(1);
        fields.push(field).expect("room");
        Report::new(
            &CEILINGS,
            ReportParts {
                title: Box::from(&b"R"[..]),
                fields,
                effect: Effect::Write,
                present: Some(true),
                nonce: [1, 2],
                age: Duration::from_nanos(5),
            },
        )
        .expect("within ceilings")
    }

    const WIRE: [u8; 34] = [
        0, 1, // version
        0, 0, 0, 1, b'R', // title
        0, 0, 0, 1, // one field
        0, 0, 0, 1, b'A', // field name
        0, 0, 0, 1, 0x7f, // field value
        1,    // effect: write
        1, 1, // present: true
        1, 2, // nonce
        0, 0, 0, 0, 0, 0, 0, 5, // age
    ];

    #[test]
    fn a_generated_record_matches_hand_written_bytes() {
        let value = report();
        assert_eq!(value.measure(), 34);
        let mut writer = Writer::new(34);
        value.encode(&mut writer).expect("measured room");
        assert_eq!(&*writer.finish(), &WIRE);
        let decoded = Report::decode(&CEILINGS, &mut Reader::new(&WIRE)).expect("canonical bytes");
        assert_eq!(decoded, value);
        assert_eq!(decoded.into_parts().title.as_ref(), b"R");
    }

    #[test]
    fn noncanonical_and_malformed_bytes_have_typed_problems() {
        let mut bad_bool = WIRE;
        *bad_bool.get_mut(23).expect("option's bool") = 2;
        let problem = Report::decode(&CEILINGS, &mut Reader::new(&bad_bool)).expect_err("bool beyond one");
        assert_eq!((problem.path, problem.reason), (Path::ReportPresent, Reason::Bool));

        let mut bad_tag = WIRE;
        *bad_tag.get_mut(21).expect("effect tag") = 2;
        let problem = Report::decode(&CEILINGS, &mut Reader::new(&bad_tag)).expect_err("unknown enum tag");
        assert_eq!((problem.path, problem.reason), (Path::EffectTag, Reason::Tag));

        let mut bad_version = WIRE;
        *bad_version.get_mut(1).expect("version byte") = 2;
        let problem = Report::decode(&CEILINGS, &mut Reader::new(&bad_version)).expect_err("wrong version");
        assert_eq!((problem.path, problem.reason), (Path::ReportVersion, Reason::Version));

        let problem =
            Report::decode(&CEILINGS, &mut Reader::new(WIRE.get(..33).expect("short prefix"))).expect_err("short age");
        assert_eq!((problem.path, problem.reason), (Path::ReportAge, Reason::Short));

        let mut trailing = Writer::new(35);
        trailing.put(&WIRE).expect("room");
        trailing.put(&[0]).expect("room");
        let trailing = trailing.finish();
        let problem = Report::decode(&CEILINGS, &mut Reader::new(&trailing)).expect_err("trailing byte");
        assert_eq!((problem.path, problem.reason), (Path::ReportTail, Reason::Trailing));
    }

    #[test]
    fn bounds_and_utf8_are_checked_before_storing_fields() {
        let mut overlong = WIRE;
        *overlong.get_mut(14).expect("name length") = 5;
        let problem = Report::decode(&CEILINGS, &mut Reader::new(&overlong)).expect_err("name beyond ceiling");
        assert_eq!((problem.path, problem.reason), (Path::FieldName, Reason::Bound));

        let mut invalid_utf8 = WIRE;
        *invalid_utf8.get_mut(15).expect("name byte") = 0xff;
        let problem = Report::decode(&CEILINGS, &mut Reader::new(&invalid_utf8)).expect_err("invalid UTF-8");
        assert_eq!((problem.path, problem.reason), (Path::FieldName, Reason::Utf8));
    }

    #[test]
    fn unbounded_scalar_family_uses_big_endian_and_canonical_tags() {
        const SCALARS_WIRE: [u8; 27] =
            [1, 2, 3, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 5, 1, 0, 0, 0, 0, 0, 0, 0, 6, 7, 8, 1];
        let value = Scalars::new(
            &SCALAR_CEILINGS,
            ScalarsParts { a: 1, b: 0x0203, c: 4, d: 5, e: true, f: Duration::from_nanos(6), g: [7, 8], h: Flag::On },
        );
        assert_eq!(value.measure(), 27);
        let mut writer = Writer::new(27);
        value.encode(&mut writer).expect("measured room");
        assert_eq!(&*writer.finish(), &SCALARS_WIRE);
        assert_eq!(Scalars::decode(&SCALAR_CEILINGS, &mut Reader::new(&SCALARS_WIRE)), Ok(value));
    }

    #[test]
    fn variant_with_record_payload_has_tag_then_record_bytes() {
        let choice = Choice::Report(report());
        assert_eq!(choice.measure(), 35);
        let mut writer = Writer::new(35);
        choice.encode(&mut writer).expect("measured room");
        let wire = writer.finish();
        assert_eq!(wire.first(), Some(&0));
        assert_eq!(wire.get(1..), Some(WIRE.as_slice()));
        assert_eq!(Choice::decode(&CEILINGS, &mut Reader::new(&wire)), Ok(choice));

        let empty = Choice::Empty;
        let mut writer = Writer::new(1);
        empty.encode(&mut writer).expect("one byte");
        assert_eq!(&*writer.finish(), &[1]);
    }

    fn optional_payload(data: Option<&[u8]>) -> skein_codec_tests::lints::OptionalPayload {
        use skein_codec_tests::lints::{OptionalPayload, OptionalPayloadParts};
        OptionalPayload::new(&LINT_CEILINGS, OptionalPayloadParts { data: data.map(Box::from) }).expect("within limits")
    }

    #[test]
    fn a_boxed_variant_encodes_as_its_unboxed_twin() {
        use skein_codec_tests::lints::{BoxedChoice, InlineChoice};
        let boxed = BoxedChoice::new(&LINT_CEILINGS, BoxedChoice::Payload(Box::new(optional_payload(Some(b"x")))))
            .expect("within limits");
        let wire = [0, 1, 0, 0, 0, 1, b'x'];
        let mut writer = Writer::new(7);
        boxed.encode(&mut writer).expect("measured room");
        assert_eq!(writer.finish().as_ref(), wire);
        assert_eq!(BoxedChoice::decode(&LINT_CEILINGS, &mut Reader::new(&wire)), Ok(boxed));
        assert_eq!(
            InlineChoice::decode(&LINT_CEILINGS, &mut Reader::new(&wire)),
            Ok(InlineChoice::Payload(optional_payload(Some(b"x"))))
        );
    }

    #[test]
    fn a_boxed_variants_heap_counts_its_records_size() {
        use skein_codec_tests::lints::{BoxedChoice, InlineChoice, OptionalPayload};
        let record_size = u64::try_from(size_of::<OptionalPayload>()).expect("size fits u64");
        assert_eq!(InlineChoice::worst_case_heap(&LINT_CEILINGS), Some(4));
        assert_eq!(BoxedChoice::worst_case_heap(&LINT_CEILINGS), Some(record_size.checked_add(4).expect("heap size")));
    }

    #[test]
    fn an_optional_bytes_field_refuses_values_past_its_bound() {
        use skein_codec_tests::lints::{OptionalPayload, OptionalPayloadParts, Path};
        let problem =
            OptionalPayload::new(&LINT_CEILINGS, OptionalPayloadParts { data: Some(Box::from(&b"12345"[..])) })
                .expect_err("beyond its bound");
        assert_eq!((problem.path, problem.reason), (Path::OptionalPayloadData, Reason::Bound));
    }

    #[test]
    fn grouped_boolean_fields_keep_their_wire_order() {
        let parts =
            HostToolParts { skein_bools: [true, false, true, false], max_tokens: 7, payload: Box::from(&b"x"[..]) };
        let value = HostTool::new(&LINT_CEILINGS, parts).expect("within limits");
        assert!(value.inspect());
        assert!(!value.modify());
        assert!(value.shell());
        assert!(!value.agents());
        let wire = [1, 0, 1, 0, 0, 0, 0, 7, 0, 0, 0, 1, b'x'];
        let mut writer = Writer::new(wire.len());
        value.encode(&mut writer).expect("measured room");
        assert_eq!(writer.finish().as_ref(), wire);
        let decoded = HostTool::decode(&LINT_CEILINGS, &mut Reader::new(&wire)).expect("canonical bytes");
        assert_eq!(decoded.into_parts().skein_bools, [true, false, true, false]);
    }
}
