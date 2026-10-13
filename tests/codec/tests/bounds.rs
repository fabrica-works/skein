//! At and one beyond each sample field ceiling (codec.md, section 6).

#[cfg(test)]
mod tests {
    use skein_codec::Reason;
    use skein_codec_tests::v1::{CEILINGS, Effect, Field, FieldParts, Limits, Path, Report, check_limits};
    use skein_lib::{Reader, Writer, bytes};

    fn field_bytes(name_length: u32, value_length: u32) -> Box<[u8]> {
        let total = 8_u32.checked_add(name_length).and_then(|size| size.checked_add(value_length)).expect("test size");
        let mut writer = Writer::new(usize::try_from(total).expect("u32 fits usize"));
        writer.put(&name_length.to_be_bytes()).expect("room");
        writer.put(&bytes::zeroed(usize::try_from(name_length).expect("u32 fits usize"))).expect("room");
        writer.put(&value_length.to_be_bytes()).expect("room");
        writer.put(&bytes::zeroed(usize::try_from(value_length).expect("u32 fits usize"))).expect("room");
        writer.finish()
    }

    fn report_bytes(title_length: u32, field_count: u32) -> Box<[u8]> {
        let total = 22_u32
            .checked_add(title_length)
            .and_then(|size| size.checked_add(field_count.checked_mul(8)?))
            .expect("test size");
        let mut writer = Writer::new(usize::try_from(total).expect("u32 fits usize"));
        writer.put(&1_u16.to_be_bytes()).expect("version room");
        writer.put(&title_length.to_be_bytes()).expect("title length room");
        writer.put(&bytes::zeroed(usize::try_from(title_length).expect("u32 fits usize"))).expect("title room");
        writer.put(&field_count.to_be_bytes()).expect("field count room");
        for _ in 0_u32..field_count {
            writer.put(&[0_u8; 8]).expect("empty field room");
        }
        writer.put(&[0_u8; 12]).expect("effect, option, nonce, age room");
        writer.finish()
    }

    fn limits_past_ceilings() -> Limits {
        Limits { field_name: 5, field_value: 5, ..CEILINGS }
    }

    fn field_parts(value_length: usize) -> FieldParts {
        FieldParts { name: bytes::zeroed(0), value: bytes::zeroed(value_length) }
    }

    #[test]
    fn family_check_names_the_first_limit_past_its_ceiling() {
        let problem = check_limits(&limits_past_ceilings()).expect_err("limits past ceilings");
        assert_eq!((problem.path, problem.reason), (Path::FieldName, Reason::Bound));
    }

    #[test]
    fn constructor_with_limits_past_ceilings_accepts_a_value_at_its_ceiling() {
        Field::new(&limits_past_ceilings(), field_parts(4)).expect("value within its ceiling");
    }

    #[test]
    fn constructor_with_limits_past_ceilings_refuses_a_value_past_its_ceiling() {
        let problem = Field::new(&limits_past_ceilings(), field_parts(5)).expect_err("value past its ceiling");
        assert_eq!((problem.path, problem.reason), (Path::FieldValue, Reason::Bound));
    }

    #[test]
    fn decoder_with_limits_past_ceilings_accepts_a_value_at_its_ceiling() {
        Field::decode(&limits_past_ceilings(), &mut Reader::new(&field_bytes(0, 4))).expect("value within its ceiling");
    }

    #[test]
    fn decoder_with_limits_past_ceilings_refuses_a_value_past_its_ceiling() {
        let problem = Field::decode(&limits_past_ceilings(), &mut Reader::new(&field_bytes(0, 5)))
            .expect_err("value past its ceiling");
        assert_eq!((problem.path, problem.reason), (Path::FieldValue, Reason::Bound));
    }

    #[test]
    fn enum_constructor_does_not_check_unrelated_family_limits() {
        assert_eq!(Effect::new(&limits_past_ceilings(), Effect::Read), Ok(Effect::Read));
    }

    #[test]
    fn enum_decoder_does_not_check_unrelated_family_limits() {
        assert_eq!(Effect::decode(&limits_past_ceilings(), &mut Reader::new(&[0])), Ok(Effect::Read));
    }

    #[test]
    fn field_name_and_value_accept_the_ceiling_and_refuse_one_past() {
        Field::decode(&CEILINGS, &mut Reader::new(&field_bytes(4, 0))).expect("name at ceiling");
        let problem = Field::decode(&CEILINGS, &mut Reader::new(&field_bytes(5, 0))).expect_err("name over ceiling");
        assert_eq!((problem.path, problem.reason), (Path::FieldName, Reason::Bound));

        Field::decode(&CEILINGS, &mut Reader::new(&field_bytes(0, 4))).expect("value at ceiling");
        let problem = Field::decode(&CEILINGS, &mut Reader::new(&field_bytes(0, 5))).expect_err("value over ceiling");
        assert_eq!((problem.path, problem.reason), (Path::FieldValue, Reason::Bound));
    }

    #[test]
    fn report_title_and_field_count_accept_the_ceiling_and_refuse_one_past() {
        Report::decode(&CEILINGS, &mut Reader::new(&report_bytes(8, 0))).expect("title at ceiling");
        let problem = Report::decode(&CEILINGS, &mut Reader::new(&report_bytes(9, 0))).expect_err("title over ceiling");
        assert_eq!((problem.path, problem.reason), (Path::ReportTitle, Reason::Bound));

        Report::decode(&CEILINGS, &mut Reader::new(&report_bytes(0, 2))).expect("field count at ceiling");
        let problem =
            Report::decode(&CEILINGS, &mut Reader::new(&report_bytes(0, 3))).expect_err("field count over ceiling");
        assert_eq!((problem.path, problem.reason), (Path::ReportFields, Reason::Bound));
    }
}
