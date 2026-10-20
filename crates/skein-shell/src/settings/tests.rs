use super::{
    Base, EnvironmentEntry, Fallback, Origin, Reason, Refusal, Setting, Settings, Value, arguments, environment, help,
    resolve,
};

const CONFIG: Setting = Setting {
    name: "config",
    value: "DIR",
    about: "The directory of temper.json and its charters.",
    fallback: Fallback::Base(Base::Config),
};
const DATA: Setting = Setting {
    name: "data",
    value: "DIR",
    about: "The directory of stored data.",
    fallback: Fallback::Base(Base::Data),
};
const STATE: Setting = Setting {
    name: "state",
    value: "DIR",
    about: "The directory of process state.",
    fallback: Fallback::Base(Base::State),
};
const MEMORY: Setting =
    Setting { name: "memory", value: "BYTES", about: "The memory bound.", fallback: Fallback::Text("67108864") };
const REQUIRED: Setting =
    Setting { name: "identity", value: "NAME", about: "The service identity.", fallback: Fallback::Refuse };
const TABLE: Settings = Settings { tool: "temper", table: &[CONFIG, DATA, STATE, MEMORY, REQUIRED] };
const CONFIG_ONLY: Settings = Settings { tool: "temper", table: &[CONFIG] };
const REQUIRED_CONFIG: Settings =
    Settings { tool: "temper", table: &[Setting { fallback: Fallback::Refuse, ..CONFIG }] };
const BASES: [(Base, &[u8], &[u8]); 4] = [
    (Base::Config, b"XDG_CONFIG_HOME", b"/home/user/.config/temper"),
    (Base::Data, b"XDG_DATA_HOME", b"/home/user/.local/share/temper"),
    (Base::State, b"XDG_STATE_HOME", b"/home/user/.local/state/temper"),
    (Base::Cache, b"XDG_CACHE_HOME", b"/home/user/.cache/temper"),
];
const BASE_SETTINGS: [Settings; 4] = [
    CONFIG_ONLY,
    Settings { tool: "temper", table: &[Setting { fallback: Fallback::Base(Base::Data), ..CONFIG }] },
    Settings { tool: "temper", table: &[Setting { fallback: Fallback::Base(Base::State), ..CONFIG }] },
    Settings { tool: "temper", table: &[Setting { fallback: Fallback::Base(Base::Cache), ..CONFIG }] },
];

fn args(values: &[&[u8]]) -> Box<[Box<[u8]>]> {
    values.iter().map(|value| Box::from(*value)).collect()
}

fn env(values: &[(&[u8], &[u8])]) -> Box<[EnvironmentEntry]> {
    values.iter().map(|(key, value)| (Box::from(*key), Box::from(*value))).collect()
}

fn value(bytes: &[u8], origin: Origin) -> Value {
    Value { bytes: Box::from(bytes), origin }
}

#[test]
fn a_flag_wins_over_its_variable_and_default() {
    let resolved = resolve(
        &CONFIG_ONLY,
        &args(&[b"--config", b"/flag"]),
        &env(&[(b"TEMPER_CONFIG", b"/variable"), (b"XDG_CONFIG_HOME", b"/base")]),
    )
    .expect("flag supplies the setting");
    assert_eq!(resolved.value(0), &value(b"/flag", Origin::Flag), "flag has precedence");
}

#[test]
fn a_variable_wins_over_the_default() {
    const SETTINGS: Settings = Settings {
        tool: "skein-echo",
        table: &[Setting { name: "listen-address", fallback: Fallback::Text("default"), ..CONFIG }],
    };
    let resolved = resolve(&SETTINGS, &[], &env(&[(b"SKEIN_ECHO_LISTEN_ADDRESS", b"/variable")]))
        .expect("derived variable supplies the setting");
    assert_eq!(resolved.value(0), &value(b"/variable", Origin::Variable), "both names translate dashes");
}

#[test]
fn an_empty_variable_is_unset() {
    let resolved = resolve(&CONFIG_ONLY, &[], &env(&[(b"TEMPER_CONFIG", b""), (b"XDG_CONFIG_HOME", b"/base")]))
        .expect("empty variable permits the default");
    assert_eq!(resolved.value(0), &value(b"/base/temper", Origin::Base), "empty means unset");
}

#[test]
fn a_text_default_is_given_as_written() {
    const SETTINGS: Settings = Settings { tool: "skein-echo", table: &[MEMORY] };
    let resolved = resolve(&SETTINGS, &[], &[]).expect("text needs no environment");
    assert_eq!(resolved.value(0), &value(b"67108864", Origin::Default), "text is literal");
}

#[test]
fn a_setting_without_a_default_refuses_naming_flag_and_variable() {
    let refusal = resolve(&REQUIRED_CONFIG, &[], &[]).expect_err("required setting is absent");
    assert_eq!(
        refusal,
        Refusal { tool: "temper", setting: *REQUIRED_CONFIG.table.first().expect("one row"), reason: Reason::Missing },
        "refusal identifies its table row"
    );
    assert_eq!(
        refusal.to_string(),
        "--config DIR is not given, and TEMPER_CONFIG is not set",
        "absence names flag, value and variable"
    );
}

#[test]
fn each_base_takes_its_xdg_variable_then_the_tool() {
    for (settings, (_, variable, _)) in BASE_SETTINGS.iter().zip(BASES) {
        for base in [b"/base".as_slice(), b"/base/", b"/"] {
            let resolved = resolve(settings, &[], &env(&[(variable, base), (b"HOME", b"/home/user")]))
                .expect("absolute XDG base is usable");
            let expected = if base == b"/" { b"/temper".as_slice() } else { b"/base/temper" };
            assert_eq!(resolved.value(0), &value(expected, Origin::Base), "XDG wins and joins once");
        }
    }
}

#[test]
fn each_base_falls_back_under_home() {
    for (settings, (_, variable, expected)) in BASE_SETTINGS.iter().zip(BASES) {
        for home in [b"/home/user".as_slice(), b"/home/user/"] {
            for xdg in [None, Some(b"".as_slice()), Some(b"relative/dir".as_slice())] {
                let mut environment = env(&[(b"HOME", home)]).into_vec();
                if let Some(xdg) = xdg {
                    environment.push((Box::from(variable), Box::from(xdg)));
                }
                let resolved = resolve(settings, &[], &environment).expect("absolute HOME is usable");
                assert_eq!(resolved.value(0), &value(expected, Origin::Home), "specification's suffix and one slash");
            }
        }
    }
}

#[test]
fn no_absolute_base_refuses() {
    for environment in [
        env(&[(b"XDG_CONFIG_HOME", b"relative/dir")]),
        env(&[(b"XDG_CONFIG_HOME", b"relative/dir"), (b"HOME", b"relative/home")]),
        env(&[(b"XDG_CONFIG_HOME", b""), (b"HOME", b"")]),
    ] {
        let refusal = resolve(&CONFIG_ONLY, &[], &environment).expect_err("no absolute base");
        assert_eq!(refusal.reason, Reason::NoBase(Base::Config), "base default cannot be formed");
        assert_eq!(
            refusal.to_string(),
            "--config DIR is not given, TEMPER_CONFIG is not set, and its default $XDG_CONFIG_HOME/temper needs XDG_CONFIG_HOME or HOME as an absolute path",
            "refusal names the default and its bases"
        );
    }
}

#[test]
fn a_flag_without_its_value_refuses() {
    let refusal = resolve(&CONFIG_ONLY, &args(&[b"--config"]), &env(&[(b"TEMPER_CONFIG", b"/variable")]))
        .expect_err("flag cannot fall back to the variable");
    assert_eq!(refusal.reason, Reason::NoValue, "flag needs the next argument");
    assert_eq!(refusal.to_string(), "--config takes a value: DIR", "value label is shared with help");
}

#[test]
fn a_flag_given_twice_refuses() {
    for arguments in
        [args(&[b"--config", b"/first", b"--config", b"/second"]), args(&[b"--config", b"/first", b"--config"])]
    {
        let refusal = resolve(&CONFIG_ONLY, &arguments, &[]).expect_err("second flag refuses at once");
        assert_eq!(refusal.reason, Reason::Twice, "a repeated flag refuses before taking a value");
        assert_eq!(refusal.to_string(), "--config is given twice", "repetition names the flag");
    }
}

#[test]
fn the_rest_keeps_order() {
    let resolved = resolve(
        &TABLE,
        &args(&[b"first", b"--state", b"/s", b"--word", b"--config", b"/c", b"--config=x", b"--data", b"/d", b"last"]),
        &env(&[(b"TEMPER_IDENTITY", b"test")]),
    )
    .expect("every setting is supplied");
    assert_eq!(
        resolved.rest(),
        args(&[b"first", b"--word", b"--config=x", b"last"]).as_ref(),
        "leftovers stay in order"
    );
    for (row, expected) in [
        value(b"/c", Origin::Flag),
        value(b"/d", Origin::Flag),
        value(b"/s", Origin::Flag),
        value(b"67108864", Origin::Default),
        value(b"test", Origin::Variable),
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(resolved.value(row), expected, "values follow table order, independent of flag order");
    }
}

#[test]
fn a_double_dash_ends_the_settings() {
    let resolved = resolve(
        &CONFIG_ONLY,
        &args(&[b"first", b"--", b"--config", b"/after", b"--config"]),
        &env(&[(b"TEMPER_CONFIG", b"/variable")]),
    )
    .expect("flags after the boundary belong to the binary");
    assert_eq!(resolved.value(0), &value(b"/variable", Origin::Variable), "boundary prevents flag resolution");
    assert_eq!(
        resolved.rest(),
        args(&[b"first", b"--", b"--config", b"/after", b"--config"]).as_ref(),
        "boundary is retained"
    );
}

#[test]
fn a_value_may_begin_with_a_dash() {
    for bytes in [b"--word".as_slice(), b"--", b"--config", b""] {
        let resolved = resolve(&CONFIG_ONLY, &args(&[b"--config", bytes]), &[]).expect("next argument is a value");
        assert_eq!(resolved.value(0), &value(bytes, Origin::Flag), "values are taken without interpretation");
        assert!(resolved.rest().is_empty(), "the value is consumed");
    }
}

#[test]
fn values_are_bytes() {
    let flag = resolve(&CONFIG_ONLY, &args(&[b"--config", b"\xff\x00"]), &[]).expect("no UTF-8 check on flags");
    assert_eq!(flag.value(0), &value(b"\xff\x00", Origin::Flag), "flag bytes survive");
    let variable =
        resolve(&CONFIG_ONLY, &[], &env(&[(b"TEMPER_CONFIG", b"\xfe")])).expect("no UTF-8 check on variables");
    assert_eq!(variable.value(0), &value(b"\xfe", Origin::Variable), "variable bytes survive");
    let base = resolve(&CONFIG_ONLY, &[], &env(&[(b"XDG_CONFIG_HOME", b"/\xff/")])).expect("byte path is absolute");
    assert_eq!(base.value(0), &value(b"/\xff/temper", Origin::Base), "base bytes survive");
    let home = resolve(&CONFIG_ONLY, &[], &env(&[(b"HOME", b"/\xfe")])).expect("byte HOME is absolute");
    assert_eq!(home.value(0), &value(b"/\xfe/.config/temper", Origin::Home), "HOME bytes survive");
}

#[test]
fn help_says_what_refusals_say() {
    const ECHO: Settings = Settings { tool: "skein-echo", table: &[MEMORY] };
    let mut out = b"Settings:\n".to_vec();
    help(&TABLE, &mut out);
    assert_eq!(
        out,
        concat!(
            "Settings:\n",
            "  --config DIR (TEMPER_CONFIG; default $XDG_CONFIG_HOME/temper)\n",
            "      The directory of temper.json and its charters.\n",
            "  --data DIR (TEMPER_DATA; default $XDG_DATA_HOME/temper)\n",
            "      The directory of stored data.\n",
            "  --state DIR (TEMPER_STATE; default $XDG_STATE_HOME/temper)\n",
            "      The directory of process state.\n",
            "  --memory BYTES (TEMPER_MEMORY; default 67108864)\n",
            "      The memory bound.\n",
            "  --identity NAME (TEMPER_IDENTITY; required)\n",
            "      The service identity.\n",
        )
        .as_bytes(),
        "help appends the table verbatim"
    );
    for setting in TABLE.table {
        let required = Setting { fallback: Fallback::Refuse, ..*setting };
        let refusal = Refusal { tool: TABLE.tool, setting: required, reason: Reason::Missing }.to_string();
        let mut entry = Vec::new();
        help(&Settings { tool: TABLE.tool, table: std::slice::from_ref(setting) }, &mut entry);
        let entry = std::str::from_utf8(&entry).expect("help is text");
        let variable = entry.split_once('(').expect("variable begins").1.split_once(';').expect("variable ends").0;
        assert!(refusal.ends_with(&format!("and {variable} is not set")), "help and refusal name the same variable");
    }
    for (settings, (_, variable, _)) in BASE_SETTINGS.iter().zip(BASES) {
        let mut entry = Vec::new();
        help(settings, &mut entry);
        let expected = format!("default ${}/temper", std::str::from_utf8(variable).expect("XDG name is text"));
        assert!(
            std::str::from_utf8(&entry).expect("help is text").contains(&expected),
            "every base documents its variable"
        );
    }
    let mut entry = Vec::new();
    help(&ECHO, &mut entry);
    assert_eq!(
        entry, b"  --memory BYTES (SKEIN_ECHO_MEMORY; default 67108864)\n      The memory bound.\n",
        "tool name translates dashes in help"
    );
}

#[test]
fn the_first_environment_entry_counts_even_when_empty_or_relative() {
    let resolved = resolve(&CONFIG_ONLY, &[], &env(&[(b"TEMPER_CONFIG", b"first"), (b"TEMPER_CONFIG", b"second")]))
        .expect("first entry is set");
    assert_eq!(resolved.value(0), &value(b"first", Origin::Variable), "later duplicate is ignored");
    let resolved = resolve(
        &CONFIG_ONLY,
        &[],
        &env(&[(b"TEMPER_CONFIG", b""), (b"TEMPER_CONFIG", b"second"), (b"XDG_CONFIG_HOME", b"/base")]),
    )
    .expect("empty first entry falls to default");
    assert_eq!(resolved.value(0), &value(b"/base/temper", Origin::Base), "later nonempty duplicate does not count");
    let resolved = resolve(
        &CONFIG_ONLY,
        &[],
        &env(&[
            (b"XDG_CONFIG_HOME", b"relative"),
            (b"XDG_CONFIG_HOME", b"/later"),
            (b"HOME", b"/first"),
            (b"HOME", b"/second"),
        ]),
    )
    .expect("first HOME supplies the base");
    assert_eq!(resolved.value(0), &value(b"/first/.config/temper", Origin::Home), "first base entries count too");
}

#[test]
fn the_process_arguments_and_environment_are_read_as_bytes() {
    let _arguments = arguments();
    let environment = environment();
    assert!(environment.iter().any(|(name, _)| name.as_ref() == b"PATH"), "nextest supplies PATH");
}
